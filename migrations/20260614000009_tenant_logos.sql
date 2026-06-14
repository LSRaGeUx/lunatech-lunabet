-- Tenant logos stored in the database so they survive a redeploy. Previously
-- logos were written to a local `uploads/` directory and referenced by
-- `tenants.logo_url = /uploads/<file>`; that directory is part of the deploy
-- artifact, so every redeploy wiped the files and the logos vanished.
--
-- With the `db` storage backend (now the default) the bytes live here and are
-- served from `/logo/:tenant_id`. The `disk` and future `s3` backends keep
-- using `tenants.logo_url` to point elsewhere, so this table is only populated
-- by the `db` backend.
CREATE TABLE tenant_logos (
    tenant_id    UUID PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    bytes        BYTEA NOT NULL,
    content_type TEXT NOT NULL,
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
