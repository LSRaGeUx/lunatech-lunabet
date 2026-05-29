use std::net::SocketAddr;

use anyhow::Context;
use axum::Router;
use axum_extra::extract::cookie::Key;
use sqlx::postgres::PgPoolOptions;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

mod characters;
mod config;
mod error;
mod fixtures;
mod football_data;
mod i18n;
mod mail;
mod middleware;
mod models;
mod notifications;
mod rate_limit;
mod routes;
mod scoring;
mod stakes;
mod state;
mod tenant;

use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "lunatech_betting=info,tower_http=info".into()),
        )
        .init();

    let cfg = config::Config::from_env().context("loading configuration from env")?;

    // No after_connect hook. RLS policies are still installed (0525_07)
    // but FORCE is off (0529_02 / _04), so the table-owner role our app
    // connects with bypasses them naturally — same effective behaviour
    // as the explicit `app.bypass_rls = on` we used before, one less
    // moving piece to worry about in production.
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(&cfg.database_url)
        .await
        .context("connecting to Postgres")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("running migrations")?;

    let default_tenant = tenant::ensure_default(&pool, &cfg)
        .await
        .context("ensuring default tenant exists")?;
    tracing::info!(slug = %default_tenant.slug, id = %default_tenant.id, "default tenant loaded");
    let tenants = tenant::TenantRegistry::new(pool.clone(), default_tenant.clone());

    if std::env::args().nth(1).as_deref() == Some("seed") {
        fixtures::seed(&pool, &default_tenant).await.context("seeding fixtures")?;
        println!("\nDone. Lance ensuite `cargo run` puis ouvre http://localhost:3000/dev");
        return Ok(());
    }

    let signup_limiter =
        rate_limit::SignupRateLimiter::new(std::time::Duration::from_secs(3600), 5);

    if std::env::args().nth(1).as_deref() == Some("notify") {
        // One-off: run the match-reminder job once and exit (for testing).
        let cookie_key = Key::from(&vec![0u8; 64]);
        let state = AppState {
            pool: pool.clone(),
            cookie_key,
            cfg: cfg.clone(),
            http: reqwest::Client::builder().user_agent("lunatech-betting/0.1").build()?,
            tenants: tenants.clone(),
            signup_limiter: signup_limiter.clone(),
        };
        notifications::send_match_reminders(&state)
            .await
            .context("running match reminders")?;
        println!("Match reminder job completed.");
        return Ok(());
    }

    let cookie_key = Key::from(&cfg.cookie_key_bytes);

    let state = AppState {
        pool,
        cookie_key,
        cfg: cfg.clone(),
        http: reqwest::Client::builder()
            .user_agent("lunatech-betting/0.1")
            .build()?,
        tenants,
        signup_limiter,
    };

    if cfg.football_data_api_key.is_some() {
        let s = state.clone();
        tokio::spawn(async move {
            if let Err(e) = football_data::sync_fixtures(&s).await {
                tracing::warn!("initial fixtures sync failed: {e:#}");
            }
        });
    }

    {
        let s = state.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(300));
            ticker.tick().await;
            loop {
                ticker.tick().await;
                if let Err(e) = football_data::sync_fixtures(&s).await {
                    tracing::warn!("fixtures sync failed: {e:#}");
                }
                if let Err(e) = scoring::recompute_all(&s.pool).await {
                    tracing::warn!("scoring recompute failed: {e:#}");
                }
                if let Err(e) = notifications::send_match_reminders(&s).await {
                    tracing::warn!("match reminders failed: {e:#}");
                }
            }
        });
    }

    // Hourly cleanup: drop pending signups that expired more than a day ago
    // and shrink the in-memory signup rate limiter's bucket map.
    {
        let s = state.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(3600));
            ticker.tick().await;
            loop {
                ticker.tick().await;
                let res = sqlx::query(
                    "DELETE FROM pending_tenants \
                     WHERE consumed_at IS NULL AND expires_at < NOW() - INTERVAL '1 day'",
                )
                .execute(&s.pool)
                .await;
                match res {
                    Ok(r) if r.rows_affected() > 0 => tracing::info!(
                        "purged {} expired pending tenant signups",
                        r.rows_affected()
                    ),
                    Ok(_) => {}
                    Err(e) => tracing::warn!("pending_tenants cleanup failed: {e:#}"),
                }
                s.signup_limiter.purge_empty();
            }
        });
    }

    let app = Router::new()
        .merge(routes::router())
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::resolve_tenant,
        ))
        .nest_service("/static", ServeDir::new("static"))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = cfg.bind_addr.parse().context("parsing BIND_ADDR")?;
    tracing::info!("listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
