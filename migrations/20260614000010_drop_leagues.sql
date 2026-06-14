-- Remove the private-leagues feature. In practice spaces are small enough that
-- the global leaderboard already is the "between friends" ranking, and the
-- invite-based spaces cover the "separate group" need, so leagues added
-- complexity without a clear use. Drop the tables (league_members first via the
-- FK, though CASCADE on the parent would handle it too).
DROP TABLE IF EXISTS league_members;
DROP TABLE IF EXISTS leagues;
