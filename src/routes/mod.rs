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
mod stake;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(home::index))
        .route("/lang/:code", get(lang::set))
        .route("/login", get(auth::login_page).post(auth::request_magic_link))
        .route("/login/sent", get(auth::login_sent))
        .route("/auth/callback", get(auth::callback))
        .route("/logout", post(auth::logout))
        .route("/matches", get(matches::list))
        .route("/matches/:id/bet", post(bets::place_or_update))
        .route("/leaderboard", get(leaderboard::index))
        .route("/stake", get(stake::page).post(stake::submit))
        .route("/admin/stakes", get(admin::stakes_page))
        .route("/admin/stakes/:user_id/paid", post(admin::mark_paid))
        .route("/admin/stakes/:user_id/unpaid", post(admin::mark_unpaid))
        .route("/dev", get(dev::index))
        .route("/dev/login", get(dev::login_as))
}
