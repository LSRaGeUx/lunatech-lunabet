-- One row per tenant per day once the "today's matches" preview email has been
-- sent, so the morning job never emails the same day's fixtures twice.
CREATE TABLE today_matches_emails (
    tenant_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    match_date DATE NOT NULL,
    sent_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, match_date)
);
