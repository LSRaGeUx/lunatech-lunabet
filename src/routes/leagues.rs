//! Private leagues: mini-rankings inside a space. A player creates a league and
//! shares a short join code; anyone in the same space can join with that code
//! and see a leaderboard filtered to the league's members. No new bets are
//! created, leagues are purely a social lens on the existing points.

use askama::Template;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::Form;
use rand::Rng;
use serde::Deserialize;
use uuid::Uuid;

use crate::characters;
use crate::error::AppResult;
use crate::i18n::Locale;
use crate::routes::auth::AuthUser;
use crate::stakes;
use crate::state::AppState;
use crate::tenant::{Tenant, TenantCtx};

/// Cap on the number of leagues a single user may own, to limit abuse.
const MAX_OWNED_LEAGUES: i64 = 20;

/// Unambiguous base32-ish alphabet for join codes (no 0/O, 1/I/L).
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
const CODE_LEN: usize = 6;

pub struct LeagueSummary {
    pub id: Uuid,
    pub name: String,
    pub join_code: String,
    pub member_count: i64,
    pub is_owner: bool,
}

#[derive(Template)]
#[template(path = "leagues.html")]
struct LeaguesTpl<'a> {
    loc: Locale,
    tenant: &'a Tenant,
    user_name: &'a str,
    total_points: i32,
    is_admin: bool,
    nav_active: &'static str,
    leagues: Vec<LeagueSummary>,
    notice: Option<String>,
    error: Option<String>,
}

pub struct LeagueMemberRow {
    pub rank: usize,
    pub user_id: Uuid,
    pub display_name: String,
    pub points: i64,
    pub exact: i64,
    pub bets: i64,
    pub current_streak: i32,
    pub best_streak: i32,
    pub is_me: bool,
    pub avatar: String,
}

#[derive(Template)]
#[template(path = "league.html")]
struct LeagueTpl<'a> {
    loc: Locale,
    tenant: &'a Tenant,
    user_name: &'a str,
    total_points: i32,
    is_admin: bool,
    nav_active: &'static str,
    league_id: Uuid,
    league_name: String,
    join_code: String,
    is_owner: bool,
    rows: Vec<LeagueMemberRow>,
}

async fn render_index(
    state: &AppState,
    tenant: &Tenant,
    loc: Locale,
    user: &crate::models::User,
    notice: Option<String>,
    error: Option<String>,
) -> AppResult<Response> {
    let leagues: Vec<LeagueSummary> = sqlx::query_as::<_, (Uuid, String, String, Uuid, i64)>(
        r#"
        SELECT l.id, l.name, l.join_code, l.owner_user_id,
               (SELECT COUNT(*) FROM league_members m WHERE m.league_id = l.id)::BIGINT
        FROM leagues l
        JOIN league_members lm ON lm.league_id = l.id AND lm.user_id = $2
        WHERE l.tenant_id = $1
        ORDER BY l.created_at ASC
        "#,
    )
    .bind(tenant.id)
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?
    .into_iter()
    .map(|(id, name, join_code, owner, member_count)| LeagueSummary {
        id,
        name,
        join_code,
        member_count,
        is_owner: owner == user.id,
    })
    .collect();

    let board = stakes::load_leaderboard(&state.pool, tenant.id).await?;
    let tpl = LeaguesTpl {
        loc,
        tenant,
        user_name: &user.display_name,
        total_points: stakes::points_for(&board, user.id),
        is_admin: user.is_admin,
        nav_active: "leagues",
        leagues,
        notice,
        error,
    };
    Ok(Html(tpl.render()?).into_response())
}

pub async fn index(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    loc: Locale,
    AuthUser(user): AuthUser,
) -> AppResult<Response> {
    render_index(&state, &tenant, loc, &user, None, None).await
}

#[derive(Deserialize)]
pub struct CreateForm {
    name: String,
}

pub async fn create(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    loc: Locale,
    AuthUser(user): AuthUser,
    Form(form): Form<CreateForm>,
) -> AppResult<Response> {
    let name = form.name.trim();
    if name.is_empty() || name.chars().count() > 60 {
        let msg = loc
            .f(
                "Le nom de la ligue est invalide (1 à 60 caractères).",
                "Invalid league name (1 to 60 characters).",
            )
            .to_string();
        return render_index(&state, &tenant, loc, &user, None, Some(msg)).await;
    }

    let owned: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM leagues WHERE tenant_id = $1 AND owner_user_id = $2")
            .bind(tenant.id)
            .bind(user.id)
            .fetch_one(&state.pool)
            .await?;
    if owned >= MAX_OWNED_LEAGUES {
        let msg = loc
            .f(
                "Tu as atteint le nombre maximum de ligues.",
                "You have reached the maximum number of leagues.",
            )
            .to_string();
        return render_index(&state, &tenant, loc, &user, None, Some(msg)).await;
    }

    // Generate a join code that is unique within the tenant. Collisions are
    // unlikely but we retry a few times just in case.
    let mut tx = state.pool.begin().await?;
    let mut league_id: Option<Uuid> = None;
    for _ in 0..8 {
        let code = random_code();
        let inserted: Option<Uuid> = sqlx::query_scalar(
            "INSERT INTO leagues (tenant_id, name, join_code, owner_user_id) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (tenant_id, join_code) DO NOTHING \
             RETURNING id",
        )
        .bind(tenant.id)
        .bind(name)
        .bind(&code)
        .bind(user.id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(id) = inserted {
            league_id = Some(id);
            break;
        }
    }
    let Some(league_id) = league_id else {
        tx.rollback().await?;
        let msg = loc
            .f(
                "Impossible de générer un code, réessaie.",
                "Could not generate a code, please retry.",
            )
            .to_string();
        return render_index(&state, &tenant, loc, &user, None, Some(msg)).await;
    };

    sqlx::query("INSERT INTO league_members (league_id, user_id) VALUES ($1, $2)")
        .bind(league_id)
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    Ok(Redirect::to(&format!("/leagues/{league_id}")).into_response())
}

#[derive(Deserialize)]
pub struct JoinForm {
    code: String,
}

pub async fn join(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    loc: Locale,
    AuthUser(user): AuthUser,
    Form(form): Form<JoinForm>,
) -> AppResult<Response> {
    let code = normalize_code(&form.code);
    let league_id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM leagues WHERE tenant_id = $1 AND join_code = $2")
            .bind(tenant.id)
            .bind(&code)
            .fetch_optional(&state.pool)
            .await?;

    let Some(league_id) = league_id else {
        let msg = loc
            .f(
                "Code invalide, aucune ligue trouvée.",
                "Invalid code, no league found.",
            )
            .to_string();
        return render_index(&state, &tenant, loc, &user, None, Some(msg)).await;
    };

    sqlx::query(
        "INSERT INTO league_members (league_id, user_id) VALUES ($1, $2) \
         ON CONFLICT DO NOTHING",
    )
    .bind(league_id)
    .bind(user.id)
    .execute(&state.pool)
    .await?;

    Ok(Redirect::to(&format!("/leagues/{league_id}")).into_response())
}

pub async fn show(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    loc: Locale,
    AuthUser(user): AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Response> {
    let league: Option<(String, String, Uuid)> = sqlx::query_as(
        "SELECT name, join_code, owner_user_id FROM leagues WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(tenant.id)
    .fetch_optional(&state.pool)
    .await?;

    let Some((league_name, join_code, owner_user_id)) = league else {
        return Ok((StatusCode::NOT_FOUND, loc.f("Ligue introuvable.", "League not found.")).into_response());
    };

    // Only members may view a league's board.
    let is_member: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM league_members WHERE league_id = $1 AND user_id = $2)",
    )
    .bind(id)
    .bind(user.id)
    .fetch_one(&state.pool)
    .await?;
    if !is_member {
        return Ok((StatusCode::FORBIDDEN, loc.f("Tu n'es pas membre de cette ligue.", "You are not a member of this league.")).into_response());
    }

    let board = stakes::load_league_leaderboard(&state.pool, tenant.id, id).await?;
    let total_points = stakes::points_for(&board, user.id);
    let rows = board
        .iter()
        .enumerate()
        .map(|(i, r)| LeagueMemberRow {
            rank: i + 1,
            user_id: r.user_id,
            display_name: r.display_name.clone(),
            points: r.points,
            exact: r.exact_count,
            bets: r.settled_bets,
            current_streak: r.current_streak,
            best_streak: r.best_streak,
            is_me: r.user_id == user.id,
            avatar: characters::path_for(r.user_id),
        })
        .collect();

    let tpl = LeagueTpl {
        loc,
        tenant: &tenant,
        user_name: &user.display_name,
        total_points,
        is_admin: user.is_admin,
        nav_active: "leagues",
        league_id: id,
        league_name,
        join_code,
        is_owner: owner_user_id == user.id,
        rows,
    };
    Ok(Html(tpl.render()?).into_response())
}

/// Leave a league. When the owner leaves, ownership passes to the oldest
/// remaining member; if nobody is left, the league is deleted.
pub async fn leave(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    AuthUser(user): AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Response> {
    let owner: Option<Uuid> =
        sqlx::query_scalar("SELECT owner_user_id FROM leagues WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(tenant.id)
            .fetch_optional(&state.pool)
            .await?;
    let Some(owner) = owner else {
        return Ok(Redirect::to("/leagues").into_response());
    };

    let mut tx = state.pool.begin().await?;
    sqlx::query("DELETE FROM league_members WHERE league_id = $1 AND user_id = $2")
        .bind(id)
        .bind(user.id)
        .execute(&mut *tx)
        .await?;

    if owner == user.id {
        let next: Option<Uuid> = sqlx::query_scalar(
            "SELECT user_id FROM league_members WHERE league_id = $1 ORDER BY joined_at ASC LIMIT 1",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        match next {
            Some(new_owner) => {
                sqlx::query("UPDATE leagues SET owner_user_id = $2 WHERE id = $1")
                    .bind(id)
                    .bind(new_owner)
                    .execute(&mut *tx)
                    .await?;
            }
            None => {
                sqlx::query("DELETE FROM leagues WHERE id = $1")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
    }
    tx.commit().await?;
    Ok(Redirect::to("/leagues").into_response())
}

#[derive(Deserialize)]
pub struct RemoveForm {
    user_id: Uuid,
}

/// Owner-only: remove another member from the league.
pub async fn remove_member(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    AuthUser(user): AuthUser,
    Path(id): Path<Uuid>,
    Form(form): Form<RemoveForm>,
) -> AppResult<Response> {
    if !is_owner(&state, tenant.id, id, user.id).await? {
        return Ok((StatusCode::FORBIDDEN, "Not allowed.").into_response());
    }
    // The owner cannot remove themselves through this path (use "leave").
    if form.user_id != user.id {
        sqlx::query("DELETE FROM league_members WHERE league_id = $1 AND user_id = $2")
            .bind(id)
            .bind(form.user_id)
            .execute(&state.pool)
            .await?;
    }
    Ok(Redirect::to(&format!("/leagues/{id}")).into_response())
}

#[derive(Deserialize)]
pub struct RenameForm {
    name: String,
}

pub async fn rename(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    AuthUser(user): AuthUser,
    Path(id): Path<Uuid>,
    Form(form): Form<RenameForm>,
) -> AppResult<Response> {
    if !is_owner(&state, tenant.id, id, user.id).await? {
        return Ok((StatusCode::FORBIDDEN, "Not allowed.").into_response());
    }
    let name = form.name.trim();
    if !name.is_empty() && name.chars().count() <= 60 {
        sqlx::query("UPDATE leagues SET name = $2 WHERE id = $1 AND tenant_id = $3")
            .bind(id)
            .bind(name)
            .bind(tenant.id)
            .execute(&state.pool)
            .await?;
    }
    Ok(Redirect::to(&format!("/leagues/{id}")).into_response())
}

pub async fn delete(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    AuthUser(user): AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Response> {
    if !is_owner(&state, tenant.id, id, user.id).await? {
        return Ok((StatusCode::FORBIDDEN, "Not allowed.").into_response());
    }
    // league_members rows are removed by ON DELETE CASCADE.
    sqlx::query("DELETE FROM leagues WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(tenant.id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/leagues").into_response())
}

async fn is_owner(
    state: &AppState,
    tenant_id: Uuid,
    league_id: Uuid,
    user_id: Uuid,
) -> AppResult<bool> {
    let owner: Option<Uuid> =
        sqlx::query_scalar("SELECT owner_user_id FROM leagues WHERE id = $1 AND tenant_id = $2")
            .bind(league_id)
            .bind(tenant_id)
            .fetch_optional(&state.pool)
            .await?;
    Ok(owner == Some(user_id))
}

fn random_code() -> String {
    let mut rng = rand::thread_rng();
    (0..CODE_LEN)
        .map(|_| CODE_ALPHABET[rng.gen_range(0..CODE_ALPHABET.len())] as char)
        .collect()
}

/// Normalise a user-typed code: uppercase and strip whitespace and dashes.
/// The generation alphabet already avoids ambiguous characters (0/O, 1/I/L),
/// so no further folding is needed.
fn normalize_code(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_have_expected_shape() {
        let code = random_code();
        assert_eq!(code.chars().count(), CODE_LEN);
        assert!(code.bytes().all(|b| CODE_ALPHABET.contains(&b)));
    }

    #[test]
    fn normalize_strips_and_uppercases() {
        assert_eq!(normalize_code("  ab cd-ef "), "ABCDEF");
    }
}
