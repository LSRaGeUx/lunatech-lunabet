use askama::Template;
use axum::extract::State;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::PrivateCookieJar;

use crate::error::AppResult;
use crate::i18n::Locale;
use crate::routes::auth;
use crate::state::AppState;

#[derive(Template)]
#[template(path = "home.html")]
struct HomeTpl {
    loc: Locale,
}

pub async fn index(
    State(state): State<AppState>,
    loc: Locale,
    jar: PrivateCookieJar,
) -> AppResult<Response> {
    if auth::current_user(&state, &jar).await?.is_some() {
        return Ok(Redirect::to("/matches").into_response());
    }
    Ok(Html(HomeTpl { loc }.render()?).into_response())
}
