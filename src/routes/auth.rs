use askama::Template;
use axum::extract::{FromRequestParts, Query, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::Form;
use axum_extra::extract::cookie::{Cookie, CookieJar, PrivateCookieJar, SameSite};
use base64::Engine;
use chrono::{Duration, Utc};
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::AppResult;
use crate::i18n::Locale;
use crate::mail;
use crate::models::User;
use crate::rate_limit::client_ip;
use crate::state::AppState;
use crate::tenant::{public_url_for_slug, MaybeTenant, Tenant, TenantCtx};

const MAGIC_LINK_TTL_MINUTES: i64 = 15;
const SESSION_TTL_DAYS: i64 = 30;

/// Name of the session cookie for a given tenant.
///
/// The session cookie is set on the shared apex domain (so a cookie set on the
/// apex during signup carries onto the freshly created subdomain). With a
/// single fixed name, signing into one org would overwrite another org's
/// cookie, so a user could only be signed into one org at a time. Namespacing
/// the cookie by slug lets several orgs' sessions coexist in the same browser,
/// each subdomain reading its own. The browser still ships every `lb_session_*`
/// cookie to every subdomain (shared domain), which is what lets the org
/// switcher tell which orgs you are already signed into.
pub fn session_cookie_name(tenant_slug: &str) -> String {
    format!("lb_session_{tenant_slug}")
}

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTpl<'a> {
    loc: Locale,
    error: Option<&'a str>,
    tenant: &'a Tenant,
    prefilled_email: &'a str,
}

#[derive(Template)]
#[template(path = "login_sent.html")]
struct LoginSentTpl<'a> {
    loc: Locale,
    tenant: &'a Tenant,
}

#[derive(Template)]
#[template(path = "central_login.html")]
struct CentralLoginTpl<'a> {
    loc: Locale,
    error: Option<&'a str>,
    prefilled_email: &'a str,
}

#[derive(Template)]
#[template(path = "central_login_choose.html")]
struct CentralChooseTpl<'a> {
    loc: Locale,
    email: &'a str,
    options: Vec<TenantOption>,
}

pub struct TenantOption {
    pub slug: String,
    pub name: String,
    pub login_url: String,
}

#[derive(Deserialize)]
pub struct LoginPageQuery {
    email: Option<String>,
}

pub async fn login_page(
    _state: State<AppState>,
    MaybeTenant(maybe_tenant): MaybeTenant,
    loc: Locale,
    Query(q): Query<LoginPageQuery>,
) -> impl IntoResponse {
    let prefilled = q.email.as_deref().unwrap_or("");
    match maybe_tenant {
        Some(tenant) => {
            let tpl = LoginTpl {
                loc,
                error: None,
                tenant: &tenant,
                prefilled_email: prefilled,
            };
            Html(tpl.render().unwrap_or_else(|e| format!("template error: {e}"))).into_response()
        }
        None => {
            let tpl = CentralLoginTpl {
                loc,
                error: None,
                prefilled_email: prefilled,
            };
            Html(tpl.render().unwrap_or_else(|e| format!("template error: {e}"))).into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct LoginForm {
    email: String,
}

/// Encode an email for use in a `?email=...` query string. The only common
/// gotcha is `+` (legal in emails like alice+work@acme.com, decoded as space
/// in query strings), plus `&` and `=` which split parameters. Everything
/// else permitted in emails (RFC 5322 / 6531) is also legal as-is in a
/// query value.
fn email_for_query(s: &str) -> String {
    s.replace('%', "%25")
        .replace('+', "%2B")
        .replace('&', "%26")
        .replace('=', "%3D")
        .replace(' ', "%20")
}

pub async fn request_magic_link(
    State(state): State<AppState>,
    MaybeTenant(maybe_tenant): MaybeTenant,
    loc: Locale,
    headers: axum::http::HeaderMap,
    Form(form): Form<LoginForm>,
) -> AppResult<Response> {
    // Rate limiting: max 10 requests per minute per IP for login endpoint
    let endpoint = "/login";
    let ip = client_ip(&headers);
    if let Some(ip) = ip {
        if !state.endpoint_limiter.check_and_record(endpoint, ip) {
            return Ok(StatusCode::TOO_MANY_REQUESTS.into_response());
        }
    }
    // If we can't determine IP (local dev), allow through

    let email = form.email.trim().to_lowercase();

    if let Some(tenant) = maybe_tenant {
        return tenant_request_magic_link(state.clone(), tenant, loc, email).await;
    }

    // Central apex login: figure out which tenant(s) this email is part of.
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT t.slug, t.name FROM users u \
         JOIN tenants t ON t.id = u.tenant_id \
         WHERE u.email = $1 \
         ORDER BY t.name ASC",
    )
    .bind(&email)
    .fetch_all(&state.pool)
    .await?;

    match rows.len() {
        0 => {
            let msg = loc.f(
                "Aucun compte LunaBet pour cet email. Tu peux créer un espace via le signup.",
                "No LunaBet account for this email. You can create a space via signup.",
            );
            let tpl = CentralLoginTpl {
                loc,
                error: Some(msg),
                prefilled_email: &email,
            };
            Ok((StatusCode::NOT_FOUND, Html(tpl.render()?)).into_response())
        }
        1 => {
            // Single match: send the magic link straight away — no second
            // form to fill. We render the "check your inbox" page in the
            // matching tenant's branding so the user sees where the link
            // will take them.
            let (slug, _name) = &rows[0];
            let Some(tenant) = state.tenants.resolve(slug).await else {
                return Ok((StatusCode::INTERNAL_SERVER_ERROR, "tenant lookup failed").into_response());
            };
            send_magic_link_for_tenant(&state, &tenant, loc, &email).await?;
            let tpl = LoginSentTpl { loc, tenant: &tenant };
            Ok(Html(tpl.render()?).into_response())
        }
        _ => {
            let options: Vec<TenantOption> = rows
                .into_iter()
                .map(|(slug, name)| {
                    let login_url = format!(
                        "{}/login?email={}",
                        public_url_for_slug(&slug, &state.cfg),
                        email_for_query(&email)
                    );
                    TenantOption {
                        slug,
                        name,
                        login_url,
                    }
                })
                .collect();
            let tpl = CentralChooseTpl {
                loc,
                email: &email,
                options,
            };
            Ok(Html(tpl.render()?).into_response())
        }
    }
}

async fn tenant_request_magic_link(
    state: AppState,
    tenant: Tenant,
    loc: Locale,
    email: String,
) -> AppResult<Response> {
    if !is_login_allowed(&state, &tenant, &email).await? {
        let error = if tenant.is_invite_mode() {
            loc.f(
                "Cet espace est sur invitation. Demande à un membre de t'inviter.",
                "This space is invite-only. Ask a member to invite you.",
            )
        } else {
            loc.f(
                "Cette app est réservée à ce tenant.",
                "This app is reserved to this tenant.",
            )
        };
        let tpl = LoginTpl {
            loc,
            error: Some(error),
            tenant: &tenant,
            prefilled_email: &email,
        };
        return Ok((StatusCode::BAD_REQUEST, Html(tpl.render()?)).into_response());
    }

    send_magic_link_for_tenant(&state, &tenant, loc, &email).await?;
    Ok(Redirect::to("/login/sent").into_response())
}

/// Decide whether `email` may sign in to `tenant`. Allowed if any holds:
/// 1. an account already exists for this email in the tenant (established
///    member), or
/// 2. the tenant is in `domain` mode and the email's domain matches its
///    `allowed_email_pattern` (company auto-join), or
/// 3. a live (pending, non-expired) invitation exists for this email.
///
/// This unifies both membership modes: in `invite` mode the pattern is the
/// match-nothing `(?!)`, so only conditions 1 and 3 can open the door.
async fn is_login_allowed(state: &AppState, tenant: &Tenant, email: &str) -> AppResult<bool> {
    let is_member: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM users WHERE tenant_id = $1 AND email = $2)",
    )
    .bind(tenant.id)
    .bind(email)
    .fetch_one(&state.pool)
    .await?;
    if is_member {
        return Ok(true);
    }

    if !tenant.is_invite_mode() {
        if let Some(domain) = email.split_once('@').map(|(_, d)| d) {
            if tenant.allowed_email_pattern.is_match(domain) {
                return Ok(true);
            }
        }
    }

    let invited: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM invitations \
         WHERE tenant_id = $1 AND email = $2 AND status = 'pending' AND expires_at > NOW())",
    )
    .bind(tenant.id)
    .bind(email)
    .fetch_one(&state.pool)
    .await?;
    Ok(invited)
}

/// Generate a magic-link token, persist it in `magic_links`, and send the
/// email. Used by both the per-tenant `/login` POST (after pattern check)
/// and the apex 1-match flow (no pattern check needed — the user is
/// already a member of the tenant).
async fn send_magic_link_for_tenant(
    state: &AppState,
    tenant: &Tenant,
    loc: Locale,
    email: &str,
) -> AppResult<()> {
    let mut raw = [0u8; 32];
    rand::rng().fill_bytes(&mut raw);
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw);
    let token_hash = hex_sha256(&token);
    let expires_at = Utc::now() + Duration::minutes(MAGIC_LINK_TTL_MINUTES);

    sqlx::query(
        "INSERT INTO magic_links (tenant_id, token_hash, email, expires_at) VALUES ($1, $2, $3, $4)",
    )
    .bind(tenant.id)
    .bind(&token_hash)
    .bind(email)
    .bind(expires_at)
    .execute(&state.pool)
    .await?;

    let tenant_url = tenant.public_url(&state.cfg);
    let link = format!("{}/auth/callback?token={}", tenant_url, token);

    if let Err(e) = mail::send_magic_link(&state.cfg, tenant, loc, &tenant_url, email, &link).await {
        tracing::warn!("could not send magic link email to {email}: {e:#}");
        tracing::info!("DEV magic link for {email}: {link}");
    }
    Ok(())
}

pub async fn login_sent(
    TenantCtx(tenant): TenantCtx,
    loc: Locale,
) -> impl IntoResponse {
    let tpl = LoginSentTpl { loc, tenant: &tenant };
    Html(tpl.render().unwrap_or_else(|e| format!("template error: {e}")))
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    token: String,
}

pub async fn callback(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    loc: Locale,
    jar: PrivateCookieJar,
    Query(q): Query<CallbackQuery>,
) -> AppResult<Response> {
    let token_hash = hex_sha256(&q.token);

    let row: Option<(String, chrono::DateTime<Utc>, Option<chrono::DateTime<Utc>>)> =
        sqlx::query_as(
            "SELECT email, expires_at, consumed_at FROM magic_links \
             WHERE token_hash = $1 AND tenant_id = $2",
        )
        .bind(&token_hash)
        .bind(tenant.id)
        .fetch_optional(&state.pool)
        .await?;

    let Some((email, expires_at, consumed_at)) = row else {
        return Ok((StatusCode::BAD_REQUEST, loc.f("Lien invalide.", "Invalid link.")).into_response());
    };
    if consumed_at.is_some() {
        return Ok((StatusCode::BAD_REQUEST, loc.f("Ce lien a déjà été utilisé.", "This link has already been used.")).into_response());
    }
    if expires_at < Utc::now() {
        return Ok((StatusCode::BAD_REQUEST, loc.f("Ce lien a expiré.", "This link has expired.")).into_response());
    }

    sqlx::query("UPDATE magic_links SET consumed_at = NOW() WHERE token_hash = $1")
        .bind(&token_hash)
        .execute(&state.pool)
        .await?;

    let display_name = email
        .split('@')
        .next()
        .unwrap_or(&email)
        .replace('.', " ")
        .replace('_', " ");

    let is_admin = tenant.is_admin(&email);
    // Seed `lang` from the locale of this sign-in request for brand-new users.
    // We intentionally don't overwrite it on conflict: a returning user keeps
    // whatever language they last chose via the FR/EN switcher.
    let user: User = sqlx::query_as(
        r#"
        INSERT INTO users (tenant_id, email, display_name, is_admin, lang)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (tenant_id, email) DO UPDATE
            SET email = EXCLUDED.email,
                is_admin = EXCLUDED.is_admin
        RETURNING id, email, display_name, is_admin, created_at,
                  stake_eur, stake_chosen_at, paid_at
        "#,
    )
    .bind(tenant.id)
    .bind(&email)
    .bind(&display_name)
    .bind(is_admin)
    .bind(loc.code())
    .fetch_one(&state.pool)
    .await?;

    let session_id = Uuid::new_v4();
    let session_expires = Utc::now() + Duration::days(SESSION_TTL_DAYS);
    sqlx::query(
        "INSERT INTO sessions (id, tenant_id, user_id, expires_at) VALUES ($1, $2, $3, $4)",
    )
    .bind(session_id)
    .bind(tenant.id)
    .bind(user.id)
    .bind(session_expires)
    .execute(&state.pool)
    .await?;

    let mut builder = Cookie::build((
        session_cookie_name(&tenant.slug),
        session_id.to_string(),
    ))
    .path("/")
    .http_only(true)
    .same_site(SameSite::Lax)
    .max_age(time::Duration::days(SESSION_TTL_DAYS));
    if let Some(domain) = state.cfg.cookie_domain() {
        builder = builder.domain(domain);
    }
    let jar = jar.add(builder.build());
    Ok((jar, Redirect::to("/today")).into_response())
}

pub async fn logout(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    jar: PrivateCookieJar,
) -> AppResult<Response> {
    // Sign out of the current org only: other orgs keep their own cookies.
    let cookie_name = session_cookie_name(&tenant.slug);
    if let Some(c) = jar.get(&cookie_name) {
        if let Ok(id) = Uuid::parse_str(c.value()) {
            let _ = sqlx::query("DELETE FROM sessions WHERE id = $1")
                .bind(id)
                .execute(&state.pool)
                .await;
        }
    }
    // The removal cookie must carry the same path/domain it was set with so the
    // browser actually clears it.
    let mut removal = Cookie::new(cookie_name, "");
    removal.set_path("/");
    if let Some(domain) = state.cfg.cookie_domain() {
        removal.set_domain(domain);
    }
    let jar = jar.remove(removal);
    Ok((jar, Redirect::to("/")).into_response())
}

pub async fn current_user(
    state: &AppState,
    tenant: &Tenant,
    jar: &PrivateCookieJar,
) -> AppResult<Option<User>> {
    let Some(c) = jar.get(&session_cookie_name(&tenant.slug)) else {
        return Ok(None);
    };
    let Ok(id) = Uuid::parse_str(c.value()) else {
        return Ok(None);
    };
    let user: Option<User> = sqlx::query_as(
        r#"
        SELECT u.id, u.email, u.display_name, u.is_admin, u.created_at,
               u.stake_eur, u.stake_chosen_at, u.paid_at
        FROM sessions s
        JOIN users u ON u.id = s.user_id
        WHERE s.id = $1 AND s.tenant_id = $2 AND s.expires_at > NOW()
        "#,
    )
    .bind(id)
    .bind(tenant.id)
    .fetch_optional(&state.pool)
    .await?;
    Ok(user)
}

pub struct AuthUser(pub User);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let TenantCtx(tenant) = TenantCtx::from_request_parts(parts, state).await?;
        let jar = PrivateCookieJar::<axum_extra::extract::cookie::Key>::from_request_parts(parts, state)
            .await
            .map_err(|e| e.into_response())?;
        match current_user(state, &tenant, &jar).await {
            Ok(Some(u)) => Ok(AuthUser(u)),
            Ok(None) => Err(Redirect::to("/login").into_response()),
            Err(e) => Err(e.into_response()),
        }
    }
}

/// One org in the switcher list.
struct SwitchOrg {
    name: String,
    slug: String,
    /// Where the card links to: the app for orgs you're already signed into,
    /// that org's login (email prefilled) otherwise.
    url: String,
    is_current: bool,
    signed_in: bool,
    /// The login used for this org when it differs from the email shown at the
    /// top of the page (an org you joined under another address). `None` when
    /// it is the same email, so the card stays uncluttered.
    email: Option<String>,
}

#[derive(Template)]
#[template(path = "switch.html")]
struct SwitchTpl<'a> {
    loc: Locale,
    tenant: &'a Tenant,
    email: &'a str,
    orgs: Vec<SwitchOrg>,
}

/// Is there a live session for `tenant_id` in the request's cookie jar? Works
/// across orgs because every `lb_session_*` cookie is sent to every subdomain
/// (shared apex domain), so the switcher can show which orgs you're signed
/// into without leaving the current one.
async fn has_live_session(
    state: &AppState,
    jar: &PrivateCookieJar,
    slug: &str,
    tenant_id: Uuid,
) -> bool {
    let Some(c) = jar.get(&session_cookie_name(slug)) else {
        return false;
    };
    let Ok(id) = Uuid::parse_str(c.value()) else {
        return false;
    };
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM sessions \
         WHERE id = $1 AND tenant_id = $2 AND expires_at > NOW())",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(false)
}

/// Org switcher. Lists the orgs you can switch to, from two sources:
///
/// 1. every org your current email belongs to (same login across them), and
/// 2. every org you already have a live session for under a *different* email
///    -- discovered from the `lb_session_<slug>` cookies, which are all sent to
///    every subdomain. Without this, a person whose spaces use different
///    addresses (e.g. `me@work.com` and `me@gmail.com`) would never see the
///    other space here.
///
/// The current one is marked, and cards show which login each space uses when
/// it isn't the current email.
pub async fn switch_page(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    AuthUser(user): AuthUser,
    jar: PrivateCookieJar,
    plain: CookieJar,
    loc: Locale,
) -> AppResult<Response> {
    struct Acc {
        id: Uuid,
        name: String,
        signed_in: bool,
        /// The org's login, when it differs from the current email.
        email: Option<String>,
    }
    // Keyed by slug so a cookie-discovered org never duplicates an email match.
    let mut by_slug: std::collections::BTreeMap<String, Acc> = std::collections::BTreeMap::new();

    // 1. Orgs the current email belongs to.
    let rows: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT t.id, t.slug, t.name FROM users u \
         JOIN tenants t ON t.id = u.tenant_id \
         WHERE u.email = $1",
    )
    .bind(&user.email)
    .fetch_all(&state.pool)
    .await?;
    for (tid, slug, name) in rows {
        let signed_in = tid == tenant.id || has_live_session(&state, &jar, &slug, tid).await;
        by_slug.insert(
            slug,
            Acc {
                id: tid,
                name,
                signed_in,
                email: None,
            },
        );
    }

    // 2. Orgs reachable via an existing session cookie, even under another
    //    email. Cookie names aren't encrypted, so we read them from the plain
    //    jar and decrypt each value through the private jar.
    for cookie in plain.iter() {
        let Some(slug) = cookie.name().strip_prefix("lb_session_") else {
            continue;
        };
        if by_slug.contains_key(slug) {
            continue;
        }
        let Some(decrypted) = jar.get(&session_cookie_name(slug)) else {
            continue;
        };
        let Ok(sid) = Uuid::parse_str(decrypted.value()) else {
            continue;
        };
        let row: Option<(Uuid, String, String)> = sqlx::query_as(
            "SELECT t.id, t.name, u.email FROM sessions s \
             JOIN tenants t ON t.id = s.tenant_id \
             JOIN users u ON u.id = s.user_id \
             WHERE s.id = $1 AND t.slug = $2 AND s.expires_at > NOW()",
        )
        .bind(sid)
        .bind(slug)
        .fetch_optional(&state.pool)
        .await?;
        if let Some((tid, name, org_email)) = row {
            let email = (org_email != user.email).then_some(org_email);
            by_slug.insert(
                slug.to_string(),
                Acc {
                    id: tid,
                    name,
                    signed_in: true,
                    email,
                },
            );
        }
    }

    let mut orgs: Vec<SwitchOrg> = by_slug
        .into_iter()
        .map(|(slug, a)| {
            let is_current = a.id == tenant.id;
            let base = public_url_for_slug(&slug, &state.cfg);
            let url = if is_current {
                "/today".to_string()
            } else if a.signed_in {
                format!("{base}/today")
            } else {
                format!("{base}/login?email={}", email_for_query(&user.email))
            };
            SwitchOrg {
                name: a.name,
                slug,
                url,
                is_current,
                signed_in: a.signed_in,
                email: a.email,
            }
        })
        .collect();
    orgs.sort_by(|x, y| x.name.cmp(&y.name));

    let tpl = SwitchTpl {
        loc,
        tenant: &tenant,
        email: &user.email,
        orgs,
    };
    Ok(Html(tpl.render()?).into_response())
}

fn hex_sha256(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let out = hasher.finalize();
    let mut s = String::with_capacity(out.len() * 2);
    for b in out {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::session_cookie_name;

    #[test]
    fn session_cookies_are_namespaced_per_tenant() {
        // Distinct names per slug are what let two orgs' sessions coexist in
        // one browser instead of overwriting each other.
        assert_eq!(session_cookie_name("lunatech"), "lb_session_lunatech");
        assert_ne!(session_cookie_name("lunatech"), session_cookie_name("acme"));
    }
}
