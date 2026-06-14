//! Mobile deep-link association files (spec 12). iOS universal links and
//! Android app links only open the native app instead of the browser if the
//! domain serves these well-known documents naming the app. Both are built from
//! config and 404 until the relevant values are set, so a web-only deployment
//! serves nothing.

use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::state::AppState;

/// `GET /.well-known/apple-app-site-association` — claims the auth and invite
/// links for the iOS app. Served as JSON (Apple fetches it without a file
/// extension). 404 when `APPLE_APP_ID` is unset.
pub async fn apple_app_site_association(State(state): State<AppState>) -> Response {
    let Some(app_id) = state.cfg.apple_app_id.as_ref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let body = json!({
        "applinks": {
            "apps": [],
            "details": [
                {
                    "appID": app_id,
                    // Open the app for sign-in and invite links; everything else
                    // stays in the browser.
                    "paths": ["/auth/callback", "/auth/callback*", "/invite/accept", "/invite/accept*"]
                }
            ]
        }
    });
    (
        [(header::CONTENT_TYPE, "application/json")],
        body.to_string(),
    )
        .into_response()
}

/// `GET /.well-known/assetlinks.json` — Android app-links statement. 404 unless
/// both `ANDROID_PACKAGE` and `ANDROID_CERT_FINGERPRINT` are set.
pub async fn android_asset_links(State(state): State<AppState>) -> Response {
    let (Some(package), Some(fingerprint)) = (
        state.cfg.android_package.as_ref(),
        state.cfg.android_cert_fingerprint.as_ref(),
    ) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let body = json!([
        {
            "relation": ["delegate_permission/common.handle_all_urls"],
            "target": {
                "namespace": "android_app",
                "package_name": package,
                "sha256_cert_fingerprints": [fingerprint]
            }
        }
    ]);
    (
        [(header::CONTENT_TYPE, "application/json")],
        body.to_string(),
    )
        .into_response()
}
