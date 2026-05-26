use askama::Template;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::Form;
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::error::AppResult;
use crate::i18n::Locale;
use crate::models::Match;
use crate::routes::auth::AuthUser;
use crate::routes::matches::MatchView;
use crate::state::AppState;
use crate::tenant::TenantCtx;

#[derive(Deserialize)]
pub struct BetForm {
    home_score: i32,
    away_score: i32,
}

#[derive(Template)]
#[template(path = "match_card.html")]
struct MatchCardTpl {
    loc: Locale,
    v: MatchView,
}

pub async fn place_or_update(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    loc: Locale,
    AuthUser(user): AuthUser,
    Path(match_id): Path<i64>,
    headers: HeaderMap,
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

    // htmx submission: return just the updated match card so the form swaps
    // in place (no full reload, no scroll jump). Fall back to a redirect for
    // plain-HTML clients.
    if headers.get("hx-request").is_some() {
        let m: Match = sqlx::query_as(
            r#"
            SELECT id, competition, stage, group_name,
                   home_team, away_team, home_team_code, away_team_code,
                   kickoff_at, status, home_score, away_score
            FROM matches WHERE id = $1
            "#,
        )
        .bind(match_id)
        .fetch_one(&state.pool)
        .await?;

        let view = MatchView {
            open: m.is_open_for_bets(),
            finished: m.has_final_result(),
            bet_home: Some(form.home_score),
            bet_away: Some(form.away_score),
            points: None,
            m,
        };
        let tpl = MatchCardTpl { loc, v: view };
        return Ok(Html(tpl.render()?).into_response());
    }

    // Non-htmx fallback: redirect with a fragment so the browser at least
    // scrolls back to the card the user just edited.
    Ok(Redirect::to(&format!("/matches#match-{match_id}")).into_response())
}
