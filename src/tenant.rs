use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::Context;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use regex::Regex;
use sqlx::PgPool;
use tokio::sync::RwLock;
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

    /// Resolved logo URL for templates: the tenant's own override if set,
    /// otherwise the bundled Lunatech logo so older deployments keep their
    /// look unchanged.
    pub fn logo_url_or_default(&self) -> &str {
        self.logo_url.as_deref().unwrap_or("/static/lunatech-logo.svg")
    }
}

/// Load every tenant row in creation order. Used by background jobs that fan
/// out work per tenant (fixture sync, reminders). The list is freshly fetched
/// on every call, so newly-created tenants are picked up on the next tick.
pub async fn load_all(pool: &PgPool) -> anyhow::Result<Vec<Tenant>> {
    let rows: Vec<TenantRow> = sqlx::query_as(
        r#"
        SELECT id, slug, name, allowed_email_pattern, logo_url,
               primary_color, accent_color, football_competition,
               stake_deadline, reminder_lead_minutes, slack_webhook_url,
               mail_from, admin_emails
        FROM tenants
        ORDER BY created_at ASC
        "#,
    )
    .fetch_all(pool)
    .await
    .context("loading all tenants")?;

    rows.into_iter().map(Tenant::try_from).collect()
}

/// Look up a tenant by slug. Returns `None` if the slug doesn't match any row.
pub async fn resolve_by_slug(pool: &PgPool, slug: &str) -> anyhow::Result<Option<Tenant>> {
    let row: Option<TenantRow> = sqlx::query_as(
        r#"
        SELECT id, slug, name, allowed_email_pattern, logo_url,
               primary_color, accent_color, football_competition,
               stake_deadline, reminder_lead_minutes, slack_webhook_url,
               mail_from, admin_emails
        FROM tenants
        WHERE slug = $1
        "#,
    )
    .bind(slug)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("looking up tenant `{slug}`"))?;

    row.map(Tenant::try_from).transpose()
}

/// In-process tenant directory: caches resolved tenants by slug, and exposes a
/// well-known "default" tenant for requests that don't carry tenant routing
/// info (no subdomain, no `X-Tenant` header). The cache is positive-only and
/// has no TTL: a server restart is required to pick up tenant config changes.
#[derive(Clone)]
pub struct TenantRegistry {
    default: Tenant,
    cache: Arc<RwLock<HashMap<String, Tenant>>>,
    pool: PgPool,
}

impl TenantRegistry {
    pub fn new(pool: PgPool, default: Tenant) -> Self {
        let mut seed = HashMap::new();
        seed.insert(default.slug.clone(), default.clone());
        Self {
            default,
            cache: Arc::new(RwLock::new(seed)),
            pool,
        }
    }

    pub fn default_tenant(&self) -> &Tenant {
        &self.default
    }

    /// Drop a slug from the in-memory cache so the next request re-fetches
    /// from the database. Call this after admin writes (create/update) so
    /// edits show up immediately without restarting the server.
    pub async fn invalidate(&self, slug: &str) {
        self.cache.write().await.remove(slug);
    }

    pub async fn resolve(&self, slug: &str) -> Option<Tenant> {
        if let Some(t) = self.cache.read().await.get(slug) {
            return Some(t.clone());
        }
        match resolve_by_slug(&self.pool, slug).await {
            Ok(Some(t)) => {
                self.cache.write().await.insert(slug.to_string(), t.clone());
                Some(t)
            }
            Ok(None) => None,
            Err(e) => {
                tracing::warn!(slug = %slug, "tenant lookup failed: {e:#}");
                None
            }
        }
    }
}

/// Axum extractor that yields the tenant the current request is scoped to.
/// The tenant resolution middleware (`resolve_tenant_middleware`) is
/// responsible for inserting a `Tenant` into request extensions; if it didn't
/// run we treat that as a server-side bug and return a 500 rather than
/// silently falling back to the default tenant (which would risk leaking data
/// across tenants).
#[derive(Clone)]
pub struct TenantCtx(pub Tenant);

#[axum::async_trait]
impl<S> FromRequestParts<S> for TenantCtx
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Tenant>()
            .cloned()
            .map(TenantCtx)
            .ok_or_else(|| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "tenant resolution middleware did not run",
                )
                    .into_response()
            })
    }
}
