use askama::Template;
use axum::extract::{FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::PrivateCookieJar;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::AppResult;
use crate::i18n::Locale;
use crate::models::User;
use crate::routes::auth;
use crate::state::AppState;

pub struct AdminUser(pub User);

#[axum::async_trait]
impl FromRequestParts<AppState> for AdminUser {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let jar = PrivateCookieJar::<axum_extra::extract::cookie::Key>::from_request_parts(parts, state)
            .await
            .map_err(|e| e.into_response())?;
        match auth::current_user(state, &jar).await {
            Ok(Some(u)) if u.is_admin => Ok(AdminUser(u)),
            Ok(Some(_)) => Err((StatusCode::FORBIDDEN, "Admin access required.").into_response()),
            Ok(None) => Err(Redirect::to("/login").into_response()),
            Err(e) => Err(e.into_response()),
        }
    }
}

pub struct StakeRow {
    pub user_id: Uuid,
    pub email: String,
    pub display_name: String,
    pub stake_eur: Option<i32>,
    pub stake_chosen_at: Option<DateTime<Utc>>,
    pub paid_at: Option<DateTime<Utc>>,
}

#[derive(Template)]
#[template(path = "admin_stakes.html")]
struct AdminStakesTpl<'a> {
    loc: Locale,
    user_name: &'a str,
    pot_total_eur: i64,
    paid_count: i64,
    rows: Vec<StakeRow>,
    deadline_local: String,
    deadline_passed: bool,
}

pub async fn stakes_page(
    State(state): State<AppState>,
    loc: Locale,
    AdminUser(admin): AdminUser,
) -> AppResult<Response> {
    let rows: Vec<(
        Uuid,
        String,
        String,
        Option<i32>,
        Option<DateTime<Utc>>,
        Option<DateTime<Utc>>,
    )> = sqlx::query_as(
        r#"
        SELECT id, email, display_name, stake_eur, stake_chosen_at, paid_at
        FROM users
        ORDER BY (paid_at IS NULL), stake_chosen_at NULLS LAST, display_name ASC
        "#,
    )
    .fetch_all(&state.pool)
    .await?;

    let rows: Vec<StakeRow> = rows
        .into_iter()
        .map(|(id, email, dn, stake, chosen, paid)| StakeRow {
            user_id: id,
            email,
            display_name: dn,
            stake_eur: stake,
            stake_chosen_at: chosen,
            paid_at: paid,
        })
        .collect();

    let pot = crate::stakes::load_pot(&state.pool, state.cfg.stake_deadline).await?;

    let tpl = AdminStakesTpl {
        loc,
        user_name: &admin.display_name,
        pot_total_eur: pot.total_eur,
        paid_count: pot.paid_count,
        rows,
        deadline_local: state.cfg.stake_deadline.format("%d/%m/%Y %H:%M UTC").to_string(),
        deadline_passed: Utc::now() > state.cfg.stake_deadline,
    };
    Ok(Html(tpl.render()?).into_response())
}

pub async fn mark_paid(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(user_id): Path<Uuid>,
) -> AppResult<Response> {
    sqlx::query(
        r#"
        UPDATE users
        SET paid_at = COALESCE(paid_at, NOW()), paid_by = $1
        WHERE id = $2 AND stake_eur IS NOT NULL
        "#,
    )
    .bind(admin.id)
    .bind(user_id)
    .execute(&state.pool)
    .await?;
    Ok(Redirect::to("/admin/stakes").into_response())
}

pub async fn mark_unpaid(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Path(user_id): Path<Uuid>,
) -> AppResult<Response> {
    sqlx::query("UPDATE users SET paid_at = NULL, paid_by = NULL WHERE id = $1")
        .bind(user_id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/admin/stakes").into_response())
}
