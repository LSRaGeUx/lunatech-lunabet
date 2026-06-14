//! PWA endpoints: a per-tenant web app manifest (so the installed app carries
//! the tenant's name and theme colour) and the service worker served at the
//! site root so its scope covers the whole app.

use axum::http::{header, HeaderName, HeaderValue};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::tenant::TenantCtx;

/// `GET /manifest.webmanifest` — built per tenant. Public (the browser fetches
/// it without credentials).
pub async fn manifest(TenantCtx(tenant): TenantCtx) -> Response {
    let manifest = json!({
        "name": format!("{} · LunaBet", tenant.name),
        "short_name": tenant.name,
        "start_url": "/today",
        "scope": "/",
        "display": "standalone",
        "background_color": "#f3e6c4",
        "theme_color": tenant.primary_color,
        "icons": [
            {
                "src": "/static/favicon.svg",
                "sizes": "any",
                "type": "image/svg+xml",
                "purpose": "any maskable"
            }
        ]
    });
    (
        [(header::CONTENT_TYPE, "application/manifest+json")],
        manifest.to_string(),
    )
        .into_response()
}

/// `GET /sw.js` — the service worker, served from the root so its default scope
/// is the whole site. Bundled into the binary so it ships with the app.
pub async fn service_worker() -> Response {
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/javascript"),
            ),
            (
                HeaderName::from_static("service-worker-allowed"),
                HeaderValue::from_static("/"),
            ),
        ],
        include_str!("../../static/sw.js"),
    )
        .into_response()
}
