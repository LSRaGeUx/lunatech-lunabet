-- Email announcement of newly-earned badges. `seen` already drives the in-app
-- unlock toast (flipped when the player opens their profile); email delivery is
-- a separate channel with its own lifecycle, so it needs its own marker:
-- a player may receive the email before they ever open the app, and vice versa.
--
-- NULL = not emailed yet. The notification job (src/notifications.rs) picks up
-- every NULL row, sends one email per player listing their fresh badges, and
-- stamps the rows it announced.
ALTER TABLE achievements ADD COLUMN notified_at TIMESTAMPTZ;

-- Backfill: every badge earned before this feature shipped is considered
-- already announced, so enabling the email doesn't blast each player with their
-- whole badge history on the first scoring tick.
UPDATE achievements SET notified_at = NOW();
