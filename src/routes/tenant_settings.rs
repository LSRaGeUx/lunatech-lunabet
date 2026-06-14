//! Self-service settings for a tenant **admin** (a user whose email is in the
//! tenant's `admin_emails`). Unlike the super-admin tenant editor in
//! `super_admin.rs`, this page is always scoped to the caller's own tenant
//! (taken from `TenantCtx`, never from the URL), and it only exposes the
//! "their space" fields: branding + operational config. The security-sensitive
//! fields (allowed email regex, mail_from, the admin list, the slug) stay
//! super-admin-only.

use std::collections::HashMap;

use askama::Template;
use axum::extract::{Multipart, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use crate::error::AppResult;
use crate::i18n::Locale;
use crate::routes::admin::AdminUser;
use crate::state::AppState;
use crate::tenant::{Tenant, TenantCtx};

/// 2 MiB — comfortably covers an SVG or a reasonably sized PNG logo while
/// keeping a stray multi-megabyte upload from landing on disk.
const MAX_LOGO_BYTES: usize = 2 * 1024 * 1024;

#[derive(Default, Clone)]
struct FormValues {
    name: String,
    primary_color: String,
    accent_color: String,
    football_competition: String,
    stake_deadline: String,
    reminder_lead_minutes: String,
    slack_webhook_url: String,
    members_can_invite: bool,
}

#[derive(Template)]
#[template(path = "admin_settings.html")]
struct SettingsTpl<'a> {
    loc: Locale,
    tenant: &'a Tenant,
    user_name: &'a str,
    total_points: i32,
    is_admin: bool,
    nav_active: &'static str,
    form: FormValues,
    current_logo: &'a str,
    has_custom_logo: bool,
    error: Option<&'a str>,
    saved: bool,
}

fn values_from_tenant(t: &Tenant) -> FormValues {
    FormValues {
        name: t.name.clone(),
        primary_color: t.primary_color.clone(),
        accent_color: t.accent_color.clone(),
        football_competition: t.football_competition.clone(),
        stake_deadline: t.stake_deadline.format("%Y-%m-%dT%H:%M").to_string(),
        reminder_lead_minutes: t.reminder_lead_minutes.to_string(),
        slack_webhook_url: t.slack_webhook_url.clone().unwrap_or_default(),
        members_can_invite: t.members_can_invite,
    }
}

pub async fn page(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    loc: Locale,
    AdminUser(admin): AdminUser,
) -> AppResult<Response> {
    let board = crate::stakes::load_leaderboard(&state.pool, tenant.id).await?;
    let tpl = SettingsTpl {
        loc,
        tenant: &tenant,
        user_name: &admin.display_name,
        total_points: crate::stakes::points_for(&board, admin.id),
        is_admin: true,
        nav_active: "admin_settings",
        form: values_from_tenant(&tenant),
        current_logo: tenant.logo_url_or_default(),
        has_custom_logo: tenant.logo_url.is_some(),
        error: None,
        saved: false,
    };
    Ok(Html(tpl.render()?).into_response())
}

/// Map an uploaded logo's content-type / filename to an extension we are
/// willing to serve. Returns `None` for anything that is not an image we
/// recognise, so we never write an arbitrary attacker-controlled blob.
fn logo_extension(content_type: Option<&str>, file_name: &str) -> Option<&'static str> {
    let ct = content_type.unwrap_or("").to_ascii_lowercase();
    let ext = file_name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ct.as_str() {
        "image/svg+xml" => return Some("svg"),
        "image/png" => return Some("png"),
        "image/jpeg" => return Some("jpg"),
        "image/webp" => return Some("webp"),
        _ => {}
    }
    match ext.as_str() {
        "svg" => Some("svg"),
        "png" => Some("png"),
        "jpg" | "jpeg" => Some("jpg"),
        "webp" => Some("webp"),
        _ => None,
    }
}

/// Validate that the file bytes match the expected content type's magic numbers.
/// This prevents attackers from uploading malicious files with spoofed Content-Type.
fn validate_logo_magic_numbers(bytes: &[u8], content_type: Option<&str>) -> bool {
    let ct = content_type.unwrap_or("").to_ascii_lowercase();
    match ct.as_str() {
        "image/svg+xml" => {
            // SVG must start with <?xml or <svg
            bytes.len() >= 5 
                && (bytes.starts_with(b"<?xml") || bytes.starts_with(b"<svg"))
        }
        "image/png" => {
            // PNG magic number: \x89PNG\r\n\x1a\n
            bytes.len() >= 8 
                && bytes[0] == 0x89 
                && &bytes[1..4] == b"PNG"
                && bytes[4] == 0x0D 
                && bytes[5] == 0x0A
                && bytes[6] == 0x1A
                && bytes[7] == 0x0A
        }
        "image/jpeg" => {
            // JPEG magic number: FF D8 FF
            bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF
        }
        "image/webp" => {
            // WebP magic number: RIFF....WEBP
            bytes.len() >= 12 
                && &bytes[0..4] == b"RIFF"
                && &bytes[8..12] == b"WEBP"
        }
        _ => false,
    }
}

pub async fn update(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    loc: Locale,
    AdminUser(admin): AdminUser,
    mut multipart: Multipart,
) -> AppResult<Response> {
    let mut fields: HashMap<String, String> = HashMap::new();
    let mut logo: Option<(String, Option<String>, Vec<u8>)> = None; // (filename, content_type, bytes)

    while let Some(field) = multipart.next_field().await? {
        let name = field.name().unwrap_or("").to_string();
        if name == "logo_file" {
            let file_name = field.file_name().unwrap_or("").to_string();
            let content_type = field.content_type().map(|s| s.to_string());
            let bytes = field.bytes().await?;
            if !bytes.is_empty() {
                logo = Some((file_name, content_type, bytes.to_vec()));
            }
        } else {
            fields.insert(name, field.text().await?);
        }
    }

    let get = |k: &str| fields.get(k).map(|s| s.trim().to_string()).unwrap_or_default();
    let members_can_invite = matches!(
        fields.get("members_can_invite").map(|s| s.as_str()),
        Some("on") | Some("1") | Some("true")
    );
    let mut values = FormValues {
        name: get("name"),
        primary_color: get("primary_color"),
        accent_color: get("accent_color"),
        football_competition: get("football_competition"),
        stake_deadline: get("stake_deadline"),
        reminder_lead_minutes: get("reminder_lead_minutes"),
        slack_webhook_url: get("slack_webhook_url"),
        members_can_invite,
    };
    let remove_logo = fields.get("remove_logo").map(|v| v == "on" || v == "1").unwrap_or(false);

    let board = crate::stakes::load_leaderboard(&state.pool, tenant.id).await?;
    let total_points = crate::stakes::points_for(&board, admin.id);

    // Re-render the form with an error message, preserving what was typed.
    let render_error = |msg: String, values: FormValues| -> AppResult<Response> {
        let tpl = SettingsTpl {
            loc,
            tenant: &tenant,
            user_name: &admin.display_name,
            total_points,
            is_admin: true,
            nav_active: "admin_settings",
            form: values,
            current_logo: tenant.logo_url_or_default(),
            has_custom_logo: tenant.logo_url.is_some(),
            error: Some(&msg),
            saved: false,
        };
        Ok((StatusCode::BAD_REQUEST, Html(tpl.render()?)).into_response())
    };

    // --- validation ---
    if values.name.is_empty() {
        return render_error(loc.f("Le nom est obligatoire.", "Name is required.").to_string(), values);
    }
    if values.football_competition.is_empty() {
        return render_error(
            loc.f("Le code compétition est obligatoire.", "Competition code is required.").to_string(),
            values,
        );
    }
    let lead: i32 = match values.reminder_lead_minutes.parse() {
        Ok(n) if (0..=10_080).contains(&n) => n,
        _ => {
            return render_error(
                loc.f(
                    "Le délai de rappel doit être un nombre entre 0 et 10080 (une semaine).",
                    "Reminder lead must be a number between 0 and 10080 (one week).",
                )
                .to_string(),
                values,
            )
        }
    };
    let deadline = match parse_deadline(&values.stake_deadline) {
        Ok(d) => d,
        Err(msg) => return render_error(msg, values),
    };

    // --- logo handling: upload > remove > keep current ---
    let mut new_logo_url: Option<String> = tenant.logo_url.clone();
    if let Some((file_name, content_type, bytes)) = logo {
        if bytes.len() > MAX_LOGO_BYTES {
            return render_error(
                loc.f("Le logo dépasse 2 Mo.", "The logo exceeds 2 MB.").to_string(),
                values,
            );
        }
        let Some(ext) = logo_extension(content_type.as_deref(), &file_name) else {
            return render_error(
                loc.f(
                    "Format de logo non supporté (SVG, PNG, JPG ou WEBP uniquement).",
                    "Unsupported logo format (SVG, PNG, JPG or WEBP only).",
                )
                .to_string(),
                values,
            );
        };
        // Validate magic numbers to prevent Content-Type spoofing
        if !validate_logo_magic_numbers(&bytes, content_type.as_deref()) {
            return render_error(
                loc.f(
                    "Le fichier ne correspond pas au type d'image déclaré.",
                    "File content does not match the declared image type.",
                )
                .to_string(),
                values,
            );
        }
        // Content-hash the bytes so the filename changes whenever the image
        // does — that doubles as cache-busting for the <img src>.
        let digest = Sha256::digest(&bytes);
        let short = hex8(&digest);
        let file_name = format!("logo-{}-{}.{}", tenant.slug, short, ext);
        let path = std::path::Path::new(&state.cfg.uploads_dir).join(&file_name);
        tokio::fs::write(&path, &bytes).await?;
        new_logo_url = Some(format!("/uploads/{file_name}"));
    } else if remove_logo {
        new_logo_url = None;
    }

    let slack: Option<&str> = if values.slack_webhook_url.is_empty() {
        None
    } else {
        Some(values.slack_webhook_url.as_str())
    };

    let result = sqlx::query(
        r#"
        UPDATE tenants SET
            name = $2,
            primary_color = $3,
            accent_color = $4,
            football_competition = $5,
            stake_deadline = $6,
            reminder_lead_minutes = $7,
            slack_webhook_url = $8,
            logo_url = $9,
            members_can_invite = $10
        WHERE id = $1
        "#,
    )
    .bind(tenant.id)
    .bind(&values.name)
    .bind(&values.primary_color)
    .bind(&values.accent_color)
    .bind(&values.football_competition)
    .bind(deadline)
    .bind(lead)
    .bind(slack)
    .bind(new_logo_url.as_deref())
    .bind(values.members_can_invite)
    .execute(&state.pool)
    .await?;

    if result.rows_affected() == 0 {
        return render_error(
            loc.f("Espace introuvable.", "Tenant not found.").to_string(),
            values,
        );
    }

    // Drop the cached tenant so the next request (and the redirect below)
    // reflects the new branding immediately.
    state.tenants.invalidate(&tenant.slug).await;

    // Reload the fresh tenant to render the post-save confirmation with the
    // new logo / colors applied.
    let fresh = state
        .tenants
        .resolve(&tenant.slug)
        .await
        .unwrap_or_else(|| tenant.clone());
    values = values_from_tenant(&fresh);
    let tpl = SettingsTpl {
        loc,
        tenant: &fresh,
        user_name: &admin.display_name,
        total_points,
        is_admin: true,
        nav_active: "admin_settings",
        form: values,
        current_logo: fresh.logo_url_or_default(),
        has_custom_logo: fresh.logo_url.is_some(),
        error: None,
        saved: true,
    };
    Ok(Html(tpl.render()?).into_response())
}

fn hex8(digest: &[u8]) -> String {
    digest.iter().take(4).map(|b| format!("{b:02x}")).collect()
}

fn parse_deadline(raw: &str) -> Result<DateTime<Utc>, String> {
    // HTML datetime-local gives "YYYY-MM-DDTHH:MM"; treat it as UTC.
    let with_tz = format!("{raw}:00Z");
    DateTime::parse_from_rfc3339(&with_tz)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| "stake_deadline is not a valid datetime.".to_string())
}
