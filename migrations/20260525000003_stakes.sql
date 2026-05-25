ALTER TABLE users
    ADD COLUMN stake_eur INTEGER CHECK (stake_eur IN (5, 10, 20)),
    ADD COLUMN stake_chosen_at TIMESTAMPTZ,
    ADD COLUMN paid_at TIMESTAMPTZ,
    ADD COLUMN paid_by UUID REFERENCES users(id);

CREATE INDEX users_paid_idx ON users (paid_at) WHERE paid_at IS NOT NULL;
