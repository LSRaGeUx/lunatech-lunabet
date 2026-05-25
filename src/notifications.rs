use chrono::{DateTime, Duration, Utc};
use serde_json::json;

use crate::mail;
use crate::state::AppState;
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

        let unbet_users: Vec<(String, String)> = sqlx::query_as(
            r#"
            SELECT email, display_name
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

        let kickoff_local = kickoff.format("%H:%M UTC").to_string();
        let mut emails_sent = 0usize;
        let mut emails_failed = 0usize;
        for (email, _name) in &unbet_users {
            match mail::send_bet_reminder(
                &state.cfg,
                tenant,
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
