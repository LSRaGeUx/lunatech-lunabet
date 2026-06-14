//! Serve tenant logos stored in the database (the `db` storage backend).
//! Public and unauthenticated: logos appear on login pages and in emails.
//! The URL carries a `?v=<hash>` cache-buster (see `storage::LogoStore`), so the
//! response is marked immutable and long-lived.

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

use crate::state::AppState;

pub async fn serve(State(state): State<AppState>, Path(tenant_id): Path<Uuid>) -> Response {
    let row: Option<(Vec<u8>, String)> =
        sqlx::query_as("SELECT bytes, content_type FROM tenant_logos WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten();

    match row {
        Some((bytes, content_type)) => (
            [
                (header::CONTENT_TYPE, content_type),
                (
                    header::CACHE_CONTROL,
                    "public, max-age=31536000, immutable".to_string(),
                ),
            ],
            bytes,
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "logo not found").into_response(),
    }
}
