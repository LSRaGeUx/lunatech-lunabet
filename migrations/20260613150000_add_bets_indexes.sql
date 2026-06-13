-- Add composite indexes on bets table for faster leaderboard and per-match queries.
-- tenant_id + user_id: speeds up leaderboard aggregation (GROUP BY user_id with tenant filter)
-- tenant_id + match_id: speeds up per-match bet lookups

CREATE INDEX IF NOT EXISTS bets_tenant_user_idx ON bets (tenant_id, user_id);
CREATE INDEX IF NOT EXISTS bets_tenant_match_idx ON bets (tenant_id, match_id);
