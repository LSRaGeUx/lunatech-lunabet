use askama::Template;
use axum::extract::State;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::PrivateCookieJar;

use crate::error::AppResult;
use crate::i18n::Locale;
use crate::routes::auth;
use crate::state::AppState;
use crate::tenant::TenantCtx;

#[derive(Template)]
#[template(path = "home.html")]
struct HomeTpl {
    loc: Locale,
}

pub async fn index(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    loc: Locale,
    jar: PrivateCookieJar,
) -> AppResult<Response> {
    if auth::current_user(&state, &tenant, &jar).await?.is_some() {
        return Ok(Redirect::to("/matches").into_response());
    }
    Ok(Html(HomeTpl { loc }.render()?).into_response())
}
