use chrono::{Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

struct SeedUser {
    email: &'static str,
    display_name: &'static str,
    is_admin: bool,
    stake_eur: Option<i32>,
    paid: bool,
}

const USERS: &[SeedUser] = &[
    SeedUser { email: "nicolas.leroux@lunatech.com",   display_name: "Nicolas Leroux", is_admin: true,  stake_eur: Some(10), paid: true  },
    SeedUser { email: "alice.dupont@lunatech.com",     display_name: "Alice Dupont",   is_admin: false, stake_eur: Some(5),  paid: true  },
    SeedUser { email: "bruno.martin@lunatech.com",     display_name: "Bruno Martin",   is_admin: false, stake_eur: Some(2),  paid: true  },
    SeedUser { email: "celine.morel@lunatech.com",     display_name: "Céline Morel",   is_admin: false, stake_eur: Some(5),  paid: false },
    SeedUser { email: "david.bernard@lunatech.com",    display_name: "David Bernard",  is_admin: false, stake_eur: None,     paid: false },
];

struct SeedMatch {
    id: i64,
    home: &'static str,
    home_code: &'static str,
    away: &'static str,
    away_code: &'static str,
    group: &'static str,
    offset_hours: i64,
    final_score: Option<(i32, i32)>,
}

const MATCHES: &[SeedMatch] = &[
    SeedMatch { id: 900001, home: "France",   home_code: "FRA", away: "Allemagne",  away_code: "GER", group: "Group A", offset_hours: -72, final_score: Some((2, 1)) },
    SeedMatch { id: 900002, home: "Brésil",   home_code: "BRA", away: "Argentine",  away_code: "ARG", group: "Group B", offset_hours: -48, final_score: Some((1, 1)) },
    SeedMatch { id: 900003, home: "Espagne",  home_code: "ESP", away: "Portugal",   away_code: "POR", group: "Group C", offset_hours: -24, final_score: Some((3, 0)) },
    SeedMatch { id: 900004, home: "Angleterre", home_code: "ENG", away: "Pays-Bas", away_code: "NED", group: "Group D", offset_hours:   1, final_score: None },
    SeedMatch { id: 900005, home: "Italie",   home_code: "ITA", away: "Belgique",   away_code: "BEL", group: "Group A", offset_hours:   3, final_score: None },
    SeedMatch { id: 900006, home: "Canada",   home_code: "CAN", away: "Maroc",      away_code: "MAR", group: "Group B", offset_hours:  24, final_score: None },
    SeedMatch { id: 900007, home: "USA",      home_code: "USA", away: "Mexique",    away_code: "MEX", group: "Group C", offset_hours:  48, final_score: None },
    SeedMatch { id: 900008, home: "Japon",    home_code: "JPN", away: "Croatie",    away_code: "CRO", group: "Group D", offset_hours:  72, final_score: None },
];

pub async fn seed(pool: &PgPool) -> anyhow::Result<()> {
    let now = Utc::now();
    let mut tx = pool.begin().await?;

    let mut user_ids: Vec<Uuid> = Vec::new();
    for u in USERS {
        let stake_chosen_at = if u.stake_eur.is_some() { Some(now) } else { None };
        let paid_at = if u.paid { Some(now) } else { None };
        let id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO users (email, display_name, is_admin, stake_eur, stake_chosen_at, paid_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (email) DO UPDATE
                SET display_name = EXCLUDED.display_name,
                    is_admin = EXCLUDED.is_admin,
                    stake_eur = EXCLUDED.stake_eur,
                    stake_chosen_at = EXCLUDED.stake_chosen_at,
                    paid_at = EXCLUDED.paid_at
            RETURNING id
            "#,
        )
        .bind(u.email)
        .bind(u.display_name)
        .bind(u.is_admin)
        .bind(u.stake_eur)
        .bind(stake_chosen_at)
        .bind(paid_at)
        .fetch_one(&mut *tx)
        .await?;
        user_ids.push(id);
    }
    println!("Seeded {} users.", user_ids.len());

    for m in MATCHES {
        let kickoff = now + Duration::hours(m.offset_hours);
        let (status, home_score, away_score) = match m.final_score {
            Some((h, a)) => ("FINISHED", Some(h), Some(a)),
            None => ("SCHEDULED", None, None),
        };
        sqlx::query(
            r#"
            INSERT INTO matches (
                id, competition, stage, group_name,
                home_team, away_team, home_team_code, away_team_code,
                kickoff_at, status, home_score, away_score, updated_at
            )
            VALUES ($1, 'FIFA World Cup (DEV)', 'GROUP_STAGE', $2,
                    $3, $4, $5, $6,
                    $7, $8, $9, $10, NOW())
            ON CONFLICT (id) DO UPDATE SET
                kickoff_at = EXCLUDED.kickoff_at,
                status     = EXCLUDED.status,
                home_score = EXCLUDED.home_score,
                away_score = EXCLUDED.away_score,
                updated_at = NOW()
            "#,
        )
        .bind(m.id)
        .bind(m.group)
        .bind(m.home)
        .bind(m.away)
        .bind(m.home_code)
        .bind(m.away_code)
        .bind(kickoff)
        .bind(status)
        .bind(home_score)
        .bind(away_score)
        .execute(&mut *tx)
        .await?;
    }
    println!("Seeded {} matches.", MATCHES.len());

    let bets: &[(usize, i64, i32, i32)] = &[
        (0, 900001, 2, 1),
        (0, 900002, 2, 0),
        (0, 900003, 2, 1),
        (0, 900004, 1, 1),
        (1, 900001, 1, 1),
        (1, 900002, 1, 1),
        (1, 900003, 3, 0),
        (1, 900004, 2, 0),
        (1, 900005, 0, 1),
        (2, 900001, 3, 2),
        (2, 900002, 2, 1),
        (2, 900003, 1, 0),
        (3, 900001, 0, 0),
        (3, 900003, 2, 2),
        (4, 900002, 2, 2),
        (4, 900003, 3, 1),
        (4, 900005, 1, 2),
    ];
    for (user_idx, match_id, h, a) in bets {
        sqlx::query(
            r#"
            INSERT INTO bets (user_id, match_id, home_score, away_score)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (user_id, match_id) DO UPDATE
            SET home_score = EXCLUDED.home_score, away_score = EXCLUDED.away_score, updated_at = NOW()
            "#,
        )
        .bind(user_ids[*user_idx])
        .bind(match_id)
        .bind(h)
        .bind(a)
        .execute(&mut *tx)
        .await?;
    }
    println!("Seeded {} bets.", bets.len());

    tx.commit().await?;

    crate::scoring::recompute_all(pool).await?;
    println!("Recomputed bet points.");

    println!();
    println!("Test users (DEV_MODE=true → /dev pour login en un clic) :");
    for u in USERS {
        println!("  - {} <{}>", u.display_name, u.email);
    }

    Ok(())
}
