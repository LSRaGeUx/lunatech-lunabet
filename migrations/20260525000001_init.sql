CREATE TABLE users (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email       TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    is_admin    BOOLEAN NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE magic_links (
    token_hash  TEXT PRIMARY KEY,
    email       TEXT NOT NULL,
    expires_at  TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ
);

CREATE TABLE sessions (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at  TIMESTAMPTZ NOT NULL
);

CREATE TABLE matches (
    id              BIGINT PRIMARY KEY,
    competition     TEXT NOT NULL,
    stage           TEXT,
    group_name      TEXT,
    home_team       TEXT NOT NULL,
    away_team       TEXT NOT NULL,
    home_team_code  TEXT,
    away_team_code  TEXT,
    kickoff_at      TIMESTAMPTZ NOT NULL,
    status          TEXT NOT NULL,
    home_score      INTEGER,
    away_score      INTEGER,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX matches_kickoff_idx ON matches (kickoff_at);

CREATE TABLE bets (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    match_id    BIGINT NOT NULL REFERENCES matches(id) ON DELETE CASCADE,
    home_score  INTEGER NOT NULL CHECK (home_score >= 0 AND home_score <= 30),
    away_score  INTEGER NOT NULL CHECK (away_score >= 0 AND away_score <= 30),
    points      INTEGER,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (user_id, match_id)
);

CREATE INDEX bets_user_idx ON bets (user_id);
CREATE INDEX bets_match_idx ON bets (match_id);
