-- Multi-tenant foundation: tenants table + tenant_id on every user-scoped table.
-- Phase 1: zero behavioral change. Every existing row is backfilled to the
-- "lunatech" tenant; the app upserts that tenant from env vars on each startup.

CREATE TABLE tenants (
    id                     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug                   TEXT UNIQUE NOT NULL,
    name                   TEXT NOT NULL,
    allowed_email_pattern  TEXT NOT NULL,
    logo_url               TEXT,
    primary_color          TEXT NOT NULL DEFAULT '#1d3557',
    accent_color           TEXT NOT NULL DEFAULT '#c8232c',
    football_competition   TEXT NOT NULL DEFAULT 'WC',
    stake_deadline         TIMESTAMPTZ NOT NULL,
    reminder_lead_minutes  INT NOT NULL DEFAULT 120,
    slack_webhook_url      TEXT,
    mail_from              TEXT NOT NULL,
    admin_emails           TEXT[] NOT NULL DEFAULT '{}',
    created_at             TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Seed the lunatech tenant so the backfill below has a target.
-- App startup will overwrite the configurable fields from env vars.
INSERT INTO tenants (slug, name, allowed_email_pattern, mail_from, stake_deadline)
VALUES (
    'lunatech',
    'Lunatech',
    'lunatech\.com',
    'lunatech-betting@lunatech.com',
    '2026-06-27T23:59:00Z'
);

-- tenant_id on every user-scoped table; nullable while we backfill.
ALTER TABLE users        ADD COLUMN tenant_id UUID REFERENCES tenants(id);
ALTER TABLE magic_links  ADD COLUMN tenant_id UUID REFERENCES tenants(id);
ALTER TABLE sessions     ADD COLUMN tenant_id UUID REFERENCES tenants(id);
ALTER TABLE bets         ADD COLUMN tenant_id UUID REFERENCES tenants(id);

-- Backfill everything to the lunatech tenant.
UPDATE users        SET tenant_id = (SELECT id FROM tenants WHERE slug = 'lunatech');
UPDATE magic_links  SET tenant_id = (SELECT id FROM tenants WHERE slug = 'lunatech');
UPDATE sessions     SET tenant_id = (SELECT id FROM tenants WHERE slug = 'lunatech');
UPDATE bets         SET tenant_id = (SELECT id FROM tenants WHERE slug = 'lunatech');

-- Now enforce non-null.
ALTER TABLE users        ALTER COLUMN tenant_id SET NOT NULL;
ALTER TABLE magic_links  ALTER COLUMN tenant_id SET NOT NULL;
ALTER TABLE sessions     ALTER COLUMN tenant_id SET NOT NULL;
ALTER TABLE bets         ALTER COLUMN tenant_id SET NOT NULL;

-- The email uniqueness used to be global. With multi-tenant the same person
-- can exist as a user in several tenants, so we move the uniqueness to
-- (tenant_id, email).
ALTER TABLE users DROP CONSTRAINT users_email_key;
CREATE UNIQUE INDEX users_tenant_email_uidx ON users (tenant_id, email);

-- Magic-link tokens are random and globally unique, but we still scope
-- consumption by tenant so a token leaked from tenant A cannot log into B.
CREATE INDEX magic_links_tenant_idx ON magic_links (tenant_id);

CREATE INDEX sessions_tenant_idx ON sessions (tenant_id);
CREATE INDEX bets_tenant_idx     ON bets     (tenant_id);

-- Matches stay global (one row per external football-data fixture, shared
-- by all tenants watching the same competition). reminded_at remains a
-- single column for now since we only have one tenant; when a second
-- tenant arrives we will move reminders to a (tenant_id, match_id) table.
