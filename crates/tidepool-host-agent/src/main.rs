//! Minimal reference host agent (task 1.2.5; typed jobs, task 1.7.4).
//!
//! Environment:
//! - `POOL_HTTP_URL` (default `http://127.0.0.1:8080`)
//! - `POOL_WS_URL` (default `ws://127.0.0.1:8080/ws/host`)
//! - `HOST_ID` and `CREDENTIAL`, or `ENROLLMENT_TOKEN` to enroll once
//! - `CAPABILITIES` optional capability JSON array
//! - `EXIT_AFTER_EXECUTION=1` exits after the first completion
//! - `CHECKOUTS` JSON object: checkout name -> local directory (the allowlist)
//! - `TIDETABLE_TOKEN_FILE` or `TIDETABLE_TOKEN`: this host's TideTable token.
//!   It goes to the TKRun child and to the readiness check, never to the pool.
//! - `TIDETABLE_BIN` (default `tidetable`)
//!
//! The agent reports capabilities, connects with the relationship credential,
//! heartbeats, answers create-agent intents, and runs typed `tkrun` jobs. Jobs
//! survive a dropped socket: the child keeps running and the agent re-adopts it
//! on reconnect with a `jobs_snapshot`.

use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, ErrorKind, Read};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tidepool_host_agent::AgentState;
use tidepool_host_agent::jobs::{
    JOBS_TKRUN_FEATURE, JobState, Refusal, StartSpec, accepted_frame, completed_frame, hello_frame,
    output_frame, preflight, refused_frame, replay_frame, run_argv, snapshot_entry, stop_argv,
    summarize, validate_start,
};
use tungstenite::client::IntoClientRequest;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

const AGENT: &str = concat!("tidepool-host-agent/", env!("CARGO_PKG_VERSION"));

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

    let mut runtime = Runtime::from_env()?;
    let mut state = AgentState::default();
    loop {
        match run_socket(
            &ws_url,
            &host_id,
            &credential,
            &mut state,
            &mut runtime,
            exit_after,
        ) {
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

/// A running `tkrun` child and the state needed to report it.
struct Live {
    child: Child,
    lines: Receiver<String>,
    readers: Vec<JoinHandle<()>>,
    directory: String,
    slug: String,
    server: String,
    seq: u64,
    tail: Vec<String>,
    stop_mode: Option<&'static str>,
}

/// Job state that outlives a socket: the ledger answers replays, `live` holds
/// the processes.
struct Runtime {
    token: Option<String>,
    checkouts: HashMap<String, String>,
    bin: String,
    ledger: HashMap<String, JobState>,
    live: HashMap<String, Live>,
}

impl Runtime {
    fn from_env() -> Result<Self> {
        let checkouts = match std::env::var("CHECKOUTS") {
            Ok(raw) if !raw.trim().is_empty() => {
                serde_json::from_str(&raw).context("CHECKOUTS must be a JSON object")?
            }
            _ => HashMap::new(),
        };
        let token = match std::env::var("TIDETABLE_TOKEN_FILE") {
            Ok(path) => Some(
                std::fs::read_to_string(&path)
                    .with_context(|| format!("reading TIDETABLE_TOKEN_FILE {path}"))?
                    .trim()
                    .to_string(),
            ),
            Err(_) => std::env::var("TIDETABLE_TOKEN").ok(),
        }
        .filter(|token| !token.is_empty());
        Ok(Self {
            token,
            checkouts,
            bin: std::env::var("TIDETABLE_BIN").unwrap_or_else(|_| "tidetable".into()),
            ledger: HashMap::new(),
            live: HashMap::new(),
        })
    }

    /// Answer a `start_job`: a replay, a refusal, or a spawned child.
    fn start(&mut self, frame: &Value, out: &mut Vec<Value>) {
        let job_id = frame["jobId"].as_str().unwrap_or_default().to_string();
        if let Some(state) = self.ledger.get(&job_id) {
            out.push(replay_frame(&job_id, state));
            return;
        }

        let spec = match validate_start(frame, &self.checkouts) {
            Ok(spec) => spec,
            Err(refusal) => return self.refuse(&job_id, refusal, out),
        };
        let Some(token) = self.token.clone() else {
            let refusal = Refusal {
                code: "tidetable_forbidden",
                detail: "this host has no TideTable token installed".into(),
            };
            return self.refuse(&job_id, refusal, out);
        };
        if let Err(refusal) = preflight(check_ready(&spec, &token)) {
            return self.refuse(&job_id, refusal, out);
        }

        let argv = run_argv(&self.bin, &spec);
        match spawn(&argv, &token) {
            Ok(mut child) => {
                let (lines, readers) = pump_output(&mut child);
                let pid = child.id();
                out.push(accepted_frame(&job_id, pid));
                self.ledger
                    .insert(job_id.clone(), JobState::Running { pid });
                self.live.insert(
                    job_id,
                    Live {
                        child,
                        lines,
                        readers,
                        directory: spec.directory.clone(),
                        slug: spec.slug.clone(),
                        server: spec.server.clone(),
                        seq: 0,
                        tail: Vec::new(),
                        stop_mode: None,
                    },
                );
            }
            Err(error) => {
                let refusal = Refusal {
                    code: "tkrun_missing",
                    detail: format!("could not start tkrun: {}", error.kind()),
                };
                self.refuse(&job_id, refusal, out);
            }
        }
    }

    fn refuse(&mut self, job_id: &str, refusal: Refusal, out: &mut Vec<Value>) {
        if !job_id.is_empty() {
            self.ledger
                .insert(job_id.to_string(), JobState::Refused(refusal.clone()));
        }
        out.push(refused_frame(job_id, &refusal));
    }

    /// Answer a `stop_job`. `stop` asks TKRun to finish its step; `kill`
    /// terminates the child process.
    fn stop(&mut self, frame: &Value, out: &mut Vec<Value>) {
        let job_id = frame["jobId"].as_str().unwrap_or_default();
        let mode = frame["mode"].as_str().unwrap_or_default();
        let Some(live) = self.live.get_mut(job_id) else {
            out.push(json!({
                "v": 2, "type": "error", "code": "invalid_request",
                "detail": "job is not running on this host"
            }));
            return;
        };
        match mode {
            "kill" => {
                let _ = live.child.kill();
                live.stop_mode = Some("killed");
            }
            "stop" => {
                let token = self.token.clone().unwrap_or_default();
                let spec = StartSpec {
                    job_id: job_id.to_string(),
                    slug: live.slug.clone(),
                    server: live.server.clone(),
                    checkout: String::new(),
                    directory: live.directory.clone(),
                    options: Default::default(),
                };
                let argv = stop_argv(&self.bin, &spec);
                let _ = Command::new(&argv[0])
                    .args(&argv[1..])
                    .env("TIDETABLE_TOKEN", token)
                    .status();
                live.stop_mode = Some("stopped");
            }
            _ => out.push(json!({
                "v": 2, "type": "error", "code": "invalid_request",
                "detail": "mode must be stop or kill"
            })),
        }
    }

    /// Frames for output that arrived since the last call, and for children that
    /// exited. A child's remaining lines are flushed before its completion.
    fn pump(&mut self, out: &mut Vec<Value>) {
        for (job_id, live) in self.live.iter_mut() {
            while let Ok(line) = live.lines.try_recv() {
                live.seq += 1;
                live.tail.push(line.clone());
                out.push(output_frame(job_id, live.seq, &line));
            }
        }

        let exited: Vec<(String, ExitStatus)> = self
            .live
            .iter_mut()
            .filter_map(|(job_id, live)| match live.child.try_wait() {
                Ok(Some(status)) => Some((job_id.clone(), status)),
                _ => None,
            })
            .collect();

        for (job_id, status) in exited {
            let Some(mut live) = self.live.remove(&job_id) else {
                continue;
            };
            for reader in live.readers.drain(..) {
                let _ = reader.join();
            }
            while let Ok(line) = live.lines.try_recv() {
                live.seq += 1;
                live.tail.push(line.clone());
                out.push(output_frame(&job_id, live.seq, &line));
            }
            let exit_code = status.code();
            let state = match live.stop_mode {
                Some(mode) => mode,
                None if status.success() => "succeeded",
                None => "failed",
            };
            let summary = summarize(&live.tail);
            out.push(completed_frame(&job_id, state, exit_code, &summary));
            self.ledger.insert(
                job_id,
                JobState::Finished {
                    status: state.to_string(),
                    exit_code,
                    summary,
                },
            );
        }
    }

    /// The `jobs_snapshot` frame sent right after `hello.ack`.
    fn snapshot(&self) -> Value {
        let jobs: Vec<Value> = self
            .ledger
            .iter()
            .filter_map(|(job_id, state)| snapshot_entry(job_id, state))
            .collect();
        json!({"v": 2, "type": "jobs_snapshot", "jobs": jobs})
    }
}

/// Ask the hub's readiness route with this host's token. Blocking, bounded.
fn check_ready(spec: &StartSpec, token: &str) -> Result<u16, ()> {
    let url = format!("{}/api/{}/ready", spec.server, spec.slug);
    match ureq::get(&url)
        .set("Authorization", &format!("Bearer {token}"))
        .timeout(Duration::from_secs(10))
        .call()
    {
        Ok(response) => Ok(response.status()),
        Err(ureq::Error::Status(code, _)) => Ok(code),
        Err(_) => Err(()),
    }
}

/// Spawn the TKRun child with the token in its environment only. The host's own
/// credentials are removed so the child cannot read them.
fn spawn(argv: &[String], token: &str) -> std::io::Result<Child> {
    Command::new(&argv[0])
        .args(&argv[1..])
        .env("TIDETABLE_TOKEN", token)
        .env_remove("CREDENTIAL")
        .env_remove("ENROLLMENT_TOKEN")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
}

/// Read stdout and stderr on threads into one channel of lines.
fn pump_output(child: &mut Child) -> (Receiver<String>, Vec<JoinHandle<()>>) {
    let (tx, rx) = mpsc::channel();
    let mut readers = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        readers.push(read_lines(stdout, tx.clone()));
    }
    if let Some(stderr) = child.stderr.take() {
        readers.push(read_lines(stderr, tx.clone()));
    }
    (rx, readers)
}

fn read_lines<R: Read + Send + 'static>(stream: R, tx: mpsc::Sender<String>) -> JoinHandle<()> {
    thread::spawn(move || {
        for line in BufReader::new(stream).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    })
}

fn run_socket(
    ws_url: &str,
    host_id: &str,
    credential: &str,
    state: &mut AgentState,
    runtime: &mut Runtime,
    exit_after: bool,
) -> Result<()> {
    let mut request = ws_url.into_client_request()?;
    request.headers_mut().insert("X-Host-Id", host_id.parse()?);
    request
        .headers_mut()
        .insert("X-Host-Credential", credential.parse()?);
    let (mut socket, _) = tungstenite::connect(request)?;
    set_read_timeout(&mut socket, Some(Duration::from_millis(100)));

    send(&mut socket, &hello_frame(AGENT))?;

    let mut last_heartbeat = Instant::now();
    loop {
        if last_heartbeat.elapsed() >= Duration::from_secs(30) {
            send(&mut socket, &json!({"v": 1, "type": "heartbeat"}))?;
            last_heartbeat = Instant::now();
        }

        let mut out = Vec::new();
        runtime.pump(&mut out);
        for frame in out {
            send(&mut socket, &frame)?;
        }

        match socket.read() {
            Ok(Message::Text(text)) => {
                let frame: Value = serde_json::from_str(&text).unwrap_or_default();
                let mut out = Vec::new();
                match frame["type"].as_str().unwrap_or_default() {
                    "heartbeat.ack" => {}
                    "hello.ack" => {
                        let agreed = frame["features"]
                            .as_array()
                            .is_some_and(|f| f.iter().any(|v| v == JOBS_TKRUN_FEATURE));
                        if agreed {
                            out.push(runtime.snapshot());
                        }
                    }
                    "start_job" => runtime.start(&frame, &mut out),
                    "stop_job" => runtime.stop(&frame, &mut out),
                    "create_agent" => {
                        let execution_id = frame["executionId"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string();
                        let prompt = frame["prompt"].as_str().unwrap_or_default().to_string();
                        out.extend(state.handle_create(&execution_id, &prompt));
                        if exit_after && state.is_complete(&execution_id) {
                            for frame in out {
                                send(&mut socket, &frame)?;
                            }
                            return Ok(());
                        }
                    }
                    "interrupt_execution" => {
                        let execution_id = frame["executionId"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string();
                        out.extend(state.handle_interrupt(&execution_id));
                    }
                    "error" => {
                        eprintln!("agent: pool error: {frame}");
                    }
                    _ => {}
                }
                for frame in out {
                    send(&mut socket, &frame)?;
                }
            }
            Ok(Message::Ping(bytes)) => socket.send(Message::Pong(bytes))?,
            Ok(Message::Close(_)) => return Ok(()),
            Ok(_) => {}
            Err(tungstenite::Error::Io(error))
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
            {
                // Read timeout: loop to pump output and send the next heartbeat.
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn send(socket: &mut WebSocket<MaybeTlsStream<std::net::TcpStream>>, frame: &Value) -> Result<()> {
    socket.send(Message::Text(frame.to_string()))?;
    Ok(())
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
