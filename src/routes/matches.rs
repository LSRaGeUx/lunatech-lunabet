use askama::Template;
use axum::extract::State;
use axum::response::{Html, IntoResponse, Response};
use std::collections::HashMap;

use crate::error::AppResult;
use crate::i18n::Locale;
use crate::models::Match;
use crate::routes::auth::AuthUser;
use crate::state::AppState;
use crate::tenant::{Tenant, TenantCtx};

pub struct MatchView {
    pub m: Match,
    pub bet_home: Option<i32>,
    pub bet_away: Option<i32>,
    pub points: Option<i32>,
    pub open: bool,
    pub finished: bool,
}

#[derive(Template)]
#[template(path = "matches.html")]
struct MatchesTpl<'a> {
    loc: Locale,
    tenant: &'a Tenant,
    user_name: &'a str,
    upcoming: Vec<MatchView>,
    finished: Vec<MatchView>,
    total_points: i32,
    is_admin: bool,
}

pub async fn list(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    loc: Locale,
    AuthUser(user): AuthUser,
) -> AppResult<Response> {
    // Matches stay global in Phase 1 (one competition synced from
    // football-data). A future migration will associate matches with
    // tenants when we onboard a second competition.
    let matches: Vec<Match> = sqlx::query_as(
        r#"
        SELECT id, competition, stage, group_name,
               home_team, away_team, home_team_code, away_team_code,
               kickoff_at, status, home_score, away_score
        FROM matches
        ORDER BY kickoff_at ASC
        "#,
    )
    .fetch_all(&state.pool)
    .await?;

    let bets: Vec<(i64, i32, i32, Option<i32>)> = sqlx::query_as(
        "SELECT match_id, home_score, away_score, points FROM bets \
         WHERE user_id = $1 AND tenant_id = $2",
    )
    .bind(user.id)
    .bind(tenant.id)
    .fetch_all(&state.pool)
    .await?;

    let mut bet_map: HashMap<i64, (i32, i32, Option<i32>)> = HashMap::new();
    let mut total_points = 0i32;
    for (mid, h, a, p) in bets {
        if let Some(pts) = p {
            total_points += pts;
        }
        bet_map.insert(mid, (h, a, p));
    }

    let mut upcoming = Vec::new();
    let mut finished = Vec::new();
    for m in matches {
        let bet = bet_map.get(&m.id).copied();
        let view = MatchView {
            open: m.is_open_for_bets(),
            finished: m.has_final_result(),
            bet_home: bet.map(|(h, _, _)| h),
            bet_away: bet.map(|(_, a, _)| a),
            points: bet.and_then(|(_, _, p)| p),
            m,
        };
        if view.finished {
            finished.push(view);
        } else {
            upcoming.push(view);
        }
    }
    finished.reverse();

    let tpl = MatchesTpl {
        loc,
        tenant: &tenant,
        user_name: &user.display_name,
        upcoming,
        finished,
        total_points,
        is_admin: user.is_admin,
    };
    Ok(Html(tpl.render()?).into_response())
}
