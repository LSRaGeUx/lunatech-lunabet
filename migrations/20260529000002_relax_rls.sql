-- Relax FORCE ROW LEVEL SECURITY. Managed Postgres providers (Clever Cloud's
-- add-on, Heroku, etc.) front the database with PgBouncer in transaction
-- pooling mode, which refuses session-level SETs. That made our previous
-- `after_connect` hook (which set `app.bypass_rls = on`) kill every new
-- connection, exhausting the pool at startup with no usable connections.
--
-- Dropping FORCE means the table owner — the role our app connects with —
-- bypasses RLS without needing to set the GUC. Other roles (or a future
-- non-owner deployment role) still get the policies. The policies stay in
-- place as documentation of the intended isolation; if/when we run the app
-- under a non-owner role we'll re-enable FORCE and wire up per-request
-- `app.current_tenant_id` via transactions instead of a connection hook.

ALTER TABLE users           NO FORCE ROW LEVEL SECURITY;
ALTER TABLE sessions        NO FORCE ROW LEVEL SECURITY;
ALTER TABLE magic_links     NO FORCE ROW LEVEL SECURITY;
ALTER TABLE bets            NO FORCE ROW LEVEL SECURITY;
ALTER TABLE match_reminders NO FORCE ROW LEVEL SECURITY;
