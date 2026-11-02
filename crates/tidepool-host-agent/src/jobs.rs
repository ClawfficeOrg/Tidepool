//! Typed `tkrun` jobs for the reference host agent (task 1.7.4).
//!
//! The pool sends a typed `start_job`; this module decides whether the host
//! will run it and builds the fixed argv. Nothing here spawns a process or
//! touches the network, so every rule is unit-tested. `main.rs` does the I/O.
//!
//! Rules, in order:
//! 1. The frame must be `kind: "tkrun"` with a bounded `jobId`, a slug, an
//!    https (or loopback http) server and only known option keys.
//! 2. The checkout name must be in this host's local allowlist. The pool's
//!    checkout name is a request; the local directory is the authority.
//! 3. The TideTable token must be present on this host. It is never in a frame.

use serde_json::{Value, json};
use std::collections::HashMap;

/// Feature name this agent offers.
pub const JOBS_TKRUN_FEATURE: &str = "jobs.tkrun";

/// Lines kept for the completion summary.
pub const SUMMARY_LINES: usize = 20;

/// A refusal the pool hears as `job_refused`. `detail` never has a secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// Stable code from HOST_PROTOCOL.md.
    pub code: &'static str,
    /// Human-readable detail.
    pub detail: String,
}

impl Refusal {
    fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

/// Options after validation. `None` means the TKRun default applies.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TkrunOptions {
    /// Hours of budget, 0-12.
    pub hours: Option<u8>,
    /// Minutes of budget, 0-59.
    pub minutes: Option<u8>,
    /// Task ids to run.
    pub tasks: Vec<String>,
    /// Parallel workers, 1-4.
    pub jobs: Option<u8>,
    /// Dry run only.
    pub dry_run: bool,
}

/// A validated job, ready to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartSpec {
    /// Pool-supplied job id.
    pub job_id: String,
    /// TideTable slug.
    pub slug: String,
    /// Hub origin.
    pub server: String,
    /// Checkout name as the pool sent it.
    pub checkout: String,
    /// Local directory for the checkout, from this host's allowlist.
    pub directory: String,
    /// Validated options.
    pub options: TkrunOptions,
}

const OPTION_KEYS: [&str; 5] = ["hours", "minutes", "tasks", "jobs", "dryRun"];

/// Validate a `start_job` frame against this host's checkout allowlist.
pub fn validate_start(
    frame: &Value,
    checkouts: &HashMap<String, String>,
) -> Result<StartSpec, Refusal> {
    let job_id = frame["jobId"].as_str().unwrap_or_default();
    if job_id.is_empty() || job_id.len() > 64 {
        return Err(Refusal::new(
            "invalid_request",
            "jobId must be 1-64 characters",
        ));
    }
    if frame["kind"].as_str() != Some("tkrun") {
        return Err(Refusal::new("invalid_request", "kind must be tkrun"));
    }

    let slug = frame["project"]["slug"].as_str().unwrap_or_default();
    if !is_slug(slug) {
        return Err(Refusal::new(
            "invalid_request",
            "project.slug is not a TideTable slug",
        ));
    }
    let server = frame["project"]["server"].as_str().unwrap_or_default();
    if !is_allowed_server(server) {
        return Err(Refusal::new(
            "invalid_request",
            "project.server must be https, or http on loopback",
        ));
    }

    let checkout = frame["checkout"].as_str().unwrap_or_default();
    let directory = checkouts.get(checkout).ok_or_else(|| {
        Refusal::new(
            "checkout_not_allowed",
            "checkout is not on this host's allowlist",
        )
    })?;

    let options = parse_options(&frame["options"])?;

    Ok(StartSpec {
        job_id: job_id.to_string(),
        slug: slug.to_string(),
        server: server.to_string(),
        checkout: checkout.to_string(),
        directory: directory.clone(),
        options,
    })
}

fn parse_options(value: &Value) -> Result<TkrunOptions, Refusal> {
    let invalid = |detail: &str| Refusal::new("invalid_request", detail.to_string());
    let object = value
        .as_object()
        .ok_or_else(|| invalid("options must be an object"))?;
    if let Some(key) = object
        .keys()
        .find(|key| !OPTION_KEYS.contains(&key.as_str()))
    {
        return Err(invalid(&format!("unknown option '{key}'")));
    }

    let bounded = |key: &str, max: u64, min: u64| -> Result<Option<u8>, Refusal> {
        match object.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => match v.as_u64() {
                Some(n) if (min..=max).contains(&n) => Ok(Some(n as u8)),
                _ => Err(invalid(&format!("{key} must be {min}-{max}"))),
            },
        }
    };
    let hours = bounded("hours", 12, 0)?;
    let minutes = bounded("minutes", 59, 0)?;
    let jobs = bounded("jobs", 4, 1)?;

    let tasks: Vec<String> = match object.get("tasks") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) => {
            if items.len() > 16 {
                return Err(invalid("at most 16 tasks"));
            }
            items
                .iter()
                .map(|item| match item.as_str() {
                    Some(id) if is_task_id(id) => Ok(id.to_string()),
                    _ => Err(invalid("task ids must look like X.Y.Z")),
                })
                .collect::<Result<_, _>>()?
        }
        Some(_) => return Err(invalid("tasks must be an array")),
    };

    let dry_run = match object.get("dryRun") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(_) => return Err(invalid("dryRun must be a boolean")),
    };

    Ok(TkrunOptions {
        hours,
        minutes,
        tasks,
        jobs,
        dry_run,
    })
}

/// The argv for `tidetable --server <server> tkrun run <slug> --repo <dir> ...`.
/// `bin` is the first element; the token travels in the environment, not here.
pub fn run_argv(bin: &str, spec: &StartSpec) -> Vec<String> {
    let mut argv = vec![
        bin.to_string(),
        "--server".into(),
        spec.server.clone(),
        "tkrun".into(),
        "run".into(),
        spec.slug.clone(),
        "--repo".into(),
        spec.directory.clone(),
    ];
    if let Some(hours) = spec.options.hours {
        argv.extend(["--hours".into(), hours.to_string()]);
    }
    if let Some(minutes) = spec.options.minutes {
        argv.extend(["--minutes".into(), minutes.to_string()]);
    }
    if let Some(jobs) = spec.options.jobs {
        argv.extend(["--jobs".into(), jobs.to_string()]);
    }
    for task in &spec.options.tasks {
        argv.extend(["--task".into(), task.clone()]);
    }
    if spec.options.dry_run {
        argv.push("--dry-run".into());
    }
    argv
}

/// The argv for `tidetable --server <server> tkrun stop <slug> --repo <dir>`.
pub fn stop_argv(bin: &str, spec: &StartSpec) -> Vec<String> {
    vec![
        bin.to_string(),
        "--server".into(),
        spec.server.clone(),
        "tkrun".into(),
        "stop".into(),
        spec.slug.clone(),
        "--repo".into(),
        spec.directory.clone(),
    ]
}

/// Map the hub's answer to the read-only readiness check to a refusal.
///
/// `status` is the HTTP status, or `Err` for a transport failure. Only a
/// 2xx answer lets the job spawn.
pub fn preflight(status: Result<u16, ()>) -> Result<(), Refusal> {
    match status {
        Ok(200..=299) => Ok(()),
        Ok(401 | 403) => Err(Refusal::new(
            "tidetable_forbidden",
            "the TideTable token is refused for this slug",
        )),
        Ok(404) => Err(Refusal::new(
            "tidetable_forbidden",
            "the hub does not list this slug for this token",
        )),
        Ok(other) => Err(Refusal::new(
            "tidetable_unreachable",
            format!("the hub answered {other}"),
        )),
        Err(()) => Err(Refusal::new(
            "tidetable_unreachable",
            "the hub is unreachable",
        )),
    }
}

/// Frame: the agent offers v2 with the jobs feature.
pub fn hello_frame(agent: &str) -> Value {
    json!({
        "v": 2,
        "type": "hello",
        "versions": [1, 2],
        "features": [JOBS_TKRUN_FEATURE],
        "agent": agent,
    })
}

/// Frame: the job was refused before anything spawned.
pub fn refused_frame(job_id: &str, refusal: &Refusal) -> Value {
    json!({
        "v": 2,
        "type": "job_refused",
        "jobId": job_id,
        "code": refusal.code,
        "detail": refusal.detail,
    })
}

/// Frame: the child process started.
pub fn accepted_frame(job_id: &str, pid: u32) -> Value {
    json!({"v": 2, "type": "job_accepted", "jobId": job_id, "pid": pid})
}

/// Frame: one output line with its per-job sequence number.
pub fn output_frame(job_id: &str, seq: u64, text: &str) -> Value {
    json!({"v": 2, "type": "job_output", "jobId": job_id, "seq": seq, "text": text})
}

/// Frame: the job ended.
pub fn completed_frame(job_id: &str, status: &str, exit_code: Option<i32>, summary: &str) -> Value {
    json!({
        "v": 2,
        "type": "job_completed",
        "jobId": job_id,
        "status": status,
        "exitCode": exit_code,
        "summary": summary,
    })
}

/// What this host remembers about a job, for replays and snapshots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobState {
    /// Refused before spawning.
    Refused(Refusal),
    /// Running with this pid.
    Running {
        /// Process id.
        pid: u32,
    },
    /// Finished.
    Finished {
        /// Terminal status.
        status: String,
        /// Exit code when the process exited normally.
        exit_code: Option<i32>,
        /// Last lines.
        summary: String,
    },
}

/// The frame that answers a repeated `start_job` for `job_id`.
pub fn replay_frame(job_id: &str, state: &JobState) -> Value {
    match state {
        JobState::Refused(refusal) => refused_frame(job_id, refusal),
        JobState::Running { pid } => accepted_frame(job_id, *pid),
        JobState::Finished {
            status,
            exit_code,
            summary,
        } => completed_frame(job_id, status, *exit_code, summary),
    }
}

/// The `jobs_snapshot` entry for one remembered job.
pub fn snapshot_entry(job_id: &str, state: &JobState) -> Option<Value> {
    match state {
        JobState::Refused(_) => None,
        JobState::Running { pid } => Some(json!({
            "jobId": job_id,
            "state": "running",
            "pid": pid,
            "exitCode": null,
        })),
        JobState::Finished { exit_code, .. } => Some(json!({
            "jobId": job_id,
            "state": "completed",
            "pid": null,
            "exitCode": exit_code,
        })),
    }
}

/// Keep the last [`SUMMARY_LINES`] lines.
pub fn summarize(lines: &[String]) -> String {
    let start = lines.len().saturating_sub(SUMMARY_LINES);
    lines[start..].join("\n")
}

/// Slug rule shared with the pool: lowercase letters, digits, dashes.
pub fn is_slug(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 63
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

/// True for `X.Y.Z` with digits only.
pub fn is_task_id(value: &str) -> bool {
    let parts: Vec<&str> = value.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// https, or http to a loopback host, with no credentials or query.
pub fn is_allowed_server(value: &str) -> bool {
    if value.contains('@') || value.contains('?') || value.contains('#') {
        return false;
    }
    let loopback = ["http://127.0.0.1", "http://localhost", "http://[::1]"]
        .iter()
        .any(|prefix| {
            value.strip_prefix(prefix).is_some_and(|rest| {
                rest.is_empty() || rest.starts_with(':') || rest.starts_with('/')
            })
        });
    let https = value
        .strip_prefix("https://")
        .is_some_and(|rest| !rest.is_empty() && !rest.starts_with('/'));
    https || loopback
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checkouts() -> HashMap<String, String> {
        HashMap::from([("chiton-main".to_string(), "/work/chiton".to_string())])
    }

    fn start(options: Value) -> Value {
        json!({
            "v": 2,
            "type": "start_job",
            "jobId": "job-1",
            "kind": "tkrun",
            "project": {"slug": "chiton", "server": "http://127.0.0.1:8471"},
            "checkout": "chiton-main",
            "options": options,
        })
    }

    #[test]
    fn a_valid_frame_resolves_the_local_directory() {
        let spec = validate_start(
            &start(json!({"hours": 2, "tasks": ["1.3.2"]})),
            &checkouts(),
        )
        .expect("valid");
        assert_eq!(spec.directory, "/work/chiton");
        assert_eq!(spec.options.hours, Some(2));
        assert_eq!(spec.options.tasks, vec!["1.3.2".to_string()]);
    }

    #[test]
    fn a_checkout_off_the_allowlist_is_refused_before_anything_spawns() {
        let mut frame = start(json!({}));
        frame["checkout"] = json!("elsewhere");
        let refusal = validate_start(&frame, &checkouts()).unwrap_err();
        assert_eq!(refusal.code, "checkout_not_allowed");
    }

    #[test]
    fn unknown_kinds_and_option_keys_are_refused() {
        let mut frame = start(json!({}));
        frame["kind"] = json!("shell");
        assert_eq!(
            validate_start(&frame, &checkouts()).unwrap_err().code,
            "invalid_request"
        );

        let extra = start(json!({"command": "rm -rf /"}));
        assert_eq!(
            validate_start(&extra, &checkouts()).unwrap_err().code,
            "invalid_request"
        );
    }

    #[test]
    fn out_of_range_options_are_refused() {
        for options in [
            json!({"hours": 13}),
            json!({"minutes": 60}),
            json!({"jobs": 0}),
            json!({"tasks": ["1.3.2; x"]}),
            json!({"dryRun": "yes"}),
        ] {
            assert_eq!(
                validate_start(&start(options.clone()), &checkouts())
                    .unwrap_err()
                    .code,
                "invalid_request",
                "{options}"
            );
        }
    }

    #[test]
    fn argv_is_fixed_and_carries_no_token() {
        let spec = validate_start(
            &start(json!({"hours": 2, "jobs": 1, "tasks": ["1.3.2"], "dryRun": true})),
            &checkouts(),
        )
        .unwrap();
        let argv = run_argv("tidetable", &spec);
        assert_eq!(
            argv,
            vec![
                "tidetable",
                "--server",
                "http://127.0.0.1:8471",
                "tkrun",
                "run",
                "chiton",
                "--repo",
                "/work/chiton",
                "--hours",
                "2",
                "--jobs",
                "1",
                "--task",
                "1.3.2",
                "--dry-run",
            ]
        );
        assert!(!argv.iter().any(|arg| arg.contains("token")));
    }

    #[test]
    fn stop_argv_names_the_repo_not_a_pid() {
        let spec = validate_start(&start(json!({})), &checkouts()).unwrap();
        assert_eq!(
            stop_argv("tidetable", &spec),
            vec![
                "tidetable",
                "--server",
                "http://127.0.0.1:8471",
                "tkrun",
                "stop",
                "chiton",
                "--repo",
                "/work/chiton",
            ]
        );
    }

    #[test]
    fn preflight_maps_status_codes_to_refusal_codes() {
        assert!(preflight(Ok(200)).is_ok());
        assert_eq!(preflight(Ok(401)).unwrap_err().code, "tidetable_forbidden");
        assert_eq!(preflight(Ok(403)).unwrap_err().code, "tidetable_forbidden");
        assert_eq!(preflight(Ok(404)).unwrap_err().code, "tidetable_forbidden");
        assert_eq!(
            preflight(Ok(503)).unwrap_err().code,
            "tidetable_unreachable"
        );
        assert_eq!(
            preflight(Err(())).unwrap_err().code,
            "tidetable_unreachable"
        );
    }

    #[test]
    fn replays_answer_with_the_remembered_state() {
        let running = replay_frame("job-1", &JobState::Running { pid: 77 });
        assert_eq!(running["type"], "job_accepted");
        assert_eq!(running["pid"], 77);

        let done = replay_frame(
            "job-1",
            &JobState::Finished {
                status: "succeeded".into(),
                exit_code: Some(0),
                summary: "ok".into(),
            },
        );
        assert_eq!(done["type"], "job_completed");
        assert_eq!(done["exitCode"], 0);
    }

    #[test]
    fn summary_keeps_only_the_tail() {
        let lines: Vec<String> = (0..25).map(|i| i.to_string()).collect();
        let summary = summarize(&lines);
        assert!(summary.starts_with("5\n"));
        assert!(summary.ends_with("24"));
    }

    #[test]
    fn server_rules_refuse_plain_remote_http_and_credentials() {
        assert!(is_allowed_server("https://tk.example"));
        assert!(is_allowed_server("http://127.0.0.1:8471"));
        assert!(!is_allowed_server("http://tk.example"));
        assert!(!is_allowed_server("https://user:pw@tk.example"));
        assert!(!is_allowed_server("https://tk.example?x=1"));
        assert!(!is_allowed_server("file:///etc/passwd"));
    }
}
