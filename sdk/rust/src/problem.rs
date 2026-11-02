//! Problem-details errors and the helpers that normalise them.
//!
//! Every hub failure is an RFC 9457 `application/problem+json` document. The
//! `code` is a stable machine-readable string; an unknown code is preserved
//! verbatim, as `docs/API_CONTRACT.md` §3.2 requires, so a client survives an
//! additive contract change.

use std::collections::HashMap;

use crate::generated::types::Problem;

/// A non-2xx response, with the problem document flattened into typed fields.
#[derive(Debug, Clone, PartialEq)]
pub struct ProblemError {
    status: u16,
    code: String,
    detail: Option<String>,
    title: Option<String>,
    problem_type: Option<String>,
    request_id: Option<String>,
    operation_id: &'static str,
    attempts: u32,
    retry_after_ms: Option<u64>,
    body: Option<serde_json::Value>,
}

impl ProblemError {
    /// Build an error from a parsed problem document and the transport facts.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_parts(
        operation_id: &'static str,
        status: u16,
        headers: &HashMap<String, String>,
        body: &[u8],
        attempts: u32,
    ) -> Self {
        let parsed = serde_json::from_slice::<Problem>(body).ok();
        let value = serde_json::from_slice::<serde_json::Value>(body).ok();
        let header_request_id = headers.get("x-request-id").cloned();
        let code = parsed
            .as_ref()
            .map(|problem| problem.code.clone())
            .filter(|code| !code.is_empty())
            .or_else(|| value.as_ref().and_then(|value| string_at(value, "error")))
            .unwrap_or_else(|| format!("http_{status}"));
        let detail = parsed
            .as_ref()
            .and_then(|problem| problem.detail.clone())
            .or_else(|| value.as_ref().and_then(|value| string_at(value, "message")))
            .or_else(|| raw_detail(body));
        let title = parsed.as_ref().and_then(|problem| problem.title.clone());
        let problem_type = parsed
            .as_ref()
            .and_then(|problem| problem.problem_type.clone());
        let request_id = parsed
            .as_ref()
            .and_then(|problem| problem.request_id.clone())
            .or_else(|| value.as_ref().and_then(|value| string_at(value, "id")))
            .or(header_request_id);
        Self {
            status,
            code,
            detail,
            title,
            problem_type,
            request_id,
            operation_id,
            attempts,
            retry_after_ms: retry_after_ms(headers.get("retry-after").map(String::as_str)),
            body: value,
        }
    }

    /// HTTP status of the failed response.
    pub fn status(&self) -> u16 {
        self.status
    }

    /// Stable machine-readable problem code, verbatim.
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Human-readable explanation, when the server sent one.
    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }

    /// Short human-readable summary, when the server sent one.
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Problem type URI, when the server sent one.
    pub fn problem_type(&self) -> Option<&str> {
        self.problem_type.as_deref()
    }

    /// Correlation id from the body or the `x-request-id` header.
    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }

    /// Operation id that produced the failure.
    pub fn operation_id(&self) -> &'static str {
        self.operation_id
    }

    /// How many HTTP attempts produced this error (1 means no retry happened).
    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    /// Server-requested delay, when a 429 carried a `Retry-After`.
    pub fn retry_after_ms(&self) -> Option<u64> {
        self.retry_after_ms
    }

    /// Parsed response body, preserved for diagnostics.
    pub fn body(&self) -> Option<&serde_json::Value> {
        self.body.as_ref()
    }

    /// True when the failure is a rate limit.
    pub fn rate_limited(&self) -> bool {
        self.status == 429
    }
}

impl std::fmt::Display for ProblemError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} -> {} {}",
            self.operation_id, self.status, self.code
        )?;
        if let Some(detail) = &self.detail {
            write!(formatter, ": {detail}")?;
        }
        if let Some(request_id) = &self.request_id {
            write!(formatter, " (requestId {request_id})")?;
        }
        Ok(())
    }
}

impl std::error::Error for ProblemError {}

/// Parse a `Retry-After` header into milliseconds.
///
/// The delta-seconds form is honoured; an HTTP-date is treated as absent
/// because the pool only sends delta-seconds. A nonsense value is ignored.
pub fn retry_after_ms(value: Option<&str>) -> Option<u64> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    let seconds = value.parse::<u64>().ok()?;
    seconds.checked_mul(1_000)
}

/// Read a string member of a JSON object.
fn string_at(value: &serde_json::Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(str::to_owned)
}

/// A short, single-line description of a body that is not a problem document.
fn raw_detail(body: &[u8]) -> Option<String> {
    if body.is_empty() {
        return None;
    }
    let text = String::from_utf8_lossy(body);
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.is_empty() {
        return None;
    }
    let truncated: String = compact.chars().take(512).collect();
    Some(truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn parses_a_hub_problem_document() {
        let body = br#"{"type":"https://example/errors","title":"Not found","status":404,"code":"not_found","detail":"no such host","requestId":"req_1"}"#;
        let problem = ProblemError::from_parts("get_v1_hosts_id", 404, &headers(&[]), body, 1);
        assert_eq!(problem.status(), 404);
        assert_eq!(problem.code(), "not_found");
        assert_eq!(problem.detail(), Some("no such host"));
        assert_eq!(problem.request_id(), Some("req_1"));
        assert_eq!(problem.attempts(), 1);
    }

    #[test]
    fn header_request_id_wins_when_the_body_omits_it() {
        let body = br#"{"error":"unauthorized"}"#;
        let problem = ProblemError::from_parts(
            "get_v1_pool_me",
            401,
            &headers(&[("x-request-id", "req_header")]),
            body,
            2,
        );
        assert_eq!(problem.code(), "unauthorized");
        assert_eq!(problem.request_id(), Some("req_header"));
        assert_eq!(problem.attempts(), 2);
    }

    #[test]
    fn unknown_codes_and_unknown_shapes_stay_opaque() {
        let body = br#"{"code":"made_up_later","detail":"new"}"#;
        let problem = ProblemError::from_parts("get_v1_hosts", 409, &headers(&[]), body, 1);
        assert_eq!(problem.code(), "made_up_later");

        let html = ProblemError::from_parts(
            "get_v1_hosts",
            502,
            &headers(&[]),
            b"<html>bad gateway</html>",
            1,
        );
        assert_eq!(html.code(), "http_502");
        assert_eq!(html.detail(), Some("<html>bad gateway</html>"));
    }

    #[test]
    fn retry_after_accepts_seconds_only() {
        assert_eq!(retry_after_ms(Some("2")), Some(2_000));
        assert_eq!(retry_after_ms(Some(" 0 ")), Some(0));
        assert_eq!(retry_after_ms(Some("Wed, 21 Oct 2015 07:28:00 GMT")), None);
        assert_eq!(retry_after_ms(Some("-1")), None);
        assert_eq!(retry_after_ms(None), None);
    }

    #[test]
    fn display_includes_code_and_request_id() {
        let body = br#"{"code":"conflict","detail":"slug taken","requestId":"req_9"}"#;
        let problem = ProblemError::from_parts("post_v1_hosts", 409, &headers(&[]), body, 1);
        let rendered = problem.to_string();
        assert!(rendered.contains("409"));
        assert!(rendered.contains("conflict"));
        assert!(rendered.contains("req_9"));
    }

    #[test]
    fn rate_limited_only_for_429() {
        let limited = ProblemError::from_parts("post_v1_runs", 429, &headers(&[]), b"", 2);
        assert!(limited.rate_limited());
        assert_eq!(limited.code(), "http_429");
    }
}
