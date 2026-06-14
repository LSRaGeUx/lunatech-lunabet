-- Jokers (confidence multipliers): a player may flag one upcoming match per
-- competition phase as their "joker", doubling the points that bet earns.
-- Opt-in per space so tenants that want plain scoring are unaffected.
ALTER TABLE tenants ADD COLUMN jokers_enabled BOOLEAN NOT NULL DEFAULT FALSE;

-- The joker is carried by the bet itself: multiplier = 2 means "doubled".
-- `bets.points` stores the EFFECTIVE points (base * multiplier), so every
-- ranking sum already reflects the joker. Exact scores are therefore detected
-- as `points >= 3` (an exact bet scores 3 or 6, an outcome bet 1 or 2), which
-- relies on this CHECK keeping the multiplier in {1, 2}.
ALTER TABLE bets ADD COLUMN multiplier INT NOT NULL DEFAULT 1
    CHECK (multiplier IN (1, 2));

-- At most one joker per (user, phase). `stage` lives on `matches`, so the
-- "one per phase" rule is enforced in the application (a partial unique index
-- can't span the bets→matches join), but this index makes the lookup cheap.
CREATE INDEX bets_joker_idx ON bets (user_id, tenant_id) WHERE multiplier = 2;
