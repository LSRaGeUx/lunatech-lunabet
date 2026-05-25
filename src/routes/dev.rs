use askama::Template;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::{Cookie, PrivateCookieJar, SameSite};
use chrono::{Duration, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::error::AppResult;
use crate::i18n::Locale;
use crate::state::AppState;

const SESSION_COOKIE: &str = "lb_session";
const SESSION_TTL_DAYS: i64 = 30;

struct DevUser {
    email: String,
    display_name: String,
    points: i64,
}

#[derive(Template)]
#[template(path = "dev.html")]
struct DevTpl {
    loc: Locale,
    users: Vec<DevUser>,
}

pub async fn index(State(state): State<AppState>, loc: Locale) -> AppResult<Response> {
    if !state.cfg.dev_mode {
        return Ok((StatusCode::NOT_FOUND, "Dev mode disabled.").into_response());
    }

    let users: Vec<(String, String, Option<i64>)> = sqlx::query_as(
        r#"
        SELECT u.email, u.display_name, COALESCE(SUM(b.points), 0)::BIGINT
        FROM users u
        LEFT JOIN bets b ON b.user_id = u.id
        GROUP BY u.email, u.display_name
        ORDER BY u.display_name ASC
        "#,
    )
    .fetch_all(&state.pool)
    .await?;

    let users = users
        .into_iter()
        .map(|(email, display_name, points)| DevUser {
            email,
            display_name,
            points: points.unwrap_or(0),
        })
        .collect();

    Ok(Html(DevTpl { loc, users }.render()?).into_response())
}

#[derive(Deserialize)]
pub struct LoginQuery {
    email: String,
}

pub async fn login_as(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Query(q): Query<LoginQuery>,
) -> AppResult<Response> {
    if !state.cfg.dev_mode {
        return Ok((StatusCode::NOT_FOUND, "Dev mode disabled.").into_response());
    }

    let email = q.email.trim().to_lowercase();

    let user_id: Option<Uuid> = sqlx::query_scalar("SELECT id FROM users WHERE email = $1")
        .bind(&email)
        .fetch_optional(&state.pool)
        .await?;

    let Some(user_id) = user_id else {
        return Ok((StatusCode::NOT_FOUND, "Utilisateur introuvable. Lance d'abord `cargo run -- seed`.").into_response());
    };

    // sync admin flag from ADMIN_EMAILS on each dev login
    let is_admin = state.cfg.admin_emails.contains(&email);
    sqlx::query("UPDATE users SET is_admin = $1 WHERE id = $2")
        .bind(is_admin)
        .bind(user_id)
        .execute(&state.pool)
        .await?;

    let session_id = Uuid::new_v4();
    let expires_at = Utc::now() + Duration::days(SESSION_TTL_DAYS);
    sqlx::query("INSERT INTO sessions (id, user_id, expires_at) VALUES ($1, $2, $3)")
        .bind(session_id)
        .bind(user_id)
        .bind(expires_at)
        .execute(&state.pool)
        .await?;

    let cookie = Cookie::build((SESSION_COOKIE, session_id.to_string()))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::days(SESSION_TTL_DAYS))
        .build();
    let jar = jar.add(cookie);

    Ok((jar, Redirect::to("/matches")).into_response())
}
