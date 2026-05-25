use askama::Template;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::Form;
use axum_extra::extract::cookie::{Cookie, PrivateCookieJar, SameSite};
use base64::Engine;
use chrono::{Duration, Utc};
use rand::RngCore;
use regex::Regex;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::AppResult;
use crate::i18n::Locale;
use crate::mail;
use crate::state::AppState;
use crate::tenant::{MaybeTenant, Tenant};

const SESSION_COOKIE: &str = "lb_session";
const SIGNUP_TOKEN_TTL_MINUTES: i64 = 30;
const SESSION_TTL_DAYS: i64 = 30;

#[derive(Template)]
#[template(path = "signup.html")]
struct SignupTpl<'a> {
    loc: Locale,
    tenant: &'a Tenant,
    form: SignupValues,
    error: Option<&'a str>,
}

#[derive(Default, Clone)]
struct SignupValues {
    slug: String,
    name: String,
    owner_email: String,
    owner_name: String,
}

#[derive(Template)]
#[template(path = "signup_sent.html")]
struct SignupSentTpl<'a> {
    loc: Locale,
    tenant: &'a Tenant,
    owner_email: String,
}

pub async fn form(
    State(_state): State<AppState>,
    MaybeTenant(maybe_tenant): MaybeTenant,
    loc: Locale,
) -> impl IntoResponse {
    let tenant = maybe_tenant.unwrap_or_else(Tenant::platform);
    let tpl = SignupTpl {
        loc,
        tenant: &tenant,
        form: SignupValues::default(),
        error: None,
    };
    Html(tpl.render().unwrap_or_else(|e| format!("template error: {e}")))
}

#[derive(Deserialize)]
pub struct SignupForm {
    slug: String,
    name: String,
    owner_email: String,
    owner_name: String,
}

fn valid_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 40
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        // Reserve names that conflict with our routes / common subdomains.
        && !matches!(
            s,
            "www" | "admin" | "api" | "static" | "auth" | "login" | "signup"
        )
}

fn domain_of(email: &str) -> Option<&str> {
    email.split_once('@').map(|(_, d)| d)
}

pub async fn submit(
    State(state): State<AppState>,
    MaybeTenant(maybe_tenant): MaybeTenant,
    loc: Locale,
    Form(form): Form<SignupForm>,
) -> AppResult<Response> {
    let tenant = maybe_tenant.unwrap_or_else(Tenant::platform);
    let slug = form.slug.trim().to_lowercase();
    let name = form.name.trim().to_string();
    let owner_email = form.owner_email.trim().to_lowercase();
    let owner_name = form.owner_name.trim().to_string();

    let values = SignupValues {
        slug: slug.clone(),
        name: name.clone(),
        owner_email: owner_email.clone(),
        owner_name: owner_name.clone(),
    };
    let render_error = |msg: &str| {
        let tpl = SignupTpl {
            loc,
            tenant: &tenant,
            form: values.clone(),
            error: Some(msg),
        };
        Html(tpl.render().unwrap_or_else(|e| format!("template error: {e}")))
    };

    if !valid_slug(&slug) {
        return Ok((
            StatusCode::BAD_REQUEST,
            render_error("Slug invalide : lettres minuscules, chiffres, '-' et '_' uniquement."),
        )
            .into_response());
    }
    if name.is_empty() {
        return Ok((StatusCode::BAD_REQUEST, render_error("Nom requis.")).into_response());
    }
    if owner_name.is_empty() {
        return Ok((StatusCode::BAD_REQUEST, render_error("Nom du référent requis.")).into_response());
    }
    let Some(domain) = domain_of(&owner_email) else {
        return Ok((StatusCode::BAD_REQUEST, render_error("Email du référent invalide.")).into_response());
    };

    // Derive the allowed email pattern from the owner's domain by default.
    // The slug must not already exist (in tenants or pending_tenants).
    let pattern = regex::escape(domain);
    // Sanity-check the regex compiles before storing.
    if Regex::new(&format!("^(?:{})$", pattern)).is_err() {
        return Ok((
            StatusCode::BAD_REQUEST,
            render_error("Domaine email invalide."),
        )
            .into_response());
    }

    let taken: Option<bool> = sqlx::query_scalar(
        "SELECT EXISTS (\
           SELECT 1 FROM tenants WHERE slug = $1 \
           UNION SELECT 1 FROM pending_tenants WHERE slug = $1 AND consumed_at IS NULL AND expires_at > NOW())",
    )
    .bind(&slug)
    .fetch_one(&state.pool)
    .await?;
    if taken.unwrap_or(false) {
        return Ok((StatusCode::CONFLICT, render_error("Ce slug est déjà pris.")).into_response());
    }

    // Generate the verification token. We store only its hash, the link
    // carries the raw value.
    let mut raw = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut raw);
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw);
    let token_hash = hex_sha256(&token);
    let expires_at = Utc::now() + Duration::minutes(SIGNUP_TOKEN_TTL_MINUTES);

    sqlx::query(
        r#"
        INSERT INTO pending_tenants
            (token_hash, slug, name, owner_email, owner_name,
             allowed_email_pattern, expires_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(&token_hash)
    .bind(&slug)
    .bind(&name)
    .bind(&owner_email)
    .bind(&owner_name)
    .bind(&pattern)
    .bind(expires_at)
    .execute(&state.pool)
    .await?;

    let link = format!(
        "{}/signup/verify?token={}",
        state.cfg.base_url.trim_end_matches('/'),
        token
    );

    if let Err(e) = mail::send_signup_verification(
        &state.cfg,
        &tenant,
        loc,
        &owner_email,
        &owner_name,
        &name,
        &link,
    )
    .await
    {
        tracing::warn!("signup verification email to {owner_email} failed: {e:#}");
        tracing::info!("DEV signup link for {owner_email}: {link}");
    }

    let tpl = SignupSentTpl {
        loc,
        tenant: &tenant,
        owner_email,
    };
    Ok(Html(tpl.render()?).into_response())
}

#[derive(Deserialize)]
pub struct VerifyQuery {
    token: String,
}

pub async fn verify(
    State(state): State<AppState>,
    loc: Locale,
    jar: PrivateCookieJar,
    Query(q): Query<VerifyQuery>,
) -> AppResult<Response> {
    let token_hash = hex_sha256(&q.token);

    let row: Option<(
        String,
        String,
        String,
        String,
        String,
        chrono::DateTime<Utc>,
        Option<chrono::DateTime<Utc>>,
    )> = sqlx::query_as(
        "SELECT slug, name, owner_email, owner_name, allowed_email_pattern, \
                expires_at, consumed_at \
         FROM pending_tenants WHERE token_hash = $1",
    )
    .bind(&token_hash)
    .fetch_optional(&state.pool)
    .await?;

    let Some((slug, name, owner_email, owner_name, pattern, expires_at, consumed_at)) = row
    else {
        return Ok((StatusCode::BAD_REQUEST, loc.f("Lien invalide.", "Invalid link.")).into_response());
    };
    if consumed_at.is_some() {
        return Ok((
            StatusCode::BAD_REQUEST,
            loc.f(
                "Ce lien a déjà été utilisé.",
                "This link has already been used.",
            ),
        )
            .into_response());
    }
    if expires_at < Utc::now() {
        return Ok((StatusCode::BAD_REQUEST, loc.f("Ce lien a expiré.", "This link has expired.")).into_response());
    }

    // Create the tenant + the owner user + a session for them, all in one
    // transaction. If anything fails, nothing is half-created.
    let mut tx = state.pool.begin().await?;

    let tenant_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO tenants
            (slug, name, allowed_email_pattern, mail_from, stake_deadline,
             admin_emails)
        VALUES ($1, $2, $3, $4, '2026-06-27T23:59:00Z'::timestamptz, $5)
        RETURNING id
        "#,
    )
    .bind(&slug)
    .bind(&name)
    .bind(&pattern)
    .bind(format!("lunabet@{}", slug))
    .bind(vec![owner_email.clone()])
    .fetch_one(&mut *tx)
    .await?;

    let user_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO users (tenant_id, email, display_name, is_admin)
        VALUES ($1, $2, $3, TRUE)
        RETURNING id
        "#,
    )
    .bind(tenant_id)
    .bind(&owner_email)
    .bind(&owner_name)
    .fetch_one(&mut *tx)
    .await?;

    let session_id = Uuid::new_v4();
    let session_expires = Utc::now() + Duration::days(SESSION_TTL_DAYS);
    sqlx::query(
        "INSERT INTO sessions (id, tenant_id, user_id, expires_at) VALUES ($1, $2, $3, $4)",
    )
    .bind(session_id)
    .bind(tenant_id)
    .bind(user_id)
    .bind(session_expires)
    .execute(&mut *tx)
    .await?;

    sqlx::query("UPDATE pending_tenants SET consumed_at = NOW() WHERE token_hash = $1")
        .bind(&token_hash)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let cookie = Cookie::build((SESSION_COOKIE, session_id.to_string()))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::days(SESSION_TTL_DAYS))
        .build();
    let jar = jar.add(cookie);

    Ok((jar, Redirect::to("/matches")).into_response())
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
