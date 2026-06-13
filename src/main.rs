use std::net::SocketAddr;

use anyhow::Context;
use axum::Router;
use axum_extra::extract::cookie::Key;
use chrono::Timelike;
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
    let endpoint_limiter =
        rate_limit::EndpointRateLimiter::new(std::time::Duration::from_secs(60), 10);

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
            endpoint_limiter: endpoint_limiter.clone(),
        };
        notifications::send_match_reminders(&state)
            .await
            .context("running match reminders")?;
        println!("Match reminder job completed.");
        return Ok(());
    }

    if std::env::args().nth(1).as_deref() == Some("daily-digest") {
        // One-off: send the daily recap once and exit. Optional date arg
        // (YYYY-MM-DD, UTC); defaults to yesterday.
        let cookie_key = Key::from(&vec![0u8; 64]);
        let state = AppState {
            pool: pool.clone(),
            cookie_key,
            cfg: cfg.clone(),
            http: reqwest::Client::builder().user_agent("lunatech-betting/0.1").build()?,
            tenants: tenants.clone(),
            signup_limiter: signup_limiter.clone(),
            endpoint_limiter: endpoint_limiter.clone(),
        };
        let date = std::env::args()
            .nth(2)
            .and_then(|s| chrono::NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok())
            .unwrap_or_else(|| (chrono::Utc::now() - chrono::Duration::days(1)).date_naive());
        notifications::send_daily_digest(&state, date)
            .await
            .context("running daily digest")?;
        println!("Daily digest job completed for {date}.");
        return Ok(());
    }

    if std::env::args().nth(1).as_deref() == Some("today-matches") {
        // One-off: send the "today's matches" preview once and exit. Optional
        // date arg (YYYY-MM-DD, UTC); defaults to today.
        let cookie_key = Key::from(&vec![0u8; 64]);
        let state = AppState {
            pool: pool.clone(),
            cookie_key,
            cfg: cfg.clone(),
            http: reqwest::Client::builder().user_agent("lunatech-betting/0.1").build()?,
            tenants: tenants.clone(),
            signup_limiter: signup_limiter.clone(),
            endpoint_limiter: endpoint_limiter.clone(),
        };
        let date = std::env::args()
            .nth(2)
            .and_then(|s| chrono::NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok())
            .unwrap_or_else(|| chrono::Utc::now().date_naive());
        notifications::send_today_matches(&state, date)
            .await
            .context("running today's matches email")?;
        println!("Today's matches email job completed for {date}.");
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
        endpoint_limiter,
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
    // and shrink the in-memory rate limiter bucket maps.
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
                s.endpoint_limiter.purge_empty();
            }
        });
    }

    // Daily recap email: from DAILY_DIGEST_HOUR (UTC) each morning, send the
    // previous day's digest once. The check ticks every 15 min; send_daily_digest
    // is idempotent per (tenant, day) via the daily_digests table, so repeated
    // ticks after the hour are harmless no-ops.
    {
        let s = state.clone();
        let digest_hour = cfg.daily_digest_hour;
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(900));
            ticker.tick().await;
            loop {
                ticker.tick().await;
                let now = chrono::Utc::now();
                if now.hour() < digest_hour {
                    continue;
                }
                let Some(yesterday) = now.date_naive().pred_opt() else {
                    continue;
                };
                if let Err(e) = notifications::send_daily_digest(&s, yesterday).await {
                    tracing::warn!("daily digest failed: {e:#}");
                }
                // Same morning slot: preview of today's matches.
                if let Err(e) = notifications::send_today_matches(&s, now.date_naive()).await {
                    tracing::warn!("today's matches email failed: {e:#}");
                }
            }
        });
    }

    // User-uploaded assets (currently tenant logos) live outside the bundled
    // `static/` tree so they survive redeploys and are never clobbered by the
    // shipped files. Created on boot so ServeDir always has a directory.
    std::fs::create_dir_all(&cfg.uploads_dir)
        .with_context(|| format!("creating uploads dir {}", cfg.uploads_dir))?;

    let app = Router::new()
        .merge(routes::router())
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::resolve_tenant,
        ))
        .nest_service("/static", ServeDir::new("static"))
        .nest_service("/uploads", ServeDir::new(&cfg.uploads_dir))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = cfg.bind_addr.parse().context("parsing BIND_ADDR")?;
    tracing::info!("listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
