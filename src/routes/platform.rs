// Platform-level admin (the "super super admin" Nicolas asked for).
// Lives only at the apex, has its own magic-link flow not tied to any
// tenant, and reads a separate session cookie.

use askama::Template;
use axum::extract::{Query, State};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::Form;
use axum_extra::extract::cookie::{Cookie, Key, PrivateCookieJar, SameSite};
use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::AppResult;
use crate::i18n::Locale;
use crate::mail;
use crate::notifications;
use crate::state::AppState;
use crate::tenant::ApexOnly;

const SESSION_COOKIE: &str = "lb_platform_session";
const MAGIC_LINK_TTL_MINUTES: i64 = 15;
const SESSION_TTL_DAYS: i64 = 14;

// ---------- templates ----------

#[derive(Template)]
#[template(path = "platform_admin_login.html")]
struct LoginTpl<'a> {
    loc: Locale,
    error: Option<&'a str>,
    prefilled_email: &'a str,
}

#[derive(Template)]
#[template(path = "platform_admin_login_sent.html")]
struct LoginSentTpl {
    loc: Locale,
}

#[derive(Template)]
#[template(path = "platform_admin_dashboard.html")]
struct DashboardTpl<'a> {
    loc: Locale,
    admin_email: &'a str,
    tenants: Vec<TenantRow>,
    total_users: i64,
    total_tenants: i64,
    /// Slug of the env-bootstrapped tenant; the dashboard hides the delete
    /// button on that row to prevent an accidental wipe.
    default_slug: &'a str,
    /// One-shot banner after a manual email trigger: "results" or "today".
    sent: Option<String>,
}

pub struct TenantRow {
    pub slug: String,
    pub name: String,
    pub allowed_email_pattern: String,
    pub created_at: DateTime<Utc>,
    pub user_count: i64,
    pub bet_count: i64,
    pub paid_count: i64,
}

// ---------- session ----------

/// Extractor: yields the platform admin's email when the request carries a
/// valid `lb_platform_session` cookie AND that email is still on the
/// `SUPER_ADMIN_EMAILS` allowlist. Apex-only enforcement comes from
/// `ApexOnly`, which the routes also take.
pub struct PlatformAdmin {
    pub email: String,
}

#[axum::async_trait]
impl FromRequestParts<AppState> for PlatformAdmin {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        // Block tenant subdomains from even hitting the auth check.
        if parts.extensions.get::<crate::tenant::Tenant>().is_some() {
            return Err((StatusCode::NOT_FOUND, "Not found.").into_response());
        }
        let jar = PrivateCookieJar::<Key>::from_request_parts(parts, state)
            .await
            .map_err(|e| e.into_response())?;
        let Some(c) = jar.get(SESSION_COOKIE) else {
            return Err(Redirect::to("/super-admin/login").into_response());
        };
        let email = c.value().to_lowercase();
        if !state.cfg.super_admin_emails.contains(&email) {
            return Err((StatusCode::FORBIDDEN, "Not authorised.").into_response());
        }
        Ok(PlatformAdmin { email })
    }
}

// ---------- handlers ----------

pub async fn login_page(_apex: ApexOnly, loc: Locale) -> impl IntoResponse {
    let tpl = LoginTpl {
        loc,
        error: None,
        prefilled_email: "",
    };
    Html(tpl.render().unwrap_or_else(|e| format!("template error: {e}")))
}

#[derive(Deserialize)]
pub struct LoginForm {
    email: String,
}

pub async fn request_link(
    State(state): State<AppState>,
    _apex: ApexOnly,
    loc: Locale,
    Form(form): Form<LoginForm>,
) -> AppResult<Response> {
    let email = form.email.trim().to_lowercase();

    // Always render the "check your inbox" page, even if the email isn't on
    // the allowlist. This stops an attacker probing for valid super-admin
    // emails: the response shape is identical whether we sent something or
    // not.
    let render_sent = || {
        let tpl = LoginSentTpl { loc };
        Html(tpl.render().unwrap_or_else(|e| format!("template error: {e}")))
    };

    if !state.cfg.super_admin_emails.contains(&email) {
        tracing::warn!(email = %email, "platform admin login attempt with unknown address");
        return Ok(render_sent().into_response());
    }

    let mut raw = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut raw);
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw);
    let token_hash = hex_sha256(&token);
    let expires_at = Utc::now() + Duration::minutes(MAGIC_LINK_TTL_MINUTES);

    sqlx::query(
        "INSERT INTO platform_magic_links (token_hash, email, expires_at) VALUES ($1, $2, $3)",
    )
    .bind(&token_hash)
    .bind(&email)
    .bind(expires_at)
    .execute(&state.pool)
    .await?;

    let base = state
        .cfg
        .platform_url
        .clone()
        .unwrap_or_else(|| state.cfg.base_url.clone());
    let link = format!(
        "{}/super-admin/auth/callback?token={}",
        base.trim_end_matches('/'),
        token
    );

    if let Err(e) = mail::send_platform_magic_link(&state.cfg, loc, &base, &email, &link).await {
        tracing::warn!("platform magic link to {email} failed: {e:#}");
        tracing::info!("DEV platform magic link for {email}: {link}");
    }

    Ok(render_sent().into_response())
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    token: String,
}

pub async fn callback(
    State(state): State<AppState>,
    _apex: ApexOnly,
    jar: PrivateCookieJar,
    Query(q): Query<CallbackQuery>,
) -> AppResult<Response> {
    let token_hash = hex_sha256(&q.token);

    let row: Option<(String, DateTime<Utc>, Option<DateTime<Utc>>)> = sqlx::query_as(
        "SELECT email, expires_at, consumed_at \
         FROM platform_magic_links WHERE token_hash = $1",
    )
    .bind(&token_hash)
    .fetch_optional(&state.pool)
    .await?;

    let Some((email, expires_at, consumed_at)) = row else {
        return Ok((StatusCode::BAD_REQUEST, "Lien invalide.").into_response());
    };
    if consumed_at.is_some() {
        return Ok((StatusCode::BAD_REQUEST, "Ce lien a déjà été utilisé.").into_response());
    }
    if expires_at < Utc::now() {
        return Ok((StatusCode::BAD_REQUEST, "Ce lien a expiré.").into_response());
    }
    // Re-check the allowlist in case the env changed since the link was sent.
    if !state.cfg.super_admin_emails.contains(&email) {
        return Ok((StatusCode::FORBIDDEN, "Not authorised.").into_response());
    }

    sqlx::query("UPDATE platform_magic_links SET consumed_at = NOW() WHERE token_hash = $1")
        .bind(&token_hash)
        .execute(&state.pool)
        .await?;

    // Cookie is path-scoped to /super-admin so tenant pages never see it.
    let mut builder = Cookie::build((SESSION_COOKIE, email))
        .path("/super-admin")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::days(SESSION_TTL_DAYS));
    if let Some(domain) = state.cfg.cookie_domain() {
        builder = builder.domain(domain);
    }
    let jar = jar.add(builder.build());

    Ok((jar, Redirect::to("/super-admin/")).into_response())
}

#[derive(Deserialize)]
pub struct DashboardQuery {
    /// Set by the manual email triggers to flash a confirmation banner.
    sent: Option<String>,
}

pub async fn dashboard(
    State(state): State<AppState>,
    PlatformAdmin { email }: PlatformAdmin,
    loc: Locale,
    Query(q): Query<DashboardQuery>,
) -> AppResult<Response> {
    let rows: Vec<(
        Uuid,
        String,
        String,
        String,
        DateTime<Utc>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
    )> = sqlx::query_as(
        r#"
        SELECT
            t.id,
            t.slug,
            t.name,
            t.allowed_email_pattern,
            t.created_at,
            (SELECT COUNT(*) FROM users u WHERE u.tenant_id = t.id)::BIGINT,
            (SELECT COUNT(*) FROM bets  b WHERE b.tenant_id = t.id)::BIGINT,
            (SELECT COUNT(*) FROM users u WHERE u.tenant_id = t.id AND u.paid_at IS NOT NULL)::BIGINT
        FROM tenants t
        ORDER BY t.created_at DESC
        "#,
    )
    .fetch_all(&state.pool)
    .await?;

    let mut total_users: i64 = 0;
    let tenants: Vec<TenantRow> = rows
        .into_iter()
        .map(|(_, slug, name, pattern, created, uc, bc, pc)| {
            let user_count = uc.unwrap_or(0);
            total_users += user_count;
            TenantRow {
                slug,
                name,
                allowed_email_pattern: pattern,
                created_at: created,
                user_count,
                bet_count: bc.unwrap_or(0),
                paid_count: pc.unwrap_or(0),
            }
        })
        .collect();

    let total_tenants = tenants.len() as i64;

    let tpl = DashboardTpl {
        loc,
        admin_email: &email,
        tenants,
        total_users,
        total_tenants,
        default_slug: &state.cfg.default_tenant_slug,
        sent: q.sent,
    };
    Ok(Html(tpl.render()?).into_response())
}

/// Manually fire the daily results recap for every tenant, covering the
/// previous UTC calendar day. Forced, so it sends even if the scheduled run
/// already went out today. Redirects back to the dashboard with a banner.
pub async fn send_results(
    State(state): State<AppState>,
    PlatformAdmin { email }: PlatformAdmin,
) -> AppResult<Response> {
    let yesterday = (Utc::now() - Duration::days(1)).date_naive();
    tracing::info!(super_admin = %email, "manual daily digest trigger for {yesterday}");
    notifications::send_daily_digest(&state, yesterday, true).await?;
    Ok(Redirect::to("/super-admin/?sent=results").into_response())
}

/// Manually fire the "today's matches" preview for every tenant, covering the
/// current UTC calendar day. Forced, so it sends even if the scheduled run
/// already went out today. Redirects back to the dashboard with a banner.
pub async fn send_today_matches(
    State(state): State<AppState>,
    PlatformAdmin { email }: PlatformAdmin,
) -> AppResult<Response> {
    let today = Utc::now().date_naive();
    tracing::info!(super_admin = %email, "manual today's matches trigger for {today}");
    notifications::send_today_matches(&state, today, true).await?;
    Ok(Redirect::to("/super-admin/?sent=today").into_response())
}

/// Cascade-delete a tenant and every row that references it. Refuses to
/// touch the deployment's default tenant (the env-bootstrapped one) so a
/// stray click doesn't wipe the home tenant. Forward only: there is no
/// undo, the action is meant for cleaning up abandoned signups or test
/// tenants.
pub async fn delete_tenant(
    State(state): State<AppState>,
    PlatformAdmin { email }: PlatformAdmin,
    axum::extract::Path(slug): axum::extract::Path<String>,
) -> AppResult<Response> {
    if slug == state.cfg.default_tenant_slug {
        return Ok((
            StatusCode::FORBIDDEN,
            "Refusing to delete the default tenant. Change DEFAULT_TENANT_SLUG first.",
        )
            .into_response());
    }

    let mut tx = state.pool.begin().await?;
    let tenant_id: Option<Uuid> = sqlx::query_scalar("SELECT id FROM tenants WHERE slug = $1")
        .bind(&slug)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(tenant_id) = tenant_id else {
        return Ok((StatusCode::NOT_FOUND, "Tenant not found.").into_response());
    };

    // Children first, no FK with ON DELETE CASCADE was declared so we
    // unwind explicitly.
    for stmt in [
        "DELETE FROM bets WHERE tenant_id = $1",
        "DELETE FROM match_reminders WHERE tenant_id = $1",
        "DELETE FROM magic_links WHERE tenant_id = $1",
        "DELETE FROM sessions WHERE tenant_id = $1",
        "DELETE FROM users WHERE tenant_id = $1",
    ] {
        sqlx::query(stmt)
            .bind(tenant_id)
            .execute(&mut *tx)
            .await?;
    }
    // pending_tenants references the slug (not the tenant id) — clean any
    // stale reservation so the slug is free for a future signup.
    sqlx::query("DELETE FROM pending_tenants WHERE slug = $1")
        .bind(&slug)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    state.tenants.invalidate(&slug).await;

    tracing::warn!(
        super_admin = %email,
        tenant_slug = %slug,
        "tenant deleted by platform admin"
    );

    Ok(Redirect::to("/super-admin/").into_response())
}

pub async fn logout(_apex: ApexOnly, jar: PrivateCookieJar) -> impl IntoResponse {
    let mut c = Cookie::from(SESSION_COOKIE);
    c.set_path("/super-admin");
    let jar = jar.remove(c);
    (jar, Redirect::to("/super-admin/login"))
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
