-- Score celebration: mark when a settled bet's result has been shown to its
-- owner, so each win is celebrated exactly once. NULL means "settled but not
-- yet seen" (or not settled at all, while points is NULL).
ALTER TABLE bets ADD COLUMN result_seen_at TIMESTAMPTZ;
