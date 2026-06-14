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
}

#[derive(Template)]
#[template(path = "landing.html")]
struct LandingTpl {
    loc: Locale,
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
        return Ok(Html(HomeTpl { loc, tenant: &tenant }.render()?).into_response());
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
    Ok(Html(LandingTpl { loc }.render()?).into_response())
}
