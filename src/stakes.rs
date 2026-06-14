use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

pub const ALLOWED_TIERS: [i32; 3] = [2, 5, 10];
pub const BASE_SHARES: [f64; 3] = [0.50, 0.30, 0.20];

#[derive(Debug, Clone)]
pub struct LeaderboardRow {
    pub user_id: Uuid,
    pub display_name: String,
    pub points: i64,
    pub exact_count: i64,
    pub settled_bets: i64,
    pub stake_eur: Option<i32>,
    pub paid: bool,
    pub current_streak: i32,
    pub best_streak: i32,
    #[allow(dead_code)]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct PotState {
    pub total_eur: i64,
    pub paid_count: i64,
    #[allow(dead_code)]
    pub deadline: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct PayoutEntry {
    pub user_id: Uuid,
    #[allow(dead_code)]
    pub display_name: String,
    #[allow(dead_code)]
    pub rank: usize,
    #[allow(dead_code)]
    pub stake_eur: i32,
    pub payout_eur: f64,
}

pub fn is_valid_tier(eur: i32) -> bool {
    ALLOWED_TIERS.contains(&eur)
}

/// Compute the weighted payout for the top-N (up to 3) paid users.
///
/// Formula: payout_i = pot × (base_i × stake_i) / sum(base_j × stake_j)
///
/// - When stakes are equal, this degenerates to 50/30/20.
/// - Higher stakes get a bigger slice of their position's share.
/// - The sum of all payouts equals `pot` exactly (modulo rounding).
pub fn compute_payouts(pot_eur: i64, top_paid: &[(Uuid, String, i32)]) -> Vec<PayoutEntry> {
    let n = top_paid.len().min(3);
    if pot_eur <= 0 || n == 0 {
        return Vec::new();
    }
    let mut weights = Vec::with_capacity(n);
    let mut sum = 0.0_f64;
    for i in 0..n {
        let stake = top_paid[i].2 as f64;
        let w = BASE_SHARES[i] * stake;
        weights.push(w);
        sum += w;
    }
    if sum == 0.0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(n);
    for (i, w) in weights.into_iter().enumerate() {
        let (user_id, name, stake) = &top_paid[i];
        out.push(PayoutEntry {
            user_id: *user_id,
            display_name: name.clone(),
            rank: i + 1,
            stake_eur: *stake,
            payout_eur: (pot_eur as f64) * w / sum,
        });
    }
    out
}

#[derive(sqlx::FromRow)]
struct LeaderboardQueryRow {
    id: Uuid,
    display_name: String,
    points: Option<i64>,
    exact_count: Option<i64>,
    settled_bets: Option<i64>,
    stake_eur: Option<i32>,
    paid_at: Option<DateTime<Utc>>,
    current_streak: i32,
    best_streak: i32,
    created_at: DateTime<Utc>,
}

pub async fn load_leaderboard(
    pool: &PgPool,
    tenant_id: Uuid,
) -> anyhow::Result<Vec<LeaderboardRow>> {
    let rows: Vec<LeaderboardQueryRow> = sqlx::query_as(
        r#"
        SELECT
            u.id,
            u.display_name,
            COALESCE(SUM(b.points), 0)::BIGINT AS points,
            COALESCE(SUM(CASE WHEN b.points >= 3 THEN 1 ELSE 0 END), 0)::BIGINT AS exact_count,
            COALESCE(COUNT(b.id) FILTER (WHERE b.points IS NOT NULL), 0)::BIGINT AS settled_bets,
            u.stake_eur,
            u.paid_at,
            u.current_streak,
            u.best_streak,
            u.created_at
        FROM users u
        LEFT JOIN bets b ON b.user_id = u.id AND b.tenant_id = u.tenant_id
        WHERE u.tenant_id = $1
        GROUP BY u.id, u.display_name, u.stake_eur, u.paid_at,
                 u.current_streak, u.best_streak, u.created_at
        ORDER BY points DESC NULLS LAST,
                 exact_count DESC NULLS LAST,
                 settled_bets DESC NULLS LAST,
                 u.created_at ASC
        "#,
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| LeaderboardRow {
            user_id: r.id,
            display_name: r.display_name,
            points: r.points.unwrap_or(0),
            exact_count: r.exact_count.unwrap_or(0),
            settled_bets: r.settled_bets.unwrap_or(0),
            stake_eur: r.stake_eur,
            paid: r.paid_at.is_some(),
            current_streak: r.current_streak,
            best_streak: r.best_streak,
            created_at: r.created_at,
        })
        .collect())
}

/// Total points scored by a single user, derived from a loaded leaderboard.
/// Returns 0 when the user has no settled bets yet (or is absent).
pub fn points_for(rows: &[LeaderboardRow], user_id: Uuid) -> i32 {
    rows.iter()
        .find(|r| r.user_id == user_id)
        .map(|r| r.points as i32)
        .unwrap_or(0)
}

pub async fn load_pot(
    pool: &PgPool,
    tenant_id: Uuid,
    deadline: DateTime<Utc>,
) -> anyhow::Result<PotState> {
    let (total, count): (Option<i64>, Option<i64>) = sqlx::query_as(
        "SELECT COALESCE(SUM(stake_eur), 0)::BIGINT, COUNT(*)::BIGINT \
         FROM users WHERE tenant_id = $1 AND paid_at IS NOT NULL",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await?;
    Ok(PotState {
        total_eur: total.unwrap_or(0),
        paid_count: count.unwrap_or(0),
        deadline,
    })
}

pub fn top_paid_from_leaderboard(rows: &[LeaderboardRow]) -> Vec<(Uuid, String, i32)> {
    rows.iter()
        .filter(|r| r.paid && r.stake_eur.is_some())
        .take(3)
        .map(|r| (r.user_id, r.display_name.clone(), r.stake_eur.unwrap()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uuid(n: u8) -> Uuid {
        let mut b = [0u8; 16];
        b[15] = n;
        Uuid::from_bytes(b)
    }

    #[test]
    fn equal_stakes_yield_50_30_20() {
        let top = vec![
            (uuid(1), "A".into(), 10),
            (uuid(2), "B".into(), 10),
            (uuid(3), "C".into(), 10),
        ];
        let payouts = compute_payouts(100, &top);
        assert!((payouts[0].payout_eur - 50.0).abs() < 1e-6);
        assert!((payouts[1].payout_eur - 30.0).abs() < 1e-6);
        assert!((payouts[2].payout_eur - 20.0).abs() < 1e-6);
    }

    #[test]
    fn payouts_sum_to_pot() {
        let top = vec![
            (uuid(1), "A".into(), 20),
            (uuid(2), "B".into(), 10),
            (uuid(3), "C".into(), 5),
        ];
        let payouts = compute_payouts(100, &top);
        let total: f64 = payouts.iter().map(|p| p.payout_eur).sum();
        assert!((total - 100.0).abs() < 1e-6);
    }

    #[test]
    fn bigger_stake_gets_bigger_share() {
        let top = vec![
            (uuid(1), "A".into(), 20),
            (uuid(2), "B".into(), 10),
            (uuid(3), "C".into(), 5),
        ];
        let payouts = compute_payouts(100, &top);
        // 1st with 20€: weight 0.5*20 = 10
        // 2nd with 10€: weight 0.3*10 = 3
        // 3rd with 5€:  weight 0.2*5  = 1
        // total weight = 14
        assert!((payouts[0].payout_eur - 100.0 * 10.0 / 14.0).abs() < 1e-6);
        assert!((payouts[1].payout_eur - 100.0 * 3.0 / 14.0).abs() < 1e-6);
        assert!((payouts[2].payout_eur - 100.0 * 1.0 / 14.0).abs() < 1e-6);
    }

    #[test]
    fn empty_pot_no_payouts() {
        let top = vec![(uuid(1), "A".into(), 10)];
        assert!(compute_payouts(0, &top).is_empty());
        assert!(compute_payouts(100, &[]).is_empty());
    }

    #[test]
    fn fewer_than_three_winners_still_split_correctly() {
        let top = vec![(uuid(1), "A".into(), 10), (uuid(2), "B".into(), 10)];
        let payouts = compute_payouts(100, &top);
        // weights: 0.5*10=5, 0.3*10=3. total=8. So 62.5 / 37.5
        assert!((payouts[0].payout_eur - 62.5).abs() < 1e-6);
        assert!((payouts[1].payout_eur - 37.5).abs() < 1e-6);
    }

    #[test]
    fn single_winner_takes_all() {
        let top = vec![(uuid(1), "Solo".into(), 10)];
        let payouts = compute_payouts(50, &top);
        assert!((payouts[0].payout_eur - 50.0).abs() < 1e-6);
    }
}
