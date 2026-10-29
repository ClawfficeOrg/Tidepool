use actix_web::{App, HttpRequest, HttpResponse, HttpServer, web};
use std::path::{Path, PathBuf};
use tidepool::{
    api,
    config::Config,
    db,
    middleware::{metrics, problem, rate_limit, request_id},
    services, sockets,
};
use tracing::info;
use tracing_actix_web::TracingLogger;

/// Environment variable naming a built control plane directory to serve.
///
/// When it is set and holds an `index.html`, the pool serves the built
/// Tidepool control plane from its own origin, so a self-hosted pool needs no
/// second deployment and no CORS. Unset means the pool serves only its API,
/// which is what a standalone control plane deployment uses.
pub const CONTROL_PLANE_DIR_ENV: &str = "TIDEPOOL_CONTROL_PLANE_DIR";

/// First path segments the pool answers itself; the static bundle never shadows them.
const RESERVED_PREFIXES: [&str; 5] = ["v1", "ws", "healthz", "metrics", "openapi"];

/// The `data-` marker the served bundle carries so the app knows its own origin is the pool.
const SAME_ORIGIN_MARKER: &str = "tidepool-pool-origin";

/// A built control plane directory the pool can serve.
///
/// The directory is canonicalised once at startup and every request resolves
/// inside it, so a traversal or a symlink cannot read outside the bundle.
#[derive(Clone, Debug)]
pub struct ControlPlaneFiles {
    /// Canonical directory holding `index.html` and the built assets.
    root: PathBuf,
}

impl ControlPlaneFiles {
    /// Root a built bundle at `dir`.
    ///
    /// # Errors
    ///
    /// Returns `NotFound` when `dir` has no `index.html`, because serving an
    /// empty directory would answer every page with a 404 after a confusing
    /// startup.
    pub fn new(dir: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        let root = std::fs::canonicalize(dir)?;
        if !root.join("index.html").is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("{} has no index.html", root.display()),
            ));
        }
        Ok(Self { root })
    }

    /// Read the bundle location from [`CONTROL_PLANE_DIR_ENV`].
    ///
    /// # Errors
    ///
    /// Returns the underlying error when the variable names a directory that
    /// cannot be used. A blank or unset variable is `Ok(None)`, not an error.
    pub fn from_env() -> Result<Option<Self>, std::io::Error> {
        let Some(dir) = std::env::var(CONTROL_PLANE_DIR_ENV).ok() else {
            return Ok(None);
        };
        if dir.trim().is_empty() {
            return Ok(None);
        }
        Self::new(dir.trim()).map(Some)
    }

    /// The canonical directory being served.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolve a request path to a file in the bundle.
    ///
    /// `/` and a directory path return `index.html`. A missing path without a
    /// file extension is also `index.html`, because the app is a hash router
    /// and an unknown page is its own not-found view. A missing asset with an
    /// extension is `None`: a script that 404s must 404, never return HTML.
    /// Any `..`, empty segment, backslash or escaping symlink is `None`.
    pub fn resolve(&self, requested: &str) -> Option<PathBuf> {
        let clean = requested.trim_start_matches('/');
        if clean.is_empty() {
            return Some(self.root.join("index.html"));
        }

        let mut path = self.root.clone();
        for segment in clean.split('/') {
            if segment.is_empty() || segment == "." || segment == ".." {
                return None;
            }
            if segment.contains('\\') || segment.contains('\0') {
                return None;
            }
            path.push(segment);
        }

        if path.is_file() {
            // A symlink inside the bundle must not point outside it.
            let canonical = std::fs::canonicalize(&path).ok()?;
            if !canonical.starts_with(&self.root) {
                return None;
            }
            return Some(canonical);
        }

        let last = clean.rsplit('/').next().unwrap_or_default();
        if last.contains('.') {
            return None;
        }
        Some(self.root.join("index.html"))
    }
}

/// True when the pool owns this path, so the static bundle must not answer it.
pub fn is_reserved_path(path: &str) -> bool {
    let first = path.trim_start_matches('/').split('/').next().unwrap_or("");
    RESERVED_PREFIXES.contains(&first)
}

/// Put the same-origin marker in a built `index.html` before serving it.
///
/// The bundle is otherwise served byte for byte. The marker is what makes the
/// app treat the pool's own origin as its base URL, with no build-time value
/// and no CORS; a standalone deployment's copy has no marker and behaves as
/// it always did.
pub fn with_pool_origin_meta(html: &str) -> String {
    if html.contains(SAME_ORIGIN_MARKER) {
        return html.to_string();
    }
    let tag = format!("<meta name=\"{SAME_ORIGIN_MARKER}\" content=\"same-origin\" />");
    match html.find("</head>") {
        Some(index) => format!("{}{}{}", &html[..index], tag, &html[index..]),
        None => format!("{html}{tag}"),
    }
}

/// The content type for a built asset, by extension.
fn content_type_for(path: &Path) -> &'static str {
    match path.extension().and_then(|value| value.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json" | "webmanifest") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("map") => "application/json",
        _ => "application/octet-stream",
    }
}

/// Cache policy for a built asset: hashed assets are immutable, pages are not.
fn cache_control_for(path: &Path) -> &'static str {
    let html = path.extension().and_then(|value| value.to_str()) == Some("html");
    if html {
        "no-cache"
    } else {
        "public, max-age=31536000, immutable"
    }
}

/// Serve the built control plane at the default route.
///
/// Registered only when [`CONTROL_PLANE_DIR_ENV`] names a bundle, so an
/// API-only deployment is unchanged. Paths the pool owns keep their own 404
/// rather than being answered with the app's HTML.
async fn serve_control_plane(
    files: web::Data<ControlPlaneFiles>,
    req: HttpRequest,
) -> HttpResponse {
    if is_reserved_path(req.path()) {
        return HttpResponse::NotFound().finish();
    }
    let Some(file) = files.resolve(req.path()) else {
        return HttpResponse::NotFound().finish();
    };
    let Ok(bytes) = tokio::fs::read(&file).await else {
        return HttpResponse::NotFound().finish();
    };

    let is_index = file.file_name().and_then(|name| name.to_str()) == Some("index.html");
    let body = if is_index {
        with_pool_origin_meta(&String::from_utf8_lossy(&bytes)).into_bytes()
    } else {
        bytes
    };

    HttpResponse::Ok()
        .insert_header(("Content-Type", content_type_for(&file)))
        .insert_header(("X-Content-Type-Options", "nosniff"))
        .insert_header(("Cache-Control", cache_control_for(&file)))
        .body(body)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Load .env if present; missing file is not an error in production.
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cfg = Config::from_env().map_err(|e| {
        tracing::error!(error = %e, "Failed to load configuration");
        std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string())
    })?;

    let bind = cfg.bind_addr();
    let cfg_data = web::Data::new(cfg.clone());

    // Build the pool and migrate. This is the only startup path: a database
    // that cannot be reached, or a migration that fails, stops the process
    // before it listens. Never serve a partially migrated schema.
    if let Some(origin) = cfg.public_origin.as_deref() {
        info!(public_origin = %origin, "Public origin configured");
    }
    let pool = match db::prepare_database(&cfg.database_url).await {
        Ok(p) => {
            info!("Database pool initialised; migrations applied");
            Some(p)
        }
        Err(e) => {
            tracing::error!(
                error = %e,
                "cannot start: database is unreachable or a migration failed"
            );
            std::process::exit(1);
        }
    };

    // No host connection survives a process restart.
    if let Some(pool) = pool.as_ref()
        && let Err(e) = services::host_presence::mark_all_offline(pool).await
    {
        tracing::warn!(error = %e, "failed to reset host presence at startup");
    }

    // Recover runs that were mid-step when the process stopped. The create
    // intent is replayed with the same execution id, so a host answers with
    // the existing execution instead of running the prompt twice.
    if let Some(pool) = pool.as_ref() {
        match services::runs::recover_all(pool).await {
            Ok(report) => info!(
                scanned = report.scanned,
                replayed = report.replayed,
                completed = report.completed,
                interrupted = report.interrupted,
                deferred = report.deferred,
                "run recovery complete"
            ),
            Err(e) => tracing::warn!(error = %e, "run recovery failed"),
        }
    }

    // Apply the unattended first-run bootstrap, if configured. This only ever
    // inserts: an existing organization is left untouched, so a restart cannot
    // re-apply the temporary password.
    if let (Some(pool), Some(bootstrap)) = (pool.as_ref(), cfg.pool_bootstrap.as_ref()) {
        match services::pool_identity::bootstrap_from_env(pool, bootstrap).await {
            Ok(services::pool_identity::BootstrapOutcome::Created {
                organization,
                owner_email,
            }) => {
                info!(
                    organization = %organization,
                    owner_email = %owner_email,
                    "Pool organization bootstrapped from TIDEBOOT_*"
                );
            }
            Ok(services::pool_identity::BootstrapOutcome::AlreadyBootstrapped) => {
                info!("Pool already initialized; TIDEBOOT_* bootstrap skipped");
            }
            Ok(services::pool_identity::BootstrapOutcome::Disabled) => {}
            Err(e) => {
                tracing::warn!(error = %e, "Pool bootstrap from TIDEBOOT_* failed");
            }
        }
    }

    // Build the Redis connection manager — caching disabled if unavailable.
    // `services::redis::connect` bounds the handshake; a bare
    // `ConnectionManager::new` can retry forever against an unreachable server
    // and stall startup before the server ever binds.
    let redis_mgr = match services::redis::connect(&cfg.redis_url).await {
        Ok(mgr) => {
            info!("Redis connection manager initialised");
            Some(mgr)
        }
        Err(e) => {
            tracing::warn!(error = %e, "Redis unavailable; caching disabled");
            None
        }
    };

    // A pool that has a built control plane serves it from its own origin, so a
    // self-hosted install is one deployment with no CORS. Without the variable
    // the pool serves only its API and a standalone app deployment is unchanged.
    let control_plane = match ControlPlaneFiles::from_env() {
        Ok(files) => {
            if let Some(files) = files.as_ref() {
                info!(
                    dir = %files.root().display(),
                    "Serving the control plane from the pool's own origin"
                );
            }
            files
        }
        Err(e) => {
            tracing::error!(
                error = %e,
                "{CONTROL_PLANE_DIR_ENV} does not name a built control plane directory"
            );
            std::process::exit(1);
        }
    };

    info!("Starting Tidepool server on {}", bind);

    HttpServer::new(move || {
        let mut app = App::new()
            .wrap(TracingLogger::default())
            .wrap(rate_limit::RateLimit)
            .wrap(metrics::MetricsMiddleware)
            .wrap(problem::HubProblem)
            .wrap(request_id::RequestIdMiddleware)
            .app_data(cfg_data.clone());
        if let Some(r) = redis_mgr.clone() {
            // Rate limiting rides the same Redis manager; without it the
            // middleware finds no limiter app data and passes through.
            if cfg_data.rate_limit.enabled {
                info!(
                    requests = cfg_data.rate_limit.requests,
                    auth_requests = cfg_data.rate_limit.auth_requests,
                    "Rate limiting enabled on public routes"
                );
                app = app.app_data(web::Data::new(rate_limit::RateLimiter::new(
                    r.clone(),
                    cfg_data.rate_limit.clone(),
                )));
            }
            app = app.app_data(web::Data::new(r));
        }
        app = app
            .configure(api::healthz::config)
            .configure(api::auth::config)
            .configure(api::pool_identity::config)
            .configure(api::api_keys::config)
            .configure(api::hosts::config)
            .configure(api::connections::config)
            .configure(api::projects::config)
            .configure(api::triggers::config)
            .configure(api::manual_runs::config)
            .configure(api::activity::config)
            .configure(api::chatrooms::config)
            .configure(api::relay::config)
            .configure(api::webhooks::github::config)
            .configure(api::webhooks::slack::config)
            .configure(api::players::config)
            .configure(api::leaderboards::config)
            .configure(api::saves::config)
            .configure(api::game_settings::config)
            .configure(api::events::config)
            .configure(api::feedback::config)
            .configure(api::socket_ticket::config)
            .configure(api::metrics::config)
            .configure(api::openapi::config)
            .route("/ws", web::get().to(sockets::ws::ws_index))
            .route("/ws/host", web::get().to(sockets::host_ws::ws_host));
        if let Some(p) = pool.clone() {
            app = app.app_data(web::Data::new(p));
        }
        if let Some(files) = control_plane.clone() {
            app = app
                .app_data(web::Data::new(files))
                .default_service(web::to(serve_control_plane));
        }
        app
    })
    .bind(&bind)?
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, test as actix_test};
    use std::sync::atomic::{AtomicU64, Ordering};

    /// The built bundle a fixture server serves, byte for byte.
    const INDEX: &str = "<!doctype html><html><head><title>t</title></head><body><div id=\"app\"></div><script type=\"module\" src=\"/assets/app.js\"></script></body></html>";
    const APP_JS: &[u8] = b"console.log('tidepool control plane');\n";

    /// A temporary bundle directory, removed when the test ends.
    struct TempBundle {
        dir: PathBuf,
    }

    impl TempBundle {
        /// Create a bundle with one index, one asset and one manifest.
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let dir = std::env::temp_dir().join(format!(
                "tidepool-cp-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("assets")).unwrap();
            std::fs::write(dir.join("index.html"), INDEX).unwrap();
            std::fs::write(dir.join("assets/app.js"), APP_JS).unwrap();
            std::fs::write(dir.join("manifest.webmanifest"), br#"{"name":"t"}"#).unwrap();
            Self { dir }
        }

        /// The rooted bundle.
        fn files(&self) -> ControlPlaneFiles {
            ControlPlaneFiles::new(&self.dir).unwrap()
        }
    }

    impl Drop for TempBundle {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// A test service with the static bundle as its default route.
    macro_rules! bundle_app {
        ($bundle:expr) => {
            actix_test::init_service(
                App::new()
                    .app_data(web::Data::new($bundle.files()))
                    .default_service(web::to(serve_control_plane)),
            )
            .await
        };
    }

    #[actix_web::test]
    async fn serves_the_built_index_at_the_origin_root() {
        let bundle = TempBundle::new();
        let app = bundle_app!(bundle);

        let req = actix_test::TestRequest::get().uri("/").to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "text/html; charset=utf-8"
        );
        let body = actix_test::read_body(resp).await;
        let html = String::from_utf8(body.to_vec()).unwrap();
        assert!(html.contains("<div id=\"app\">"));
        assert!(html.contains("src=\"/assets/app.js\""));
    }

    #[actix_web::test]
    async fn serves_built_assets_byte_for_byte_with_their_type() {
        let bundle = TempBundle::new();
        let app = bundle_app!(bundle);

        let req = actix_test::TestRequest::get()
            .uri("/assets/app.js")
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "text/javascript; charset=utf-8"
        );
        let body = actix_test::read_body(resp).await;
        assert_eq!(body.as_ref(), APP_JS);

        let req = actix_test::TestRequest::get()
            .uri("/manifest.webmanifest")
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "application/json"
        );
    }

    #[actix_web::test]
    async fn a_missing_asset_is_a_404_and_a_missing_page_is_the_app() {
        let bundle = TempBundle::new();
        let app = bundle_app!(bundle);

        let req = actix_test::TestRequest::get()
            .uri("/assets/missing.js")
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);

        // The app is a hash router, so an unknown page is still index.html.
        let req = actix_test::TestRequest::get().uri("/settings").to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        let body = actix_test::read_body(resp).await;
        assert!(String::from_utf8_lossy(&body).contains("id=\"app\""));
    }

    #[actix_web::test]
    async fn does_not_serve_outside_the_bundle_or_over_the_api() {
        let bundle = TempBundle::new();
        let app = bundle_app!(bundle);

        for uri in [
            "/../Cargo.toml",
            "/%2e%2e%2fCargo.toml",
            "/assets/../../Cargo.toml",
        ] {
            let req = actix_test::TestRequest::get().uri(uri).to_request();
            let resp = actix_test::call_service(&app, req).await;
            assert_eq!(resp.status(), 404, "{uri} must not be served");
        }

        for uri in ["/v1/hosts", "/ws", "/healthz", "/metrics", "/openapi.json"] {
            let req = actix_test::TestRequest::get().uri(uri).to_request();
            let resp = actix_test::call_service(&app, req).await;
            assert_eq!(resp.status(), 404, "{uri} belongs to the pool");
        }
    }

    #[actix_web::test]
    async fn the_served_index_marks_the_pool_as_its_own_origin() {
        let bundle = TempBundle::new();
        let app = bundle_app!(bundle);

        let req = actix_test::TestRequest::get().uri("/").to_request();
        let resp = actix_test::call_service(&app, req).await;
        let body = actix_test::read_body(resp).await;
        let html = String::from_utf8(body.to_vec()).unwrap();
        assert!(html.contains("name=\"tidepool-pool-origin\" content=\"same-origin\""));
        // Everything else in the built page is untouched.
        let stripped = html.replace(
            "<meta name=\"tidepool-pool-origin\" content=\"same-origin\" />",
            "",
        );
        assert_eq!(stripped, INDEX);
        // Nor is the marker added twice if a bundle already carries it.
        assert_eq!(with_pool_origin_meta(&html), html);
    }

    #[actix_web::test]
    async fn the_app_loads_and_talks_to_the_serving_origin() {
        // The served page references the same asset the standalone build does,
        // and that asset is served, so both deployments run the same app.
        let bundle = TempBundle::new();
        let app = bundle_app!(bundle);

        let req = actix_test::TestRequest::get().uri("/").to_request();
        let resp = actix_test::call_service(&app, req).await;
        let html = String::from_utf8(actix_test::read_body(resp).await.to_vec()).unwrap();
        let src = html
            .split("src=\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .unwrap();
        assert_eq!(src, "/assets/app.js");

        let req = actix_test::TestRequest::get().uri(src).to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        assert_eq!(actix_test::read_body(resp).await.as_ref(), APP_JS);
    }

    #[actix_web::test]
    async fn serves_the_real_built_bundle_when_it_is_present() {
        // The control plane build is not part of the Rust gate (dist/ is
        // generated), so this runs only in a checkout that has built it. When
        // it does, the pool serves the exact bytes a standalone deployment
        // would: the app's entry asset and its markup are unchanged apart from
        // the same-origin marker.
        let bundle = Path::new("tidepool-control-plane/dist");
        if !bundle.join("index.html").is_file() {
            return;
        }
        let files = ControlPlaneFiles::new(bundle).unwrap();
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(files))
                .default_service(web::to(serve_control_plane)),
        )
        .await;

        let req = actix_test::TestRequest::get().uri("/").to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        let html = String::from_utf8(actix_test::read_body(resp).await.to_vec()).unwrap();
        assert!(html.contains("id=\"app\""));
        assert!(html.contains(SAME_ORIGIN_MARKER));

        let asset = html
            .split("src=\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("the built index references its entry script");
        let req = actix_test::TestRequest::get().uri(asset).to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert!(resp.status().is_success(), "{asset} must be served");
        let on_disk = std::fs::read(bundle.join(asset.trim_start_matches('/'))).unwrap();
        assert_eq!(
            actix_test::read_body(resp).await.as_ref(),
            on_disk.as_slice()
        );
    }

    #[test]
    fn refuses_a_directory_with_no_built_index() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "tidepool-cp-empty-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let err = ControlPlaneFiles::new(&dir).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolves_only_inside_the_bundle() {
        let bundle = TempBundle::new();
        let files = bundle.files();

        assert_eq!(
            files.resolve("/"),
            Some(std::fs::canonicalize(bundle.dir.join("index.html")).unwrap())
        );
        assert_eq!(
            files.resolve("/assets/app.js").unwrap(),
            std::fs::canonicalize(bundle.dir.join("assets/app.js")).unwrap()
        );
        assert!(files.resolve("/settings").is_some());
        assert!(files.resolve("/assets/missing.js").is_none());
        assert!(files.resolve("/../Cargo.toml").is_none());
        assert!(files.resolve("/assets/..").is_none());
        assert!(files.resolve("/assets//app.js").is_none());
        assert!(files.resolve("/a\\b.js").is_none());
    }

    #[test]
    fn reserved_paths_are_the_pools_own() {
        for path in [
            "/v1",
            "/v1/hosts",
            "/ws",
            "/ws/host",
            "/healthz",
            "/metrics",
        ] {
            assert!(is_reserved_path(path), "{path}");
        }
        for path in ["/", "/assets/app.js", "/settings"] {
            assert!(!is_reserved_path(path), "{path}");
        }
    }
}
