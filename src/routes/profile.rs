//! Player profile and head-to-head. The profile shows a player's stats
//! (points, accuracy, streak, badges) plus their best and worst call; it works
//! both for the logged-in player (`/me`, `/profile`, with a one-off unlock
//! toast) and as a read-only public page for any member of the same tenant
//! (`/profile/:user_id`). The head-to-head (`/h2h/:user_id`) compares the
//! viewer with another member match by match. All data shown is already public
//! via the leaderboard, and every query is scoped to the current tenant.

use askama::Template;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use uuid::Uuid;

use crate::achievements::{self, POINT_TIERS, STREAK_TIERS};
use crate::characters;
use crate::error::AppResult;
use crate::i18n::Locale;
use crate::routes::auth::AuthUser;
use crate::stakes::{self, LeaderboardRow};
use crate::state::AppState;
use crate::tenant::{Tenant, TenantCtx};

pub struct BadgeView {
    pub name: &'static str,
    pub desc: &'static str,
    pub icon: String,
    pub earned: bool,
    /// e.g. "120 / 250" for tiered badges still in progress.
    pub progress: Option<String>,
}

/// A single notable prediction (best or worst), already formatted for display.
pub struct CallView {
    pub home_team: String,
    pub away_team: String,
    /// Actual final score, e.g. "3 - 2".
    pub actual: String,
    /// The player's prediction, e.g. "1 - 0". `None` for the best call, where
    /// the prediction equals the actual score (it was exact).
    pub predicted: Option<String>,
}

#[derive(Template)]
#[template(path = "profile.html")]
struct ProfileTpl<'a> {
    loc: Locale,
    tenant: &'a Tenant,
    // Nav (always the logged-in viewer).
    user_name: &'a str,
    total_points: i32,
    is_admin: bool,
    nav_active: &'static str,
    // Profile subject (may differ from the viewer on a public profile).
    is_me: bool,
    subject_id: Uuid,
    subject_name: String,
    subject_avatar: String,
    points: i64,
    best_streak: i32,
    current_streak: i32,
    earned_count: usize,
    total_badges: usize,
    badges: Vec<BadgeView>,
    // Derived stats. Percentages are `None` until the player has a settled bet.
    settled_bets: i64,
    exact_pct: Option<String>,
    result_pct: Option<String>,
    best_call: Option<CallView>,
    worst_call: Option<CallView>,
    /// Names of badges unlocked since the last visit, for the toast. Always
    /// empty on a public profile.
    unlocked: Vec<&'static str>,
    /// Whether the deployment has Web Push configured (VAPID keys present), so
    /// the profile's notifications card knows whether to offer the opt-in. Only
    /// meaningful on the viewer's own profile.
    push_available: bool,
}

/// Accuracy counts plus the best and worst call for one player, all derived
/// from settled bets on finished matches.
struct ProfileStats {
    settled: i64,
    exact: i64,
    good_result: i64,
    best_call: Option<CallView>,
    worst_call: Option<CallView>,
}

async fn load_profile_stats(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
) -> anyhow::Result<ProfileStats> {
    let (settled, exact, good_result): (i64, i64, i64) = sqlx::query_as(
        r#"
        SELECT
            COUNT(*) FILTER (WHERE b.points IS NOT NULL)::BIGINT,
            COUNT(*) FILTER (WHERE b.points >= 3)::BIGINT,
            COUNT(*) FILTER (WHERE b.points >= 1)::BIGINT
        FROM bets b
        JOIN matches m ON m.id = b.match_id
        WHERE b.tenant_id = $1 AND b.user_id = $2 AND m.status = 'FINISHED'
        "#,
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    // Best call: an exact score on the match with the most goals (a rough proxy
    // for "improbable"), most recent first to break ties.
    let best: Option<(String, String, i32, i32)> = sqlx::query_as(
        r#"
        SELECT m.home_team, m.away_team, m.home_score, m.away_score
        FROM bets b
        JOIN matches m ON m.id = b.match_id
        WHERE b.tenant_id = $1 AND b.user_id = $2 AND b.points >= 3
          AND m.status = 'FINISHED'
          AND m.home_score IS NOT NULL AND m.away_score IS NOT NULL
        ORDER BY (m.home_score + m.away_score) DESC, m.kickoff_at DESC
        LIMIT 1
        "#,
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    // Worst call: the settled bet furthest from the real score (largest sum of
    // goal-difference errors), most recent first to break ties.
    let worst: Option<(String, String, i32, i32, i32, i32)> = sqlx::query_as(
        r#"
        SELECT m.home_team, m.away_team, m.home_score, m.away_score, b.home_score, b.away_score
        FROM bets b
        JOIN matches m ON m.id = b.match_id
        WHERE b.tenant_id = $1 AND b.user_id = $2
          AND m.status = 'FINISHED'
          AND m.home_score IS NOT NULL AND m.away_score IS NOT NULL
          AND b.points IS NOT NULL
        ORDER BY (ABS(b.home_score - m.home_score) + ABS(b.away_score - m.away_score)) DESC,
                 m.kickoff_at DESC
        LIMIT 1
        "#,
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    let best_call = best.map(|(h, a, hs, as_)| CallView {
        home_team: h,
        away_team: a,
        actual: format!("{hs} - {as_}"),
        predicted: None,
    });
    let worst_call = worst.map(|(h, a, hs, as_, ph, pa)| CallView {
        home_team: h,
        away_team: a,
        actual: format!("{hs} - {as_}"),
        predicted: Some(format!("{ph} - {pa}")),
    });

    Ok(ProfileStats {
        settled,
        exact,
        good_result,
        best_call,
        worst_call,
    })
}

/// Render a profile page. `subject` is the player being viewed; `viewer` is the
/// logged-in user (drives the nav and `is_me`). `unlocked` is the toast, only
/// passed for the viewer's own profile.
async fn render_profile(
    state: &AppState,
    tenant: &Tenant,
    loc: Locale,
    viewer: &crate::models::User,
    subject: &LeaderboardRow,
    board: &[LeaderboardRow],
    unlocked: Vec<&'static str>,
) -> AppResult<Response> {
    let earned: std::collections::HashSet<String> =
        achievements::earned_codes(&state.pool, tenant.id, subject.user_id)
            .await?
            .into_iter()
            .collect();

    let pts = subject.points;
    let best_streak = subject.best_streak;
    let badges: Vec<BadgeView> = achievements::CATALOG
        .iter()
        .map(|b| {
            let is_earned = earned.contains(b.code);
            // Numeric progress for the tiered badges while still locked.
            let progress = if is_earned {
                None
            } else {
                POINT_TIERS
                    .iter()
                    .find(|t| b.code == format!("pts_{t}"))
                    .map(|tier| format!("{} / {}", pts.min(*tier), tier))
                    .or_else(|| {
                        STREAK_TIERS
                            .iter()
                            .find(|t| b.code == format!("streak_{t}"))
                            .map(|tier| format!("{} / {}", best_streak.min(*tier), tier))
                    })
            };
            BadgeView {
                name: b.name(loc),
                desc: b.desc(loc),
                icon: b.icon_path(),
                earned: is_earned,
                progress,
            }
        })
        .collect();

    let stats = load_profile_stats(&state.pool, tenant.id, subject.user_id).await?;
    let pct = |n: i64| {
        if stats.settled == 0 {
            None
        } else {
            Some(format!("{:.0}%", n as f64 * 100.0 / stats.settled as f64))
        }
    };

    let tpl = ProfileTpl {
        loc,
        tenant,
        user_name: &viewer.display_name,
        total_points: stakes::points_for(board, viewer.id),
        is_admin: viewer.is_admin,
        nav_active: "profile",
        is_me: subject.user_id == viewer.id,
        subject_id: subject.user_id,
        subject_name: subject.display_name.clone(),
        subject_avatar: characters::path_for(subject.user_id),
        points: subject.points,
        best_streak: subject.best_streak,
        current_streak: subject.current_streak,
        earned_count: earned.len(),
        total_badges: achievements::CATALOG.len(),
        badges,
        settled_bets: stats.settled,
        exact_pct: pct(stats.exact),
        result_pct: pct(stats.good_result),
        best_call: stats.best_call,
        worst_call: stats.worst_call,
        unlocked,
        push_available: state.cfg.vapid.is_some(),
    };
    Ok(Html(tpl.render()?).into_response())
}

/// The logged-in player's own profile, with the unlock toast.
pub async fn me(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    loc: Locale,
    AuthUser(user): AuthUser,
) -> AppResult<Response> {
    let board = stakes::load_leaderboard(&state.pool, tenant.id).await?;
    let Some(subject) = board.iter().find(|r| r.user_id == user.id).cloned() else {
        // A brand-new user with no leaderboard row yet: send them to the board.
        return Ok(axum::response::Redirect::to("/leaderboard").into_response());
    };

    // Toast any badge the player hasn't seen yet, then mark them seen.
    let unlocked: Vec<&'static str> = achievements::take_unseen(&state.pool, tenant.id, user.id)
        .await?
        .iter()
        .filter_map(|code| achievements::def(code).map(|d| d.name(loc)))
        .collect();

    render_profile(&state, &tenant, loc, &user, &subject, &board, unlocked).await
}

/// Read-only public profile of another member of the same tenant.
pub async fn public(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    loc: Locale,
    AuthUser(user): AuthUser,
    Path(user_id): Path<Uuid>,
) -> AppResult<Response> {
    // Viewing your own public link just shows your profile (with the toast).
    if user_id == user.id {
        return me(State(state), TenantCtx(tenant), loc, AuthUser(user)).await;
    }
    let board = stakes::load_leaderboard(&state.pool, tenant.id).await?;
    let Some(subject) = board.iter().find(|r| r.user_id == user_id).cloned() else {
        return Ok((
            StatusCode::NOT_FOUND,
            loc.f("Joueur introuvable.", "Player not found."),
        )
            .into_response());
    };
    render_profile(&state, &tenant, loc, &user, &subject, &board, Vec::new()).await
}

/// One row of the head-to-head match list.
pub struct H2hMatch {
    pub home_team: String,
    pub away_team: String,
    pub my_points: i32,
    pub their_points: i32,
    /// "win" / "draw" / "loss" from the viewer's point of view.
    pub outcome: &'static str,
}

#[derive(Template)]
#[template(path = "h2h.html")]
struct H2hTpl<'a> {
    loc: Locale,
    tenant: &'a Tenant,
    user_name: &'a str,
    total_points: i32,
    is_admin: bool,
    nav_active: &'static str,
    me_name: String,
    me_avatar: String,
    them_name: String,
    them_avatar: String,
    their_id: Uuid,
    wins: usize,
    draws: usize,
    losses: usize,
    my_total: i64,
    their_total: i64,
    matches: Vec<H2hMatch>,
}

/// Head-to-head between the viewer and another member: match-by-match comparison
/// over every finished match both players bet on.
pub async fn h2h(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    loc: Locale,
    AuthUser(user): AuthUser,
    Path(user_id): Path<Uuid>,
) -> AppResult<Response> {
    if user_id == user.id {
        return Ok(axum::response::Redirect::to("/leaderboard").into_response());
    }

    let board = stakes::load_leaderboard(&state.pool, tenant.id).await?;
    let Some(them) = board.iter().find(|r| r.user_id == user_id).cloned() else {
        return Ok((
            StatusCode::NOT_FOUND,
            loc.f("Joueur introuvable.", "Player not found."),
        )
            .into_response());
    };

    // Every finished match both players have a settled bet on, newest first.
    let rows: Vec<(String, String, i32, i32)> = sqlx::query_as(
        r#"
        SELECT m.home_team, m.away_team, mine.points, theirs.points
        FROM bets mine
        JOIN bets theirs
          ON theirs.match_id = mine.match_id AND theirs.tenant_id = mine.tenant_id
        JOIN matches m ON m.id = mine.match_id
        WHERE mine.tenant_id = $1 AND mine.user_id = $2 AND theirs.user_id = $3
          AND m.status = 'FINISHED'
          AND mine.points IS NOT NULL AND theirs.points IS NOT NULL
        ORDER BY m.kickoff_at DESC
        "#,
    )
    .bind(tenant.id)
    .bind(user.id)
    .bind(user_id)
    .fetch_all(&state.pool)
    .await?;

    let (mut wins, mut draws, mut losses) = (0usize, 0usize, 0usize);
    let (mut my_total, mut their_total) = (0i64, 0i64);
    let matches: Vec<H2hMatch> = rows
        .into_iter()
        .map(|(home, away, my_points, their_points)| {
            my_total += my_points as i64;
            their_total += their_points as i64;
            let outcome = if my_points > their_points {
                wins += 1;
                "win"
            } else if my_points < their_points {
                losses += 1;
                "loss"
            } else {
                draws += 1;
                "draw"
            };
            H2hMatch {
                home_team: home,
                away_team: away,
                my_points,
                their_points,
                outcome,
            }
        })
        .collect();

    let tpl = H2hTpl {
        loc,
        tenant: &tenant,
        user_name: &user.display_name,
        total_points: stakes::points_for(&board, user.id),
        is_admin: user.is_admin,
        nav_active: "leaderboard",
        me_name: user.display_name.clone(),
        me_avatar: characters::path_for(user.id),
        them_name: them.display_name.clone(),
        them_avatar: characters::path_for(user_id),
        their_id: user_id,
        wins,
        draws,
        losses,
        my_total,
        their_total,
        matches,
    };
    Ok(Html(tpl.render()?).into_response())
}
