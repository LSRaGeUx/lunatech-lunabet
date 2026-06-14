-- Streaks: how many FINISHED matches in a row (by kickoff order) a user has
-- scored points on. Materialised on `users` and recomputed after each scoring
-- pass, so they can be read on the leaderboard without an extra aggregation.
ALTER TABLE users ADD COLUMN current_streak INT NOT NULL DEFAULT 0;
ALTER TABLE users ADD COLUMN best_streak    INT NOT NULL DEFAULT 0;
