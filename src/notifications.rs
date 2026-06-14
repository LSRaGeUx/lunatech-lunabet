use std::collections::HashMap;

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Europe::Amsterdam;
use serde_json::json;
use uuid::Uuid;

use crate::mail;
use crate::state::AppState;
use crate::stakes;
use crate::tenant::{self, Tenant};

pub async fn send_match_reminders(state: &AppState) -> anyhow::Result<()> {
    let tenants = tenant::load_all(&state.pool).await?;
    for t in tenants {
        if let Err(e) = send_for_tenant(state, &t).await {
            tracing::warn!(tenant = %t.slug, "match reminders failed: {e:#}");
        }
    }
    Ok(())
}

async fn send_for_tenant(state: &AppState, tenant: &Tenant) -> anyhow::Result<()> {
    let lead = Duration::minutes(tenant.reminder_lead_minutes);
    let now = Utc::now();
    let window_end = now + lead;

    // Matches kicking off within the lead window for which this tenant hasn't
    // sent a reminder yet. We join the per-tenant `match_reminders` table so
    // each tenant gets its own reminder lifecycle (one tenant's send no longer
    // suppresses another's).
    let matches: Vec<(
        i64,
        String,
        String,
        DateTime<Utc>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        r#"
        SELECT m.id, m.home_team, m.away_team, m.kickoff_at, m.stage, m.group_name
        FROM matches m
        WHERE m.kickoff_at > $1
          AND m.kickoff_at <= $2
          AND (m.status = 'SCHEDULED' OR m.status = 'TIMED')
          AND NOT EXISTS (
            SELECT 1 FROM match_reminders r
            WHERE r.tenant_id = $3 AND r.match_id = m.id
          )
        ORDER BY m.kickoff_at ASC
        "#,
    )
    .bind(now)
    .bind(window_end)
    .bind(tenant.id)
    .fetch_all(&state.pool)
    .await?;

    if matches.is_empty() {
        return Ok(());
    }

    let tenant_url = tenant.public_url(&state.cfg);

    for (match_id, home, away, kickoff, stage, group) in matches {
        tracing::info!(
            tenant = %tenant.slug,
            "sending reminders for match {match_id}: {home} - {away}"
        );

        // Only users who haven't bet on this match get a kick-off reminder.
        let unbet_users: Vec<(String, String)> = sqlx::query_as(
            r#"
            SELECT u.email, u.lang
            FROM users u
            WHERE u.tenant_id = $1
              AND NOT EXISTS (
                SELECT 1 FROM bets b
                WHERE b.user_id = u.id AND b.match_id = $2 AND b.tenant_id = u.tenant_id
              )
            "#,
        )
        .bind(tenant.id)
        .bind(match_id)
        .fetch_all(&state.pool)
        .await?;

        let kickoff_local = kickoff.with_timezone(&Amsterdam).format("%H:%M %Z").to_string();
        let mut emails_sent = 0usize;
        let mut emails_failed = 0usize;
        for (email, lang) in &unbet_users {
            // French only if the user explicitly chose it, English otherwise.
            let loc = crate::i18n::Locale::from_code(lang).unwrap_or_default();
            match mail::send_bet_reminder(
                &state.cfg,
                tenant,
                loc,
                email,
                &home,
                &away,
                &kickoff_local,
                &tenant_url,
            )
            .await
            {
                Ok(_) => emails_sent += 1,
                Err(e) => {
                    emails_failed += 1;
                    tracing::warn!("reminder email to {email} failed: {e:#}");
                }
            }
        }
        tracing::info!(
            tenant = %tenant.slug,
            "match {match_id}: {emails_sent} reminder emails sent, {emails_failed} failed, {} users without bet",
            unbet_users.len()
        );

        if let Some(webhook) = &tenant.slack_webhook_url {
            let context_bits = [stage.as_deref(), group.as_deref()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" - ");
            let context_line = if context_bits.is_empty() {
                String::new()
            } else {
                format!(" ({context_bits})")
            };
            let text = format!(
                ":soccer: *{home}* vs *{away}*{context_line} commence à {kickoff_local} (dans <{lead_min} min).\nPariez maintenant : {base}/matches",
                lead_min = tenant.reminder_lead_minutes,
                base = tenant_url,
            );
            let payload = json!({ "text": text });
            match state.http.post(webhook).json(&payload).send().await {
                Ok(resp) if resp.status().is_success() => {
                    tracing::info!(tenant = %tenant.slug, "Slack reminder posted for match {match_id}");
                }
                Ok(resp) => {
                    tracing::warn!(
                        tenant = %tenant.slug,
                        "Slack webhook returned {} for match {match_id}",
                        resp.status()
                    );
                }
                Err(e) => tracing::warn!(
                    tenant = %tenant.slug,
                    "Slack webhook failed for match {match_id}: {e:#}"
                ),
            }
        }

        sqlx::query(
            "INSERT INTO match_reminders (tenant_id, match_id) VALUES ($1, $2) \
             ON CONFLICT DO NOTHING",
        )
        .bind(tenant.id)
        .bind(match_id)
        .execute(&state.pool)
        .await?;

        // Keep the legacy denormalised flag in sync so existing dashboards or
        // ad-hoc queries that watch `matches.reminded_at` still see activity.
        sqlx::query(
            "UPDATE matches SET reminded_at = COALESCE(reminded_at, NOW()) WHERE id = $1",
        )
        .bind(match_id)
        .execute(&state.pool)
        .await?;
    }

    Ok(())
}

/// One-time-per-code backfill guard for badge-unlock emails. The first time a
/// catalogue code is seen, every grant of it that already exists is stamped as
/// already-announced (without emailing) and the code is recorded in
/// `badge_notify_codes`. This is what actually prevents an email blast when the
/// feature ships (the whole catalogue is "new" against an empty table) and when
/// a new badge is added later (`evaluate_all` back-grants it to all qualifying
/// users). Grants of an already-recorded code are left untouched so genuine
/// unlocks still get emailed. Idempotent and cheap once every code is recorded,
/// so it's safe to run on every startup.
pub async fn init_badge_notifications(state: &AppState) -> anyhow::Result<()> {
    // Populate the table first so "grants that already exist" reflects current
    // standings, not whatever happened to be there before this process started.
    crate::achievements::evaluate_all(&state.pool).await?;

    let recorded: Vec<(String,)> = sqlx::query_as("SELECT code FROM badge_notify_codes")
        .fetch_all(&state.pool)
        .await?;
    let recorded: std::collections::HashSet<String> = recorded.into_iter().map(|(c,)| c).collect();

    for def in crate::achievements::CATALOG {
        if recorded.contains(def.code) {
            continue;
        }
        // Brand-new code: treat every pre-existing grant as already announced.
        sqlx::query("UPDATE achievements SET notified_at = NOW() WHERE code = $1 AND notified_at IS NULL")
            .bind(def.code)
            .execute(&state.pool)
            .await?;
        sqlx::query("INSERT INTO badge_notify_codes (code) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(def.code)
            .execute(&state.pool)
            .await?;
        tracing::info!(code = def.code, "badge notifications initialised (existing grants backfilled)");
    }
    Ok(())
}

/// Email every player the badges they just unlocked. Picks up `achievements`
/// rows not yet emailed (`notified_at IS NULL`), sends one message per player
/// listing all their fresh badges, then stamps the rows it announced. Idempotent
/// via `notified_at`, so it's safe to call on every scoring tick: badges already
/// emailed are skipped, and a send failure leaves the row unstamped to retry.
/// Relies on [`init_badge_notifications`] having run first so retroactive grants
/// don't trigger a blast.
pub async fn send_badge_unlocks(state: &AppState) -> anyhow::Result<()> {
    for t in tenant::load_all(&state.pool).await? {
        if let Err(e) = badge_unlocks_for_tenant(state, &t).await {
            tracing::warn!(tenant = %t.slug, "badge unlock emails failed: {e:#}");
        }
    }
    Ok(())
}

async fn badge_unlocks_for_tenant(state: &AppState, tenant: &Tenant) -> anyhow::Result<()> {
    // Unannounced badges with their owner's contact details, in earned order so
    // a player's email lists the badges in the sequence they unlocked them.
    let rows: Vec<(Uuid, String, String, String)> = sqlx::query_as(
        r#"
        SELECT a.user_id, u.email, u.lang, a.code
        FROM achievements a
        JOIN users u ON u.id = a.user_id AND u.tenant_id = a.tenant_id
        WHERE a.tenant_id = $1 AND a.notified_at IS NULL
        ORDER BY a.user_id, a.earned_at
        "#,
    )
    .bind(tenant.id)
    .fetch_all(&state.pool)
    .await?;

    if rows.is_empty() {
        return Ok(());
    }

    // Group the fresh badges per player, preserving order.
    let mut per_user: Vec<(Uuid, String, String, Vec<String>)> = Vec::new();
    for (uid, email, lang, code) in rows {
        match per_user.last_mut() {
            Some((last_uid, _, _, codes)) if *last_uid == uid => codes.push(code),
            _ => per_user.push((uid, email, lang, vec![code])),
        }
    }

    let tenant_url = tenant.public_url(&state.cfg);
    let mut sent = 0usize;
    let mut failed = 0usize;
    for (uid, email, lang, codes) in &per_user {
        let loc = crate::i18n::Locale::from_code(lang).unwrap_or_default();
        // Resolve only the codes this binary knows about. An unknown code is left
        // unstamped (notified_at stays NULL) so it's retried: during a rolling
        // deploy a newer node may have added a badge this node hasn't learned yet
        // — stamping it here would silently drop that user's email forever.
        let known: Vec<&String> = codes
            .iter()
            .filter(|c| crate::achievements::def(c).is_some())
            .collect();
        if known.is_empty() {
            continue;
        }
        let badges: Vec<mail::BadgeUnlock> = known
            .iter()
            .filter_map(|c| crate::achievements::def(c))
            .map(|d| mail::BadgeUnlock {
                name: d.name(loc).to_string(),
                desc: d.desc(loc).to_string(),
                icon_url: mail::absolute_url(&tenant_url, &d.icon_path()),
            })
            .collect();

        match mail::send_badge_unlock_email(&state.cfg, tenant, loc, email, &badges, &tenant_url)
            .await
        {
            Ok(_) => sent += 1,
            Err(e) => {
                failed += 1;
                tracing::warn!("badge unlock email to {email} failed: {e:#}");
                // Leave notified_at NULL so the next tick retries this user.
                continue;
            }
        }

        // Stamp only the codes we actually announced. A failure here (after the
        // email already went out) must not abort the whole tenant batch, so log
        // and continue rather than `?`: at worst the row stays NULL and the next
        // tick re-sends — at-least-once is preferred over dropping the rest of
        // the batch.
        let known_codes: Vec<String> = known.iter().map(|c| (*c).clone()).collect();
        if let Err(e) = sqlx::query(
            "UPDATE achievements SET notified_at = NOW() \
             WHERE tenant_id = $1 AND user_id = $2 AND code = ANY($3) AND notified_at IS NULL",
        )
        .bind(tenant.id)
        .bind(uid)
        .bind(&known_codes)
        .execute(&state.pool)
        .await
        {
            tracing::warn!(user = %uid, "stamping badge notifications failed: {e:#}");
        }
    }

    if sent > 0 || failed > 0 {
        tracing::info!(tenant = %tenant.slug, "badge unlocks: {sent} emails sent, {failed} failed");
    }
    Ok(())
}

/// Daily recap: for the given UTC calendar `date`, email every user of every
/// tenant the day's results, the points they earned that day, and the current
/// leaderboard. Idempotent per (tenant, date) via the `daily_digests` table, so
/// it's safe to call repeatedly. Days with no finished match are skipped.
/// When `force` is set the per-tenant idempotency check is bypassed, so a
/// super-admin can resend the digest on demand even after the scheduled run.
pub async fn send_daily_digest(
    state: &AppState,
    date: NaiveDate,
    force: bool,
) -> anyhow::Result<()> {
    let start = Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap());
    let end = start + Duration::days(1);
    let day_label = date.format("%d/%m/%Y").to_string();

    // Matches aren't tenant-scoped yet (Phase 1), so the day's results are the
    // same set for everyone; fetch them once.
    let rows: Vec<(String, String, i32, i32, Option<String>)> = sqlx::query_as(
        r#"
        SELECT home_team, away_team, home_score, away_score, group_name
        FROM matches
        WHERE status = 'FINISHED' AND home_score IS NOT NULL AND away_score IS NOT NULL
          AND kickoff_at >= $1 AND kickoff_at < $2
        ORDER BY kickoff_at ASC
        "#,
    )
    .bind(start)
    .bind(end)
    .fetch_all(&state.pool)
    .await?;

    if rows.is_empty() {
        tracing::info!("daily digest {day_label}: no finished matches, skipping");
        return Ok(());
    }

    let results: Vec<mail::DigestResult> = rows
        .into_iter()
        .map(|(home, away, hs, as_, group)| mail::DigestResult {
            home,
            away,
            home_score: hs,
            away_score: as_,
            group,
        })
        .collect();

    for t in tenant::load_all(&state.pool).await? {
        if let Err(e) =
            digest_for_tenant(state, &t, date, &day_label, &results, start, end, force).await
        {
            tracing::warn!(tenant = %t.slug, "daily digest failed: {e:#}");
        }
    }
    Ok(())
}

async fn digest_for_tenant(
    state: &AppState,
    tenant: &Tenant,
    date: NaiveDate,
    day_label: &str,
    results: &[mail::DigestResult],
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    force: bool,
) -> anyhow::Result<()> {
    if !force {
        let already: Option<i32> = sqlx::query_scalar(
            "SELECT 1 FROM daily_digests WHERE tenant_id = $1 AND digest_date = $2",
        )
        .bind(tenant.id)
        .bind(date)
        .fetch_optional(&state.pool)
        .await?;
        if already.is_some() {
            return Ok(());
        }
    }

    let board = stakes::load_leaderboard(&state.pool, tenant.id).await?;
    if board.is_empty() {
        return Ok(());
    }

    // Best predictor of the day, recorded so the Today screen can show it too.
    let potd = crate::highlights::upsert_player_of_the_day(&state.pool, tenant.id, date).await?;
    let digest_potd = potd.as_ref().map(|p| mail::DigestPotd {
        name: p.display_name.clone(),
        points: p.points,
    });

    // Points each user earned on this day.
    let day_rows: Vec<(Uuid, i64)> = sqlx::query_as(
        r#"
        SELECT b.user_id, COALESCE(SUM(b.points), 0)::BIGINT
        FROM bets b
        JOIN matches m ON m.id = b.match_id
        WHERE b.tenant_id = $1
          AND m.status = 'FINISHED'
          AND m.kickoff_at >= $2 AND m.kickoff_at < $3
          AND b.points IS NOT NULL
        GROUP BY b.user_id
        "#,
    )
    .bind(tenant.id)
    .bind(start)
    .bind(end)
    .fetch_all(&state.pool)
    .await?;
    let day_points: HashMap<Uuid, i64> = day_rows.into_iter().collect();

    let contacts: Vec<(Uuid, String, String)> =
        sqlx::query_as("SELECT id, email, lang FROM users WHERE tenant_id = $1")
            .bind(tenant.id)
            .fetch_all(&state.pool)
            .await?;
    let contact_map: HashMap<Uuid, (String, String)> =
        contacts.into_iter().map(|(id, e, l)| (id, (e, l))).collect();

    // Top 10 standings, shared base; `is_me` is set per recipient.
    let top: Vec<(Uuid, usize, String, i64)> = board
        .iter()
        .enumerate()
        .take(10)
        .map(|(i, r)| (r.user_id, i + 1, r.display_name.clone(), r.points))
        .collect();

    let tenant_url = tenant.public_url(&state.cfg);
    let mut sent = 0usize;
    let mut failed = 0usize;
    for (idx, row) in board.iter().enumerate() {
        let Some((email, lang)) = contact_map.get(&row.user_id) else {
            continue;
        };
        let loc = crate::i18n::Locale::from_code(lang).unwrap_or_default();
        let standings: Vec<mail::DigestStanding> = top
            .iter()
            .map(|(uid, rank, name, points)| mail::DigestStanding {
                rank: *rank,
                name: name.clone(),
                points: *points,
                is_me: *uid == row.user_id,
            })
            .collect();
        let my_points = day_points.get(&row.user_id).copied().unwrap_or(0);
        match mail::send_daily_digest_email(
            &state.cfg,
            tenant,
            loc,
            email,
            day_label,
            results,
            digest_potd.as_ref(),
            my_points,
            idx + 1,
            row.points,
            &standings,
            &tenant_url,
        )
        .await
        {
            Ok(_) => sent += 1,
            Err(e) => {
                failed += 1;
                tracing::warn!("digest to {email} failed: {e:#}");
            }
        }
    }

    sqlx::query("INSERT INTO daily_digests (tenant_id, digest_date) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(tenant.id)
        .bind(date)
        .execute(&state.pool)
        .await?;
    tracing::info!(tenant = %tenant.slug, "daily digest {day_label}: {sent} sent, {failed} failed");
    Ok(())
}

/// Morning preview: for the given UTC `date`, email every user of every tenant
/// the list of matches kicking off that day, each annotated with their current
/// prediction and a "still time to change it" nudge. Idempotent per (tenant,
/// date) via `today_matches_emails`. Skipped when no match is scheduled.
/// When `force` is set the per-tenant idempotency check is bypassed, so a
/// super-admin can resend the preview on demand even after the scheduled run.
pub async fn send_today_matches(
    state: &AppState,
    date: NaiveDate,
    force: bool,
) -> anyhow::Result<()> {
    let start = Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap());
    let end = start + Duration::days(1);
    let day_label = date.format("%d/%m/%Y").to_string();

    let match_rows: Vec<(i64, String, String, DateTime<Utc>, Option<String>)> = sqlx::query_as(
        r#"
        SELECT id, home_team, away_team, kickoff_at, group_name
        FROM matches
        WHERE kickoff_at >= $1 AND kickoff_at < $2
        ORDER BY kickoff_at ASC
        "#,
    )
    .bind(start)
    .bind(end)
    .fetch_all(&state.pool)
    .await?;

    if match_rows.is_empty() {
        tracing::info!("today's matches {day_label}: none scheduled, skipping");
        return Ok(());
    }
    let match_ids: Vec<i64> = match_rows.iter().map(|r| r.0).collect();

    for t in tenant::load_all(&state.pool).await? {
        if let Err(e) =
            today_matches_for_tenant(state, &t, date, &day_label, &match_rows, &match_ids, force)
                .await
        {
            tracing::warn!(tenant = %t.slug, "today's matches email failed: {e:#}");
        }
    }
    Ok(())
}

async fn today_matches_for_tenant(
    state: &AppState,
    tenant: &Tenant,
    date: NaiveDate,
    day_label: &str,
    match_rows: &[(i64, String, String, DateTime<Utc>, Option<String>)],
    match_ids: &[i64],
    force: bool,
) -> anyhow::Result<()> {
    if !force {
        let already: Option<i32> = sqlx::query_scalar(
            "SELECT 1 FROM today_matches_emails WHERE tenant_id = $1 AND match_date = $2",
        )
        .bind(tenant.id)
        .bind(date)
        .fetch_optional(&state.pool)
        .await?;
        if already.is_some() {
            return Ok(());
        }
    }

    let contacts: Vec<(Uuid, String, String)> =
        sqlx::query_as("SELECT id, email, lang FROM users WHERE tenant_id = $1")
            .bind(tenant.id)
            .fetch_all(&state.pool)
            .await?;
    if contacts.is_empty() {
        return Ok(());
    }

    // Each user's prediction on today's matches.
    let bet_rows: Vec<(Uuid, i64, i32, i32)> = sqlx::query_as(
        "SELECT user_id, match_id, home_score, away_score \
         FROM bets WHERE tenant_id = $1 AND match_id = ANY($2)",
    )
    .bind(tenant.id)
    .bind(match_ids)
    .fetch_all(&state.pool)
    .await?;
    let bets: HashMap<(Uuid, i64), (i32, i32)> = bet_rows
        .into_iter()
        .map(|(uid, mid, h, a)| ((uid, mid), (h, a)))
        .collect();

    let tenant_url = tenant.public_url(&state.cfg);
    let mut sent = 0usize;
    let mut failed = 0usize;
    for (uid, email, lang) in &contacts {
        let loc = crate::i18n::Locale::from_code(lang).unwrap_or_default();
        let matches: Vec<mail::TodayMatch> = match_rows
            .iter()
            .map(|(id, home, away, kickoff, _group)| mail::TodayMatch {
                home: home.clone(),
                away: away.clone(),
                kickoff_local: kickoff.with_timezone(&Amsterdam).format("%H:%M %Z").to_string(),
                bet: bets
                    .get(&(*uid, *id))
                    .map(|(h, a)| mail::ScorePair { home: *h, away: *a }),
            })
            .collect();
        match mail::send_today_matches_email(
            &state.cfg, tenant, loc, email, day_label, &matches, &tenant_url,
        )
        .await
        {
            Ok(_) => sent += 1,
            Err(e) => {
                failed += 1;
                tracing::warn!("today's matches email to {email} failed: {e:#}");
            }
        }
    }

    sqlx::query("INSERT INTO today_matches_emails (tenant_id, match_date) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(tenant.id)
        .bind(date)
        .execute(&state.pool)
        .await?;
    tracing::info!(tenant = %tenant.slug, "today's matches {day_label}: {sent} sent, {failed} failed");
    Ok(())
}
