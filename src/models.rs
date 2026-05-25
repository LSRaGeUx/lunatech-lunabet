use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

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
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Bet {
    pub id: Uuid,
    pub user_id: Uuid,
    pub match_id: i64,
    pub home_score: i32,
    pub away_score: i32,
    pub points: Option<i32>,
}
