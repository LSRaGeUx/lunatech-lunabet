-- One row per tenant per day once the daily recap email has been sent, so the
-- background job never emails the same digest twice (mirrors match_reminders).
CREATE TABLE daily_digests (
    tenant_id   UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    digest_date DATE NOT NULL,
    sent_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, digest_date)
);
