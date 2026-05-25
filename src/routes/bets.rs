use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::error::AppResult;
use crate::i18n::Locale;
use crate::routes::auth::AuthUser;
use crate::state::AppState;
use crate::tenant::TenantCtx;

#[derive(Deserialize)]
pub struct BetForm {
    home_score: i32,
    away_score: i32,
}

pub async fn place_or_update(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    loc: Locale,
    AuthUser(user): AuthUser,
    Path(match_id): Path<i64>,
    Form(form): Form<BetForm>,
) -> AppResult<Response> {
    if !(0..=30).contains(&form.home_score) || !(0..=30).contains(&form.away_score) {
        return Ok((StatusCode::BAD_REQUEST, loc.f("Score invalide (0-30).", "Invalid score (0-30).")).into_response());
    }

    let row: Option<(DateTime<Utc>, String)> =
        sqlx::query_as("SELECT kickoff_at, status FROM matches WHERE id = $1")
            .bind(match_id)
            .fetch_optional(&state.pool)
            .await?;

    let Some((kickoff, status)) = row else {
        return Ok((StatusCode::NOT_FOUND, loc.f("Match introuvable.", "Match not found.")).into_response());
    };
    if kickoff <= Utc::now() || !(status == "SCHEDULED" || status == "TIMED") {
        return Ok((StatusCode::FORBIDDEN, loc.f("Les paris sont fermés pour ce match.", "Bets are closed for this match.")).into_response());
    }

    sqlx::query(
        r#"
        INSERT INTO bets (tenant_id, user_id, match_id, home_score, away_score)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (user_id, match_id) DO UPDATE
        SET home_score = EXCLUDED.home_score,
            away_score = EXCLUDED.away_score,
            updated_at = NOW()
        "#,
    )
    .bind(tenant.id)
    .bind(user.id)
    .bind(match_id)
    .bind(form.home_score)
    .bind(form.away_score)
    .execute(&state.pool)
    .await?;

    Ok(Redirect::to("/matches").into_response())
}
