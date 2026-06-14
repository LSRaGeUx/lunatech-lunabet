//! Web Push subscription endpoints (spec 08, part 2).
//!
//! The browser owns the subscription lifecycle; these routes just persist what
//! it reports. `/push/public-key` hands out the VAPID application server key so
//! the client can subscribe, `/push/subscribe` and `/push/unsubscribe` keep the
//! `push_subscriptions` table in sync, and `/push/preferences` flips the
//! per-user master toggle.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Form, Json};
use serde::Deserialize;

use crate::error::AppResult;
use crate::routes::auth::AuthUser;
use crate::state::AppState;
use crate::tenant::TenantCtx;

/// `GET /push/public-key` — the VAPID public key (base64url) the browser needs
/// to subscribe. Plain text; `404` when push isn't configured so the client can
/// hide the UI cleanly.
pub async fn public_key(State(state): State<AppState>) -> Response {
    match &state.cfg.vapid {
        Some(v) => (StatusCode::OK, v.public_key_b64.clone()).into_response(),
        None => (StatusCode::NOT_FOUND, "push not configured").into_response(),
    }
}

/// Subscribe body. Web clients send `PushSubscription.toJSON()` (endpoint +
/// keys); native clients (spec 12) send `platform` + `device_token`.
#[derive(Deserialize)]
pub struct SubscribeBody {
    #[serde(default)]
    endpoint: Option<String>,
    #[serde(default)]
    keys: Option<SubscribeKeys>,
    /// `web` (default), `ios` or `android`.
    #[serde(default)]
    platform: Option<String>,
    #[serde(default)]
    device_token: Option<String>,
}

#[derive(Deserialize)]
pub struct SubscribeKeys {
    p256dh: String,
    auth: String,
}

/// `POST /push/subscribe` — store (or refresh) the current user's subscription
/// on any platform. Keyed on `(user_id, endpoint)`; for native devices the
/// `device_token` doubles as the endpoint so the upsert stays one-row-per-device
/// across both worlds.
pub async fn subscribe(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    AuthUser(user): AuthUser,
    Json(body): Json<SubscribeBody>,
) -> AppResult<Response> {
    let platform = body.platform.as_deref().unwrap_or("web");

    // Resolve the per-platform fields into the columns the table expects.
    let (endpoint, p256dh, auth, device_token) = match platform {
        "web" => {
            let (Some(endpoint), Some(keys)) = (body.endpoint.as_deref(), body.keys.as_ref())
            else {
                return Ok((StatusCode::BAD_REQUEST, "incomplete web subscription").into_response());
            };
            if endpoint.is_empty() || keys.p256dh.is_empty() || keys.auth.is_empty() {
                return Ok((StatusCode::BAD_REQUEST, "incomplete web subscription").into_response());
            }
            (
                endpoint.to_string(),
                Some(keys.p256dh.clone()),
                Some(keys.auth.clone()),
                None,
            )
        }
        "ios" | "android" => {
            let Some(token) = body.device_token.as_deref().filter(|t| !t.is_empty()) else {
                return Ok((StatusCode::BAD_REQUEST, "missing device token").into_response());
            };
            // The token is the device identity; reuse it as the endpoint so the
            // (user_id, endpoint) upsert key works without a schema change.
            (token.to_string(), None, None, Some(token.to_string()))
        }
        _ => return Ok((StatusCode::BAD_REQUEST, "unknown platform").into_response()),
    };

    sqlx::query(
        r#"
        INSERT INTO push_subscriptions
            (tenant_id, user_id, endpoint, p256dh, auth, platform, device_token)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (user_id, endpoint) DO UPDATE
        SET p256dh = EXCLUDED.p256dh,
            auth   = EXCLUDED.auth,
            platform = EXCLUDED.platform,
            device_token = EXCLUDED.device_token,
            tenant_id = EXCLUDED.tenant_id
        "#,
    )
    .bind(tenant.id)
    .bind(user.id)
    .bind(&endpoint)
    .bind(&p256dh)
    .bind(&auth)
    .bind(platform)
    .bind(&device_token)
    .execute(&state.pool)
    .await?;

    Ok(StatusCode::NO_CONTENT.into_response())
}

#[derive(Deserialize)]
pub struct UnsubscribeBody {
    endpoint: String,
}

/// `POST /push/unsubscribe` — drop one subscription (the browser unsubscribed
/// or revoked permission).
pub async fn unsubscribe(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<UnsubscribeBody>,
) -> AppResult<Response> {
    sqlx::query("DELETE FROM push_subscriptions WHERE user_id = $1 AND endpoint = $2")
        .bind(user.id)
        .bind(&body.endpoint)
        .execute(&state.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

#[derive(Deserialize)]
pub struct PreferencesForm {
    /// HTML checkboxes only submit when checked, so a missing field means off.
    notify_push: Option<String>,
}

/// `POST /push/preferences` — flip the master push toggle. Posted by the
/// preferences form on the profile page (htmx), so we answer `204` and let the
/// page keep its state.
pub async fn preferences(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Form(form): Form<PreferencesForm>,
) -> AppResult<Response> {
    let enabled = form.notify_push.is_some();
    sqlx::query("UPDATE users SET notify_push = $1 WHERE id = $2")
        .bind(enabled)
        .bind(user.id)
        .execute(&state.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
