use askama::Template;
use axum::extract::{FromRequestParts, Query, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::Form;
use axum_extra::extract::cookie::{Cookie, PrivateCookieJar, SameSite};
use base64::Engine;
use chrono::{Duration, Utc};
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::AppResult;
use crate::i18n::Locale;
use crate::mail;
use crate::models::User;
use crate::state::AppState;

const SESSION_COOKIE: &str = "lb_session";
const MAGIC_LINK_TTL_MINUTES: i64 = 15;
const SESSION_TTL_DAYS: i64 = 30;

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTpl<'a> {
    loc: Locale,
    error: Option<&'a str>,
}

#[derive(Template)]
#[template(path = "login_sent.html")]
struct LoginSentTpl {
    loc: Locale,
}

pub async fn login_page(_state: State<AppState>, loc: Locale) -> impl IntoResponse {
    let tpl = LoginTpl { loc, error: None };
    Html(tpl.render().unwrap_or_else(|e| format!("template error: {e}")))
}

#[derive(Deserialize)]
pub struct LoginForm {
    email: String,
}

pub async fn request_magic_link(
    State(state): State<AppState>,
    loc: Locale,
    Form(form): Form<LoginForm>,
) -> AppResult<Response> {
    let email = form.email.trim().to_lowercase();
    let domain = email.split_once('@').map(|(_, d)| d);
    let allowed = domain
        .map(|d| state.cfg.allowed_email_domain_pattern.is_match(d))
        .unwrap_or(false);
    if !allowed {
        let tpl = LoginTpl {
            loc,
            error: Some(loc.f(
                "Cette app est réservée aux emails Lunatech.",
                "This app is reserved for Lunatech emails.",
            )),
        };
        return Ok((StatusCode::BAD_REQUEST, Html(tpl.render()?)).into_response());
    }

    let mut raw = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut raw);
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw);
    let token_hash = hex_sha256(&token);
    let expires_at = Utc::now() + Duration::minutes(MAGIC_LINK_TTL_MINUTES);

    sqlx::query("INSERT INTO magic_links (token_hash, email, expires_at) VALUES ($1, $2, $3)")
        .bind(&token_hash)
        .bind(&email)
        .bind(expires_at)
        .execute(&state.pool)
        .await?;

    let link = format!("{}/auth/callback?token={}", state.cfg.base_url.trim_end_matches('/'), token);

    if let Err(e) = mail::send_magic_link(&state.cfg, loc, &email, &link).await {
        tracing::warn!("could not send magic link email to {email}: {e:#}");
        tracing::info!("DEV magic link for {email}: {link}");
    }

    Ok(Redirect::to("/login/sent").into_response())
}

pub async fn login_sent(loc: Locale) -> impl IntoResponse {
    Html(LoginSentTpl { loc }.render().unwrap_or_else(|e| format!("template error: {e}")))
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    token: String,
}

pub async fn callback(
    State(state): State<AppState>,
    loc: Locale,
    jar: PrivateCookieJar,
    Query(q): Query<CallbackQuery>,
) -> AppResult<Response> {
    let token_hash = hex_sha256(&q.token);

    let row: Option<(String, chrono::DateTime<Utc>, Option<chrono::DateTime<Utc>>)> =
        sqlx::query_as("SELECT email, expires_at, consumed_at FROM magic_links WHERE token_hash = $1")
            .bind(&token_hash)
            .fetch_optional(&state.pool)
            .await?;

    let Some((email, expires_at, consumed_at)) = row else {
        return Ok((StatusCode::BAD_REQUEST, loc.f("Lien invalide.", "Invalid link.")).into_response());
    };
    if consumed_at.is_some() {
        return Ok((StatusCode::BAD_REQUEST, loc.f("Ce lien a déjà été utilisé.", "This link has already been used.")).into_response());
    }
    if expires_at < Utc::now() {
        return Ok((StatusCode::BAD_REQUEST, loc.f("Ce lien a expiré.", "This link has expired.")).into_response());
    }

    sqlx::query("UPDATE magic_links SET consumed_at = NOW() WHERE token_hash = $1")
        .bind(&token_hash)
        .execute(&state.pool)
        .await?;

    let display_name = email
        .split('@')
        .next()
        .unwrap_or(&email)
        .replace('.', " ")
        .replace('_', " ");

    let is_admin = state.cfg.admin_emails.contains(&email);
    let user: User = sqlx::query_as(
        r#"
        INSERT INTO users (email, display_name, is_admin)
        VALUES ($1, $2, $3)
        ON CONFLICT (email) DO UPDATE
            SET email = EXCLUDED.email,
                is_admin = EXCLUDED.is_admin
        RETURNING id, email, display_name, is_admin, created_at,
                  stake_eur, stake_chosen_at, paid_at
        "#,
    )
    .bind(&email)
    .bind(&display_name)
    .bind(is_admin)
    .fetch_one(&state.pool)
    .await?;

    let session_id = Uuid::new_v4();
    let session_expires = Utc::now() + Duration::days(SESSION_TTL_DAYS);
    sqlx::query("INSERT INTO sessions (id, user_id, expires_at) VALUES ($1, $2, $3)")
        .bind(session_id)
        .bind(user.id)
        .bind(session_expires)
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

pub async fn logout(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
) -> AppResult<Response> {
    if let Some(c) = jar.get(SESSION_COOKIE) {
        if let Ok(id) = Uuid::parse_str(c.value()) {
            let _ = sqlx::query("DELETE FROM sessions WHERE id = $1")
                .bind(id)
                .execute(&state.pool)
                .await;
        }
    }
    let jar = jar.remove(Cookie::from(SESSION_COOKIE));
    Ok((jar, Redirect::to("/")).into_response())
}

pub async fn current_user(state: &AppState, jar: &PrivateCookieJar) -> AppResult<Option<User>> {
    let Some(c) = jar.get(SESSION_COOKIE) else {
        return Ok(None);
    };
    let Ok(id) = Uuid::parse_str(c.value()) else {
        return Ok(None);
    };
    let user: Option<User> = sqlx::query_as(
        r#"
        SELECT u.id, u.email, u.display_name, u.is_admin, u.created_at,
               u.stake_eur, u.stake_chosen_at, u.paid_at
        FROM sessions s
        JOIN users u ON u.id = s.user_id
        WHERE s.id = $1 AND s.expires_at > NOW()
        "#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    Ok(user)
}

pub struct AuthUser(pub User);

#[axum::async_trait]
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let jar = PrivateCookieJar::<axum_extra::extract::cookie::Key>::from_request_parts(parts, state)
            .await
            .map_err(|e| e.into_response())?;
        match current_user(state, &jar).await {
            Ok(Some(u)) => Ok(AuthUser(u)),
            Ok(None) => Err(Redirect::to("/login").into_response()),
            Err(e) => Err(e.into_response()),
        }
    }
}

fn hex_sha256(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let out = hasher.finalize();
    let mut s = String::with_capacity(out.len() * 2);
    for b in out {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
    }
    s
}
