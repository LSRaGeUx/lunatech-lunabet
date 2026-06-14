use askama::Template;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::PrivateCookieJar;

use crate::error::AppResult;
use crate::i18n::Locale;
use crate::routes::auth;
use crate::state::AppState;
use crate::tenant::{MaybeTenant, MaybeUnknownSlug, Tenant};

#[derive(Template)]
#[template(path = "home.html")]
struct HomeTpl<'a> {
    loc: Locale,
    tenant: &'a Tenant,
    canonical: String,
    og_image: String,
}

#[derive(Template)]
#[template(path = "landing.html")]
struct LandingTpl {
    loc: Locale,
    canonical: String,
    og_image: String,
}

/// Turn a possibly-relative asset path into an absolute URL under `base`.
fn absolute(path: &str, base: &str) -> String {
    if path.starts_with("http") {
        path.to_string()
    } else {
        format!("{}{}", base.trim_end_matches('/'), path)
    }
}

#[derive(Template)]
#[template(path = "tenant_not_found.html")]
struct NotFoundTpl<'a> {
    loc: Locale,
    slug: &'a str,
    /// Where the embedded signup form should POST. Either `/signup` (if no
    /// PLATFORM_URL is configured, single-tenant fallback mode) or
    /// `{PLATFORM_URL}/signup` so the request lands on the apex.
    signup_action: &'a str,
}

pub async fn index(
    State(state): State<AppState>,
    MaybeTenant(maybe_tenant): MaybeTenant,
    MaybeUnknownSlug(maybe_unknown): MaybeUnknownSlug,
    loc: Locale,
    jar: PrivateCookieJar,
) -> AppResult<Response> {
    if let Some(tenant) = maybe_tenant {
        if auth::current_user(&state, &tenant, &jar).await?.is_some() {
            return Ok(Redirect::to("/today").into_response());
        }
        let canonical = tenant.public_url(&state.cfg);
        let og_image = absolute(tenant.logo_url_or_default(), &canonical);
        return Ok(Html(
            HomeTpl {
                loc,
                tenant: &tenant,
                canonical,
                og_image,
            }
            .render()?,
        )
        .into_response());
    }
    if let Some(unknown_slug) = maybe_unknown {
        let signup_action = state
            .cfg
            .platform_url
            .as_deref()
            .map(|u| format!("{}/signup", u.trim_end_matches('/')))
            .unwrap_or_else(|| "/signup".into());
        let tpl = NotFoundTpl {
            loc,
            slug: &unknown_slug,
            signup_action: &signup_action,
        };
        return Ok((StatusCode::NOT_FOUND, Html(tpl.render()?)).into_response());
    }
    let canonical = state
        .cfg
        .platform_url
        .clone()
        .unwrap_or_else(|| state.cfg.base_url.clone())
        .trim_end_matches('/')
        .to_string();
    let og_image = format!("{canonical}/static/lunatech-logo.svg");
    Ok(Html(
        LandingTpl {
            loc,
            canonical,
            og_image,
        }
        .render()?,
    )
    .into_response())
}
