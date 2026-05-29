-- Restore FORCE ROW LEVEL SECURITY on the tenant-scoped tables. Migration
-- 0529_02 relaxed it temporarily while diagnosing a Clever Cloud deploy
-- failure that turned out to be unrelated (Postgres connection limit
-- saturated by neighbouring apps). The after_connect hook is back in
-- main.rs, so the bypass GUC is set on every connection and the policies
-- now enforce in lockstep with the original 0525_07 intent.

ALTER TABLE users           FORCE ROW LEVEL SECURITY;
ALTER TABLE sessions        FORCE ROW LEVEL SECURITY;
ALTER TABLE magic_links     FORCE ROW LEVEL SECURITY;
ALTER TABLE bets            FORCE ROW LEVEL SECURITY;
ALTER TABLE match_reminders FORCE ROW LEVEL SECURITY;
