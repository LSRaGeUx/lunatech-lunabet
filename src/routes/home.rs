use askama::Template;
use axum::extract::State;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::PrivateCookieJar;

use crate::error::AppResult;
use crate::i18n::Locale;
use crate::routes::auth;
use crate::state::AppState;
use crate::tenant::{MaybeTenant, Tenant};

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

pub async fn index(
    State(state): State<AppState>,
    MaybeTenant(maybe_tenant): MaybeTenant,
    loc: Locale,
    jar: PrivateCookieJar,
) -> AppResult<Response> {
    match maybe_tenant {
        Some(tenant) => {
            if auth::current_user(&state, &tenant, &jar).await?.is_some() {
                return Ok(Redirect::to("/matches").into_response());
            }
            Ok(Html(HomeTpl { loc, tenant: &tenant }.render()?).into_response())
        }
        None => Ok(Html(LandingTpl { loc }.render()?).into_response()),
    }
}
