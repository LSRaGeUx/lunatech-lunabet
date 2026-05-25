use askama::Template;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::Form;
use chrono::Utc;
use serde::Deserialize;

use crate::error::AppResult;
use crate::i18n::Locale;
use crate::models::User;
use crate::routes::auth::AuthUser;
use crate::stakes;
use crate::state::AppState;
use crate::tenant::{Tenant, TenantCtx};

#[derive(Template)]
#[template(path = "stake.html")]
struct StakeTpl<'a> {
    loc: Locale,
    tenant: &'a Tenant,
    user: &'a User,
    deadline_passed: bool,
    deadline_local: String,
    paid: bool,
    admin_emails_display: String,
}

pub async fn page(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    loc: Locale,
    AuthUser(user): AuthUser,
) -> AppResult<Response> {
    let deadline_passed = Utc::now() > tenant.stake_deadline;
    let tpl = StakeTpl {
        loc,
        tenant: &tenant,
        user: &user,
        deadline_passed,
        deadline_local: tenant.stake_deadline.format("%d/%m/%Y %H:%M UTC").to_string(),
        paid: user.paid_at.is_some(),
        admin_emails_display: if tenant.admin_emails.is_empty() {
            "admin".into()
        } else {
            tenant
                .admin_emails
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        },
    };
    Ok(Html(tpl.render()?).into_response())
}

#[derive(Deserialize)]
pub struct StakeForm {
    tier: i32,
}

pub async fn submit(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    AuthUser(user): AuthUser,
    Form(form): Form<StakeForm>,
) -> AppResult<Response> {
    if user.paid_at.is_some() {
        return Ok((StatusCode::CONFLICT, "Stake already paid; ask an admin to change it.").into_response());
    }
    if Utc::now() > tenant.stake_deadline {
        return Ok((StatusCode::FORBIDDEN, "Stake deadline has passed.").into_response());
    }
    if !stakes::is_valid_tier(form.tier) {
        return Ok((StatusCode::BAD_REQUEST, "Invalid tier.").into_response());
    }
    sqlx::query(
        r#"
        UPDATE users
        SET stake_eur = $1, stake_chosen_at = NOW()
        WHERE id = $2 AND tenant_id = $3
        "#,
    )
    .bind(form.tier)
    .bind(user.id)
    .bind(tenant.id)
    .execute(&state.pool)
    .await?;
    Ok(Redirect::to("/stake").into_response())
}
