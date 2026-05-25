use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Context;
use chrono::{DateTime, Utc};
use regex::Regex;
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::Config;

#[derive(Debug, Clone)]
pub struct Tenant {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub allowed_email_pattern: Arc<Regex>,
    pub logo_url: Option<String>,
    pub primary_color: String,
    pub accent_color: String,
    pub football_competition: String,
    pub stake_deadline: DateTime<Utc>,
    pub reminder_lead_minutes: i64,
    pub slack_webhook_url: Option<String>,
    pub mail_from: String,
    pub admin_emails: HashSet<String>,
}

#[derive(sqlx::FromRow)]
struct TenantRow {
    id: Uuid,
    slug: String,
    name: String,
    allowed_email_pattern: String,
    logo_url: Option<String>,
    primary_color: String,
    accent_color: String,
    football_competition: String,
    stake_deadline: DateTime<Utc>,
    reminder_lead_minutes: i32,
    slack_webhook_url: Option<String>,
    mail_from: String,
    admin_emails: Vec<String>,
}

impl TryFrom<TenantRow> for Tenant {
    type Error = anyhow::Error;
    fn try_from(r: TenantRow) -> anyhow::Result<Self> {
        let pattern = Regex::new(&format!("^(?:{})$", r.allowed_email_pattern))
            .with_context(|| {
                format!(
                    "tenant `{}` has an invalid allowed_email_pattern: {}",
                    r.slug, r.allowed_email_pattern
                )
            })?;
        Ok(Tenant {
            id: r.id,
            slug: r.slug,
            name: r.name,
            allowed_email_pattern: Arc::new(pattern),
            logo_url: r.logo_url,
            primary_color: r.primary_color,
            accent_color: r.accent_color,
            football_competition: r.football_competition,
            stake_deadline: r.stake_deadline,
            reminder_lead_minutes: r.reminder_lead_minutes as i64,
            slack_webhook_url: r.slack_webhook_url,
            mail_from: r.mail_from,
            admin_emails: r.admin_emails.into_iter().collect(),
        })
    }
}

/// Upsert the default tenant from the current env-driven `Config`, then return
/// it. This is the bridge between the single-tenant deployment we already have
/// in production and the multi-tenant data model. As long as a deployment keeps
/// setting the legacy env vars, the matching tenant row is kept in sync on
/// every boot.
pub async fn upsert_from_config(pool: &PgPool, cfg: &Config) -> anyhow::Result<Tenant> {
    let pattern_src = cfg.allowed_email_domain_pattern.as_str();
    // Strip the `^(?:…)$` wrapper that `Config::from_env` adds, so the value we
    // store in the DB is the same shape a tenant admin would type.
    let pattern_stored = pattern_src
        .trim_start_matches("^(?:")
        .trim_end_matches(")$")
        .to_string();

    let admins: Vec<String> = cfg.admin_emails.iter().cloned().collect();

    let row: TenantRow = sqlx::query_as(
        r#"
        INSERT INTO tenants
            (slug, name, allowed_email_pattern, mail_from, stake_deadline,
             reminder_lead_minutes, slack_webhook_url, football_competition,
             admin_emails)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (slug) DO UPDATE SET
            allowed_email_pattern = EXCLUDED.allowed_email_pattern,
            mail_from             = EXCLUDED.mail_from,
            stake_deadline        = EXCLUDED.stake_deadline,
            reminder_lead_minutes = EXCLUDED.reminder_lead_minutes,
            slack_webhook_url     = EXCLUDED.slack_webhook_url,
            football_competition  = EXCLUDED.football_competition,
            admin_emails          = EXCLUDED.admin_emails
        RETURNING id, slug, name, allowed_email_pattern, logo_url,
                  primary_color, accent_color, football_competition,
                  stake_deadline, reminder_lead_minutes, slack_webhook_url,
                  mail_from, admin_emails
        "#,
    )
    .bind(&cfg.default_tenant_slug)
    .bind(&cfg.default_tenant_name)
    .bind(&pattern_stored)
    .bind(&cfg.mail_from)
    .bind(cfg.stake_deadline)
    .bind(cfg.reminder_lead_minutes as i32)
    .bind(&cfg.slack_webhook_url)
    .bind(&cfg.football_data_competition)
    .bind(&admins)
    .fetch_one(pool)
    .await
    .context("upserting default tenant from env config")?;

    Tenant::try_from(row)
}

impl Tenant {
    pub fn is_admin(&self, email: &str) -> bool {
        self.admin_emails.contains(&email.to_lowercase())
    }
}
