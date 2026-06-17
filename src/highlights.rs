//! Daily highlights: the "player of the day", i.e. the best predictor over a
//! single matchday (the [`crate::matchday`] window, not a raw calendar day).
//! Computed once a day next to the daily digest (so the email can cite it) and
//! read by the Today screen. The (tenant_id, day) row is the source of truth
//! and the computation is idempotent.

use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PlayerOfTheDay {
    pub user_id: Uuid,
    pub display_name: String,
    pub points: i64,
    /// Kept for the tiebreak and stored for later use (not surfaced yet).
    #[allow(dead_code)]
    pub exact_count: i64,
}

/// Compute and persist the best predictor for the matchday keyed on `date` for
/// one tenant, then return them. The winner is the user with the most points on
/// that day's finished matches; ties break on exact scores, then display name.
/// A day where nobody scored (or with no finished match) has no winner: nothing
/// is stored and `None` is returned. Idempotent via the (tenant_id, day) key,
/// and re-runnable: a later call refreshes the row if the standings changed.
pub async fn upsert_player_of_the_day(
    pool: &PgPool,
    tenant_id: Uuid,
    date: NaiveDate,
) -> anyhow::Result<Option<PlayerOfTheDay>> {
    let (start, end) = crate::matchday::window(date);

    let row: Option<(Uuid, String, i64, i64)> = sqlx::query_as(
        r#"
        SELECT u.id,
               u.display_name,
               COALESCE(SUM(b.points), 0)::BIGINT AS points,
               COALESCE(SUM(CASE WHEN b.points >= 3 THEN 1 ELSE 0 END), 0)::BIGINT AS exact_count
        FROM users u
        JOIN bets b ON b.user_id = u.id AND b.tenant_id = u.tenant_id
        JOIN matches m ON m.id = b.match_id
        WHERE u.tenant_id = $1
          AND m.status = 'FINISHED'
          AND m.home_score IS NOT NULL
          AND m.away_score IS NOT NULL
          AND m.kickoff_at >= $2 AND m.kickoff_at < $3
          AND b.points IS NOT NULL
        GROUP BY u.id, u.display_name
        HAVING COALESCE(SUM(b.points), 0) > 0
        ORDER BY points DESC, exact_count DESC, u.display_name ASC
        LIMIT 1
        "#,
    )
    .bind(tenant_id)
    .bind(start)
    .bind(end)
    .fetch_optional(pool)
    .await?;

    let Some((user_id, display_name, points, exact_count)) = row else {
        return Ok(None);
    };

    sqlx::query(
        r#"
        INSERT INTO player_of_the_day (tenant_id, day, user_id, points, exact_count)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (tenant_id, day) DO UPDATE
            SET user_id = EXCLUDED.user_id,
                points = EXCLUDED.points,
                exact_count = EXCLUDED.exact_count,
                computed_at = NOW()
        "#,
    )
    .bind(tenant_id)
    .bind(date)
    .bind(user_id)
    .bind(points as i32)
    .bind(exact_count as i32)
    .execute(pool)
    .await?;

    Ok(Some(PlayerOfTheDay {
        user_id,
        display_name,
        points,
        exact_count,
    }))
}

/// Read the stored player of the day for `date`, or `None` if none was recorded
/// (no finished match, or nobody scored). Joins `users` so a renamed player
/// shows their current display name.
pub async fn player_of_the_day(
    pool: &PgPool,
    tenant_id: Uuid,
    date: NaiveDate,
) -> anyhow::Result<Option<PlayerOfTheDay>> {
    let row: Option<(Uuid, String, i32, i32)> = sqlx::query_as(
        r#"
        SELECT p.user_id, u.display_name, p.points, p.exact_count
        FROM player_of_the_day p
        JOIN users u ON u.id = p.user_id
        WHERE p.tenant_id = $1 AND p.day = $2
        "#,
    )
    .bind(tenant_id)
    .bind(date)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(user_id, display_name, points, exact_count)| PlayerOfTheDay {
        user_id,
        display_name,
        points: points as i64,
        exact_count: exact_count as i64,
    }))
}
