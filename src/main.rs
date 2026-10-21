use actix_web::{App, HttpServer, web};
use tidepool::{
    api,
    config::Config,
    db,
    middleware::{metrics, problem, rate_limit, request_id},
    services, sockets,
};
use tracing::info;
use tracing_actix_web::TracingLogger;

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

    // Build the DB pool — degraded mode if unavailable.
    let pool = match db::build_pool(&cfg.database_url).await {
        Ok(p) => {
            info!("Database pool initialised");
            Some(p)
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to initialise DB pool; continuing in degraded mode");
            None
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
        app
    })
    .bind(&bind)?
    .run()
    .await
}
