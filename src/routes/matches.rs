use askama::Template;
use axum::extract::State;
use axum::response::{Html, IntoResponse, Response};
use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::error::AppResult;
use crate::i18n::Locale;
use crate::models::{stage_label_for, Match, STAGE_ORDER};
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

/// A team as displayed in a group-stage overview card.
pub struct TeamRef {
    pub code: String,
    pub name: String,
}

/// One team-pool inside the group-stage overview.
pub struct GroupOverview {
    pub name: String,
    pub teams: Vec<TeamRef>,
}

/// Matches grouped by tournament stage. Renders as one anchored section
/// per stage; the page header links jump to each section.
pub struct StageSection {
    /// Anchor id (e.g. `stage-LAST_32`).
    pub anchor: String,
    /// Localised display label ("Phase de groupes", "Quarts de finale", ...).
    pub label: String,
    /// Group-stage only: one card per group with its teams. Empty otherwise.
    pub groups: Vec<GroupOverview>,
    pub upcoming: Vec<MatchView>,
    pub finished: Vec<MatchView>,
}

impl StageSection {
    #[allow(dead_code)]
    pub fn has_any_matches(&self) -> bool {
        !self.upcoming.is_empty() || !self.finished.is_empty()
    }
}

#[derive(Template)]
#[template(path = "matches.html")]
struct MatchesTpl<'a> {
    loc: Locale,
    tenant: &'a Tenant,
    user_name: &'a str,
    sections: Vec<StageSection>,
    total_points: i32,
    is_admin: bool,
    /// True when at least one finished match exists, so the template renders
    /// the Results block at the top of the page.
    has_results: bool,
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

    // Bucket matches by their stage key (or "OTHER" when missing). Within
    // each bucket they stay sorted by kickoff because the SQL ORDER BY
    // already did that.
    let mut by_stage: HashMap<String, Vec<MatchView>> = HashMap::new();
    for m in matches {
        let key = m.stage.clone().unwrap_or_else(|| "OTHER".into());
        let bet = bet_map.get(&m.id).copied();
        let view = MatchView {
            open: m.is_open_for_bets(),
            finished: m.has_final_result(),
            bet_home: bet.map(|(h, _, _)| h),
            bet_away: bet.map(|(_, a, _)| a),
            points: bet.and_then(|(_, _, p)| p),
            m,
        };
        by_stage.entry(key).or_default().push(view);
    }

    // Render sections in the canonical World Cup order, then anything
    // unknown ("OTHER" or any stage not in STAGE_ORDER) at the end so we
    // never silently drop a match.
    let mut sections: Vec<StageSection> = Vec::new();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for stage in STAGE_ORDER {
        if let Some(views) = by_stage.remove(*stage) {
            sections.push(build_section(stage, views, loc));
            seen.insert(*stage);
        }
    }
    // Sort any remaining stages alphabetically for stability.
    let leftovers: Vec<(String, Vec<MatchView>)> = by_stage.into_iter().collect();
    let mut leftovers: BTreeMap<String, Vec<MatchView>> = leftovers.into_iter().collect();
    while let Some((k, views)) = leftovers.iter().next().map(|(k, _)| k.clone()).and_then(|k| {
        let v = leftovers.remove(&k)?;
        Some((k, v))
    }) {
        sections.push(build_section(&k, views, loc));
    }

    let has_results = sections.iter().any(|s| !s.finished.is_empty());

    let tpl = MatchesTpl {
        loc,
        tenant: &tenant,
        user_name: &user.display_name,
        sections,
        total_points,
        is_admin: user.is_admin,
        has_results,
    };
    Ok(Html(tpl.render()?).into_response())
}

fn build_section(stage_key: &str, mut views: Vec<MatchView>, loc: Locale) -> StageSection {
    let label = stage_label_for(stage_key, loc).to_string();

    // Group-stage section: derive the "teams in each group" overview from
    // the matches themselves so we never go out of sync with the data.
    let groups = if stage_key == "GROUP_STAGE" {
        build_group_overview(&views)
    } else {
        Vec::new()
    };

    // Separate finished (newest first) from upcoming (oldest first).
    let mut finished: Vec<MatchView> = Vec::new();
    let mut upcoming: Vec<MatchView> = Vec::new();
    for v in views.drain(..) {
        if v.finished {
            finished.push(v);
        } else {
            upcoming.push(v);
        }
    }
    finished.reverse();

    StageSection {
        anchor: format!("stage-{}", stage_key.to_lowercase()),
        label,
        groups,
        upcoming,
        finished,
    }
}

fn build_group_overview(views: &[MatchView]) -> Vec<GroupOverview> {
    // BTreeMap so groups come out sorted alphabetically by their name.
    let mut by_group: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for v in views {
        let Some(g) = &v.m.group_name else { continue };
        let entry = by_group.entry(g.clone()).or_default();
        if let Some(code) = &v.m.home_team_code {
            entry.entry(code.clone()).or_insert_with(|| v.m.home_team.clone());
        }
        if let Some(code) = &v.m.away_team_code {
            entry.entry(code.clone()).or_insert_with(|| v.m.away_team.clone());
        }
    }
    by_group
        .into_iter()
        .map(|(name, teams)| GroupOverview {
            name,
            teams: teams
                .into_iter()
                .map(|(code, name)| TeamRef { code, name })
                .collect(),
        })
        .collect()
}
