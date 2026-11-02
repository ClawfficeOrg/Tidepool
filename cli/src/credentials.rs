//! The credential store the CLI owns.
//!
//! One JSON file per user, outside any repository: `$TIDEPOOL_HOME/credentials.json`
//! when that variable is set, otherwise `$XDG_CONFIG_HOME/tidepool/credentials.json`
//! and finally `$HOME/.config/tidepool/credentials.json`. The file is written
//! atomically with `0600` permissions where the platform has them, and
//! [`CredentialStore::save`] refuses a path that sits inside a git working
//! tree, so a credential can never be committed by accident.
//!
//! Nothing in this module is printed: [`CredentialFile`]'s `Debug` redacts every
//! secret, and [`redact`] is the only way a secret reaches a terminal.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{CliError, Result};

/// Environment variable that relocates the whole store to a directory.
pub const HOME_ENV: &str = "TIDEPOOL_HOME";
/// Environment variable that relocates the store under a config directory.
pub const XDG_ENV: &str = "XDG_CONFIG_HOME";
/// Environment variable that gives the per-user config directory.
pub const USER_HOME_ENV: &str = "HOME";
/// Name of the store file inside its directory.
pub const STORE_FILE: &str = "credentials.json";

/// What the CLI remembers between commands.
///
/// `Debug` prints the shape and redacted secrets, never a full credential, so a
/// panic or a log line cannot leak one.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CredentialFile {
    /// The pool this store was last used against.
    pub pool: Option<String>,
    /// The signed-in member's email, used as the default dispatch actor.
    pub email: Option<String>,
    /// The operator JWT.
    pub token: Option<String>,
    /// Host relationship credentials, keyed by host id.
    #[serde(default)]
    pub host_credentials: BTreeMap<String, String>,
    /// The host enrolled most recently.
    #[serde(default)]
    pub active_host: Option<String>,
}

impl std::fmt::Debug for CredentialFile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CredentialFile")
            .field("pool", &self.pool)
            .field("email", &self.email)
            .field("token", &self.token.as_deref().map(redact))
            .field(
                "host_credentials",
                &self.host_credentials.keys().collect::<Vec<_>>(),
            )
            .field("active_host", &self.active_host)
            .finish()
    }
}

/// Render a secret so a person can tell which one it is without receiving it.
///
/// A short secret is not shown at all; a long one keeps four leading characters
/// so a store and a pool can be compared, never the whole value.
pub fn redact(secret: &str) -> String {
    let count = secret.chars().count();
    if count <= 8 {
        return format!("<redacted; {count} chars>");
    }
    let prefix: String = secret.chars().take(4).collect();
    format!("{prefix}…<redacted; {count} chars>")
}

/// A loaded credential file and where it came from.
pub struct CredentialStore {
    path: PathBuf,
    file: CredentialFile,
    present: bool,
}

impl std::fmt::Debug for CredentialStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CredentialStore")
            .field("path", &self.path)
            .field("present", &self.present)
            .field("file", &self.file)
            .finish()
    }
}

impl CredentialStore {
    /// Load the store at `path`, treating a missing file as an empty store.
    ///
    /// A file that exists but does not parse is an error rather than silently
    /// replacing whatever the owner had: the CLI never overwrites a store it
    /// could not read.
    pub fn load(path: PathBuf) -> Result<Self> {
        match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(file) => Ok(Self {
                    path,
                    file,
                    present: true,
                }),
                Err(_) => Err(CliError::CorruptStore { path }),
            },
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(Self {
                path,
                file: CredentialFile::default(),
                present: false,
            }),
            Err(source) => Err(CliError::Read { path, source }),
        }
    }

    /// Where the store lives.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The remembered values.
    pub fn file(&self) -> &CredentialFile {
        &self.file
    }

    /// The remembered values, for a command that changes them.
    pub fn file_mut(&mut self) -> &mut CredentialFile {
        &mut self.file
    }

    /// Whether the file existed when it was loaded.
    pub fn present(&self) -> bool {
        self.present
    }

    /// Write the store atomically, outside any repository, mode `0600`.
    pub fn save(&self) -> Result<()> {
        ensure_outside_repository(&self.path)?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|source| CliError::Write {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let bytes = serde_json::to_vec_pretty(&self.file)?;
        let temporary = self.path.with_extension("json.tmp");
        write_private(&temporary, &bytes)?;
        fs::rename(&temporary, &self.path).map_err(|source| CliError::Write {
            path: self.path.clone(),
            source,
        })
    }
}

/// Where the store lives when no path was given explicitly.
///
/// The lookup is a parameter so it can be tested without touching the
/// environment of the test process.
pub fn default_path(env: impl Fn(&str) -> Option<String>) -> Result<PathBuf> {
    if let Some(directory) = non_empty(env(HOME_ENV)) {
        return Ok(PathBuf::from(directory).join(STORE_FILE));
    }
    if let Some(directory) = non_empty(env(XDG_ENV)) {
        return Ok(PathBuf::from(directory).join("tidepool").join(STORE_FILE));
    }
    if let Some(directory) = non_empty(env(USER_HOME_ENV)) {
        return Ok(PathBuf::from(directory)
            .join(".config")
            .join("tidepool")
            .join(STORE_FILE));
    }
    Err(CliError::NoHome)
}

/// Refuse a store path that sits inside a git working tree.
///
/// The check walks the lexical ancestors of the path and looks for a `.git`
/// entry, which covers a repository worktree and a linked worktree alike. It is
/// the mechanical form of "a credential is never written into a repository".
pub fn ensure_outside_repository(path: &Path) -> Result<()> {
    for ancestor in path.ancestors().skip(1) {
        if ancestor.join(".git").exists() {
            return Err(CliError::RepositoryStore(path.to_path_buf()));
        }
    }
    Ok(())
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|source| CliError::Write {
        path: path.to_path_buf(),
        source,
    })?;
    file.write_all(bytes).map_err(|source| CliError::Write {
        path: path.to_path_buf(),
        source,
    })?;
    file.sync_all().map_err(|source| CliError::Write {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or_default();
            let path = std::env::temp_dir().join(format!(
                "tidepool-cli-store-{label}-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("temp dir");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn default_path_prefers_the_override_then_xdg_then_home() {
        let resolved = default_path(|name| match name {
            HOME_ENV => Some("/store".to_owned()),
            XDG_ENV => Some("/xdg".to_owned()),
            _ => Some("/home/ops".to_owned()),
        })
        .expect("path");
        assert_eq!(resolved, PathBuf::from("/store/credentials.json"));

        let resolved = default_path(|name| match name {
            HOME_ENV => None,
            XDG_ENV => Some("/xdg".to_owned()),
            _ => Some("/home/ops".to_owned()),
        })
        .expect("path");
        assert_eq!(resolved, PathBuf::from("/xdg/tidepool/credentials.json"));

        let resolved = default_path(|name| match name {
            USER_HOME_ENV => Some("/home/ops".to_owned()),
            _ => None,
        })
        .expect("path");
        assert_eq!(
            resolved,
            PathBuf::from("/home/ops/.config/tidepool/credentials.json")
        );

        let error = default_path(|_| None).expect_err("no home");
        assert!(matches!(error, CliError::NoHome));
    }

    #[test]
    fn an_empty_override_is_ignored() {
        let resolved = default_path(|name| match name {
            HOME_ENV => Some("  ".to_owned()),
            USER_HOME_ENV => Some("/home/ops".to_owned()),
            _ => None,
        })
        .expect("path");
        assert_eq!(
            resolved,
            PathBuf::from("/home/ops/.config/tidepool/credentials.json")
        );
    }

    #[test]
    fn redaction_never_returns_the_secret() {
        let secret = "abcdefghijklmnopqrstuvwxyz0123456789";
        let printed = redact(secret);
        assert!(printed.starts_with("abcd"));
        assert!(!printed.contains(secret));
        assert!(printed.contains("36 chars"));

        let short = redact("abc123");
        assert_eq!(short, "<redacted; 6 chars>");
        assert!(!short.contains("abc123"));
    }

    #[test]
    fn debug_prints_no_secret() {
        let mut file = CredentialFile {
            pool: Some("https://pool.example.com".to_owned()),
            email: Some("ops@example.com".to_owned()),
            token: Some("a-very-long-operator-token".to_owned()),
            ..CredentialFile::default()
        };
        file.host_credentials
            .insert("host_1".to_owned(), "a-long-host-credential".to_owned());
        let rendered = format!("{file:?}");
        assert!(rendered.contains("https://pool.example.com"));
        assert!(rendered.contains("host_1"));
        assert!(!rendered.contains("a-very-long-operator-token"));
        assert!(!rendered.contains("a-long-host-credential"));
    }

    #[test]
    fn a_missing_store_loads_empty() {
        let directory = TempDir::new("missing");
        let store =
            CredentialStore::load(directory.path().join("credentials.json")).expect("empty store");
        assert!(!store.present());
        assert_eq!(store.file(), &CredentialFile::default());
    }

    #[test]
    fn the_store_round_trips_and_is_private() {
        let directory = TempDir::new("roundtrip");
        let path = directory.path().join("credentials.json");
        let mut store = CredentialStore::load(path.clone()).expect("store");
        store.file_mut().pool = Some("https://pool.example.com".to_owned());
        store.file_mut().email = Some("ops@example.com".to_owned());
        store.file_mut().token = Some("token".to_owned());
        store.save().expect("save");

        let reloaded = CredentialStore::load(path.clone()).expect("reload");
        assert!(reloaded.present());
        assert_eq!(reloaded.file().token.as_deref(), Some("token"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = fs::metadata(&path).expect("metadata").permissions().mode();
            assert_eq!(mode & 0o077, 0, "store must not be group or world readable");
        }
    }

    #[test]
    fn a_corrupt_store_is_reported_without_printing_it() {
        let directory = TempDir::new("corrupt");
        let path = directory.path().join("credentials.json");
        fs::write(&path, b"{not json").expect("write");
        let error = CredentialStore::load(path).expect_err("corrupt");
        assert!(matches!(error, CliError::CorruptStore { .. }));
        assert!(!error.to_string().contains("not json"));
    }

    #[test]
    fn a_store_inside_a_repository_is_refused() {
        let directory = TempDir::new("repo");
        fs::create_dir_all(directory.path().join(".git")).expect(".git dir");
        let nested = directory.path().join("credentials");
        let mut store = CredentialStore::load(nested.join("credentials.json")).expect("store");
        store.file_mut().token = Some("token".to_owned());
        let error = store.save().expect_err("must refuse");
        assert!(matches!(error, CliError::RepositoryStore(_)));
        assert!(!nested.join("credentials.json").exists());

        let plain = directory.path().join("outside-credentials.json");
        ensure_outside_repository(&plain).expect_err("still inside the tree");
    }

    #[test]
    fn a_store_outside_a_repository_is_accepted() {
        let directory = TempDir::new("outside");
        ensure_outside_repository(&directory.path().join("credentials.json")).expect("accepted");
    }
}
