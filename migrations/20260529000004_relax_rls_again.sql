-- Undo migration 0529_03 (which itself undid 0529_02): we want the
-- relaxed posture back. The `after_connect` hook is gone from main.rs
-- too, so there is nothing trying to set `app.bypass_rls` anymore. With
-- FORCE off, the table-owner role bypasses RLS the way superusers do
-- and queries return rows normally without any GUC setup.

ALTER TABLE users           NO FORCE ROW LEVEL SECURITY;
ALTER TABLE sessions        NO FORCE ROW LEVEL SECURITY;
ALTER TABLE magic_links     NO FORCE ROW LEVEL SECURITY;
ALTER TABLE bets            NO FORCE ROW LEVEL SECURITY;
ALTER TABLE match_reminders NO FORCE ROW LEVEL SECURITY;
