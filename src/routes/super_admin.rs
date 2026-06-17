use askama::Template;
use axum::extract::{FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::PrivateCookieJar;
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::Deserialize;
use uuid::Uuid;

use crate::error::AppResult;
use crate::i18n::Locale;
use crate::models::User;
use crate::routes::auth;
use crate::state::AppState;
use crate::tenant::{Tenant, TenantCtx};

/// A signed-in user whose email appears in `SUPER_ADMIN_EMAILS`. Super-admins
/// cross tenant boundaries: their identity is the email, not membership of
/// any specific tenant.
pub struct SuperAdmin(pub User);

impl FromRequestParts<AppState> for SuperAdmin {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let TenantCtx(tenant) = TenantCtx::from_request_parts(parts, state).await?;
        let jar = PrivateCookieJar::<axum_extra::extract::cookie::Key>::from_request_parts(parts, state)
            .await
            .map_err(|e| e.into_response())?;
        let user = match auth::current_user(state, &tenant, &jar).await {
            Ok(Some(u)) => u,
            Ok(None) => return Err(Redirect::to("/login").into_response()),
            Err(e) => return Err(e.into_response()),
        };
        if state.cfg.super_admin_emails.contains(&user.email.to_lowercase()) {
            Ok(SuperAdmin(user))
        } else {
            Err((StatusCode::FORBIDDEN, "Super-admin access required.").into_response())
        }
    }
}

struct TenantRow {
    #[allow(dead_code)]
    id: Uuid,
    slug: String,
    name: String,
    primary_color: String,
    accent_color: String,
    football_competition: String,
    stake_deadline: DateTime<Utc>,
    user_count: i64,
    bet_count: i64,
}

#[derive(Template)]
#[template(path = "admin_tenants.html")]
struct ListTpl<'a> {
    loc: Locale,
    tenant: &'a Tenant,
    user_name: &'a str,
    tenants: Vec<TenantRow>,
}

pub async fn list(
    State(state): State<AppState>,
    TenantCtx(active): TenantCtx,
    loc: Locale,
    SuperAdmin(user): SuperAdmin,
) -> AppResult<Response> {
    let rows: Vec<(
        Uuid,
        String,
        String,
        String,
        String,
        String,
        DateTime<Utc>,
        Option<i64>,
        Option<i64>,
    )> = sqlx::query_as(
        r#"
        SELECT
            t.id,
            t.slug,
            t.name,
            t.primary_color,
            t.accent_color,
            t.football_competition,
            t.stake_deadline,
            (SELECT COUNT(*) FROM users u WHERE u.tenant_id = t.id)::BIGINT,
            (SELECT COUNT(*) FROM bets  b WHERE b.tenant_id = t.id)::BIGINT
        FROM tenants t
        ORDER BY t.created_at ASC
        "#,
    )
    .fetch_all(&state.pool)
    .await?;

    let tenants = rows
        .into_iter()
        .map(|(id, slug, name, primary_color, accent_color, comp, deadline, uc, bc)| TenantRow {
            id,
            slug,
            name,
            primary_color,
            accent_color,
            football_competition: comp,
            stake_deadline: deadline,
            user_count: uc.unwrap_or(0),
            bet_count: bc.unwrap_or(0),
        })
        .collect();

    let tpl = ListTpl {
        loc,
        tenant: &active,
        user_name: &user.display_name,
        tenants,
    };
    Ok(Html(tpl.render()?).into_response())
}

#[derive(Template)]
#[template(path = "admin_tenant_form.html")]
struct FormTpl<'a> {
    loc: Locale,
    tenant: &'a Tenant,
    user_name: &'a str,
    /// `None` when creating a new tenant; `Some(existing_slug)` when editing.
    editing: Option<String>,
    form: FormValues,
    error: Option<&'a str>,
}

#[derive(Default, Clone)]
struct FormValues {
    slug: String,
    name: String,
    allowed_email_pattern: String,
    primary_color: String,
    accent_color: String,
    football_competition: String,
    stake_deadline: String,
    reminder_lead_minutes: String,
    mail_from: String,
    admin_emails: String,
    logo_url: String,
    slack_webhook_url: String,
}

impl FormValues {
    fn defaults() -> Self {
        Self {
            slug: String::new(),
            name: String::new(),
            allowed_email_pattern: String::new(),
            primary_color: "#1d3557".into(),
            accent_color: "#c8232c".into(),
            football_competition: "WC".into(),
            stake_deadline: "2026-06-27T23:59".into(),
            reminder_lead_minutes: "120".into(),
            mail_from: String::new(),
            admin_emails: String::new(),
            logo_url: String::new(),
            slack_webhook_url: String::new(),
        }
    }
}

pub async fn new_form(
    State(_state): State<AppState>,
    TenantCtx(active): TenantCtx,
    loc: Locale,
    SuperAdmin(user): SuperAdmin,
) -> AppResult<Response> {
    let tpl = FormTpl {
        loc,
        tenant: &active,
        user_name: &user.display_name,
        editing: None,
        form: FormValues::defaults(),
        error: None,
    };
    Ok(Html(tpl.render()?).into_response())
}

#[derive(Deserialize)]
pub struct TenantForm {
    slug: Option<String>,
    name: String,
    allowed_email_pattern: String,
    primary_color: String,
    accent_color: String,
    football_competition: String,
    stake_deadline: String,
    reminder_lead_minutes: i32,
    mail_from: String,
    admin_emails: String,
    logo_url: String,
    slack_webhook_url: String,
}

fn parse_admin_emails(raw: &str) -> Vec<String> {
    raw.split(|c| c == ',' || c == '\n' || c == ' ')
        .filter_map(|s| {
            let t = s.trim().to_lowercase();
            if t.is_empty() {
                None
            } else {
                Some(t)
            }
        })
        .collect()
}

fn validate_form(f: &TenantForm) -> Result<(), String> {
    if f.name.trim().is_empty() {
        return Err("Name is required.".into());
    }
    Regex::new(&format!("^(?:{})$", f.allowed_email_pattern))
        .map_err(|e| format!("allowed_email_pattern is not a valid regex: {e}"))?;
    if !f.mail_from.contains('@') {
        return Err("mail_from must look like an email address.".into());
    }
    if f.reminder_lead_minutes < 0 || f.reminder_lead_minutes > 10_080 {
        return Err("reminder_lead_minutes must be between 0 and 10080 (one week).".into());
    }
    Ok(())
}

fn parse_deadline(raw: &str) -> Result<DateTime<Utc>, String> {
    // HTML datetime-local gives "YYYY-MM-DDTHH:MM"; treat it as UTC.
    let with_tz = format!("{raw}:00Z");
    DateTime::parse_from_rfc3339(&with_tz)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| format!("stake_deadline is not a valid datetime: {e}"))
}

pub async fn create(
    State(state): State<AppState>,
    TenantCtx(active): TenantCtx,
    loc: Locale,
    SuperAdmin(user): SuperAdmin,
    axum::Form(form): axum::Form<TenantForm>,
) -> AppResult<Response> {
    let render_error = |msg: &str, values: FormValues| {
        let tpl = FormTpl {
            loc,
            tenant: &active,
            user_name: &user.display_name,
            editing: None,
            form: values,
            error: Some(msg),
        };
        Html(tpl.render().unwrap_or_else(|e| format!("template error: {e}")))
    };

    let slug = form.slug.clone().unwrap_or_default().trim().to_lowercase();
    if slug.is_empty()
        || !slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        let values = form_to_values(&form, &slug);
        return Ok((StatusCode::BAD_REQUEST, render_error(
            "Slug is required and may only contain lowercase letters, digits, '-' and '_'.",
            values,
        ))
        .into_response());
    }
    if let Err(msg) = validate_form(&form) {
        let values = form_to_values(&form, &slug);
        return Ok((StatusCode::BAD_REQUEST, render_error(&msg, values)).into_response());
    }
    let deadline = match parse_deadline(&form.stake_deadline) {
        Ok(d) => d,
        Err(msg) => {
            let values = form_to_values(&form, &slug);
            return Ok((StatusCode::BAD_REQUEST, render_error(&msg, values)).into_response());
        }
    };

    let admin_emails = parse_admin_emails(&form.admin_emails);
    let logo_url: Option<&str> = if form.logo_url.trim().is_empty() {
        None
    } else {
        Some(form.logo_url.trim())
    };
    let slack: Option<&str> = if form.slack_webhook_url.trim().is_empty() {
        None
    } else {
        Some(form.slack_webhook_url.trim())
    };

    let result = sqlx::query(
        r#"
        INSERT INTO tenants
            (slug, name, allowed_email_pattern, mail_from, stake_deadline,
             reminder_lead_minutes, slack_webhook_url, football_competition,
             admin_emails, primary_color, accent_color, logo_url)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        "#,
    )
    .bind(&slug)
    .bind(form.name.trim())
    .bind(form.allowed_email_pattern.trim())
    .bind(form.mail_from.trim())
    .bind(deadline)
    .bind(form.reminder_lead_minutes)
    .bind(slack)
    .bind(form.football_competition.trim())
    .bind(&admin_emails)
    .bind(form.primary_color.trim())
    .bind(form.accent_color.trim())
    .bind(logo_url)
    .execute(&state.pool)
    .await;

    match result {
        Ok(_) => {
            state.tenants.invalidate(&slug).await;
            Ok(Redirect::to("/admin/tenants").into_response())
        }
        Err(e) => {
            let msg = format!("Insert failed (slug already taken?): {e}");
            let values = form_to_values(&form, &slug);
            Ok((StatusCode::CONFLICT, render_error(&msg, values)).into_response())
        }
    }
}

pub async fn edit_form(
    State(state): State<AppState>,
    TenantCtx(active): TenantCtx,
    loc: Locale,
    SuperAdmin(user): SuperAdmin,
    Path(slug): Path<String>,
) -> AppResult<Response> {
    let row: Option<(
        String,
        String,
        String,
        Option<String>,
        String,
        String,
        String,
        DateTime<Utc>,
        i32,
        Option<String>,
        String,
        Vec<String>,
    )> = sqlx::query_as(
        r#"
        SELECT slug, name, allowed_email_pattern, logo_url,
               primary_color, accent_color, football_competition,
               stake_deadline, reminder_lead_minutes, slack_webhook_url,
               mail_from, admin_emails
        FROM tenants WHERE slug = $1
        "#,
    )
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await?;

    let Some((
        slug,
        name,
        pattern,
        logo_url,
        primary,
        accent,
        comp,
        deadline,
        lead,
        slack,
        mail_from,
        admins,
    )) = row
    else {
        return Ok((StatusCode::NOT_FOUND, "Tenant not found.").into_response());
    };

    let form = FormValues {
        slug: slug.clone(),
        name,
        allowed_email_pattern: pattern,
        primary_color: primary,
        accent_color: accent,
        football_competition: comp,
        stake_deadline: deadline.format("%Y-%m-%dT%H:%M").to_string(),
        reminder_lead_minutes: lead.to_string(),
        mail_from,
        admin_emails: admins.join(", "),
        logo_url: logo_url.unwrap_or_default(),
        slack_webhook_url: slack.unwrap_or_default(),
    };

    let tpl = FormTpl {
        loc,
        tenant: &active,
        user_name: &user.display_name,
        editing: Some(slug),
        form,
        error: None,
    };
    Ok(Html(tpl.render()?).into_response())
}

pub async fn update(
    State(state): State<AppState>,
    TenantCtx(active): TenantCtx,
    loc: Locale,
    SuperAdmin(user): SuperAdmin,
    Path(slug): Path<String>,
    axum::Form(form): axum::Form<TenantForm>,
) -> AppResult<Response> {
    let render_error = |msg: &str, values: FormValues| {
        let tpl = FormTpl {
            loc,
            tenant: &active,
            user_name: &user.display_name,
            editing: Some(slug.clone()),
            form: values,
            error: Some(msg),
        };
        Html(tpl.render().unwrap_or_else(|e| format!("template error: {e}")))
    };

    if let Err(msg) = validate_form(&form) {
        let values = form_to_values(&form, &slug);
        return Ok((StatusCode::BAD_REQUEST, render_error(&msg, values)).into_response());
    }
    let deadline = match parse_deadline(&form.stake_deadline) {
        Ok(d) => d,
        Err(msg) => {
            let values = form_to_values(&form, &slug);
            return Ok((StatusCode::BAD_REQUEST, render_error(&msg, values)).into_response());
        }
    };

    let admin_emails = parse_admin_emails(&form.admin_emails);
    let logo_url: Option<&str> = if form.logo_url.trim().is_empty() {
        None
    } else {
        Some(form.logo_url.trim())
    };
    let slack: Option<&str> = if form.slack_webhook_url.trim().is_empty() {
        None
    } else {
        Some(form.slack_webhook_url.trim())
    };

    let result = sqlx::query(
        r#"
        UPDATE tenants SET
            name = $2,
            allowed_email_pattern = $3,
            mail_from = $4,
            stake_deadline = $5,
            reminder_lead_minutes = $6,
            slack_webhook_url = $7,
            football_competition = $8,
            admin_emails = $9,
            primary_color = $10,
            accent_color = $11,
            logo_url = $12
        WHERE slug = $1
        "#,
    )
    .bind(&slug)
    .bind(form.name.trim())
    .bind(form.allowed_email_pattern.trim())
    .bind(form.mail_from.trim())
    .bind(deadline)
    .bind(form.reminder_lead_minutes)
    .bind(slack)
    .bind(form.football_competition.trim())
    .bind(&admin_emails)
    .bind(form.primary_color.trim())
    .bind(form.accent_color.trim())
    .bind(logo_url)
    .execute(&state.pool)
    .await?;

    if result.rows_affected() == 0 {
        return Ok((StatusCode::NOT_FOUND, "Tenant not found.").into_response());
    }
    state.tenants.invalidate(&slug).await;
    Ok(Redirect::to("/admin/tenants").into_response())
}

pub async fn delete(
    State(state): State<AppState>,
    SuperAdmin(_admin): SuperAdmin,
    Path(slug): Path<String>,
) -> AppResult<Response> {
    // Refuse if the tenant has any users; we don't want a stray click to
    // cascade-delete a populated tenant. The super-admin can flush users
    // manually first if they really mean it.
    let user_count: Option<i64> = sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM users WHERE tenant_id = (SELECT id FROM tenants WHERE slug = $1)",
    )
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await?;

    if user_count.unwrap_or(0) > 0 {
        return Ok((
            StatusCode::CONFLICT,
            "Refusing to delete a tenant that still has users.",
        )
            .into_response());
    }

    sqlx::query("DELETE FROM tenants WHERE slug = $1")
        .bind(&slug)
        .execute(&state.pool)
        .await?;
    state.tenants.invalidate(&slug).await;
    Ok(Redirect::to("/admin/tenants").into_response())
}

fn form_to_values(f: &TenantForm, slug: &str) -> FormValues {
    FormValues {
        slug: slug.to_string(),
        name: f.name.clone(),
        allowed_email_pattern: f.allowed_email_pattern.clone(),
        primary_color: f.primary_color.clone(),
        accent_color: f.accent_color.clone(),
        football_competition: f.football_competition.clone(),
        stake_deadline: f.stake_deadline.clone(),
        reminder_lead_minutes: f.reminder_lead_minutes.to_string(),
        mail_from: f.mail_from.clone(),
        admin_emails: f.admin_emails.clone(),
        logo_url: f.logo_url.clone(),
        slack_webhook_url: f.slack_webhook_url.clone(),
    }
}
