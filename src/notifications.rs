use chrono::{DateTime, Duration, Utc};
use serde_json::json;

use crate::mail;
use crate::state::AppState;

pub async fn send_match_reminders(state: &AppState) -> anyhow::Result<()> {
    // Background jobs operate against the deployment's default tenant for
    // now. Phase 5 will iterate over all tenants registered in the directory.
    let tenant = state.tenants.default_tenant().clone();
    let lead = Duration::minutes(tenant.reminder_lead_minutes);
    let now = Utc::now();
    let window_end = now + lead;

    let matches: Vec<(
        i64,
        String,
        String,
        DateTime<Utc>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        r#"
        SELECT id, home_team, away_team, kickoff_at, stage, group_name
        FROM matches
        WHERE reminded_at IS NULL
          AND kickoff_at > $1
          AND kickoff_at <= $2
          AND (status = 'SCHEDULED' OR status = 'TIMED')
        ORDER BY kickoff_at ASC
        "#,
    )
    .bind(now)
    .bind(window_end)
    .fetch_all(&state.pool)
    .await?;

    if matches.is_empty() {
        return Ok(());
    }

    for (match_id, home, away, kickoff, stage, group) in matches {
        tracing::info!("sending reminders for match {match_id}: {home} - {away}");

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
                &tenant,
                email,
                &home,
                &away,
                &kickoff_local,
                &state.cfg.base_url,
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
                base = state.cfg.base_url.trim_end_matches('/'),
            );
            let payload = json!({ "text": text });
            match state.http.post(webhook).json(&payload).send().await {
                Ok(resp) if resp.status().is_success() => {
                    tracing::info!("Slack reminder posted for match {match_id}");
                }
                Ok(resp) => {
                    tracing::warn!(
                        "Slack webhook returned {} for match {match_id}",
                        resp.status()
                    );
                }
                Err(e) => tracing::warn!("Slack webhook failed for match {match_id}: {e:#}"),
            }
        }

        sqlx::query("UPDATE matches SET reminded_at = NOW() WHERE id = $1")
            .bind(match_id)
            .execute(&state.pool)
            .await?;
    }

    Ok(())
}
