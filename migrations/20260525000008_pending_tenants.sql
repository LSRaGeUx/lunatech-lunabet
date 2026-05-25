-- Self-serve tenant signup: stash the request until the owner confirms by
-- clicking a magic link, then materialise the tenant row.

CREATE TABLE pending_tenants (
    token_hash    TEXT PRIMARY KEY,
    slug          TEXT NOT NULL,
    name          TEXT NOT NULL,
    owner_email   TEXT NOT NULL,
    owner_name    TEXT NOT NULL,
    allowed_email_pattern TEXT NOT NULL,
    expires_at    TIMESTAMPTZ NOT NULL,
    consumed_at   TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Reserve the slug while the link is alive so two simultaneous signups
    -- can't both claim the same one.
    UNIQUE (slug)
);

CREATE INDEX pending_tenants_expires_idx ON pending_tenants (expires_at)
    WHERE consumed_at IS NULL;
