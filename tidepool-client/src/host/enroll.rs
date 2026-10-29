//! This machine's relationship with a pool.
//!
//! The client signs in as an operator, registers this machine as a host with
//! the same two-step exchange the CLI performs — the pool mints a single-use
//! enrollment token, then this machine generates its own relationship
//! credential and exchanges the token for it — and then reads back the machine's
//! presence and execution state.
//!
//! # The grant is never assumed
//!
//! Enrollment and connection do not permit execution. `host.execute` is a
//! separate grant, and [`ExecutionPermission`] has no `Default`: until the pool
//! has said `execution_enabled: true`, the answer is [`ExecutionPermission::Unknown`]
//! or [`ExecutionPermission::NotGranted`], and [`Machine::may_run_agents`]
//! answers `false` for both. A pool that cannot be reached downgrades a known
//! grant to [`ExecutionPermission::Unknown`] rather than leaving a stale `true`
//! in place.
//!
//! # Secrets
//!
//! The host credential is generated here, handed to [`SettingsStore::save`],
//! and never placed anywhere else: no `Debug` output, no notice, no status line
//! and no error carries it. The operator's JWT does the same. That is what lets
//! the pool URL and the credentials live in the client's settings store and
//! never in a repository, a log or a layout file.

use thiserror::Error;

#[cfg(test)]
use std::collections::VecDeque;

use crate::host::client::{HostStatus, HostView, PoolApi, PoolError, Session};
use crate::settings::{ClientSettings, SettingsError, SettingsStore, StoredHost};

/// What granting execution means, in the words the client shows next to it.
pub const GRANT_CONSEQUENCE: &str = "Granting lets the pool dispatch agent work to this machine; those agents run here with this machine's permissions.";
/// What revoking execution means.
pub const REVOKE_CONSEQUENCE: &str = "Revoking stops pooled agents running here; this machine stays connected but performs no agent work.";

/// Whether this machine may run agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionPermission {
    /// The pool reports `host.execute` as granted.
    Granted,
    /// The pool reports the host without the grant.
    NotGranted,
    /// The pool has not answered, so nothing is claimed.
    Unknown,
}

impl ExecutionPermission {
    /// Read the permission from a host the pool answered for.
    pub fn from_host(host: &HostView) -> Self {
        if host.execution_enabled {
            Self::Granted
        } else {
            Self::NotGranted
        }
    }

    /// Whether pooled agents may run here.
    ///
    /// Only an explicit grant answers `true`.
    pub fn may_run_agents(self) -> bool {
        matches!(self, Self::Granted)
    }

    /// The short label the status shows.
    pub fn label(self) -> &'static str {
        match self {
            Self::Granted => "granted",
            Self::NotGranted => "not granted",
            Self::Unknown => "unknown",
        }
    }

    /// What this permission means for work on the machine.
    pub fn consequence(self) -> &'static str {
        match self {
            Self::Granted => GRANT_CONSEQUENCE,
            Self::NotGranted => {
                "Agents will not run here. Execution is a separate grant, made from this machine and not implied by enrolling."
            }
            Self::Unknown => {
                "The pool has not answered, so nothing is assumed and no agent work will start here."
            }
        }
    }
}

/// One enrolled machine as this client last saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrolledHost {
    /// Public host id.
    pub id: String,
    /// The pool's mutable slug.
    pub slug: String,
    /// Display name.
    pub display_name: String,
    /// Hostname reported at enrollment.
    pub hostname: String,
    /// Presence.
    pub status: HostStatus,
    /// Execution permission.
    pub execution: ExecutionPermission,
}

impl EnrolledHost {
    fn from_pool(host: &HostView) -> Self {
        Self {
            id: host.id.clone(),
            slug: host.slug.clone(),
            display_name: host.display_name.clone(),
            hostname: host.hostname.clone(),
            status: host.host_status(),
            execution: ExecutionPermission::from_host(host),
        }
    }

    /// A machine this client remembers but has not asked the pool about yet.
    fn from_store(id: &str, stored: &StoredHost) -> Self {
        Self {
            id: id.to_owned(),
            slug: stored.slug.clone(),
            display_name: stored.display_name.clone(),
            hostname: stored.hostname.clone(),
            status: HostStatus::Unknown("not checked with the pool".to_owned()),
            execution: ExecutionPermission::Unknown,
        }
    }

    /// Replace the pool-reported fields, keeping the id.
    fn update_from_pool(&mut self, host: &HostView) {
        *self = Self::from_pool(host);
    }

    /// Forget the last pool answer about execution.
    fn mark_execution_unknown(&mut self) {
        self.execution = ExecutionPermission::Unknown;
    }
}

/// Whether the pool answered the last request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reachability {
    /// No request has been made in this run.
    Unknown,
    /// The last request was answered.
    Reachable,
    /// The last request failed before it could be answered.
    Unreachable(String),
}

impl Reachability {
    /// Whether the pool is currently unreachable.
    pub fn is_unreachable(&self) -> bool {
        matches!(self, Self::Unreachable(_))
    }
}

/// This machine and its pool session, ready to be rendered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineStatus {
    /// The configured pool URL, when there is one.
    pub pool_url: Option<String>,
    /// The signed-in operator's email, when a session is held.
    pub signed_in_as: Option<String>,
    /// This machine's host, when it is enrolled (or was on a previous launch).
    pub host: Option<EnrolledHost>,
    /// Whether the pool answered the last request.
    pub reachability: Reachability,
}

impl MachineStatus {
    /// Whether this machine may run agents.
    ///
    /// False unless the pool explicitly granted execution.
    pub fn may_run_agents(&self) -> bool {
        self.host
            .as_ref()
            .is_some_and(|host| host.execution.may_run_agents())
    }

    /// Whether an enrollment is remembered for this machine.
    pub fn is_enrolled(&self) -> bool {
        self.host.is_some()
    }
}

/// What could not be done with this machine's enrollment.
#[derive(Debug, Error)]
pub enum EnrollError {
    /// No operator session is held; sign in first.
    #[error("not signed in to a pool")]
    NotSignedIn,
    /// This machine has no remembered host; enroll it first.
    #[error("this machine is not enrolled as a host")]
    NotEnrolled,
    /// Enrolling needs a hostname.
    #[error("a hostname is required to enroll this machine")]
    MissingHostname,
    /// The pool no longer lists the enrolled host.
    #[error("the pool no longer lists this machine")]
    HostMissing,
    /// The pool request failed.
    #[error("pool: {0}")]
    Pool(#[from] PoolError),
    /// The settings store failed.
    #[error("settings: {0}")]
    Settings(#[from] SettingsError),
}

/// This machine's enrollment and status.
///
/// The state machine makes the credential path narrow: the only place a host
/// credential is written is [`Machine::enroll_this_machine`], and the only
/// destination is the [`SettingsStore`].
pub struct Machine<A: PoolApi> {
    api: A,
    store: SettingsStore,
    session: Option<Session>,
    host: Option<EnrolledHost>,
    reachability: Reachability,
    notice: Option<String>,
}

impl<A: PoolApi> std::fmt::Debug for Machine<A> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Machine")
            .field("status", &self.status())
            .field("notice", &self.notice)
            .finish_non_exhaustive()
    }
}

impl<A: PoolApi> Machine<A> {
    /// Open the default settings store and restore what the last launch knew.
    ///
    /// Never fails: a store that cannot be read becomes an empty one plus a
    /// notice, so the local app stays usable.
    pub fn open(api: A) -> Self {
        match SettingsStore::open() {
            Ok(store) => Self::with_store(api, store),
            Err(error) => {
                let path = SettingsStore::default_path()
                    .unwrap_or_else(|_| std::path::PathBuf::from("settings.json"));
                let mut machine = Self::with_store(api, SettingsStore::empty_at(path));
                machine.notice = Some(error.to_string());
                machine
            }
        }
    }

    /// Build the machine over `store`.
    ///
    /// The remembered session and host are shown immediately; the pool's live
    /// answer replaces them on [`Machine::refresh`].
    pub fn with_store(api: A, store: SettingsStore) -> Self {
        let settings = store.settings();
        let session = match (
            settings.pool_url.as_deref(),
            settings.member_token.as_deref(),
        ) {
            (Some(url), Some(token)) => Some(Session::from_stored(
                url,
                settings.member_email.as_deref().unwrap_or_default(),
                token,
            )),
            _ => None,
        };
        let host = settings
            .active_host()
            .map(|(id, stored)| EnrolledHost::from_store(id, stored));
        Self {
            api,
            store,
            session,
            host,
            reachability: Reachability::Unknown,
            notice: None,
        }
    }

    /// The status to render.
    pub fn status(&self) -> MachineStatus {
        MachineStatus {
            pool_url: self.store.settings().pool_url.clone(),
            signed_in_as: self
                .session
                .as_ref()
                .map(|session| session.email().to_owned()),
            host: self.host.clone(),
            reachability: self.reachability.clone(),
        }
    }

    /// Whether this machine may run agents, per the last pool answer.
    pub fn may_run_agents(&self) -> bool {
        self.status().may_run_agents()
    }

    /// The last problem worth telling the operator, if any.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// The settings file this machine reads and writes.
    pub fn settings_path(&self) -> &std::path::Path {
        self.store.path()
    }

    /// The stored host credential for tests and for the host-agent handoff.
    ///
    /// Crate-private: a UI has no reason to render the secret.
    #[cfg(test)]
    pub(crate) fn staged_credential(&self) -> Option<&str> {
        self.store
            .settings()
            .active_host()
            .map(|(_, stored)| stored.credential.as_str())
    }

    /// The status as lines a settings surface can render.
    ///
    /// No line contains a credential or a token.
    pub fn status_lines(&self) -> Vec<String> {
        let status = self.status();
        let pool = match (&status.reachability, &status.pool_url) {
            (Reachability::Unreachable(reason), _) => format!("Pool: unreachable — {reason}"),
            (_, Some(url)) => match &status.signed_in_as {
                Some(email) => format!("Pool: {url} — signed in as {email}"),
                None => format!("Pool: {url} — not signed in"),
            },
            (_, None) => "Pool: not signed in".to_owned(),
        };
        let machine = match &status.host {
            Some(host) => format!("This machine: {} — {}", host.slug, host.status.label()),
            None => "This machine: not enrolled".to_owned(),
        };
        let execution = match status.host.as_ref().map(|host| host.execution) {
            Some(permission) => format!(
                "Execution: {} — {}",
                permission.label(),
                permission.consequence()
            ),
            None => "Execution: not available — enroll this machine first.".to_owned(),
        };
        vec![pool, machine, execution]
    }

    /// Sign in to `pool_url` and remember the session in the settings store.
    ///
    /// Nothing is written until the pool has accepted the credentials, so a
    /// failed or unreachable sign-in leaves the previous settings intact.
    pub fn sign_in(
        &mut self,
        pool_url: &str,
        email: &str,
        password: &str,
    ) -> Result<(), EnrollError> {
        match self.api.sign_in(pool_url, email, password) {
            Ok(session) => {
                let mut settings = self.store.settings().clone();
                settings.remember_sign_in(
                    session.pool_url().to_owned(),
                    session.email().to_owned(),
                    session.token().to_owned(),
                );
                self.commit(settings)?;
                self.session = Some(session);
                self.reachability = Reachability::Reachable;
                self.notice = None;
                Ok(())
            }
            Err(error) => {
                self.record_failure(&error);
                Err(error.into())
            }
        }
    }

    /// Forget the operator session, keeping this machine's host credential.
    pub fn sign_out(&mut self) -> Result<(), EnrollError> {
        let mut settings = self.store.settings().clone();
        settings.forget_sign_in();
        self.commit(settings)?;
        self.session = None;
        self.notice = None;
        Ok(())
    }

    /// Enroll this machine as a host.
    ///
    /// Step one: register the host and mint a single-use token. Step two:
    /// generate the relationship credential here and exchange the token for it.
    /// The credential reaches the settings store and nowhere else; a failure at
    /// any step leaves the store exactly as it was.
    pub fn enroll_this_machine(&mut self, hostname: &str) -> Result<EnrolledHost, EnrollError> {
        let session = self.session.clone().ok_or(EnrollError::NotSignedIn)?;
        let hostname = hostname.trim();
        if hostname.is_empty() {
            return Err(EnrollError::MissingHostname);
        }
        match self.two_step_enrollment(&session, hostname) {
            Ok((host, credential)) => {
                let mut settings = self.store.settings().clone();
                settings.record_host(
                    host.id.clone(),
                    StoredHost {
                        credential,
                        slug: host.slug.clone(),
                        hostname: host.hostname.clone(),
                        display_name: host.display_name.clone(),
                    },
                );
                self.commit(settings)?;
                let enrolled = EnrolledHost::from_pool(&host);
                self.host = Some(enrolled.clone());
                self.reachability = Reachability::Reachable;
                self.notice = None;
                Ok(enrolled)
            }
            Err(error) => {
                self.record_failure(&error);
                Err(error.into())
            }
        }
    }

    fn two_step_enrollment(
        &mut self,
        session: &Session,
        hostname: &str,
    ) -> Result<(HostView, String), PoolError> {
        let registered = self.api.create_host(session, hostname, None)?;
        let token = self.api.mint_enrollment_token(session, &registered.id)?;
        let credential = generate_credential();
        let enrolled = self.api.enroll(session.pool_url(), &token, &credential)?;
        // The host the enroll answer names is the one the credential belongs to;
        // it is normally the registered host. Trust the answer, not the request.
        Ok((enrolled, credential))
    }

    /// Re-read this machine's presence and permission from the pool.
    pub fn refresh(&mut self) -> Result<EnrolledHost, EnrollError> {
        let session = self.session.clone().ok_or(EnrollError::NotSignedIn)?;
        let host_id = self
            .store
            .settings()
            .active_host()
            .map(|(id, _)| id.to_owned())
            .ok_or(EnrollError::NotEnrolled)?;
        match self.api.list_hosts(&session) {
            Ok(hosts) => {
                let Some(view) = hosts.into_iter().find(|host| host.id == host_id) else {
                    if let Some(host) = self.host.as_mut() {
                        host.mark_execution_unknown();
                    }
                    self.reachability = Reachability::Reachable;
                    self.notice = Some("the pool no longer lists this machine".to_owned());
                    return Err(EnrollError::HostMissing);
                };
                let mut host = self
                    .host
                    .clone()
                    .unwrap_or_else(|| EnrolledHost::from_pool(&view));
                host.update_from_pool(&view);
                self.host = Some(host.clone());
                self.reachability = Reachability::Reachable;
                self.notice = None;
                Ok(host)
            }
            Err(error) => {
                if let Some(host) = self.host.as_mut() {
                    host.mark_execution_unknown();
                }
                self.record_failure(&error);
                Err(error.into())
            }
        }
    }

    /// Grant `host.execute` from this machine.
    ///
    /// Read [`GRANT_CONSEQUENCE`] to the operator before calling this: the
    /// grant makes the pool dispatch agent work here.
    pub fn grant_execution(&mut self) -> Result<EnrolledHost, EnrollError> {
        let (session, host_id) = self.execution_context()?;
        match self.api.grant_execution(&session, &host_id) {
            Ok(view) => {
                let host = self.apply_host(&view);
                self.reachability = Reachability::Reachable;
                self.notice = None;
                Ok(host)
            }
            Err(error) => {
                self.record_failure(&error);
                Err(error.into())
            }
        }
    }

    /// Revoke `host.execute` from this machine.
    pub fn revoke_execution(&mut self) -> Result<EnrolledHost, EnrollError> {
        let (session, host_id) = self.execution_context()?;
        match self.api.revoke_execution(&session, &host_id) {
            Ok(view) => {
                let host = self.apply_host(&view);
                self.reachability = Reachability::Reachable;
                self.notice = None;
                Ok(host)
            }
            Err(error) => {
                self.record_failure(&error);
                Err(error.into())
            }
        }
    }

    fn execution_context(&self) -> Result<(Session, String), EnrollError> {
        let session = self.session.clone().ok_or(EnrollError::NotSignedIn)?;
        let host_id = self
            .store
            .settings()
            .active_host()
            .map(|(id, _)| id.to_owned())
            .ok_or(EnrollError::NotEnrolled)?;
        Ok((session, host_id))
    }

    fn apply_host(&mut self, view: &HostView) -> EnrolledHost {
        let mut host = self
            .host
            .clone()
            .unwrap_or_else(|| EnrolledHost::from_pool(view));
        host.update_from_pool(view);
        self.host = Some(host.clone());
        host
    }

    /// Replace the settings only after a successful write.
    fn commit(&mut self, settings: ClientSettings) -> Result<(), SettingsError> {
        let previous = self.store.settings().clone();
        *self.store.settings_mut() = settings;
        if let Err(error) = self.store.save() {
            *self.store.settings_mut() = previous;
            return Err(error);
        }
        Ok(())
    }

    fn record_failure(&mut self, error: &PoolError) {
        if let PoolError::Unreachable { detail } = error {
            self.reachability = Reachability::Unreachable(detail.clone());
        }
        self.notice = Some(error.to_string());
    }

    /// The API implementation, for the tests' call log.
    #[cfg(test)]
    pub(crate) fn api(&self) -> &A {
        &self.api
    }
}

/// Generate this machine's host relationship credential.
///
/// 32 random bytes as 64 lowercase hex characters, comfortably above the
/// protocol's 32-character minimum. `/dev/urandom` is read where it exists; the
/// fallback mixes the clock, the process id and `RandomState`, which is enough
/// to never repeat within a process and is only reached on platforms without a
/// random device.
fn generate_credential() -> String {
    let mut bytes = [0_u8; 32];
    #[cfg(unix)]
    {
        use std::io::Read as _;
        if let Ok(mut random) = std::fs::File::open("/dev/urandom")
            && random.read_exact(&mut bytes).is_ok()
        {
            return hex(&bytes);
        }
    }
    fallback_bytes(&mut bytes);
    hex(&bytes)
}

fn fallback_bytes(bytes: &mut [u8]) {
    use std::hash::{BuildHasher as _, Hasher as _};
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    let mut written = 0;
    let mut counter: u64 = 0;
    while written < bytes.len() {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u128(seed);
        hasher.write_u32(std::process::id());
        hasher.write_u64(counter);
        let chunk = hasher.finish().to_le_bytes();
        let take = chunk.len().min(bytes.len() - written);
        bytes[written..written + take].copy_from_slice(&chunk[..take]);
        written += take;
        counter += 1;
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut output, byte| {
        let _ = write!(output, "{byte:02x}");
        output
    })
}

/// Everything the tests need from the pool, without a socket.
///
/// Each method pops one scripted answer and records the call, so an unexpected
/// request is a clear failure rather than a silent success.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct ScriptedPool {
    pub(crate) calls: Vec<String>,
    pub(crate) logins: VecDeque<Result<Session, PoolError>>,
    pub(crate) hosts: VecDeque<Result<Vec<HostView>, PoolError>>,
    pub(crate) created: VecDeque<Result<HostView, PoolError>>,
    pub(crate) minted: VecDeque<Result<String, PoolError>>,
    pub(crate) enrolled: VecDeque<Result<HostView, PoolError>>,
    pub(crate) granted: VecDeque<Result<HostView, PoolError>>,
    pub(crate) revoked: VecDeque<Result<HostView, PoolError>>,
}

#[cfg(test)]
impl ScriptedPool {
    fn pop<T>(queue: &mut VecDeque<Result<T, PoolError>>, what: &str) -> Result<T, PoolError> {
        queue.pop_front().unwrap_or_else(|| {
            Err(PoolError::Protocol {
                detail: format!("unscripted {what} call"),
            })
        })
    }
}

#[cfg(test)]
impl PoolApi for ScriptedPool {
    fn sign_in(
        &mut self,
        pool_url: &str,
        email: &str,
        _password: &str,
    ) -> Result<Session, PoolError> {
        self.calls.push(format!("login:{pool_url}:{email}"));
        Self::pop(&mut self.logins, "login")
    }

    fn list_hosts(&mut self, _session: &Session) -> Result<Vec<HostView>, PoolError> {
        self.calls.push("list_hosts".to_owned());
        Self::pop(&mut self.hosts, "list_hosts")
    }

    fn create_host(
        &mut self,
        _session: &Session,
        hostname: &str,
        _display_name: Option<&str>,
    ) -> Result<HostView, PoolError> {
        self.calls.push(format!("create_host:{hostname}"));
        Self::pop(&mut self.created, "create_host")
    }

    fn mint_enrollment_token(
        &mut self,
        _session: &Session,
        host_id: &str,
    ) -> Result<String, PoolError> {
        self.calls.push(format!("mint:{host_id}"));
        Self::pop(&mut self.minted, "mint_enrollment_token")
    }

    fn enroll(
        &mut self,
        _pool_url: &str,
        _token: &str,
        _credential: &str,
    ) -> Result<HostView, PoolError> {
        self.calls.push("enroll".to_owned());
        Self::pop(&mut self.enrolled, "enroll")
    }

    fn grant_execution(
        &mut self,
        _session: &Session,
        host_id: &str,
    ) -> Result<HostView, PoolError> {
        self.calls.push(format!("grant:{host_id}"));
        Self::pop(&mut self.granted, "grant_execution")
    }

    fn revoke_execution(
        &mut self,
        _session: &Session,
        host_id: &str,
    ) -> Result<HostView, PoolError> {
        self.calls.push(format!("revoke:{host_id}"));
        Self::pop(&mut self.revoked, "revoke_execution")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::client::HttpPool;
    use crate::host::client::testing::ScriptedHttpPool;
    use serde_json::Value;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn scratch(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default();
        let dir = std::env::temp_dir().join(format!(
            "tidepool-client-enroll-{label}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("scratch directory");
        dir
    }

    fn host(status: &str, execution_enabled: bool) -> HostView {
        HostView {
            id: "host-1".to_owned(),
            slug: "build-box".to_owned(),
            display_name: "Build box".to_owned(),
            hostname: "build-box.local".to_owned(),
            status: status.to_owned(),
            execution_enabled,
        }
    }

    fn unreachable() -> PoolError {
        PoolError::Unreachable {
            detail: "connection refused".to_owned(),
        }
    }

    fn signed_in_pool() -> ScriptedPool {
        let mut pool = ScriptedPool::default();
        pool.logins.push_back(Ok(Session::for_test(
            "http://127.0.0.1:8080",
            "operator@example.test",
            "jwt-token",
        )));
        pool
    }

    fn scripted_enrollment(pool: &mut ScriptedPool) {
        pool.created.push_back(Ok(host("pending_approval", false)));
        pool.minted.push_back(Ok("token.public".to_owned()));
        pool.enrolled.push_back(Ok(host("pending_approval", false)));
    }

    fn store_at(dir: &std::path::Path) -> SettingsStore {
        SettingsStore::open_at(dir.join("settings.json")).expect("open store")
    }

    #[test]
    fn enrollment_is_two_steps_and_stores_the_credential_only_in_settings() {
        let dir = scratch("enroll");
        let mut pool = signed_in_pool();
        scripted_enrollment(&mut pool);
        pool.hosts.push_back(Ok(vec![host("connected", false)]));
        let mut machine = Machine::with_store(pool, store_at(&dir));
        machine
            .sign_in("http://127.0.0.1:8080", "operator@example.test", "password")
            .expect("sign in");

        let enrolled = machine
            .enroll_this_machine("build-box.local")
            .expect("enroll");
        assert_eq!(enrolled.id, "host-1");
        assert_eq!(enrolled.execution, ExecutionPermission::NotGranted);
        assert!(!machine.may_run_agents());

        assert_eq!(
            machine.api().calls,
            vec![
                "login:http://127.0.0.1:8080:operator@example.test",
                "create_host:build-box.local",
                "mint:host-1",
                "enroll",
            ],
            "enrollment is the CLI's two-step exchange and nothing else"
        );

        let credential = machine
            .staged_credential()
            .expect("credential stored")
            .to_owned();
        assert_eq!(credential.len(), 64);
        assert!(
            credential
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        );

        let stored = fs::read_to_string(machine.settings_path()).expect("settings file");
        let json: Value = serde_json::from_str(&stored).expect("settings json");
        assert_eq!(json["hosts"]["host-1"]["credential"], credential);
        assert_eq!(json["active_host"], "host-1");
        assert_eq!(json["member_token"], "jwt-token");
        assert_eq!(
            json["pool_url"], "http://127.0.0.1:8080",
            "the pool URL lives with the credential"
        );
        assert!(
            !json.to_string().contains("password"),
            "the password is not stored"
        );

        let entries: Vec<_> = fs::read_dir(&dir)
            .expect("read dir")
            .map(|entry| entry.expect("entry").file_name())
            .collect();
        assert_eq!(entries, vec![std::ffi::OsString::from("settings.json")]);

        let rendered = format!("{machine:?}");
        assert!(!rendered.contains(&credential), "no secret in Debug output");
        assert!(!rendered.contains("jwt-token"));

        machine.refresh().expect("refresh");
        let status = machine.status();
        assert_eq!(
            status.host.as_ref().expect("host").status,
            HostStatus::Connected
        );
        assert_eq!(
            status.host.as_ref().expect("host").execution,
            ExecutionPermission::NotGranted,
            "connected is not granted"
        );
        assert!(!status.may_run_agents());
    }

    #[test]
    fn grant_and_revoke_are_explicit_and_state_the_consequence() {
        let dir = scratch("grant");
        let mut pool = signed_in_pool();
        scripted_enrollment(&mut pool);
        pool.granted.push_back(Ok(host("connected", true)));
        pool.revoked.push_back(Ok(host("connected", false)));
        let mut machine = Machine::with_store(pool, store_at(&dir));
        machine
            .sign_in("http://127.0.0.1:8080", "operator@example.test", "password")
            .expect("sign in");
        machine
            .enroll_this_machine("build-box.local")
            .expect("enroll");

        assert!(
            !machine
                .api()
                .calls
                .iter()
                .any(|call| call.starts_with("grant")),
            "enrolling never grants execution"
        );
        let before = machine.status_lines().join("\n");
        assert!(before.contains("Execution: not granted"));
        assert!(before.contains("Agents will not run here"));

        let granted = machine.grant_execution().expect("grant");
        assert_eq!(granted.execution, ExecutionPermission::Granted);
        assert!(machine.may_run_agents());
        let after = machine.status_lines().join("\n");
        assert!(after.contains("Execution: granted"));
        assert!(after.contains(GRANT_CONSEQUENCE));
        assert!(machine.api().calls.contains(&"grant:host-1".to_owned()));

        let revoked = machine.revoke_execution().expect("revoke");
        assert_eq!(revoked.execution, ExecutionPermission::NotGranted);
        assert!(!machine.may_run_agents());
        assert!(machine.api().calls.contains(&"revoke:host-1".to_owned()));
    }

    #[test]
    fn an_unreachable_pool_leaves_the_local_app_usable() {
        let dir = scratch("unreachable");
        let mut pool = ScriptedPool::default();
        pool.logins.push_back(Err(unreachable()));
        let mut machine = Machine::with_store(pool, store_at(&dir));

        let error = machine
            .sign_in("http://127.0.0.1:1", "operator@example.test", "password")
            .expect_err("unreachable");
        assert!(matches!(
            error,
            EnrollError::Pool(PoolError::Unreachable { .. })
        ));

        let status = machine.status();
        assert_eq!(status.signed_in_as, None);
        assert_eq!(
            status.reachability,
            Reachability::Unreachable("connection refused".to_owned())
        );
        assert!(!status.may_run_agents());

        let lines = machine.status_lines().join("\n");
        assert!(lines.contains("unreachable"));
        assert!(lines.contains("not enrolled"));
        assert!(!machine.settings_path().exists(), "nothing half-written");

        // The local surfaces still answer, and a mutating action is refused
        // with a plain reason rather than a panic.
        assert!(matches!(
            machine.grant_execution(),
            Err(EnrollError::NotSignedIn)
        ));
    }

    #[test]
    fn a_failed_exchange_stores_no_credential_and_leaves_settings_untouched() {
        let dir = scratch("failed-exchange");
        let mut pool = signed_in_pool();
        pool.created.push_back(Ok(host("pending_approval", false)));
        pool.minted.push_back(Err(PoolError::Problem {
            status: 410,
            code: "enrollment_token_expired".to_owned(),
            detail: ": the token has expired".to_owned(),
        }));
        let mut machine = Machine::with_store(pool, store_at(&dir));
        machine
            .sign_in("http://127.0.0.1:8080", "operator@example.test", "password")
            .expect("sign in");
        let before = fs::read(machine.settings_path()).expect("settings after sign-in");

        let error = machine
            .enroll_this_machine("build-box.local")
            .expect_err("expired token");
        assert!(matches!(
            error,
            EnrollError::Pool(PoolError::Problem { .. })
        ));
        let after = fs::read(machine.settings_path()).expect("settings after failure");
        assert_eq!(before, after, "a failed exchange writes nothing");
        assert!(machine.status().host.is_none());
        assert!(machine.staged_credential().is_none());
    }

    #[test]
    fn a_pool_that_stops_answering_downgrades_the_grant_to_unknown() {
        let dir = scratch("downgrade");
        let mut pool = signed_in_pool();
        scripted_enrollment(&mut pool);
        pool.hosts.push_back(Ok(vec![host("connected", true)]));
        pool.hosts.push_back(Err(unreachable()));
        let mut machine = Machine::with_store(pool, store_at(&dir));
        machine
            .sign_in("http://127.0.0.1:8080", "operator@example.test", "password")
            .expect("sign in");
        machine
            .enroll_this_machine("build-box.local")
            .expect("enroll");
        machine.refresh().expect("refresh with grant");
        assert!(machine.may_run_agents());

        machine.refresh().expect_err("gone");
        let status = machine.status();
        assert_eq!(
            status.host.as_ref().expect("host").execution,
            ExecutionPermission::Unknown
        );
        assert!(!status.may_run_agents(), "a stale grant is not re-used");
        assert!(
            machine
                .status_lines()
                .join("\n")
                .contains("Execution: unknown")
        );
    }

    #[test]
    fn actions_need_a_session_and_an_enrolled_machine() {
        let dir = scratch("preconditions");
        let mut machine = Machine::with_store(ScriptedPool::default(), store_at(&dir));
        assert!(matches!(
            machine.enroll_this_machine("build-box.local"),
            Err(EnrollError::NotSignedIn)
        ));
        assert!(matches!(machine.refresh(), Err(EnrollError::NotSignedIn)));
        assert!(matches!(
            machine.revoke_execution(),
            Err(EnrollError::NotSignedIn)
        ));
    }

    #[test]
    fn an_unknown_host_status_is_shown_as_unknown_not_connected() {
        let dir = scratch("unknown-status");
        let mut pool = signed_in_pool();
        scripted_enrollment(&mut pool);
        pool.hosts.push_back(Ok(vec![host("maintenance", false)]));
        let mut machine = Machine::with_store(pool, store_at(&dir));
        machine
            .sign_in("http://127.0.0.1:8080", "operator@example.test", "password")
            .expect("sign in");
        machine
            .enroll_this_machine("build-box.local")
            .expect("enroll");
        let refreshed = machine.refresh().expect("refresh");
        assert_eq!(
            refreshed.status,
            HostStatus::Unknown("maintenance".to_owned())
        );
        assert_ne!(refreshed.status, HostStatus::Connected);
    }

    #[test]
    fn a_remembered_machine_shows_up_before_the_pool_answers() {
        let dir = scratch("remembered");
        let mut store = store_at(&dir);
        store.settings_mut().remember_sign_in(
            "http://127.0.0.1:8080".to_owned(),
            "operator@example.test".to_owned(),
            "jwt-token".to_owned(),
        );
        store.settings_mut().record_host(
            "host-1".to_owned(),
            StoredHost {
                credential: "c".repeat(64),
                slug: "build-box".to_owned(),
                hostname: "build-box.local".to_owned(),
                display_name: "Build box".to_owned(),
            },
        );
        store.save().expect("save");
        let reloaded = store_at(&dir);
        let machine = Machine::with_store(ScriptedPool::default(), reloaded);
        let status = machine.status();
        assert_eq!(
            status.signed_in_as.as_deref(),
            Some("operator@example.test")
        );
        assert!(status.is_enrolled());
        assert_eq!(
            status.host.as_ref().expect("host").execution,
            ExecutionPermission::Unknown
        );
        assert!(
            !status.may_run_agents(),
            "a remembered enrollment is not a remembered grant"
        );
    }

    #[test]
    fn end_to_end_enrollment_over_the_documented_http_routes() {
        // The same two-step exchange, the grant and the revoke, driven by the
        // real `HttpPool` against a pool that answers on loopback with the
        // documented routes and bodies. No live database, Redis or network.
        const LOGIN: &str = r#"{"token":"jwt-api-token","expires_in":3600,"must_change_password":false,"user":{"id":"u1","email":"operator@example.test","display_name":"Operator","role":"owner"}}"#;
        const PENDING: &str = r#"{"id":"host-1","slug":"build-box","display_name":"Build box","hostname":"build-box.local","status":"pending_approval","execution_enabled":false,"created_at":"2026-10-29T00:00:00Z"}"#;
        const CONNECTED: &str = r#"{"id":"host-1","slug":"build-box","display_name":"Build box","hostname":"build-box.local","status":"connected","execution_enabled":false,"created_at":"2026-10-29T00:00:00Z"}"#;
        const GRANTED: &str = r#"{"id":"host-1","slug":"build-box","display_name":"Build box","hostname":"build-box.local","status":"connected","execution_enabled":true,"created_at":"2026-10-29T00:00:00Z"}"#;
        let hosts = format!(r#"{{"hosts":[{CONNECTED}]}}"#);
        let pool = ScriptedHttpPool::start(&[
            ("POST", "/v1/pool/login", 200, LOGIN),
            ("POST", "/v1/hosts", 201, PENDING),
            (
                "POST",
                "/v1/hosts/host-1/enrollment-tokens",
                201,
                r#"{"token":"enroll-token","enrollment_token":{"id":"e1","expires_at":"2026-10-29T00:10:00Z","created_at":"2026-10-29T00:00:00Z"}}"#,
            ),
            ("POST", "/v1/hosts/enroll", 201, PENDING),
            ("GET", "/v1/hosts", 200, &hosts),
            ("POST", "/v1/hosts/host-1/execution", 200, GRANTED),
            ("DELETE", "/v1/hosts/host-1/execution", 200, CONNECTED),
        ]);
        let dir = scratch("e2e");
        let mut machine = Machine::with_store(HttpPool::new(), store_at(&dir));
        machine
            .sign_in(pool.url(), "operator@example.test", "password")
            .expect("sign in over HTTP");
        machine
            .enroll_this_machine("build-box.local")
            .expect("enroll over HTTP");
        machine.refresh().expect("pool reports the host");
        let status = machine.status();
        assert_eq!(
            status.host.as_ref().expect("host").status,
            HostStatus::Connected
        );
        assert!(!status.may_run_agents());

        machine.grant_execution().expect("grant over HTTP");
        assert!(machine.may_run_agents());
        machine.revoke_execution().expect("revoke over HTTP");
        assert!(!machine.may_run_agents());

        let credential = machine.staged_credential().expect("stored").to_owned();
        let captured = pool.finish();
        assert_eq!(
            captured
                .iter()
                .map(|request| format!("{} {}", request.method, request.path))
                .collect::<Vec<_>>(),
            vec![
                "POST /v1/pool/login",
                "POST /v1/hosts",
                "POST /v1/hosts/host-1/enrollment-tokens",
                "POST /v1/hosts/enroll",
                "GET /v1/hosts",
                "POST /v1/hosts/host-1/execution",
                "DELETE /v1/hosts/host-1/execution",
            ]
        );
        let enrollment = captured
            .iter()
            .find(|request| request.path == "/v1/hosts/enroll")
            .expect("enroll request");
        assert_eq!(enrollment.json()["token"], "enroll-token");
        assert_eq!(
            enrollment.json()["credential"],
            credential,
            "the credential was generated on this machine"
        );
        let list = captured
            .iter()
            .find(|request| request.path == "/v1/hosts" && request.method == "GET")
            .expect("hosts request");
        assert_eq!(list.authorization.as_deref(), Some("Bearer jwt-api-token"));
        let stored = fs::read_to_string(machine.settings_path()).expect("settings");
        assert!(stored.contains(&credential));
        let entries: Vec<_> = fs::read_dir(&dir)
            .expect("read dir")
            .map(|entry| entry.expect("entry").file_name())
            .collect();
        assert_eq!(entries, vec![std::ffi::OsString::from("settings.json")]);
    }

    /// End-to-end against a running dev stack, when one is configured.
    ///
    /// Skipped unless `TIDEPOOL_E2E_POOL_URL`, `TIDEPOOL_E2E_EMAIL`,
    /// `TIDEPOOL_E2E_PASSWORD` and `TIDEPOOL_E2E_HOSTNAME` are all set. The
    /// values are read and used, never printed: the test's own report is the
    /// host id and whether the pool lists it. It leaves the machine without the
    /// execution grant, so running it cannot silently arm a host.
    #[test]
    fn end_to_end_against_the_dev_stack_when_configured() {
        let (Ok(pool_url), Ok(email), Ok(password), Ok(hostname)) = (
            std::env::var("TIDEPOOL_E2E_POOL_URL"),
            std::env::var("TIDEPOOL_E2E_EMAIL"),
            std::env::var("TIDEPOOL_E2E_PASSWORD"),
            std::env::var("TIDEPOOL_E2E_HOSTNAME"),
        ) else {
            return;
        };
        let dir = scratch("dev-stack");
        let mut machine = Machine::with_store(HttpPool::new(), store_at(&dir));
        machine
            .sign_in(&pool_url, &email, &password)
            .expect("the dev stack accepts the operator");
        let enrolled = machine
            .enroll_this_machine(&hostname)
            .expect("the dev stack enrolls this machine");
        let listed = machine.refresh().expect("the pool lists the host");
        assert_eq!(listed.id, enrolled.id);
        assert_ne!(listed.status, HostStatus::Revoked);

        if machine.may_run_agents() {
            machine
                .revoke_execution()
                .expect("leave the machine without the grant");
        }
        assert!(!machine.may_run_agents(), "the test leaves no grant behind");

        let credential = machine.staged_credential().expect("credential stored");
        let stored = fs::read_to_string(machine.settings_path()).expect("settings");
        assert!(stored.contains(credential));
        let entries: Vec<_> = fs::read_dir(&dir)
            .expect("read dir")
            .map(|entry| entry.expect("entry").file_name())
            .collect();
        assert_eq!(entries, vec![std::ffi::OsString::from("settings.json")]);
    }
}
