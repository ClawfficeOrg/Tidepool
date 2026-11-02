//! Application configuration loaded from environment.
//!
//! Centralises all runtime configuration so the rest of the codebase can
//! depend on a typed `Config` rather than scattered `std::env::var` calls.

use anyhow::{Context, Result, anyhow};

/// Rate limiting configuration (task 0.4.8).
///
/// Fixed-window counters are kept in Redis; limits are expressed as
/// `(requests, window_secs)` pairs per request class:
///
/// - `auth` — the tightest budget, applied to credential endpoints
///   (`/v1/auth/**`, legacy `/auth/**`) to slow brute-force attempts.
/// - `default` — the general budget for every other rate-limited public
///   route (e.g. the `/ws` handshake).
///
/// All values load from `RATE_LIMIT_*` environment variables and validate
/// at startup so nonsense configuration fails fast.
#[derive(Debug, Clone, PartialEq)]
pub struct RateLimitConfig {
    /// Master switch. When `false` no limiter is registered and requests
    /// pass through untouched.
    pub enabled: bool,
    /// Default-class request budget per window.
    pub requests: u32,
    /// Default-class window length in seconds.
    pub window_secs: u64,
    /// Auth-class request budget per window.
    pub auth_requests: u32,
    /// Auth-class window length in seconds.
    pub auth_window_secs: u64,
    /// Path prefixes subject to default-class limiting. Paths under the
    /// auth prefixes (`/v1/auth`, `/auth`) always use the tighter auth
    /// budget instead. `/healthz` must never be listed here — liveness
    /// probes may not be rate limited.
    pub public_prefixes: Vec<String>,
    /// Trust `X-Forwarded-For` for client identification instead of the
    /// socket peer address. Only enable behind a proxy that overwrites
    /// the header; otherwise clients can spoof their bucket.
    pub trust_xff: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            requests: 100,
            window_secs: 60,
            auth_requests: 10,
            auth_window_secs: 60,
            public_prefixes: vec!["/v1/auth".into(), "/auth".into(), "/ws".into()],
            trust_xff: false,
        }
    }
}

impl RateLimitConfig {
    /// Upper bound accepted for any window length (seconds) — guards against
    /// a typo freezing buckets for hours.
    const MAX_WINDOW_SECS: u64 = 3600;

    /// Load rate-limit settings from environment.
    ///
    /// Unset variables fall back to [`RateLimitConfig::default`] values;
    /// present-but-invalid values return `Err` so startup fails fast.
    pub fn from_env() -> Result<Self> {
        let mut cfg = Self::default();

        if std::env::var("RATE_LIMIT_ENABLED").is_ok() {
            cfg.enabled = parse_bool("RATE_LIMIT_ENABLED")?;
        }
        if std::env::var("RATE_LIMIT_REQUESTS").is_ok() {
            cfg.requests = std::env::var("RATE_LIMIT_REQUESTS")
                .context("reading RATE_LIMIT_REQUESTS")?
                .parse::<u32>()
                .context("RATE_LIMIT_REQUESTS must be a valid u32")?;
        }
        if std::env::var("RATE_LIMIT_WINDOW_SECS").is_ok() {
            cfg.window_secs = std::env::var("RATE_LIMIT_WINDOW_SECS")
                .context("reading RATE_LIMIT_WINDOW_SECS")?
                .parse::<u64>()
                .context("RATE_LIMIT_WINDOW_SECS must be a valid u64")?;
        }
        if std::env::var("RATE_LIMIT_AUTH_REQUESTS").is_ok() {
            cfg.auth_requests = std::env::var("RATE_LIMIT_AUTH_REQUESTS")
                .context("reading RATE_LIMIT_AUTH_REQUESTS")?
                .parse::<u32>()
                .context("RATE_LIMIT_AUTH_REQUESTS must be a valid u32")?;
        }
        if std::env::var("RATE_LIMIT_AUTH_WINDOW_SECS").is_ok() {
            cfg.auth_window_secs = std::env::var("RATE_LIMIT_AUTH_WINDOW_SECS")
                .context("reading RATE_LIMIT_AUTH_WINDOW_SECS")?
                .parse::<u64>()
                .context("RATE_LIMIT_AUTH_WINDOW_SECS must be a valid u64")?;
        }
        if std::env::var("RATE_LIMIT_PUBLIC_PREFIXES").is_ok() {
            let raw = std::env::var("RATE_LIMIT_PUBLIC_PREFIXES")
                .context("reading RATE_LIMIT_PUBLIC_PREFIXES")?;
            cfg.public_prefixes = raw
                .split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect();
        }
        if std::env::var("RATE_LIMIT_TRUST_XFF").is_ok() {
            cfg.trust_xff = parse_bool("RATE_LIMIT_TRUST_XFF")?;
        }

        cfg.validate()?;
        Ok(cfg)
    }

    /// Reject nonsensical limits/windows before the server starts.
    fn validate(&self) -> Result<()> {
        if self.requests == 0 {
            return Err(anyhow!("RATE_LIMIT_REQUESTS must be at least 1"));
        }
        if self.auth_requests == 0 {
            return Err(anyhow!("RATE_LIMIT_AUTH_REQUESTS must be at least 1"));
        }
        if self.window_secs == 0 || self.window_secs > Self::MAX_WINDOW_SECS {
            return Err(anyhow!(
                "RATE_LIMIT_WINDOW_SECS must be between 1 and {}",
                Self::MAX_WINDOW_SECS
            ));
        }
        if self.auth_window_secs == 0 || self.auth_window_secs > Self::MAX_WINDOW_SECS {
            return Err(anyhow!(
                "RATE_LIMIT_AUTH_WINDOW_SECS must be between 1 and {}",
                Self::MAX_WINDOW_SECS
            ));
        }
        Ok(())
    }
}

/// Parse a strict boolean env var (`true`/`false`, case-insensitive, or `1`/`0`).
fn parse_bool(var: &str) -> Result<bool> {
    match std::env::var(var)
        .context(format!("reading {var}"))?
        .to_lowercase()
        .as_str()
    {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        other => Err(anyhow!("{var} must be true or false (got {other})")),
    }
}

/// Minimal email shape check used before an address reaches the database.
///
/// API payloads additionally carry `#[validate(email)]`; this helper covers
/// values loaded from the environment and service-level callers.
pub(crate) fn is_valid_email(email: &str) -> bool {
    let email = email.trim();
    if email.is_empty() || email.len() > 255 || email.chars().any(char::is_whitespace) {
        return false;
    }
    let mut parts = email.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
}

/// First-run operator bootstrap loaded from `TIDEBOOT_*` (task 1.1.1).
///
/// The three required variables are all-or-nothing: setting one without the
/// others is a startup error, so a typo cannot silently leave an unattended
/// pool without an owner.
///
/// `TIDEBOOT_OWNER_PASSWORD` is a privileged secret. The custom [`Debug`]
/// implementation redacts it so it cannot leak through a `tracing` field or a
/// panic message.
#[derive(Clone, PartialEq, Eq)]
pub struct PoolBootstrapConfig {
    /// Organization name (`TIDEBOOT_ORGANIZATION_NAME`).
    pub organization_name: String,
    /// Owner login email (`TIDEBOOT_OWNER_EMAIL`).
    pub owner_email: String,
    /// Temporary owner password (`TIDEBOOT_OWNER_PASSWORD`), at least 12 chars.
    pub owner_password: String,
    /// Display name (`TIDEBOOT_OWNER_NAME`, defaults to `Owner`).
    pub owner_display_name: String,
}

impl std::fmt::Debug for PoolBootstrapConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PoolBootstrapConfig")
            .field("organization_name", &self.organization_name)
            .field("owner_email", &self.owner_email)
            .field("owner_password", &"<redacted>")
            .field("owner_display_name", &self.owner_display_name)
            .finish()
    }
}

impl PoolBootstrapConfig {
    /// Minimum temporary-password length accepted for bootstrap.
    pub const MIN_PASSWORD_LEN: usize = 12;

    /// Load `TIDEBOOT_*` from the environment.
    ///
    /// Returns `Ok(None)` when none of the required variables are set, so a
    /// browser setup install needs no bootstrap configuration.
    pub fn from_env() -> Result<Option<Self>> {
        const ORG: &str = "TIDEBOOT_ORGANIZATION_NAME";
        const EMAIL: &str = "TIDEBOOT_OWNER_EMAIL";
        const PASSWORD: &str = "TIDEBOOT_OWNER_PASSWORD";
        const NAME: &str = "TIDEBOOT_OWNER_NAME";

        let org = std::env::var(ORG).ok();
        let email = std::env::var(EMAIL).ok();
        let password = std::env::var(PASSWORD).ok();

        let any = org.is_some() || email.is_some() || password.is_some();
        if !any {
            return Ok(None);
        }

        let organization_name = org
            .filter(|v| !v.trim().is_empty())
            .ok_or_else(|| anyhow!("{ORG} is required when TIDEBOOT_* is configured"))?
            .trim()
            .to_string();
        let owner_email = email
            .filter(|v| !v.trim().is_empty())
            .ok_or_else(|| anyhow!("{EMAIL} is required when TIDEBOOT_* is configured"))?
            .trim()
            .to_string();
        let owner_password = password
            .filter(|v| !v.is_empty())
            .ok_or_else(|| anyhow!("{PASSWORD} is required when TIDEBOOT_* is configured"))?;

        if owner_password.len() < Self::MIN_PASSWORD_LEN {
            return Err(anyhow!(
                "{PASSWORD} must be at least {} characters",
                Self::MIN_PASSWORD_LEN
            ));
        }
        if !is_valid_email(&owner_email) {
            return Err(anyhow!("{EMAIL} must be a valid email address"));
        }

        let owner_display_name = std::env::var(NAME)
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "Owner".to_string());

        Ok(Some(Self {
            organization_name,
            owner_email,
            owner_password,
            owner_display_name,
        }))
    }

    /// True when every field is empty (only used by callers that construct
    /// this type directly, e.g. tests).
    pub fn is_empty(&self) -> bool {
        self.organization_name.trim().is_empty()
            && self.owner_email.trim().is_empty()
            && self.owner_password.is_empty()
            && self.owner_display_name.trim().is_empty()
    }
}

/// Runtime configuration loaded from environment variables (and optionally
/// an `.env` file via [`dotenvy::dotenv`]).
///
/// `Config::from_env` is the single entry-point and validates inputs at
/// startup so misconfiguration fails fast.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields are consumed in R-7 (sqlx pool), R-8 (migrations), and Phase 0.2 (auth).
pub struct Config {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub redis_url: String,
    pub jwt_secret: String,
    pub jwt_expiry_secs: u64,
    pub rate_limit: RateLimitConfig,
    /// Unattended first-run bootstrap; `None` means browser setup.
    pub pool_bootstrap: Option<PoolBootstrapConfig>,
    /// Public origin operators reach the pool on, e.g. `https://pool.example.com`.
    ///
    /// This is the one setting a deployment changes when its address changes:
    /// webhook URLs handed to providers, links put in notifications and the
    /// reverse proxy's `Host` all derive from it. It is validated as an
    /// absolute `http`/`https` origin with no path, query or fragment.
    pub public_origin: Option<String>,
    /// Browser origins allowed to call the pool cross-origin
    /// (`TIDEPOOL_CORS_ORIGINS`, comma-separated). Empty means no CORS headers.
    ///
    /// Every entry is an exact `http`/`https` origin (scheme, host, optional
    /// port); wildcards are refused.
    pub cors_origins: Vec<String>,
}

impl Config {
    /// Minimum acceptable JWT secret length, per `docs/GUIDELINES.md` security policy.
    pub const MIN_JWT_SECRET_LEN: usize = 32;

    /// Load configuration from process environment.
    ///
    /// Returns `Err` if a required variable is missing or invalid.
    pub fn from_env() -> Result<Self> {
        let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let port = std::env::var("PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse::<u16>()
            .context("PORT must be a valid u16")?;

        let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL is required")?;
        let redis_url = std::env::var("REDIS_URL").context("REDIS_URL is required")?;

        let jwt_secret = std::env::var("JWT_SECRET").context("JWT_SECRET is required")?;
        if jwt_secret.len() < Self::MIN_JWT_SECRET_LEN {
            return Err(anyhow!(
                "JWT_SECRET must be at least {} characters (got {})",
                Self::MIN_JWT_SECRET_LEN,
                jwt_secret.len()
            ));
        }

        let jwt_expiry_secs = std::env::var("JWT_EXPIRY_SECONDS")
            .unwrap_or_else(|_| "3600".to_string())
            .parse::<u64>()
            .context("JWT_EXPIRY_SECONDS must be a valid u64")?;

        let rate_limit = RateLimitConfig::from_env()?;
        let pool_bootstrap = PoolBootstrapConfig::from_env()?;
        let public_origin = public_origin_from_env()?;
        let cors_origins = match std::env::var("TIDEPOOL_CORS_ORIGINS") {
            Ok(raw) => parse_cors_origins(&raw)?,
            Err(_) => Vec::new(),
        };

        Ok(Self {
            host,
            port,
            database_url,
            redis_url,
            jwt_secret,
            jwt_expiry_secs,
            rate_limit,
            pool_bootstrap,
            public_origin,
            cors_origins,
        })
    }

    /// Convenience accessor for the bind socket string `host:port`.
    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// Read and validate `TIDEPOOL_PUBLIC_ORIGIN`.
fn public_origin_from_env() -> Result<Option<String>> {
    match std::env::var("TIDEPOOL_PUBLIC_ORIGIN") {
        Ok(raw) => parse_public_origin(&raw),
        Err(_) => Ok(None),
    }
}

/// Validate a configured public origin: scheme, host and optional port only.
fn parse_public_origin(raw: &str) -> Result<Option<String>> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Ok(None);
    }
    let rest = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .ok_or_else(|| anyhow!("TIDEPOOL_PUBLIC_ORIGIN must start with http:// or https://"))?;
    if rest.is_empty() {
        return Err(anyhow!("TIDEPOOL_PUBLIC_ORIGIN must name a host"));
    }
    if rest.contains('/') || rest.contains('?') || rest.contains('#') {
        return Err(anyhow!(
            "TIDEPOOL_PUBLIC_ORIGIN must be an origin only, without a path, query or fragment"
        ));
    }
    Ok(Some(trimmed.to_string()))
}

/// Parse `TIDEPOOL_CORS_ORIGINS`: a comma-separated list of exact origins.
///
/// Each entry must be an absolute `http` or `https` origin with no path, query,
/// fragment, userinfo or trailing slash. `*` is refused: the pool never allows
/// every origin. Empty entries are skipped and duplicates collapse.
fn parse_cors_origins(raw: &str) -> Result<Vec<String>> {
    let mut origins: Vec<String> = Vec::new();
    for entry in raw.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        if entry == "*" {
            return Err(anyhow!(
                "TIDEPOOL_CORS_ORIGINS must list exact origins; `*` is not allowed"
            ));
        }
        let rest = entry
            .strip_prefix("https://")
            .or_else(|| entry.strip_prefix("http://"))
            .ok_or_else(|| {
                anyhow!("TIDEPOOL_CORS_ORIGINS entry `{entry}` must start with http:// or https://")
            })?;
        if rest.is_empty() || rest.starts_with(':') {
            return Err(anyhow!(
                "TIDEPOOL_CORS_ORIGINS entry `{entry}` must name a host"
            ));
        }
        if rest.contains(['/', '?', '#', '@', '*']) || rest.chars().any(char::is_whitespace) {
            return Err(anyhow!(
                "TIDEPOOL_CORS_ORIGINS entry `{entry}` must be an origin only: scheme, host and optional port, with no path, trailing slash, query or wildcard"
            ));
        }
        if !origins.iter().any(|o| o == entry) {
            origins.push(entry.to_string());
        }
    }
    Ok(origins)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: set all required env vars for a test.
    /// Tests run serially via `--test-threads=1` is not assumed, so each test
    /// uses unique-enough values; we hold the lock implicitly by setting the
    /// same canonical values everywhere.
    fn set_required(jwt: &str) {
        // SAFETY: tests in this module are gated behind a Mutex below.
        unsafe {
            std::env::set_var("DATABASE_URL", "mysql://u:p@localhost/db");
            std::env::set_var("REDIS_URL", "redis://localhost:6379");
            std::env::set_var("JWT_SECRET", jwt);
            std::env::remove_var("HOST");
            std::env::remove_var("PORT");
            std::env::remove_var("JWT_EXPIRY_SECONDS");
            clear_rate_limit_env();
            clear_bootstrap_env();
        }
    }

    /// Clear every `TIDEBOOT_*` variable so browser setup is the default.
    fn clear_bootstrap_env() {
        // SAFETY: all callers hold ENV_LOCK.
        unsafe {
            for var in [
                "TIDEBOOT_ORGANIZATION_NAME",
                "TIDEBOOT_OWNER_EMAIL",
                "TIDEBOOT_OWNER_PASSWORD",
                "TIDEBOOT_OWNER_NAME",
            ] {
                std::env::remove_var(var);
            }
        }
    }

    /// Clear every `RATE_LIMIT_*` variable so defaults apply.
    fn clear_rate_limit_env() {
        // SAFETY: all callers hold ENV_LOCK.
        unsafe {
            for var in [
                "RATE_LIMIT_ENABLED",
                "RATE_LIMIT_REQUESTS",
                "RATE_LIMIT_WINDOW_SECS",
                "RATE_LIMIT_AUTH_REQUESTS",
                "RATE_LIMIT_AUTH_WINDOW_SECS",
                "RATE_LIMIT_PUBLIC_PREFIXES",
                "RATE_LIMIT_TRUST_XFF",
            ] {
                std::env::remove_var(var);
            }
        }
    }

    // Serialise env access across tests in this module.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn from_env_happy_path_uses_defaults() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        set_required("a".repeat(32).as_str());

        let cfg = Config::from_env().expect("config loads");
        assert_eq!(cfg.host, "127.0.0.1");
        assert_eq!(cfg.port, 8080);
        assert_eq!(cfg.jwt_expiry_secs, 3600);
        assert_eq!(cfg.bind_addr(), "127.0.0.1:8080");
    }

    #[test]
    fn from_env_rejects_short_jwt_secret() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        set_required("too-short");

        let err = Config::from_env().expect_err("short secret should fail");
        assert!(err.to_string().contains("JWT_SECRET"));
    }

    #[test]
    fn from_env_requires_database_url() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        set_required("a".repeat(32).as_str());
        // SAFETY: guarded by ENV_LOCK
        unsafe {
            std::env::remove_var("DATABASE_URL");
        }

        let err = Config::from_env().expect_err("missing DATABASE_URL should fail");
        assert!(err.to_string().contains("DATABASE_URL"));
    }

    #[test]
    fn rate_limit_defaults_when_env_unset() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        set_required("a".repeat(32).as_str());

        let cfg = Config::from_env().expect("config loads").rate_limit;
        assert_eq!(cfg, RateLimitConfig::default());
        assert!(cfg.enabled);
        assert_eq!(cfg.requests, 100);
        assert_eq!(cfg.window_secs, 60);
        assert_eq!(cfg.auth_requests, 10);
        assert_eq!(cfg.auth_window_secs, 60);
        assert!(!cfg.trust_xff);
        assert_eq!(cfg.public_prefixes, vec!["/v1/auth", "/auth", "/ws"]);
    }

    #[test]
    fn rate_limit_parses_custom_env() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        set_required("a".repeat(32).as_str());
        // SAFETY: guarded by ENV_LOCK
        unsafe {
            std::env::set_var("RATE_LIMIT_ENABLED", "true");
            std::env::set_var("RATE_LIMIT_REQUESTS", "250");
            std::env::set_var("RATE_LIMIT_WINDOW_SECS", "30");
            std::env::set_var("RATE_LIMIT_AUTH_REQUESTS", "5");
            std::env::set_var("RATE_LIMIT_AUTH_WINDOW_SECS", "120");
            std::env::set_var("RATE_LIMIT_PUBLIC_PREFIXES", " /ws , /v1/events , ");
            std::env::set_var("RATE_LIMIT_TRUST_XFF", "true");
        }

        let cfg = Config::from_env().expect("config loads").rate_limit;
        assert!(cfg.enabled);
        assert_eq!(cfg.requests, 250);
        assert_eq!(cfg.window_secs, 30);
        assert_eq!(cfg.auth_requests, 5);
        assert_eq!(cfg.auth_window_secs, 120);
        assert!(cfg.trust_xff);
        // Entries trim; empty segments drop.
        assert_eq!(cfg.public_prefixes, vec!["/ws", "/v1/events"]);
    }

    #[test]
    fn rate_limit_can_disable_via_env() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        set_required("a".repeat(32).as_str());
        // SAFETY: guarded by ENV_LOCK
        unsafe {
            std::env::set_var("RATE_LIMIT_ENABLED", "false");
        }

        let cfg = Config::from_env().expect("config loads").rate_limit;
        assert!(!cfg.enabled);
    }

    #[test]
    fn rate_limit_rejects_zero_requests() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        set_required("a".repeat(32).as_str());
        // SAFETY: guarded by ENV_LOCK
        unsafe {
            std::env::set_var("RATE_LIMIT_REQUESTS", "0");
        }

        let err = Config::from_env().expect_err("zero requests should fail");
        assert!(err.to_string().contains("RATE_LIMIT_REQUESTS"));
    }

    #[test]
    fn rate_limit_rejects_zero_auth_requests() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        set_required("a".repeat(32).as_str());
        // SAFETY: guarded by ENV_LOCK
        unsafe {
            std::env::set_var("RATE_LIMIT_AUTH_REQUESTS", "0");
        }

        let err = Config::from_env().expect_err("zero auth requests should fail");
        assert!(err.to_string().contains("RATE_LIMIT_AUTH_REQUESTS"));
    }

    #[test]
    fn rate_limit_rejects_zero_window() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        set_required("a".repeat(32).as_str());
        // SAFETY: guarded by ENV_LOCK
        unsafe {
            std::env::set_var("RATE_LIMIT_WINDOW_SECS", "0");
        }

        let err = Config::from_env().expect_err("zero window should fail");
        assert!(err.to_string().contains("RATE_LIMIT_WINDOW_SECS"));
    }

    #[test]
    fn rate_limit_rejects_oversized_window() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        set_required("a".repeat(32).as_str());
        // SAFETY: guarded by ENV_LOCK
        unsafe {
            std::env::set_var("RATE_LIMIT_AUTH_WINDOW_SECS", "3601");
        }

        let err = Config::from_env().expect_err("oversized window should fail");
        assert!(err.to_string().contains("RATE_LIMIT_AUTH_WINDOW_SECS"));
    }

    #[test]
    fn rate_limit_rejects_bad_bool() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        set_required("a".repeat(32).as_str());
        // SAFETY: guarded by ENV_LOCK
        unsafe {
            std::env::set_var("RATE_LIMIT_TRUST_XFF", "maybe");
        }

        let err = Config::from_env().expect_err("bad bool should fail");
        assert!(err.to_string().contains("RATE_LIMIT_TRUST_XFF"));
    }

    // ── TIDEBOOT_* bootstrap ───────────────────────────────────────────────

    #[test]
    fn bootstrap_absent_means_browser_setup() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        set_required("a".repeat(32).as_str());

        let cfg = Config::from_env().expect("config loads");
        assert!(cfg.pool_bootstrap.is_none());
    }

    #[test]
    fn bootstrap_parses_complete_environment() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        set_required("a".repeat(32).as_str());
        // SAFETY: guarded by ENV_LOCK
        unsafe {
            std::env::set_var("TIDEBOOT_ORGANIZATION_NAME", " Rock Pool ");
            std::env::set_var("TIDEBOOT_OWNER_EMAIL", "owner@example.com");
            std::env::set_var("TIDEBOOT_OWNER_PASSWORD", "twelve-chars!");
        }

        let cfg = Config::from_env().expect("config loads");
        let boot = cfg.pool_bootstrap.expect("bootstrap configured");
        assert_eq!(boot.organization_name, "Rock Pool");
        assert_eq!(boot.owner_email, "owner@example.com");
        assert_eq!(boot.owner_password, "twelve-chars!");
        assert_eq!(boot.owner_display_name, "Owner");
    }

    #[test]
    fn bootstrap_rejects_partial_environment() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        set_required("a".repeat(32).as_str());
        // SAFETY: guarded by ENV_LOCK
        unsafe {
            std::env::set_var("TIDEBOOT_ORGANIZATION_NAME", "Rock Pool");
        }

        let err = Config::from_env().expect_err("partial bootstrap must fail");
        assert!(err.to_string().contains("TIDEBOOT_OWNER_EMAIL"));
    }

    #[test]
    fn bootstrap_rejects_short_temporary_password() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        set_required("a".repeat(32).as_str());
        // SAFETY: guarded by ENV_LOCK
        unsafe {
            std::env::set_var("TIDEBOOT_ORGANIZATION_NAME", "Rock Pool");
            std::env::set_var("TIDEBOOT_OWNER_EMAIL", "owner@example.com");
            std::env::set_var("TIDEBOOT_OWNER_PASSWORD", "too-short");
        }

        let err = Config::from_env().expect_err("short bootstrap password must fail");
        assert!(err.to_string().contains("at least 12 characters"));
    }

    #[test]
    fn bootstrap_debug_redacts_password() {
        let boot = PoolBootstrapConfig {
            organization_name: "Rock Pool".into(),
            owner_email: "owner@example.com".into(),
            owner_password: "super-secret-bootstrap".into(),
            owner_display_name: "Owner".into(),
        };
        let rendered = format!("{boot:?}");
        assert!(!rendered.contains("super-secret-bootstrap"));
        assert!(rendered.contains("<redacted>"));
    }
    #[test]
    fn public_origin_accepts_only_an_origin() {
        assert_eq!(
            parse_public_origin("https://pool.example.com/").expect("origin"),
            Some("https://pool.example.com".to_string())
        );
        assert_eq!(
            parse_public_origin("http://127.0.0.1:8080").expect("origin"),
            Some("http://127.0.0.1:8080".to_string())
        );
        assert_eq!(parse_public_origin("   ").expect("empty"), None);
        assert!(parse_public_origin("pool.example.com").is_err());
        assert!(parse_public_origin("https://pool.example.com/path").is_err());
        assert!(parse_public_origin("https://pool.example.com?x=1").is_err());
        assert!(parse_public_origin("https://").is_err());
    }

    #[test]
    fn cors_origins_accept_exact_origins() {
        assert_eq!(parse_cors_origins("").expect("empty"), Vec::<String>::new());
        assert_eq!(
            parse_cors_origins(
                " https://control.example.com , http://localhost:5173,,https://control.example.com"
            )
            .expect("origins"),
            vec![
                "https://control.example.com".to_string(),
                "http://localhost:5173".to_string()
            ]
        );
    }

    #[test]
    fn cors_origins_reject_anything_but_an_exact_origin() {
        for bad in [
            "*",
            "https://a.example.com,*",
            "https://control.example.com/",
            "https://control.example.com/app",
            "control.example.com",
            "//control.example.com",
            "ftp://control.example.com",
            "https://",
            "https://*.example.com",
            "https://control.example.com?x=1",
            "https://user@control.example.com",
        ] {
            assert!(parse_cors_origins(bad).is_err(), "{bad} should be rejected");
        }
    }
}
