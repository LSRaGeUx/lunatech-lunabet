-- Email announcement of newly-earned badges. `seen` already drives the in-app
-- unlock toast (flipped when the player opens their profile); email delivery is
-- a separate channel with its own lifecycle, so it needs its own marker:
-- a player may receive the email before they ever open the app, and vice versa.
--
-- NULL = not emailed yet. The notification job (src/notifications.rs) picks up
-- every NULL row, sends one email per player listing their fresh badges, and
-- stamps the rows it announced.
ALTER TABLE achievements ADD COLUMN notified_at TIMESTAMPTZ;

-- Stamp any badges that already exist as already-announced, so enabling the
-- email doesn't blast each player with their whole badge history. This alone is
-- NOT enough: the achievements table is populated only at runtime by
-- achievements::evaluate_all, so at migration time it is usually EMPTY and this
-- UPDATE touches nothing. The real guard is the per-code backfill below, driven
-- by the app once evaluate_all has actually populated the table.
UPDATE achievements SET notified_at = NOW();

-- Per-badge "emails enabled" marker. The first time a code appears in the
-- catalogue, every grant of it that already exists must be treated as already
-- announced — otherwise turning emails on would blast players who earned it
-- retroactively. This covers both the whole catalogue on first deploy (the
-- empty-table case above) and a single new badge added in a later release:
-- evaluate_all back-grants it to all historically-qualifying users, and without
-- this marker they'd all be emailed about an "unlock" that reflects old activity.
--
-- The app (notifications::init_badge_notifications) runs evaluate_all, then for
-- every catalogue code NOT listed here stamps its existing grants as notified
-- and records the code. Grants earned after a code is initialised are left NULL
-- so they still get emailed.
CREATE TABLE badge_notify_codes (
    code           TEXT PRIMARY KEY,
    initialized_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
