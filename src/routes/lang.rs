use axum::extract::Path;
use axum::http::header::REFERER;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use time::Duration as TimeDuration;

use crate::i18n::{Locale, LANG_COOKIE};

pub async fn set(
    Path(code): Path<String>,
    headers: HeaderMap,
    jar: CookieJar,
) -> Response {
    let loc = Locale::from_code(&code).unwrap_or_default();
    let cookie = Cookie::build((LANG_COOKIE, loc.code()))
        .path("/")
        .same_site(SameSite::Lax)
        .max_age(TimeDuration::days(365))
        .build();
    let jar = jar.add(cookie);
    let back = headers
        .get(REFERER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "/".into());
    (jar, Redirect::to(&back)).into_response()
}
