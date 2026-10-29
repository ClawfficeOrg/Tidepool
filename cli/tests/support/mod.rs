//! A scripted HTTP server and a temporary directory for hermetic CLI tests.
//!
//! This is the SDK's own recorded-server fixture, copied here because the CLI
//! is a separate crate: no live pool, database or Redis is involved. The server
//! answers a fixed list of responses in order and records every request.

#![allow(dead_code)]

pub mod fixtures;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// One request the client made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedRequest {
    /// Uppercase method.
    pub method: String,
    /// Path including the query string.
    pub path: String,
    /// Headers keyed by lowercase name.
    pub headers: HashMap<String, String>,
    /// Raw request body.
    pub body: Vec<u8>,
}

impl RecordedRequest {
    /// A header, case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }

    /// The body decoded as JSON.
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).expect("request body was JSON")
    }
}

/// One scripted response.
#[derive(Debug, Clone)]
pub struct Reply {
    /// HTTP status.
    pub status: u16,
    /// Extra headers.
    pub headers: Vec<(String, String)>,
    /// Response body.
    pub body: Vec<u8>,
}

impl Reply {
    /// A JSON reply.
    pub fn json(status: u16, body: &str) -> Self {
        Self {
            status,
            headers: vec![("content-type".to_owned(), "application/json".to_owned())],
            body: body.as_bytes().to_vec(),
        }
    }

    /// An `application/problem+json` reply.
    pub fn problem(status: u16, body: &str) -> Self {
        Self {
            status,
            headers: vec![(
                "content-type".to_owned(),
                "application/problem+json".to_owned(),
            )],
            body: body.as_bytes().to_vec(),
        }
    }

    /// An empty reply, as a `204` route sends.
    pub fn empty(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    /// Add one response header.
    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }
}

/// A server that answers a fixed script and records every request.
pub struct ScriptedServer {
    url: String,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    finished: Receiver<()>,
    handle: Option<thread::JoinHandle<()>>,
}

impl ScriptedServer {
    /// Bind to an ephemeral port and answer `replies` in order.
    pub fn start(replies: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        listener
            .set_nonblocking(true)
            .expect("nonblocking test listener");
        let address = listener.local_addr().expect("test server address");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&requests);
        let (sender, finished): (Sender<()>, Receiver<()>) = mpsc::channel();
        let handle = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            for reply in replies {
                // A client may probe or abandon a connection between attempts;
                // skip such a connection rather than failing the test.
                let mut accepted = None;
                while let Some(mut stream) = accept(&listener, deadline) {
                    if let Ok(request) = read_request(&mut stream) {
                        accepted = Some((request, stream));
                        break;
                    }
                }
                let Some((request, mut stream)) = accepted else {
                    break;
                };
                recorded.lock().expect("requests lock").push(request);
                if write_reply(&mut stream, &reply).is_err() {
                    break;
                }
            }
            let _ = sender.send(());
        });
        Self {
            url: format!("http://{address}"),
            requests,
            finished,
            handle: Some(handle),
        }
    }

    /// The base URL to point a client at.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Every request recorded so far.
    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().expect("requests lock").clone()
    }

    /// Wait for the scripted exchange to finish and return what was recorded.
    pub fn join(mut self) -> Vec<RecordedRequest> {
        let _ = self.finished.recv_timeout(Duration::from_secs(10));
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        self.requests()
    }
}

impl Drop for ScriptedServer {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// A directory under the platform temporary directory, removed on drop.
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Create a uniquely named temporary directory.
    pub fn new(label: &str) -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default();
        let path = std::env::temp_dir().join(format!(
            "tidepool-cli-{label}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("temp dir");
        Self { path }
    }

    /// The directory's path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn accept(listener: &TcpListener, deadline: Instant) -> Option<TcpStream> {
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((stream, _)) => return Some(stream),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(5));
            }
            Err(_) => return None,
        }
    }
    None
}

fn read_request(stream: &mut TcpStream) -> std::io::Result<RecordedRequest> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(position) = find_head_end(&buffer) {
            break position;
        }
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "connection closed before request headers",
            ));
        }
        buffer.extend_from_slice(&chunk[..read]);
    };
    let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let path = parts.next().unwrap_or_default().to_owned();
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }
    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = buffer[(head_end + 4).min(buffer.len())..].to_vec();
    while body.len() < content_length {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
    }
    Ok(RecordedRequest {
        method,
        path,
        headers,
        body,
    })
}

fn find_head_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn write_reply(stream: &mut TcpStream, reply: &Reply) -> std::io::Result<()> {
    let reason = match reply.status {
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Status",
    };
    let mut head = format!(
        "HTTP/1.1 {} {reason}\r\ncontent-length: {}\r\nconnection: close\r\n",
        reply.status,
        reply.body.len()
    );
    for (name, value) in &reply.headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    stream.write_all(&reply.body)?;
    stream.flush()
}
