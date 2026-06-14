-- Private leagues: mini-rankings inside a space. A league groups a subset of
-- the space's players and shows a leaderboard filtered to its members. No new
-- bets are created; the existing points are simply filtered.

CREATE TABLE leagues (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id     UUID NOT NULL REFERENCES tenants(id),
    name          TEXT NOT NULL,
    join_code     TEXT NOT NULL,
    owner_user_id UUID NOT NULL REFERENCES users(id),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, join_code)
);

CREATE TABLE league_members (
    league_id UUID NOT NULL REFERENCES leagues(id) ON DELETE CASCADE,
    user_id   UUID NOT NULL REFERENCES users(id),
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (league_id, user_id)
);

CREATE INDEX leagues_tenant_idx ON leagues (tenant_id);
CREATE INDEX league_members_user_idx ON league_members (user_id);

-- RLS to match the other tenant-scoped tables. FORCE is off across the app
-- (see the 20260529 migrations), so the table-owner connection bypasses this;
-- it is defence-in-depth for any future non-owner role.
ALTER TABLE leagues ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON leagues
    USING (
        current_setting('app.bypass_rls', true) = 'on'
        OR tenant_id::text = current_setting('app.current_tenant_id', true)
    )
    WITH CHECK (
        current_setting('app.bypass_rls', true) = 'on'
        OR tenant_id::text = current_setting('app.current_tenant_id', true)
    );

-- league_members has no tenant_id of its own; it inherits isolation from its
-- parent league.
ALTER TABLE league_members ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON league_members
    USING (
        current_setting('app.bypass_rls', true) = 'on'
        OR EXISTS (
            SELECT 1 FROM leagues l
            WHERE l.id = league_members.league_id
              AND l.tenant_id::text = current_setting('app.current_tenant_id', true)
        )
    )
    WITH CHECK (
        current_setting('app.bypass_rls', true) = 'on'
        OR EXISTS (
            SELECT 1 FROM leagues l
            WHERE l.id = league_members.league_id
              AND l.tenant_id::text = current_setting('app.current_tenant_id', true)
        )
    );
