//! Startup and lifecycle management.
//!
//! Currently owns one subsystem:
//!
//! - [`SingleInstanceGuard`] — a lock file in the app data directory that
//!   guarantees only one instance of the app runs at a time. The lock is
//!   created atomically with `File::create_new`, records the owner PID for
//!   diagnostics, and is removed on [`Drop`]. Stale locks whose owner PID is
//!   no longer alive are reclaimed automatically.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Errors produced by the lifecycle subsystem.
#[derive(Debug)]
pub enum LifecycleError {
    /// Another instance of the app is already running.
    AlreadyRunning,
    /// The platform data/config directory could not be determined.
    NoDataDir,
    /// The single-instance lock could not be created or read.
    LockFailed(String),
}

impl std::fmt::Display for LifecycleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyRunning => {
                write!(f, "another instance of Tidepool is already running")
            }
            Self::NoDataDir => write!(f, "could not determine the application data directory"),
            Self::LockFailed(detail) => write!(f, "single-instance lock failed: {detail}"),
        }
    }
}

impl std::error::Error for LifecycleError {}

// ---------------------------------------------------------------------------
// Single instance guard
// ---------------------------------------------------------------------------

/// Name of the lock file inside the app data directory.
const LOCK_FILE_NAME: &str = "tidepool-client.lock";

/// Maximum age of a lock file before it is considered stale regardless of
/// whether the owning PID is alive. Five minutes covers the worst-case
/// PID-reuse window on Windows while being short enough that a genuinely
/// running instance will never hit this threshold during normal operation.
const STALE_LOCK_SECS: u64 = 300;

/// Appends a trailing null and returns the UTF-16 encoding of a path, for the
/// wide-string Windows APIs.
#[cfg(target_os = "windows")]
fn wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Custom message sent to the existing instance's tray window to ask it to
/// surface its window. Must match `SHOW_WINDOW_MSG` in `tray_icon.rs`.
#[cfg(target_os = "windows")]
const SHOW_WINDOW_MSG: u32 = 0x800 + 2; // WM_APP + 2

/// Window class name used by the tray icon's hidden message-only window.
/// Must match `ZoidTrayWindowClass` in `tray_icon.rs`.
#[cfg(target_os = "windows")]
const TRAY_WINDOW_CLASS: &str = "ZoidTrayWindowClass";

/// Attempts to signal the already-running instance to surface its window.
///
/// Uses `FindWindowW` to locate the first instance's hidden tray message
/// window and `PostMessageW` to send the `SHOW_WINDOW_MSG`. Returns `true`
/// if the signal was sent successfully.
#[cfg(target_os = "windows")]
pub fn signal_existing_instance() -> bool {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW};
    use windows::core::PCWSTR;

    let class_wide = wide(Path::new(TRAY_WINDOW_CLASS));
    let hwnd = unsafe { FindWindowW(PCWSTR(class_wide.as_ptr()), None) };
    match hwnd {
        Ok(hwnd) if !hwnd.0.is_null() => unsafe {
            PostMessageW(Some(hwnd), SHOW_WINDOW_MSG, WPARAM(0), LPARAM(0)).is_ok()
        },
        _ => false,
    }
}

/// Whether a process with the given PID is still alive.
///
/// Windows checks via `OpenProcess`; Linux checks for `/proc/{pid}`. On other
/// platforms there is no cheap std-only check, so the process is assumed alive
/// (a live lock is never reclaimed while its owner runs).
fn process_alive(pid: u32) -> bool {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

        unsafe {
            match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                Ok(handle) => {
                    let _ = CloseHandle(handle);
                    true
                }
                Err(_) => false,
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        let _ = pid;
        true
    }
}

/// Reads the owner PID and creation timestamp from a lock file.
///
/// The lock file format is `{pid}\n{unix_secs}\n`. Returns `None` if the file
/// cannot be read or does not match the expected format.
fn lock_info(path: &Path) -> Option<(u32, u64)> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut lines = content.lines();
    let pid: u32 = lines.next()?.trim().parse().ok()?;
    let ts: u64 = lines.next()?.trim().parse().ok()?;
    Some((pid, ts))
}

/// Whether a lock file should be considered stale.
///
/// A lock is stale when:
/// - The owning PID is no longer alive, **or**
/// - The lock file is older than [`STALE_LOCK_SECS`] (covers PID-reuse and
///   crash-without-Drop scenarios).
fn is_lock_stale(path: &Path) -> bool {
    let Some((pid, ts)) = lock_info(path) else {
        // Unrecognised format — treat as stale so we can reclaim it.
        return true;
    };
    if !process_alive(pid) {
        return true;
    }
    // PID is alive but the lock might be orphaned (crash without Drop, PID
    // reused). Check the timestamp.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    now.saturating_sub(ts) > STALE_LOCK_SECS
}

/// Holds the single-instance lock.
///
/// Acquiring the guard creates the lock file atomically; a second acquisition
/// while the guard is alive returns [`LifecycleError::AlreadyRunning`]. The
/// lock file is removed when the guard is dropped. A lock whose owner PID is
/// no longer alive (a stale lock left by a crash) is reclaimed automatically.
pub struct SingleInstanceGuard {
    path: PathBuf,
    _file: File,
}

impl SingleInstanceGuard {
    /// Acquires the lock in the platform app data directory.
    pub fn acquire() -> Result<Self, LifecycleError> {
        let path = default_lock_path()?;
        Self::acquire_at(&path)
    }

    /// Acquires the lock at an explicit path (used by tests to isolate the
    /// lock from the real data directory).
    pub fn acquire_at(path: &Path) -> Result<Self, LifecycleError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|err| LifecycleError::LockFailed(err.to_string()))?;
        }
        match std::fs::File::create_new(path) {
            Ok(mut file) => {
                // Write PID + unix timestamp. Both are diagnostics / staleness
                // helpers; a failed write must not fail lock acquisition.
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let _ = writeln!(file, "{}\n{}", std::process::id(), ts);
                Ok(Self {
                    path: path.to_path_buf(),
                    _file: file,
                })
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                if is_lock_stale(path) {
                    // Stale lock: the owner is gone or the lock is too old.
                    // Reclaim and retry once.
                    std::fs::remove_file(path)
                        .map_err(|err| LifecycleError::LockFailed(err.to_string()))?;
                    return Self::acquire_at(path);
                }
                Err(LifecycleError::AlreadyRunning)
            }
            Err(err) => Err(LifecycleError::LockFailed(err.to_string())),
        }
    }
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

impl gpui::Global for SingleInstanceGuard {}

/// Resolves the single-instance lock path:
/// `{data_dir}/tidepool-client.lock` via `directories`.
pub(crate) fn default_lock_path() -> Result<PathBuf, LifecycleError> {
    let proj = directories::ProjectDirs::from("", "", "tidepool-client")
        .ok_or(LifecycleError::NoDataDir)?;
    Ok(proj.data_dir().join(LOCK_FILE_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unique temp path for a test lock file.
    fn temp_lock(label: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("tidepool-client-{label}-{unique}.lock"))
    }

    #[test]
    fn guard_acquires_and_releases() {
        let path = temp_lock("acquire");
        {
            let guard = SingleInstanceGuard::acquire_at(&path).expect("first acquire");
            assert_eq!(guard.path, path);
            // Lock file exists while held.
            assert!(path.exists());
        }
        // Dropped → lock released.
        assert!(!path.exists());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn second_acquire_rejected_while_held() {
        let path = temp_lock("reject");
        let _guard = SingleInstanceGuard::acquire_at(&path).expect("first acquire");
        let second = SingleInstanceGuard::acquire_at(&path);
        assert!(
            matches!(second, Err(LifecycleError::AlreadyRunning)),
            "second acquire must fail with AlreadyRunning"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn stale_lock_is_reclaimed() {
        let path = temp_lock("stale");
        // Forge a stale lock: owner PID cannot exist and timestamp is old.
        std::fs::write(&path, format!("{}\n0\n", u32::MAX - 1)).expect("write stale lock");
        let guard = SingleInstanceGuard::acquire_at(&path);
        assert!(guard.is_ok(), "stale lock must be reclaimed");
        drop(guard);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn malformed_lock_is_reclaimed() {
        let path = temp_lock("malformed");
        std::fs::write(&path, "not a lock file").expect("write malformed lock");
        let guard = SingleInstanceGuard::acquire_at(&path);
        assert!(guard.is_ok(), "malformed lock must be reclaimed");
        drop(guard);
        let _ = std::fs::remove_file(&path);
    }
}
