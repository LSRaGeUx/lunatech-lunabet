//! Streak tracking: how many FINISHED matches in a row (by kickoff order) a
//! user has scored points on. Derived entirely from settled bets and
//! recomputed after each scoring pass. A settled bet worth 0 points breaks the
//! run; the best (longest) streak ever reached is kept separately.

use sqlx::PgPool;
use uuid::Uuid;

/// Fold a user's settled-bet points, in kickoff order, into
/// `(current_streak, best_streak)`. A value > 0 extends the run, a 0 resets the
/// current streak while leaving the best untouched.
fn streak_of(points_in_order: &[i32]) -> (i32, i32) {
    let mut current = 0;
    let mut best = 0;
    for &p in points_in_order {
        if p > 0 {
            current += 1;
            best = best.max(current);
        } else {
            current = 0;
        }
    }
    (current, best)
}

/// Recompute `current_streak` and `best_streak` for every user across all
/// tenants. Idempotent: it replays the full settled-bet history on each call,
/// so repeated runs converge to the same values. Users with no settled bets
/// keep their default of 0.
pub async fn recompute_all(pool: &PgPool) -> anyhow::Result<()> {
    // Every settled bet with its match, ordered per user by kickoff so we can
    // fold each user's history in a single linear pass below.
    let rows: Vec<(Uuid, i32)> = sqlx::query_as(
        r#"
        SELECT b.user_id, COALESCE(b.points, 0)
        FROM bets b
        JOIN matches m ON m.id = b.match_id
        WHERE m.status = 'FINISHED'
          AND m.home_score IS NOT NULL
          AND m.away_score IS NOT NULL
          AND b.points IS NOT NULL
        ORDER BY b.user_id, m.kickoff_at ASC
        "#,
    )
    .fetch_all(pool)
    .await?;

    // Group consecutive rows by user (the ORDER BY guarantees each user's rows
    // are contiguous), preserving kickoff order within each group.
    let mut grouped: Vec<(Uuid, Vec<i32>)> = Vec::new();
    for (uid, points) in rows {
        match grouped.last_mut() {
            Some((last_uid, pts)) if *last_uid == uid => pts.push(points),
            _ => grouped.push((uid, vec![points])),
        }
    }

    if grouped.is_empty() {
        return Ok(());
    }

    let mut ids: Vec<Uuid> = Vec::with_capacity(grouped.len());
    let mut currents: Vec<i32> = Vec::with_capacity(grouped.len());
    let mut bests: Vec<i32> = Vec::with_capacity(grouped.len());
    for (uid, pts) in &grouped {
        let (cur, best) = streak_of(pts);
        ids.push(*uid);
        currents.push(cur);
        bests.push(best);
    }

    // Single bulk update via parallel arrays. The WHERE clause skips rows that
    // already match, keeping the write small on the common no-change tick.
    sqlx::query(
        r#"
        UPDATE users u
        SET current_streak = data.cur,
            best_streak = data.best
        FROM (
            SELECT unnest($1::uuid[]) AS id,
                   unnest($2::int[]) AS cur,
                   unnest($3::int[]) AS best
        ) data
        WHERE u.id = data.id
          AND (u.current_streak <> data.cur OR u.best_streak <> data.best)
        "#,
    )
    .bind(&ids)
    .bind(&currents)
    .bind(&bests)
    .execute(pool)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::streak_of;

    #[test]
    fn no_bets_is_zero() {
        assert_eq!(streak_of(&[]), (0, 0));
    }

    #[test]
    fn unbroken_run_counts_current_and_best() {
        assert_eq!(streak_of(&[3, 1, 3]), (3, 3));
    }

    #[test]
    fn trailing_zero_resets_current_keeps_best() {
        assert_eq!(streak_of(&[3, 3, 0]), (0, 2));
    }

    #[test]
    fn break_then_resume_tracks_both() {
        assert_eq!(streak_of(&[1, 1, 1, 0, 1]), (1, 3));
    }

    #[test]
    fn leading_zero_does_not_count() {
        assert_eq!(streak_of(&[0, 1, 1]), (2, 2));
    }
}
