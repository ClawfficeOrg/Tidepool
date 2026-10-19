//! Minimal reference host agent (task 1.2.5).
//!
//! Environment:
//! - `POOL_HTTP_URL` (default `http://127.0.0.1:8080`)
//! - `POOL_WS_URL` (default `ws://127.0.0.1:8080/ws/host`)
//! - `HOST_ID` and `CREDENTIAL`, or `ENROLLMENT_TOKEN` to enroll once
//! - `CAPABILITIES` optional capability JSON array
//! - `EXIT_AFTER_EXECUTION=1` exits after the first completion
//!
//! The agent reports capabilities, connects with the relationship credential,
//! heartbeats, answers create-agent intents and replays completions by id.

use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};
use std::io::ErrorKind;
use std::time::{Duration, Instant};
use tidepool_host_agent::AgentState;
use tungstenite::client::IntoClientRequest;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

fn main() -> Result<()> {
    let http = std::env::var("POOL_HTTP_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".into());
    let ws_url =
        std::env::var("POOL_WS_URL").unwrap_or_else(|_| "ws://127.0.0.1:8080/ws/host".into());
    let credential = std::env::var("CREDENTIAL").context("CREDENTIAL is required")?;
    let exit_after = std::env::var("EXIT_AFTER_EXECUTION").as_deref() == Ok("1");

    let host_id = match std::env::var("HOST_ID") {
        Ok(id) if !id.is_empty() => id,
        _ => enroll(&http, &credential)?,
    };

    report_capabilities(&http, &host_id, &credential)?;
    println!("agent: host {host_id} ready");

    let mut state = AgentState::default();
    loop {
        match run_socket(&ws_url, &host_id, &credential, &mut state, exit_after) {
            Ok(()) => break,
            Err(error) => {
                eprintln!("agent: connection ended: {error:#}; reconnecting");
                std::thread::sleep(Duration::from_secs(2));
            }
        }
    }
    Ok(())
}

fn enroll(http: &str, credential: &str) -> Result<String> {
    let token =
        std::env::var("ENROLLMENT_TOKEN").context("HOST_ID or ENROLLMENT_TOKEN required")?;
    let url = format!("{http}/v1/hosts/enroll");
    let body = json!({"token": token, "credential": credential});
    let response: Value = ureq::post(&url)
        .set("content-type", "application/json")
        .send_json(body)
        .map_err(|e| anyhow!("enroll failed: {e}"))?
        .into_json()
        .context("enroll response")?;
    response["id"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| anyhow!("enroll response has no id"))
}

fn report_capabilities(http: &str, host_id: &str, credential: &str) -> Result<()> {
    let raw = std::env::var("CAPABILITIES").unwrap_or_else(|_| {
        json!([
            {"provider": "local", "model": "echo", "execution_mode": "default"},
            {"provider": "anthropic", "model": "claude", "execution_mode": "default"}
        ])
        .to_string()
    });
    let capabilities: Value = serde_json::from_str(&raw).context("CAPABILITIES JSON")?;
    let url = format!("{http}/v1/hosts/{host_id}/capabilities");
    match ureq::post(&url)
        .set("content-type", "application/json")
        .set("X-Host-Id", host_id)
        .set("X-Host-Credential", credential)
        .send_json(json!({"capabilities": capabilities}))
    {
        Ok(_) => Ok(()),
        Err(ureq::Error::Status(403, _)) => {
            eprintln!("agent: host.execute is not granted; waiting for the operator");
            Ok(())
        }
        Err(error) => Err(anyhow!("capability report failed: {error}")),
    }
}

fn run_socket(
    ws_url: &str,
    host_id: &str,
    credential: &str,
    state: &mut AgentState,
    exit_after: bool,
) -> Result<()> {
    let mut request = ws_url.into_client_request()?;
    request.headers_mut().insert("X-Host-Id", host_id.parse()?);
    request
        .headers_mut()
        .insert("X-Host-Credential", credential.parse()?);
    let (mut socket, _) = tungstenite::connect(request)?;
    set_read_timeout(&mut socket, Some(Duration::from_secs(5)));

    let mut last_heartbeat = Instant::now();
    loop {
        if last_heartbeat.elapsed() >= Duration::from_secs(30) {
            socket.send(Message::Text(
                json!({"v": 1, "type": "heartbeat"}).to_string(),
            ))?;
            last_heartbeat = Instant::now();
        }

        match socket.read() {
            Ok(Message::Text(text)) => {
                let frame: Value = serde_json::from_str(&text).unwrap_or_default();
                match frame["type"].as_str().unwrap_or_default() {
                    "heartbeat.ack" => {}
                    "create_agent" => {
                        let execution_id = frame["executionId"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string();
                        let prompt = frame["prompt"].as_str().unwrap_or_default().to_string();
                        let frames = state.handle_create(&execution_id, &prompt);
                        for frame in frames {
                            socket.send(Message::Text(frame.to_string()))?;
                        }
                        if exit_after && state.is_complete(&execution_id) {
                            return Ok(());
                        }
                    }
                    "interrupt_execution" => {
                        let execution_id = frame["executionId"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string();
                        for frame in state.handle_interrupt(&execution_id) {
                            socket.send(Message::Text(frame.to_string()))?;
                        }
                    }
                    "error" => {
                        eprintln!("agent: pool error: {frame}");
                    }
                    _ => {}
                }
            }
            Ok(Message::Ping(bytes)) => socket.send(Message::Pong(bytes))?,
            Ok(Message::Close(_)) => return Ok(()),
            Ok(_) => {}
            Err(tungstenite::Error::Io(error))
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
            {
                // Read timeout: loop to send the next heartbeat.
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn set_read_timeout(
    socket: &mut WebSocket<MaybeTlsStream<std::net::TcpStream>>,
    timeout: Option<Duration>,
) {
    match socket.get_mut() {
        MaybeTlsStream::Plain(stream) => {
            let _ = stream.set_read_timeout(timeout);
        }
        MaybeTlsStream::Rustls(stream) => {
            let _ = stream.get_ref().set_read_timeout(timeout);
        }
        _ => {}
    }
}
