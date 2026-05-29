-- Magic-link tokens for the platform-level "super admin" login at the apex.
-- Distinct from `magic_links` because those are tenant-scoped (every row has
-- a NOT NULL tenant_id); the platform admin logs in once at lunabet.eu
-- without belonging to any specific tenant.

CREATE TABLE platform_magic_links (
    token_hash  TEXT PRIMARY KEY,
    email       TEXT NOT NULL,
    expires_at  TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX platform_magic_links_email_active_idx
    ON platform_magic_links (email)
    WHERE consumed_at IS NULL;
