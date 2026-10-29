//! The console's session: the CLI's credential store plus the pool's roles.
//!
//! The console owns no credential format. It loads and saves the very file
//! [`tidepool_cli::credentials`] writes (`$TIDEPOOL_HOME/credentials.json`,
//! otherwise `$XDG_CONFIG_HOME/tidepool/credentials.json`, otherwise
//! `$HOME/.config/tidepool/credentials.json`), so one `tidepool login` serves
//! both skins and a sign-in made from `tidepool-console --headless login` is
//! visible to `tidepool hosts list` without a second store.
//!
//! # Roles mirror the pool, not a second opinion
//!
//! The pool's own rule is small (`src/middleware/auth.rs`): every mutating
//! handler calls `require_mutating`, which refuses a `viewer`, and the
//! organization-level handlers call `require_owner`. This module states that
//! same line and nothing more, so a console that hides an action and a pool that
//! refuses it can never disagree. There is no console-only permission, no
//! cached role and no local policy: [`Session::role`] asks `GET /v1/pool/me`
//! each time it matters — headless mode before every mutating command, the
//! terminal UI at each connection probe and on reconnect — which is why a role
//! change is honoured the moment the console next asks the pool.
//!
//! An unknown role is read as [`Role::Viewer`] rather than as a default member.
//! The contract tolerates unknown values for forward compatibility; treating one
//! as least-privilege means a pool that grows a role can never cause an older
//! console to offer a mutation it does not understand.

use std::path::PathBuf;

use tidepool_cli::credentials::{self, CredentialFile, CredentialStore};
use tidepool_client::blocking::Client;
use tidepool_client::endpoints::GetV1PoolMe;

/// An operator's role inside a pool, as `GET /v1/pool/me` reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    /// Owns the organization; manages members and API keys.
    Owner,
    /// May change configuration but not membership.
    Admin,
    /// May operate the pool but not change organization-level settings.
    Member,
    /// Read-only access; every mutating request is refused.
    Viewer,
}

impl Role {
    /// The wire spelling (`owner`, `admin`, `member`, `viewer`).
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Owner => "owner",
            Role::Admin => "admin",
            Role::Member => "member",
            Role::Viewer => "viewer",
        }
    }

    /// Parse a wire value, or `None` when the contract added a role this build
    /// does not know.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "owner" => Some(Role::Owner),
            "admin" => Some(Role::Admin),
            "member" => Some(Role::Member),
            "viewer" => Some(Role::Viewer),
            _ => None,
        }
    }

    /// Parse a wire value, reading an unknown one as the least privilege.
    pub fn from_wire(value: &str) -> Self {
        Role::parse(value).unwrap_or(Role::Viewer)
    }

    /// Whether this role may perform `capability`.
    ///
    /// This is the pool's `require_mutating` line: a viewer may read, everyone
    /// else may mutate. Owner-only actions are outside this console's surface
    /// (it has no member or API-key screen), so they are not modelled here.
    pub fn allows(self, capability: Capability) -> bool {
        match capability {
            Capability::Read => true,
            Capability::Mutate => self != Role::Viewer,
        }
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// The kinds of work the console and the CLI ask the pool to do.
///
/// The split is deliberate: it is the pool's two lines, not a console
/// taxonomy. `Read` covers everything that needs no mutation, `Mutate` covers
/// every command whose handler calls `require_mutating`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    /// Read pool state; every role may do this.
    Read,
    /// Change pool state; a viewer may not.
    Mutate,
}

impl Capability {
    /// A short phrase for an explanation or an error.
    pub fn describe(self) -> &'static str {
        match self {
            Capability::Read => "read the pool",
            Capability::Mutate => "change pool state",
        }
    }
    /// Why `role` cannot do this, or `None` when it can.
    ///
    /// The sentence names the role, the line the pool draws and the one fix a
    /// person has, so a refusal is never a bare "forbidden".
    pub fn denial(self, role: Role) -> Option<String> {
        if role.allows(self) {
            return None;
        }
        Some(format!(
            "the `{role}` role cannot {}: the pool refuses every mutating request \
             from a viewer, so the console does not attempt it. Ask an owner to \
             change your role if you need to {}.",
            self.describe(),
            self.describe()
        ))
    }
}

impl std::fmt::Display for Capability {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.describe())
    }
}

/// Everything session handling can fail with.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// The CLI's credential store could not be read or written.
    #[error("credential store: {0}")]
    Store(#[from] tidepool_cli::CliError),
    /// The SDK could not build or perform the role query.
    #[error("pool identity: {0}")]
    Pool(#[from] tidepool_client::Error),
    /// No pool was given and the store remembers none.
    #[error(
        "no pool configured: pass --pool <url> or run `tidepool login --pool <url> --email <you> --password-stdin`"
    )]
    NoPool,
}

/// The result alias for session handling.
pub type Result<T> = std::result::Result<T, AuthError>;

/// A credential store shared with the CLI, loaded once per run.
///
/// The type is a thin view over [`CredentialStore`]: it adds the pool-role
/// lookup the console needs and nothing else, so the file format and its
/// redaction rules have exactly one implementation.
pub struct Session {
    store: CredentialStore,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Session")
            .field("store", &self.store)
            .finish()
    }
}

impl Session {
    /// Load the store at `path`, treating a missing file as signed out.
    pub fn load(path: PathBuf) -> Result<Self> {
        Ok(Self {
            store: CredentialStore::load(path)?,
        })
    }

    /// Load the store from the CLI's default location.
    pub fn from_environment() -> Result<Self> {
        Self::load(credentials::default_path(|name| std::env::var(name).ok())?)
    }

    /// The file the CLI and the console share.
    pub fn file(&self) -> &CredentialFile {
        self.store.file()
    }

    /// The remembered pool, if any.
    pub fn pool(&self) -> Option<&str> {
        self.store
            .file()
            .pool
            .as_deref()
            .filter(|pool| !pool.trim().is_empty())
    }

    /// The remembered operator token, if any.
    pub fn token(&self) -> Option<&str> {
        self.store
            .file()
            .token
            .as_deref()
            .filter(|token| !token.is_empty())
    }

    /// Whether both a pool and a token are known.
    pub fn signed_in(&self) -> bool {
        self.pool().is_some() && self.token().is_some()
    }

    /// The pool a command should talk to: `override_pool` first, then the store.
    pub fn base_url(&self, override_pool: Option<&str>) -> Result<String> {
        override_pool
            .map(str::to_owned)
            .or_else(|| self.pool().map(str::to_owned))
            .ok_or(AuthError::NoPool)
    }

    /// The pool's current role for this member.
    ///
    /// The pool reads the role from the member row on every request, so asking
    /// it is the only way a console and the pool cannot disagree; `None` when
    /// the store is not signed in.
    pub fn role(&self, override_pool: Option<&str>) -> Result<Option<Role>> {
        if !self.signed_in() {
            return Ok(None);
        }
        let Some(token) = self.token() else {
            return Ok(None);
        };
        let client = Client::from_base_url(self.base_url(override_pool)?)?.with_token(token);
        let identity = client.call(&GetV1PoolMe::new())?;
        Ok(Some(Role::from_wire(&identity.user.role)))
    }
}

#[cfg(test)]
pub(crate) mod testing {
    //! Shared hermetic fixtures for this crate's session and headless tests.

    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::PathBuf;
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};

    /// One request the scripted pool received.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) struct Recorded {
        /// Uppercase method.
        pub method: String,
        /// Path including the query string.
        pub path: String,
        /// Raw request body.
        pub body: Vec<u8>,
    }

    /// One scripted response.
    #[derive(Debug, Clone)]
    pub(crate) struct Reply {
        /// HTTP status.
        pub status: u16,
        /// Response body.
        pub body: String,
    }

    impl Reply {
        /// A JSON reply.
        pub(crate) fn json(status: u16, body: &str) -> Self {
            Self {
                status,
                body: body.to_owned(),
            }
        }
    }

    /// A `GET /v1/pool/me` body naming a viewer.
    pub(crate) const POOL_ME_VIEWER: &str = r#"{
        "organization": {"id": "org_1", "name": "Org", "created_at": "2026-10-29T00:00:00Z"},
        "user": {
            "id": "user_1",
            "email": "ops@example.com",
            "display_name": "Ops",
            "role": "viewer",
            "created_at": "2026-10-29T00:00:00Z",
            "last_login_at": null,
            "password_change_required": false
        }
    }"#;

    /// A `GET /v1/pool/me` body naming a member.
    pub(crate) const POOL_ME_MEMBER: &str = r#"{
        "organization": {"id": "org_1", "name": "Org", "created_at": "2026-10-29T00:00:00Z"},
        "user": {
            "id": "user_1",
            "email": "ops@example.com",
            "display_name": "Ops",
            "role": "member",
            "created_at": "2026-10-29T00:00:00Z",
            "last_login_at": null,
            "password_change_required": false
        }
    }"#;

    /// `POST /v1/pool/login`, with a token long enough to survive redaction.
    pub(crate) const LOGIN: &str = r#"{
        "token": "abcd1234-the-whole-operator-token",
        "expires_in": 3600,
        "must_change_password": false,
        "user": {
            "id": "user_1",
            "email": "ops@example.com",
            "display_name": "Ops",
            "role": "owner",
            "created_at": "2026-10-29T00:00:00Z",
            "last_login_at": null,
            "password_change_required": false
        }
    }"#;

    /// `GET /v1/setup/status`.
    pub(crate) const SETUP_STATUS: &str = r#"{"needs_setup": false, "bootstrap_configured": true}"#;

    /// `GET /v1/hosts`, with one connected host.
    pub(crate) const HOSTS: &str = r#"{"hosts": [
        {"id": "host_1", "slug": "build-box", "display_name": "Build box",
         "hostname": "build-box.local", "status": "connected",
         "execution_enabled": true, "created_at": "2026-10-29T00:00:00Z", "revoked_at": null}
    ]}"#;

    /// `POST /v1/runs`, one fresh manual run.
    pub(crate) const DISPATCHED: &str = r#"{
        "deduplicated": false,
        "run": {
            "id": "run_1", "project": "proj_1", "trigger": "trigger_1", "triggerName": "on-push",
            "workflow": "build", "status": "pending", "origin": "manual",
            "actor": "ops@example.com", "deliveryKey": "key-1", "inputs": {},
            "requestText": "", "createdAt": "2026-10-29T00:00:00Z",
            "startedAt": null, "completedAt": null, "deadlineAt": null, "steps": []
        }
    }"#;

    /// A pool that answers a fixed script and records every request.
    pub(crate) struct ScriptedPool {
        url: String,
        requests: Arc<Mutex<Vec<Recorded>>>,
        finished: Receiver<()>,
        handle: Option<thread::JoinHandle<()>>,
    }

    impl ScriptedPool {
        /// Bind an ephemeral port and answer `replies` in order.
        pub(crate) fn start(replies: Vec<Reply>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind scripted pool");
            listener
                .set_nonblocking(true)
                .expect("non-blocking listener");
            let address = listener.local_addr().expect("address");
            let requests = Arc::new(Mutex::new(Vec::new()));
            let recorded = Arc::clone(&requests);
            let (sender, finished): (Sender<()>, Receiver<()>) = mpsc::channel();
            let handle = thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(10);
                for reply in replies {
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
                    recorded.lock().expect("requests").push(request);
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
        pub(crate) fn url(&self) -> &str {
            &self.url
        }

        /// Wait for the script to finish and return what was recorded.
        pub(crate) fn join(mut self) -> Vec<Recorded> {
            let _ = self.finished.recv_timeout(Duration::from_secs(10));
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
            self.requests.lock().expect("requests").clone()
        }
    }

    impl Drop for ScriptedPool {
        fn drop(&mut self) {
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    /// A temporary directory removed on drop.
    pub(crate) struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        /// Create a uniquely named directory under the platform temp dir.
        pub(crate) fn new(label: &str) -> Self {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or_default();
            let path = std::env::temp_dir().join(format!(
                "tidepool-console-{label}-{}-{unique}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("temp dir");
            Self { path }
        }

        /// The credential store inside the directory.
        pub(crate) fn store(&self) -> PathBuf {
            self.path.join("credentials.json")
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
                Ok((stream, _)) => {
                    if stream.set_nonblocking(false).is_err() {
                        continue;
                    }
                    return Some(stream);
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(_) => return None,
            }
        }
        None
    }

    fn read_request(stream: &mut TcpStream) -> std::io::Result<Recorded> {
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        let mut buffer = Vec::new();
        let mut chunk = [0_u8; 4096];
        let head_end = loop {
            if let Some(position) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                break position;
            }
            let read = stream.read(&mut chunk)?;
            if read == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "closed before headers",
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
        let mut content_length = 0;
        for line in lines {
            if let Some((name, value)) = line.split_once(':')
                && name.trim().eq_ignore_ascii_case("content-length")
            {
                content_length = value.trim().parse::<usize>().unwrap_or_default();
            }
        }
        let mut body = buffer[(head_end + 4).min(buffer.len())..].to_vec();
        while body.len() < content_length {
            let read = stream.read(&mut chunk)?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..read]);
        }
        Ok(Recorded { method, path, body })
    }

    fn write_reply(stream: &mut TcpStream, reply: &Reply) -> std::io::Result<()> {
        let reason = match reply.status {
            200 => "OK",
            201 => "Created",
            204 => "No Content",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            422 => "Unprocessable Entity",
            _ => "Status",
        };
        let head = format!(
            "HTTP/1.1 {} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            reply.status,
            reply.body.len()
        );
        stream.write_all(head.as_bytes())?;
        stream.write_all(reply.body.as_bytes())?;
        stream.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{POOL_ME_VIEWER, Reply, ScriptedPool, TempDir};
    use super::*;
    use tidepool_cli::credentials::CredentialStore;

    const ROLES: [Role; 4] = [Role::Owner, Role::Admin, Role::Member, Role::Viewer];

    fn store_with(pool: &str, token: &str) -> (TempDir, PathBuf) {
        let directory = TempDir::new("auth-store");
        let path = directory.store();
        let mut store = CredentialStore::load(path.clone()).expect("store");
        store.file_mut().pool = Some(pool.to_owned());
        store.file_mut().email = Some("ops@example.com".to_owned());
        store.file_mut().token = Some(token.to_owned());
        store.save().expect("save");
        (directory, path)
    }

    #[test]
    fn every_role_parses_and_round_trips() {
        for role in ROLES {
            assert_eq!(Role::parse(role.as_str()), Some(role));
        }
        assert_eq!(Role::parse(" OWNER "), Some(Role::Owner));
        assert_eq!(Role::parse("moderator"), None);
    }

    #[test]
    fn a_viewer_is_read_only_and_the_rest_may_mutate() {
        assert!(Role::Viewer.allows(Capability::Read));
        assert!(!Role::Viewer.allows(Capability::Mutate));
        for role in [Role::Owner, Role::Admin, Role::Member] {
            assert!(role.allows(Capability::Read), "{role}");
            assert!(role.allows(Capability::Mutate), "{role}");
        }
    }

    #[test]
    fn the_denial_names_the_role_the_line_and_the_fix() {
        let denial = Capability::Mutate
            .denial(Role::Viewer)
            .expect("a viewer is refused");
        assert!(denial.contains("`viewer`"), "{denial}");
        assert!(denial.contains("mutating"), "{denial}");
        assert!(denial.contains("Ask an owner"), "{denial}");
        assert_eq!(Capability::Mutate.denial(Role::Member), None);
        assert_eq!(Capability::Read.denial(Role::Viewer), None);
    }

    #[test]
    fn an_unknown_wire_role_is_read_as_a_viewer() {
        assert_eq!(Role::from_wire("owner"), Role::Owner);
        assert_eq!(Role::from_wire("future-role"), Role::Viewer);
    }

    #[test]
    fn the_store_is_the_clis_file_in_both_directions() {
        let (directory, path) = store_with("https://pool.test", "token-from-the-cli");
        let session = Session::load(path.clone()).expect("session");
        assert!(session.signed_in());
        assert_eq!(session.pool(), Some("https://pool.test"));
        assert_eq!(session.token(), Some("token-from-the-cli"));

        // The CLI reading the same file sees exactly what the console read.
        let cli = CredentialStore::load(path).expect("cli store");
        assert_eq!(cli.file().pool.as_deref(), session.pool());
        assert_eq!(cli.file().token.as_deref(), session.token());

        // A store written by the console-visible path is a CLI store.
        let other = directory.store().with_file_name("other-credentials.json");
        let mut written = CredentialStore::load(other.clone()).expect("store");
        written.file_mut().pool = Some("https://other.test".to_owned());
        written.file_mut().token = Some("console-token".to_owned());
        let session = Session::load(other.clone()).expect("session");
        written.save().expect("save");
        let reloaded = Session::load(other).expect("reload");
        assert_eq!(reloaded.pool(), Some("https://other.test"));
        assert_eq!(reloaded.token(), Some("console-token"));
        // The in-memory session before the save was signed out, which is the
        // mechanical proof the file is the only carrier between the two skins.
        assert!(!session.signed_in());
    }

    #[test]
    fn a_missing_store_is_signed_out_not_an_error() {
        let directory = TempDir::new("missing");
        let session = Session::load(directory.store()).expect("session");
        assert!(!session.signed_in());
        assert_eq!(session.pool(), None);
        assert_eq!(session.token(), None);
    }

    #[test]
    fn the_role_is_asked_of_the_pool() {
        let pool = ScriptedPool::start(vec![Reply::json(200, POOL_ME_VIEWER)]);
        let (_directory, path) = store_with(pool.url(), "token");
        let session = Session::load(path).expect("session");
        let role = session.role(None).expect("role").expect("a role");
        assert_eq!(role, Role::Viewer);
        let requests = pool.join();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].path, "/v1/pool/me");
    }

    #[test]
    fn a_signed_out_session_asks_nothing() {
        let directory = TempDir::new("signed-out");
        let session = Session::load(directory.store()).expect("session");
        assert_eq!(session.role(None).expect("role"), None);
    }
}
