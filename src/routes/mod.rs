use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;

pub mod admin;
pub mod auth;
mod bets;
mod dev;
mod home;
mod lang;
mod leaderboard;
mod matches;
mod platform;
mod signup;
mod stake;
mod super_admin;
mod tenant_settings;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(home::index))
        .route("/lang/:code", get(lang::set))
        .route("/login", get(auth::login_page).post(auth::request_magic_link))
        .route("/login/sent", get(auth::login_sent))
        .route("/auth/callback", get(auth::callback))
        .route("/signup", get(signup::form).post(signup::submit))
        .route("/signup/verify", get(signup::verify))
        .route("/super-admin/", get(platform::dashboard))
        .route(
            "/super-admin/login",
            get(platform::login_page).post(platform::request_link),
        )
        .route("/super-admin/auth/callback", get(platform::callback))
        .route("/super-admin/logout", post(platform::logout))
        .route(
            "/super-admin/tenants/:slug/delete",
            post(platform::delete_tenant),
        )
        .route("/super-admin/send-results", post(platform::send_results))
        .route(
            "/super-admin/send-today-matches",
            post(platform::send_today_matches),
        )
        .route("/logout", post(auth::logout))
        .route("/matches", get(matches::list))
        .route("/matches/:id/bet", post(bets::place_or_update))
        .route("/leaderboard", get(leaderboard::index))
        .route("/stake", get(stake::page).post(stake::submit))
        .route("/admin/stakes", get(admin::stakes_page))
        .route("/admin/stakes/:user_id/paid", post(admin::mark_paid))
        .route("/admin/stakes/:user_id/unpaid", post(admin::mark_unpaid))
        .route(
            "/admin/settings",
            get(tenant_settings::page)
                .post(tenant_settings::update)
                // Logo uploads are capped at 2 MiB in the handler; raise the
                // request limit above axum's 2 MiB default so the multipart
                // body (logo + form fields) isn't rejected before we can
                // return a friendly error.
                .layer(DefaultBodyLimit::max(4 * 1024 * 1024)),
        )
        .route(
            "/admin/tenants",
            get(super_admin::list).post(super_admin::create),
        )
        .route("/admin/tenants/new", get(super_admin::new_form))
        .route("/admin/tenants/:slug/edit", get(super_admin::edit_form))
        .route("/admin/tenants/:slug", post(super_admin::update))
        .route("/admin/tenants/:slug/delete", post(super_admin::delete))
        .route("/dev", get(dev::index))
        .route("/dev/login", get(dev::login_as))
}
