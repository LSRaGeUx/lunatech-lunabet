use std::collections::HashSet;
use std::env;

use anyhow::{anyhow, Context};
use base64::Engine;
use chrono::{DateTime, Utc};
use rand::RngCore;
use regex::Regex;

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub bind_addr: String,
    pub base_url: String,
    pub cookie_key_bytes: Vec<u8>,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_username: Option<String>,
    pub smtp_password: Option<String>,
    pub smtp_starttls: bool,
    pub mail_from: String,
    pub football_data_api_key: Option<String>,
    pub football_data_competition: String,
    pub allowed_email_domain_pattern: Regex,
    pub slack_webhook_url: Option<String>,
    pub reminder_lead_minutes: i64,
    pub dev_mode: bool,
    pub admin_emails: HashSet<String>,
    pub stake_deadline: DateTime<Utc>,
    pub default_tenant_slug: String,
    pub default_tenant_name: String,
    /// Cross-tenant super-admins (typically the platform operators). These
    /// emails get access to `/admin/tenants` regardless of which tenant they
    /// happen to be signed into.
    pub super_admin_emails: HashSet<String>,
    /// Host names (without port) that should be treated as the platform
    /// apex — no tenant resolution, marketing landing. Anything not in this
    /// list with a subdomain is treated as a tenant request.
    pub apex_hosts: HashSet<String>,
    /// Absolute URL of the platform's marketing apex (e.g. `https://lunabet.eu`).
    /// When set, `/signup` requests that arrive on a tenant subdomain are
    /// redirected here so signup stays a platform-level action. Leave unset
    /// in pre-DNS / single-tenant deployments to keep `/signup` reachable
    /// from any host.
    pub platform_url: Option<String>,
}

impl Config {
    /// Cookie `Domain` attribute to use for session cookies. In multi-tenant
    /// mode (PLATFORM_URL set), this returns the apex host so the same
    /// session cookie is sent across all `*.apex` subdomains. In
    /// single-tenant mode, returns `None` and the browser scopes cookies
    /// to the exact request host.
    pub fn cookie_domain(&self) -> Option<String> {
        let url = self.platform_url.as_deref()?;
        let s = url.trim_end_matches('/');
        let s = s
            .strip_prefix("https://")
            .or_else(|| s.strip_prefix("http://"))
            .unwrap_or(s);
        let host = s.split(':').next().unwrap_or(s);
        if host.is_empty() {
            None
        } else {
            Some(host.to_string())
        }
    }

    pub fn from_env() -> anyhow::Result<Self> {
        let database_url = env::var("DATABASE_URL").context("DATABASE_URL is required")?;
        let bind_addr = env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".into());
        let base_url = env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:3000".into());

        let dev_mode = env::var("DEV_MODE")
            .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);

        let cookie_key_bytes = match env::var("COOKIE_KEY") {
            Ok(b64) => {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(b64.trim())
                    .context("COOKIE_KEY must be valid base64")?;
                if bytes.len() < 64 {
                    return Err(anyhow!("COOKIE_KEY must decode to at least 64 bytes"));
                }
                bytes
            }
            Err(_) if dev_mode => {
                let mut bytes = vec![0u8; 64];
                rand::thread_rng().fill_bytes(&mut bytes);
                tracing::warn!(
                    "DEV_MODE: no COOKIE_KEY set, generated a random one (sessions reset on restart)"
                );
                bytes
            }
            Err(_) => return Err(anyhow!("COOKIE_KEY is required (or enable DEV_MODE=true)")),
        };

        let smtp_host = env::var("SMTP_HOST").unwrap_or_else(|_| "localhost".into());
        let smtp_port = env::var("SMTP_PORT")
            .unwrap_or_else(|_| "1025".into())
            .parse()
            .context("SMTP_PORT must be a number")?;
        let smtp_username = env::var("SMTP_USERNAME").ok().filter(|s| !s.is_empty());
        let smtp_password = env::var("SMTP_PASSWORD").ok().filter(|s| !s.is_empty());
        let smtp_starttls = env::var("SMTP_STARTTLS")
            .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);
        let mail_from = env::var("MAIL_FROM")
            .unwrap_or_else(|_| "lunatech-betting@lunatech.com".into());

        let football_data_api_key = env::var("FOOTBALL_DATA_API_KEY").ok().filter(|s| !s.is_empty());
        let football_data_competition =
            env::var("FOOTBALL_DATA_COMPETITION").unwrap_or_else(|_| "WC".into());

        let allowed_email_domain_raw =
            env::var("ALLOWED_EMAIL_DOMAIN").unwrap_or_else(|_| "lunatech\\.com".into());
        let allowed_email_domain_pattern = Regex::new(&format!("^(?:{allowed_email_domain_raw})$"))
            .context("ALLOWED_EMAIL_DOMAIN must be a valid regex")?;

        let slack_webhook_url = env::var("SLACK_WEBHOOK_URL").ok().filter(|s| !s.is_empty());
        let reminder_lead_minutes = env::var("REMINDER_LEAD_MINUTES")
            .unwrap_or_else(|_| "120".into())
            .parse()
            .context("REMINDER_LEAD_MINUTES must be a number of minutes")?;

        let admin_emails: HashSet<String> = env::var("ADMIN_EMAILS")
            .unwrap_or_default()
            .split(',')
            .filter_map(|s| {
                let t = s.trim().to_lowercase();
                if t.is_empty() {
                    None
                } else {
                    Some(t)
                }
            })
            .collect();

        let stake_deadline = match env::var("STAKE_DEADLINE") {
            Ok(s) if !s.is_empty() => DateTime::parse_from_rfc3339(s.trim())
                .context("STAKE_DEADLINE must be an RFC3339 timestamp")?
                .with_timezone(&Utc),
            _ => {
                // End of group stage WC2026: June 27 2026, 23:59 UTC
                DateTime::parse_from_rfc3339("2026-06-27T23:59:00Z")
                    .unwrap()
                    .with_timezone(&Utc)
            }
        };

        let default_tenant_slug =
            env::var("DEFAULT_TENANT_SLUG").unwrap_or_else(|_| "lunatech".into());
        let default_tenant_name =
            env::var("DEFAULT_TENANT_NAME").unwrap_or_else(|_| "Lunatech".into());

        let platform_url = env::var("PLATFORM_URL").ok().filter(|s| !s.is_empty());

        let apex_hosts: HashSet<String> = env::var("APEX_HOSTS")
            .unwrap_or_else(|_| "lunabet.eu,www.lunabet.eu".into())
            .split(',')
            .filter_map(|s| {
                let t = s.trim().to_lowercase();
                if t.is_empty() {
                    None
                } else {
                    Some(t)
                }
            })
            .collect();

        let super_admin_emails: HashSet<String> = env::var("SUPER_ADMIN_EMAILS")
            .unwrap_or_default()
            .split(',')
            .filter_map(|s| {
                let t = s.trim().to_lowercase();
                if t.is_empty() {
                    None
                } else {
                    Some(t)
                }
            })
            .collect();

        Ok(Self {
            database_url,
            bind_addr,
            base_url,
            cookie_key_bytes,
            smtp_host,
            smtp_port,
            smtp_username,
            smtp_password,
            smtp_starttls,
            mail_from,
            football_data_api_key,
            football_data_competition,
            allowed_email_domain_pattern,
            slack_webhook_url,
            reminder_lead_minutes,
            dev_mode,
            admin_emails,
            stake_deadline,
            default_tenant_slug,
            default_tenant_name,
            super_admin_emails,
            apex_hosts,
            platform_url,
        })
    }
}
