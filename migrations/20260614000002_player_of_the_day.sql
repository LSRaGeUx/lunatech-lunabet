-- Player of the day: the best predictor for a given calendar day, per tenant.
-- Computed once a day alongside the daily digest and read by the Today screen.
-- The (tenant_id, day) primary key keeps the computation idempotent.
CREATE TABLE player_of_the_day (
    tenant_id   UUID NOT NULL REFERENCES tenants(id),
    day         DATE NOT NULL,
    user_id     UUID NOT NULL REFERENCES users(id),
    points      INT  NOT NULL,
    exact_count INT  NOT NULL,
    computed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, day)
);
