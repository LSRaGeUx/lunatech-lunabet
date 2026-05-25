-- Per-tenant tracking of match reminders.
--
-- The original schema stored `reminded_at` directly on the global `matches`
-- table, which works for a single tenant but means the first tenant to send
-- a reminder for a given match suppresses it for everybody else. This
-- migration moves the tracking to a (tenant_id, match_id) join table so the
-- reminder fan-out is tenant-scoped.
--
-- The legacy `matches.reminded_at` column is kept for now (denormalised
-- "anyone reminded?" flag); we'll drop it once the new code is settled.

CREATE TABLE match_reminders (
    tenant_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    match_id   BIGINT NOT NULL REFERENCES matches(id) ON DELETE CASCADE,
    sent_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, match_id)
);

CREATE INDEX match_reminders_match_idx ON match_reminders (match_id);

-- Backfill the lunatech tenant so its already-sent reminders aren't replayed
-- when the new code takes over.
INSERT INTO match_reminders (tenant_id, match_id, sent_at)
SELECT
    (SELECT id FROM tenants WHERE slug = 'lunatech'),
    m.id,
    m.reminded_at
FROM matches m
WHERE m.reminded_at IS NOT NULL
ON CONFLICT DO NOTHING;
