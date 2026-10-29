//! Conformance adapter for the Rust SDK (`tidepool-client`).
//!
//! Thin translation only: the suite's generic commands become SDK calls and
//! the answers are normalised. No scenario lives here.
//!
//! Protocol: one JSON request per line on stdin, one JSON response per line on
//! stdout. Requests are `{id, command, params}`; responses are
//! `{id, ok: true, result}` or `{id, ok: false, error}`. Diagnostics go to
//! stderr.

use std::io::{self, BufRead, Write};
use std::time::Duration;

use serde_json::{Map, Value, json};
use tidepool_client::blocking::Client;
use tidepool_client::endpoints::{
    GetV1ChatroomsRoomStream, GetV1ChatroomsRoomStreamQuery,
};
use tidepool_client::paging::{self, ActivityFilters, PageOptions};
use tidepool_client::stream::ChatFrame;
use tidepool_client::{CallOptions, ClientConfig};
use tidepool_client::generated::routes::ROUTES;

/// Cap on pages walked by one `page` command; a cursor that never ends must
/// fail the run rather than hang it.
const MAX_PAGES: usize = 50;

/// Everything the adapter itself can fail with.
#[derive(Debug, thiserror::Error)]
enum AdapterError {
    /// The request is missing a field or names something unknown.
    #[error("{0}")]
    Protocol(String),
    /// The SDK returned an error.
    #[error("{0}")]
    Sdk(String),
    /// A value could not be encoded for the wire.
    #[error("could not encode a value: {0}")]
    Json(#[from] serde_json::Error),
}

/// Read the ambient credentials a step names.
fn auth_of(params: &Value) -> (Option<String>, Option<String>, Option<String>) {
    let auth = params.get("auth").cloned().unwrap_or(Value::Null);
    let text = |name: &str| {
        auth.get(name)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .filter(|value| !value.is_empty())
    };
    (text("token"), text("apiKey"), text("hostCredential"))
}

/// Build one blocking client for the step's ambient credentials.
fn client(base_url: &str, params: &Value) -> Result<Client, AdapterError> {
    let mut config = ClientConfig::new(base_url).map_err(|error| AdapterError::Sdk(error.to_string()))?;
    let (token, api_key, host_credential) = auth_of(params);
    if let Some(token) = token {
        config = config.with_token(token);
    }
    if let Some(api_key) = api_key {
        config = config.with_api_key(api_key);
    }
    if let Some(host_credential) = host_credential {
        config = config.with_host_credential(host_credential);
    }
    config = config.with_timeout(Duration::from_secs(60));
    Client::new(config).map_err(|error| AdapterError::Sdk(error.to_string()))
}

/// Render one JSON scalar as the string a path or query parameter expects.
fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

/// Turn an object of parameters into the SDK's pair representation.
fn pairs(value: Option<&Value>) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    if let Some(Value::Object(object)) = value {
        for (name, item) in object {
            if item.is_null() {
                continue;
            }
            if let Some(text) = scalar(item) {
                pairs.push((name.clone(), text));
            }
        }
    }
    pairs
}

/// Turn an object of headers into the SDK's pair representation.
fn headers(value: Option<&Value>) -> Vec<(String, String)> {
    pairs(value)
}

/// Fetch `operation` from the request, naming the omission when absent.
fn operation_of(params: &Value) -> Result<&str, AdapterError> {
    params
        .get("operation")
        .and_then(Value::as_str)
        .ok_or_else(|| AdapterError::Protocol("operation is required".into()))
}

/// Call one operation by id and normalise success or problem details.
fn call(base_url: &str, params: &Value) -> Result<Value, AdapterError> {
    let client = client(base_url, params)?;
    let operation = operation_of(params)?;
    let path_params = pairs(params.get("pathParams"));
    let borrow: Vec<(&str, &str)> = path_params
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    let query = pairs(params.get("query"));
    let query_borrow: Vec<(&str, &str)> = query
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    let body = match params.get("body") {
        None | Some(Value::Null) => None,
        Some(value) => Some(value),
    };
    let mut options = CallOptions::new();
    if let Some(request_id) = params.get("requestId").and_then(Value::as_str) {
        options = options.with_request_id(request_id);
    }
    for (name, value) in headers(params.get("headers")) {
        options = options.with_header(name, value);
    }
    match client.request_json_with(
        operation,
        &borrow,
        &query_borrow,
        body,
        &options,
    ) {
        Ok(payload) => Ok(json!({ "outcome": "ok", "body": payload })),
        Err(error) => match error.problem() {
            Some(problem) => Ok(json!({
                "outcome": "error",
                "status": problem.status(),
                "code": problem.code(),
                "title": problem.title(),
                "detail": problem.detail(),
                "type": problem.problem_type(),
                "request_id": problem.request_id(),
                "attempts": problem.attempts(),
                "retry_after_ms": problem.retry_after_ms(),
            })),
            None => Err(AdapterError::Sdk(error.to_string())),
        },
    }
}

/// Return the Rust SDK's route metadata for one operation id.
fn describe(params: &Value) -> Result<Value, AdapterError> {
    let operation = operation_of(params)?;
    let route = tidepool_client::describe(operation)
        .map_err(|error| AdapterError::Sdk(error.to_string()))?;
    Ok(json!({
        "operation_id": route.operation_id,
        "method": route.method.as_str(),
        "path": route.path,
        "auth": route.auth.as_str(),
        "scope": route.scope,
        "role": route.role,
        "surface": route.surface.as_str(),
        "path_params": route.path_params,
        "query_params": route.query_params,
    }))
}

/// Walk one of the SDK's cursor iterators and normalise the items.
fn page(base_url: &str, params: &Value) -> Result<Value, AdapterError> {
    let client = client(base_url, params)?;
    let kind = params
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| AdapterError::Protocol("kind is required".into()))?;
    let identifier = params.get("id").and_then(Value::as_str).unwrap_or("");
    let options = PageOptions {
        after: params.get("after").and_then(Value::as_u64),
        limit: params.get("limit").and_then(Value::as_u64),
        max_pages: Some(MAX_PAGES),
    };
    let mut items = Vec::new();
    match kind {
        "chat" => {
            for message in paging::chat_messages(&client, identifier, options) {
                let message = message.map_err(|error| AdapterError::Sdk(error.to_string()))?;
                items.push(json!({ "sequence": message.sequence, "body": message.body }));
            }
        }
        "relay" => {
            for event in paging::relay_events(&client, identifier, options) {
                let event = event.map_err(|error| AdapterError::Sdk(error.to_string()))?;
                items.push(json!({ "sequence": event.sequence, "kind": event.kind }));
            }
        }
        "activity" => {
            for entry in paging::activity(&client, options, ActivityFilters::default()) {
                let entry = entry.map_err(|error| AdapterError::Sdk(error.to_string()))?;
                items.push(json!({ "kind": entry.kind, "status": entry.status }));
            }
        }
        other => {
            return Err(AdapterError::Protocol(format!(
                "unknown paging kind `{other}`"
            )));
        }
    }
    Ok(json!({ "items": items }))
}

/// Split a query string into decoded-enough pairs for the fields the suite
/// asserts on (the token is a JWT and the cursor is a number).
fn query_pairs(url: &str) -> Vec<(String, String)> {
    let Some((_, query)) = url.split_once('?') else {
        return Vec::new();
    };
    query
        .split('&')
        .filter(|part| !part.is_empty())
        .filter_map(|part| {
            let (name, value) = part.split_once('=').unwrap_or((part, ""));
            Some((name.to_owned(), value.to_owned()))
        })
        .collect()
}

/// Look up one query parameter in an already-built URL.
fn query_value(url: &str, name: &str) -> Option<String> {
    query_pairs(url)
        .into_iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
}

/// Build the SDK's live-stream handle without opening a socket.
fn stream(base_url: &str, params: &Value) -> Result<Value, AdapterError> {
    let client = client(base_url, params)?;
    let room = pairs(params.get("pathParams"))
        .into_iter()
        .find(|(name, _)| name == "room")
        .map(|(_, value)| value)
        .ok_or_else(|| AdapterError::Protocol("pathParams.room is required".into()))?;
    let after = params.get("after").and_then(Value::as_u64);
    let handle = client
        .stream(
            &GetV1ChatroomsRoomStream::new(room).with_query(
                GetV1ChatroomsRoomStreamQuery {
                    token: None,
                    after,
                },
            ),
        )
        .map_err(|error| AdapterError::Sdk(error.to_string()))?;
    let url = handle.url();
    let scheme = url.split("://").next().unwrap_or("").to_owned();
    let path = url
        .split("://")
        .nth(1)
        .and_then(|rest| rest.split('?').next())
        .and_then(|rest| rest.find('/').map(|index| rest[index..].to_owned()))
        .unwrap_or_default();
    let token = handle.token();
    let token_present = query_value(url, "token").is_some();
    let token_matches = query_value(url, "token").as_deref() == Some(token);
    Ok(json!({
        "scheme": scheme,
        "after": handle.after(),
        "token_present": token_present,
        "token_matches": token_matches,
        "after_param": query_value(url, "after"),
        "path": path,
    }))
}

/// Parse socket payloads with the SDK's typed frame parser.
fn frames(params: &Value) -> Result<Value, AdapterError> {
    let mut parsed = Vec::new();
    for text in params
        .get("texts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let text = text
            .as_str()
            .ok_or_else(|| AdapterError::Protocol("texts must be strings".into()))?;
        let frame = ChatFrame::parse(text);
        let mut sequences = Vec::new();
        if let ChatFrame::History { messages, .. } = &frame {
            sequences.extend(messages.iter().map(|message| message.sequence));
        }
        if let ChatFrame::Message { message, .. } = &frame {
            sequences.push(message.sequence);
        }
        let code = match &frame {
            ChatFrame::Error { code, .. } => code.clone(),
            _ => None,
        };
        let catch_up = match &frame {
            ChatFrame::History { catch_up, .. }
            | ChatFrame::Current { catch_up, .. }
            | ChatFrame::Message { catch_up, .. }
            | ChatFrame::Presence { catch_up, .. }
            | ChatFrame::CatchUp { catch_up, .. } => *catch_up,
            _ => false,
        };
        parsed.push(json!({
            "kind": frame.frame_type(),
            "catch_up": catch_up,
            "cursor": frame.cursor(),
            "sequences": sequences,
            "code": code,
        }));
    }
    Ok(json!({ "frames": parsed }))
}

/// Dispatcher for the six protocol commands.
fn handle(base_url: &str, request: &Value) -> Result<Value, AdapterError> {
    let params = request.get("params").cloned().unwrap_or(Value::Null);
    let command = request
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match command {
        "contract" => Ok(json!({
            "contract_version": tidepool_client::CONTRACT_VERSION,
            "operation_count": ROUTES.len(),
        })),
        "describe" => describe(&params),
        "call" => call(base_url, &params),
        "page" => page(base_url, &params),
        "stream" => stream(base_url, &params),
        "frames" => frames(&params),
        other => Err(AdapterError::Protocol(format!(
            "unknown command `{other}`"
        ))),
    }
}

/// Serve requests until stdin closes.
fn main() {
    let base_url = match std::env::var("TIDEPOOL_CONFORMANCE_BASE_URL") {
        Ok(value) if !value.is_empty() => value,
        _ => {
            eprintln!("rust adapter: TIDEPOOL_CONFORMANCE_BASE_URL is not set");
            std::process::exit(2);
        }
    };
    let stdin = io::stdin();
    let stdout = io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            std::process::exit(2);
        };
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => {
                eprintln!("rust adapter: request is not JSON");
                std::process::exit(2);
            }
        };
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let response: Map<String, Value> = match handle(&base_url, &request) {
            Ok(result) => {
                let mut response = Map::new();
                response.insert("id".into(), id);
                response.insert("ok".into(), Value::Bool(true));
                response.insert("result".into(), result);
                response
            }
            Err(error) => {
                let kind = match &error {
                    AdapterError::Protocol(_) => "ProtocolError",
                    AdapterError::Sdk(_) => "SdkError",
                    AdapterError::Json(_) => "EncodeError",
                };
                let mut response = Map::new();
                response.insert("id".into(), id);
                response.insert("ok".into(), Value::Bool(false));
                response.insert(
                    "error".into(),
                    json!({ "type": kind, "message": error.to_string() }),
                );
                response
            }
        };
        let mut writer = stdout.lock();
        if writeln!(writer, "{}", Value::Object(response)).is_err() {
            std::process::exit(2);
        }
        if writer.flush().is_err() {
            std::process::exit(2);
        }
    }
}
