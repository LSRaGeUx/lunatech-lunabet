-- Tiers changed from (5, 10, 20) to (2, 5, 10).
-- Any existing stake choice / payment is reset to NULL so the new
-- constraint can be applied cleanly; paid users will need to re-confirm
-- their tier. (No production users yet — pre-launch dev migration.)

ALTER TABLE users DROP CONSTRAINT IF EXISTS users_stake_eur_check;

UPDATE users
SET stake_eur = NULL,
    stake_chosen_at = NULL,
    paid_at = NULL,
    paid_by = NULL
WHERE stake_eur IS NOT NULL AND stake_eur NOT IN (2, 5, 10);

ALTER TABLE users
    ADD CONSTRAINT users_stake_eur_check CHECK (stake_eur IN (2, 5, 10));
