use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::i18n::Locale;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub is_admin: bool,
    pub created_at: DateTime<Utc>,
    pub stake_eur: Option<i32>,
    pub stake_chosen_at: Option<DateTime<Utc>>,
    pub paid_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct Match {
    pub id: i64,
    pub competition: String,
    pub stage: Option<String>,
    pub group_name: Option<String>,
    pub home_team: String,
    pub away_team: String,
    pub home_team_code: Option<String>,
    pub away_team_code: Option<String>,
    pub kickoff_at: DateTime<Utc>,
    pub status: String,
    pub home_score: Option<i32>,
    pub away_score: Option<i32>,
}

impl Match {
    pub fn is_open_for_bets(&self) -> bool {
        self.kickoff_at > Utc::now() && (self.status == "SCHEDULED" || self.status == "TIMED")
    }
    pub fn has_final_result(&self) -> bool {
        self.status == "FINISHED" && self.home_score.is_some() && self.away_score.is_some()
    }

    /// Localised display label for the match's tournament stage. Falls back
    /// to the raw football-data.org value when the stage isn't one we know
    /// (e.g. competitions other than the World Cup).
    pub fn stage_label(&self, loc: Locale) -> Option<&str> {
        self.stage.as_deref().map(|s| stage_label_for(s, loc))
    }
}

pub fn stage_label_for(stage: &str, loc: Locale) -> &str {
    match (loc, stage) {
        (Locale::Fr, "GROUP_STAGE") => "Phase de groupes",
        (Locale::En, "GROUP_STAGE") => "Group stage",
        (Locale::Fr, "LAST_32") => "16èmes de finale",
        (Locale::En, "LAST_32") => "Round of 32",
        (Locale::Fr, "LAST_16") => "8èmes de finale",
        (Locale::En, "LAST_16") => "Round of 16",
        (Locale::Fr, "QUARTER_FINALS") => "Quarts de finale",
        (Locale::En, "QUARTER_FINALS") => "Quarter-finals",
        (Locale::Fr, "SEMI_FINALS") => "Demi-finales",
        (Locale::En, "SEMI_FINALS") => "Semi-finals",
        (Locale::Fr, "THIRD_PLACE") => "Match pour la 3e place",
        (Locale::En, "THIRD_PLACE") => "Third-place match",
        (Locale::Fr, "FINAL") => "Finale",
        (Locale::En, "FINAL") => "Final",
        (_, other) => other,
    }
}

/// Canonical ordering of WC stages for grouping the matches page.
pub const STAGE_ORDER: &[&str] = &[
    "GROUP_STAGE",
    "LAST_32",
    "LAST_16",
    "QUARTER_FINALS",
    "SEMI_FINALS",
    "THIRD_PLACE",
    "FINAL",
];

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Bet {
    pub id: Uuid,
    pub user_id: Uuid,
    pub match_id: i64,
    pub home_score: i32,
    pub away_score: i32,
    pub points: Option<i32>,
}
