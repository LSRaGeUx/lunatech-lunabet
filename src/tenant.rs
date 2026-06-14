use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::Context;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Redirect, Response};
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
    /// `domain` (company: auto-join by email domain) or `invite` (friends:
    /// join only via an invitation).
    pub membership_mode: String,
    /// Whether non-admin members may send invitations.
    pub members_can_invite: bool,
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
    membership_mode: String,
    members_can_invite: bool,
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
            membership_mode: r.membership_mode,
            members_can_invite: r.members_can_invite,
        })
    }
}

/// Make sure the deployment's default tenant exists, creating it from the
/// env-driven `Config` on the very first boot. After the row exists, the
/// env vars are no longer consulted: subsequent edits go through
/// `/admin/tenants/<slug>/edit` and stick across deploys. Env vars like
/// `ALLOWED_EMAIL_DOMAIN`, `MAIL_FROM`, `STAKE_DEADLINE`, etc. are therefore
/// bootstrap-only.
pub async fn ensure_default(pool: &PgPool, cfg: &Config) -> anyhow::Result<Tenant> {
    let pattern_src = cfg.allowed_email_domain_pattern.as_str();
    // Strip the `^(?:…)$` wrapper that `Config::from_env` adds, so the value
    // we store in the DB matches the shape a tenant admin would type.
    let pattern_stored = pattern_src
        .trim_start_matches("^(?:")
        .trim_end_matches(")$")
        .to_string();

    let admins: Vec<String> = cfg.admin_emails.iter().cloned().collect();

    let inserted: Option<TenantRow> = sqlx::query_as(
        r#"
        INSERT INTO tenants
            (slug, name, allowed_email_pattern, mail_from, stake_deadline,
             reminder_lead_minutes, slack_webhook_url, football_competition,
             admin_emails)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (slug) DO NOTHING
        RETURNING id, slug, name, allowed_email_pattern, logo_url,
                  primary_color, accent_color, football_competition,
                  stake_deadline, reminder_lead_minutes, slack_webhook_url,
                  mail_from, admin_emails, membership_mode, members_can_invite
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
    .fetch_optional(pool)
    .await
    .context("bootstrapping default tenant from env config")?;

    if let Some(row) = inserted {
        tracing::info!(slug = %cfg.default_tenant_slug, "default tenant bootstrapped from env");
        return Tenant::try_from(row);
    }

    // Row already exists; the env vars are not authoritative anymore, just
    // load whatever the admin UI / migrations have stored.
    resolve_by_slug(pool, &cfg.default_tenant_slug)
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "default tenant `{}` disappeared between INSERT and SELECT",
                cfg.default_tenant_slug
            )
        })
}

impl Tenant {
    pub fn is_admin(&self, email: &str) -> bool {
        self.admin_emails.contains(&email.to_lowercase())
    }

    /// True when this space grows by invitation rather than by email domain.
    pub fn is_invite_mode(&self) -> bool {
        self.membership_mode == "invite"
    }

    /// Resolved logo URL for templates: the tenant's own override if set,
    /// otherwise the bundled Lunatech logo so older deployments keep their
    /// look unchanged.
    pub fn logo_url_or_default(&self) -> &str {
        self.logo_url.as_deref().unwrap_or("/static/lunatech-logo.svg")
    }

    /// Absolute base URL where this tenant's pages live (no trailing slash).
    /// Used to build magic-link callback URLs, email logo URLs, Slack
    /// message links, etc.
    ///
    /// - When `PLATFORM_URL` is set (multi-tenant DNS mode), tenant URLs are
    ///   derived as `https://{slug}.{apex_host}`.
    /// - Otherwise (single-tenant / pre-DNS deployments), falls back to
    ///   `BASE_URL`, which then holds the deployment's single URL.
    pub fn public_url(&self, cfg: &crate::config::Config) -> String {
        public_url_for_slug(&self.slug, cfg)
    }
}

/// Same as `Tenant::public_url`, but takes a slug directly so callers (e.g.
/// the central login at the apex) can build a tenant URL without having a
/// fully hydrated `Tenant`.
pub fn public_url_for_slug(slug: &str, cfg: &crate::config::Config) -> String {
    if let Some(platform) = cfg.platform_url.as_deref() {
        let trimmed = platform.trim_end_matches('/');
        if let Some(rest) = trimmed.strip_prefix("https://") {
            return format!("https://{}.{}", slug, rest);
        }
        if let Some(rest) = trimmed.strip_prefix("http://") {
            return format!("http://{}.{}", slug, rest);
        }
    }
    cfg.base_url.trim_end_matches('/').to_string()
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
               mail_from, admin_emails, membership_mode, members_can_invite
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
               mail_from, admin_emails, membership_mode, members_can_invite
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
/// has no TTL; edits become visible by calling [`TenantRegistry::invalidate`]
/// after a write, which drops the slug so the next request re-fetches it.
#[derive(Clone)]
pub struct TenantRegistry {
    default: Tenant,
    default_slug: String,
    cache: Arc<RwLock<HashMap<String, Tenant>>>,
    pool: PgPool,
}

impl TenantRegistry {
    pub fn new(pool: PgPool, default: Tenant) -> Self {
        let mut seed = HashMap::new();
        seed.insert(default.slug.clone(), default.clone());
        Self {
            default_slug: default.slug.clone(),
            default,
            cache: Arc::new(RwLock::new(seed)),
            pool,
        }
    }

    /// The startup snapshot of the default tenant. Frozen at boot — prefer
    /// [`TenantRegistry::resolve_default`] on the request path so edits to the
    /// default tenant (e.g. via the admin settings page) are picked up without
    /// a restart. This stays as a last-resort fallback.
    pub fn default_tenant(&self) -> &Tenant {
        &self.default
    }

    /// Resolve the default tenant through the cache so admin edits are
    /// reflected after an `invalidate`. Falls back to the boot snapshot if the
    /// row can't be loaded (e.g. transient DB error).
    pub async fn resolve_default(&self) -> Tenant {
        self.resolve(&self.default_slug)
            .await
            .unwrap_or_else(|| self.default.clone())
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
/// On apex / marketing requests no tenant is attached, so this extractor
/// redirects to `/` instead of leaking data from a default tenant.
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
            .ok_or_else(|| Redirect::to("/").into_response())
    }
}

/// Sentinel inserted into request extensions when the request targets a
/// tenant subdomain (or `X-Tenant` / `?tenant=`) that doesn't match any row.
/// Distinct from "no tenant intended at all" (apex) so we can show a "this
/// space doesn't exist yet, create it?" page instead of silently falling
/// back to the default tenant or to the marketing landing.
#[derive(Clone)]
pub struct UnknownSlug(pub String);

/// Same as `TenantCtx` but tolerant of apex requests: returns `None` when
/// no tenant is attached. Use on routes that legitimately work both on the
/// marketing apex and inside a tenant (the home page, signup).
pub struct MaybeTenant(pub Option<Tenant>);

#[axum::async_trait]
impl<S> FromRequestParts<S> for MaybeTenant
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(MaybeTenant(parts.extensions.get::<Tenant>().cloned()))
    }
}

/// Reject the request with 404 unless it is targeting the platform apex.
/// Use this on routes that must only ever respond on `lunabet.eu`, never
/// on a tenant subdomain (e.g. `/super-admin/*`).
pub struct ApexOnly;

#[axum::async_trait]
impl<S> FromRequestParts<S> for ApexOnly
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        if parts.extensions.get::<Tenant>().is_some() {
            return Err((axum::http::StatusCode::NOT_FOUND, "Not found.").into_response());
        }
        Ok(ApexOnly)
    }
}

pub struct MaybeUnknownSlug(pub Option<String>);

#[axum::async_trait]
impl<S> FromRequestParts<S> for MaybeUnknownSlug
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(MaybeUnknownSlug(
            parts
                .extensions
                .get::<UnknownSlug>()
                .map(|u| u.0.clone()),
        ))
    }
}

impl Tenant {
    /// Synthetic "tenant" representing the LunaBet platform itself. Used as
    /// branding context for pages served from the apex (marketing, signup)
    /// so templates that expect a `tenant` reference can render without a
    /// real DB row.
    pub fn platform() -> Self {
        use regex::Regex;
        Tenant {
            id: Uuid::nil(),
            slug: "_platform".into(),
            name: "LunaBet".into(),
            // Match-nothing regex: no one signs up "as the platform".
            allowed_email_pattern: Arc::new(Regex::new("^$").unwrap()),
            logo_url: None,
            primary_color: "#1d3557".into(),
            accent_color: "#c8232c".into(),
            football_competition: "WC".into(),
            stake_deadline: DateTime::<Utc>::from_timestamp(0, 0).unwrap(),
            reminder_lead_minutes: 0,
            slack_webhook_url: None,
            mail_from: "noreply@lunabet.eu".into(),
            admin_emails: HashSet::new(),
            membership_mode: "domain".into(),
            members_can_invite: false,
        }
    }
}
