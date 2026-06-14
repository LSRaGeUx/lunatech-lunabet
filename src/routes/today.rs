//! "Today" screen: a focused matchday view showing the day's matches (with the
//! user's bets), the previous day's results, and the top-10 leaderboard.
//!
//! Matchday windows follow CEST (UTC+2, the WC2026 timezone), 15:00 CEST to
//! 08:00 CEST the next morning (13:00 UTC to 06:00 UTC):
//! - "Today's matches": today 15:00 CEST -> tomorrow 08:00 CEST.
//! - "Yesterday's results": yesterday 15:00 CEST -> today 08:00 CEST.

use std::collections::HashMap;

use askama::Template;
use axum::extract::State;
use axum::response::{Html, IntoResponse, Response};
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};

use crate::error::AppResult;
use crate::i18n::Locale;
use crate::models::Match;
use crate::routes::auth::AuthUser;
use crate::routes::matches::MatchView;
use crate::stakes;
use crate::state::AppState;
use crate::tenant::{Tenant, TenantCtx};

/// CEST is UTC+2 (summer time, in effect for the whole tournament).
const CEST_OFFSET_HOURS: i64 = 2;

pub struct Standing {
    pub rank: usize,
    pub name: String,
    pub points: i64,
    pub is_me: bool,
}

#[derive(Template)]
#[template(path = "today.html")]
struct TodayTpl<'a> {
    loc: Locale,
    tenant: &'a Tenant,
    user_name: &'a str,
    total_points: i32,
    is_admin: bool,
    nav_active: &'static str,
    today_label: String,
    yest_label: String,
    today: Vec<MatchView>,
    results: Vec<MatchView>,
    standings: Vec<Standing>,
}

fn utc_at(d: NaiveDate, hour: u32, min: u32) -> DateTime<Utc> {
    Utc.from_utc_datetime(&d.and_hms_opt(hour, min, 0).unwrap())
}

pub async fn page(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    loc: Locale,
    AuthUser(user): AuthUser,
) -> AppResult<Response> {
    // 15:00 CEST = 13:00 UTC, 08:00 CEST = 06:00 UTC.
    let start_utc_hour = (15 - CEST_OFFSET_HOURS) as u32; // 13
    let end_utc_hour = (8 - CEST_OFFSET_HOURS) as u32; // 6

    let today_cest = (Utc::now() + Duration::hours(CEST_OFFSET_HOURS)).date_naive();
    let yesterday = today_cest.pred_opt().unwrap_or(today_cest);
    let tomorrow = today_cest.succ_opt().unwrap_or(today_cest);

    // "Today's" matchday: today 15:00 CEST -> tomorrow 08:00 CEST.
    let today_start = utc_at(today_cest, start_utc_hour, 0);
    let today_end = utc_at(tomorrow, end_utc_hour, 0);
    // "Yesterday's" matchday: yesterday 15:00 CEST -> today 08:00 CEST.
    let yest_start = utc_at(yesterday, start_utc_hour, 0);
    let yest_end = utc_at(today_cest, end_utc_hour, 0);

    // One query covers both windows (yesterday's start to today's end).
    let matches: Vec<Match> = sqlx::query_as(
        r#"
        SELECT id, competition, stage, group_name,
               home_team, away_team, home_team_code, away_team_code,
               kickoff_at, status, home_score, away_score
        FROM matches
        WHERE kickoff_at >= $1 AND kickoff_at < $2
        ORDER BY kickoff_at ASC
        "#,
    )
    .bind(yest_start)
    .bind(today_end)
    .fetch_all(&state.pool)
    .await?;

    let ids: Vec<i64> = matches.iter().map(|m| m.id).collect();
    let bets: Vec<(i64, i32, i32, Option<i32>)> = sqlx::query_as(
        "SELECT match_id, home_score, away_score, points FROM bets \
         WHERE user_id = $1 AND tenant_id = $2 AND match_id = ANY($3)",
    )
    .bind(user.id)
    .bind(tenant.id)
    .bind(&ids)
    .fetch_all(&state.pool)
    .await?;
    let bet_map: HashMap<i64, (i32, i32, Option<i32>)> =
        bets.into_iter().map(|(m, h, a, p)| (m, (h, a, p))).collect();

    let mut today = Vec::new();
    let mut results = Vec::new();
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
        let ko = view.m.kickoff_at;
        if ko >= today_start && ko < today_end {
            today.push(view);
        } else if ko >= yest_start && ko < yest_end && view.finished {
            results.push(view);
        }
    }

    let board = stakes::load_leaderboard(&state.pool, tenant.id).await?;
    let standings: Vec<Standing> = board
        .iter()
        .enumerate()
        .take(10)
        .map(|(i, r)| Standing {
            rank: i + 1,
            name: r.display_name.clone(),
            points: r.points,
            is_me: r.user_id == user.id,
        })
        .collect();
    let total_points = board
        .iter()
        .find(|r| r.user_id == user.id)
        .map(|r| r.points as i32)
        .unwrap_or(0);

    let fmt = |d: NaiveDate| d.format("%d/%m").to_string();
    let tpl = TodayTpl {
        loc,
        tenant: &tenant,
        user_name: &user.display_name,
        total_points,
        is_admin: user.is_admin,
        nav_active: "today",
        today_label: fmt(today_cest),
        yest_label: fmt(yesterday),
        today,
        results,
        standings,
    };
    Ok(Html(tpl.render()?).into_response())
}
