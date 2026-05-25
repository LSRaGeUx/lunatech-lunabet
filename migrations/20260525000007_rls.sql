-- Row Level Security: defence in depth on top of the WHERE tenant_id = $X
-- filters scattered across the app code.
--
-- Each tenant-scoped table gets a policy that requires
-- `app.current_tenant_id` (a Postgres GUC the app sets per request) to match
-- the row's tenant_id, unless `app.bypass_rls` is 'on' (background jobs,
-- migrations, super-admin queries).
--
-- ACTIVATION:
--   This migration installs the policies and forces RLS even for the table
--   owner. However, until the app removes the `SET app.bypass_rls = on`
--   shipped from `after_connect`, every connection starts in bypass mode
--   and the policies are effectively pass-through.
--
-- TO ENFORCE STRICTLY (one of):
--   (1) Drop the `after_connect` bypass setting and refactor each request
--       handler to open a transaction that does
--       `SELECT set_config('app.current_tenant_id', $1, true)` before
--       running its queries. SET LOCAL inside a tx is the safe way to
--       scope the GUC to one request without poisoning the next.
--   (2) Or run the app under a non-superuser role with NOBYPASSRLS. The
--       policies then apply automatically; the app still has to remember
--       to set `app.current_tenant_id`, but a missed filter no longer
--       leaks data.

ALTER TABLE users           ENABLE ROW LEVEL SECURITY;
ALTER TABLE users           FORCE  ROW LEVEL SECURITY;
ALTER TABLE sessions        ENABLE ROW LEVEL SECURITY;
ALTER TABLE sessions        FORCE  ROW LEVEL SECURITY;
ALTER TABLE magic_links     ENABLE ROW LEVEL SECURITY;
ALTER TABLE magic_links     FORCE  ROW LEVEL SECURITY;
ALTER TABLE bets            ENABLE ROW LEVEL SECURITY;
ALTER TABLE bets            FORCE  ROW LEVEL SECURITY;
ALTER TABLE match_reminders ENABLE ROW LEVEL SECURITY;
ALTER TABLE match_reminders FORCE  ROW LEVEL SECURITY;

-- The same policy expression on every tenant-scoped table. We compare
-- tenant_id::text to current_setting(...) because GUCs are always text.
CREATE POLICY tenant_isolation ON users
    USING (
        current_setting('app.bypass_rls', true) = 'on'
        OR tenant_id::text = current_setting('app.current_tenant_id', true)
    )
    WITH CHECK (
        current_setting('app.bypass_rls', true) = 'on'
        OR tenant_id::text = current_setting('app.current_tenant_id', true)
    );

CREATE POLICY tenant_isolation ON sessions
    USING (
        current_setting('app.bypass_rls', true) = 'on'
        OR tenant_id::text = current_setting('app.current_tenant_id', true)
    )
    WITH CHECK (
        current_setting('app.bypass_rls', true) = 'on'
        OR tenant_id::text = current_setting('app.current_tenant_id', true)
    );

CREATE POLICY tenant_isolation ON magic_links
    USING (
        current_setting('app.bypass_rls', true) = 'on'
        OR tenant_id::text = current_setting('app.current_tenant_id', true)
    )
    WITH CHECK (
        current_setting('app.bypass_rls', true) = 'on'
        OR tenant_id::text = current_setting('app.current_tenant_id', true)
    );

CREATE POLICY tenant_isolation ON bets
    USING (
        current_setting('app.bypass_rls', true) = 'on'
        OR tenant_id::text = current_setting('app.current_tenant_id', true)
    )
    WITH CHECK (
        current_setting('app.bypass_rls', true) = 'on'
        OR tenant_id::text = current_setting('app.current_tenant_id', true)
    );

CREATE POLICY tenant_isolation ON match_reminders
    USING (
        current_setting('app.bypass_rls', true) = 'on'
        OR tenant_id::text = current_setting('app.current_tenant_id', true)
    )
    WITH CHECK (
        current_setting('app.bypass_rls', true) = 'on'
        OR tenant_id::text = current_setting('app.current_tenant_id', true)
    );
