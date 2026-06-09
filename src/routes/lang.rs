use axum::extract::{Path, State};
use axum::http::header::REFERER;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::{Cookie, CookieJar, PrivateCookieJar, SameSite};
use time::Duration as TimeDuration;

use crate::i18n::{Locale, LANG_COOKIE};
use crate::routes::auth;
use crate::state::AppState;
use crate::tenant::MaybeTenant;

pub async fn set(
    State(state): State<AppState>,
    Path(code): Path<String>,
    MaybeTenant(tenant): MaybeTenant,
    headers: HeaderMap,
    private_jar: PrivateCookieJar,
    jar: CookieJar,
) -> Response {
    let loc = Locale::from_code(&code).unwrap_or_default();

    // Persist the explicit choice on the signed-in user so background emails
    // (match reminders, sent without a request cookie) follow it: French when
    // the user picked French, English otherwise.
    if let Some(tenant) = tenant {
        if let Ok(Some(user)) = auth::current_user(&state, &tenant, &private_jar).await {
            let _ = sqlx::query("UPDATE users SET lang = $1 WHERE id = $2 AND tenant_id = $3")
                .bind(loc.code())
                .bind(user.id)
                .bind(tenant.id)
                .execute(&state.pool)
                .await;
        }
    }

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
