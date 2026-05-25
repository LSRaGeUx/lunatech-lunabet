use std::net::SocketAddr;

use anyhow::Context;
use axum::Router;
use axum_extra::extract::cookie::Key;
use sqlx::postgres::PgPoolOptions;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

mod config;
mod error;
mod fixtures;
mod football_data;
mod i18n;
mod mail;
mod models;
mod notifications;
mod routes;
mod scoring;
mod stakes;
mod state;

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

    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(&cfg.database_url)
        .await
        .context("connecting to Postgres")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("running migrations")?;

    if std::env::args().nth(1).as_deref() == Some("seed") {
        fixtures::seed(&pool).await.context("seeding fixtures")?;
        println!("\nDone. Lance ensuite `cargo run` puis ouvre http://localhost:3000/dev");
        return Ok(());
    }

    if std::env::args().nth(1).as_deref() == Some("notify") {
        // One-off: run the match-reminder job once and exit (for testing).
        let cookie_key = Key::from(&vec![0u8; 64]);
        let state = AppState {
            pool: pool.clone(),
            cookie_key,
            cfg: cfg.clone(),
            http: reqwest::Client::builder().user_agent("lunatech-betting/0.1").build()?,
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

    let app = Router::new()
        .merge(routes::router())
        .nest_service("/static", ServeDir::new("static"))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = cfg.bind_addr.parse().context("parsing BIND_ADDR")?;
    tracing::info!("listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
