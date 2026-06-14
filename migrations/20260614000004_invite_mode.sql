-- Invitation-based spaces ("friends mode"): membership can be granted by
-- explicit invitation instead of relying on the email-domain gate.

ALTER TABLE tenants ADD COLUMN membership_mode TEXT NOT NULL DEFAULT 'domain'
    CHECK (membership_mode IN ('domain', 'invite'));
ALTER TABLE tenants ADD COLUMN members_can_invite BOOLEAN NOT NULL DEFAULT TRUE;

-- Carry the chosen mode through the pending-signup verification step.
ALTER TABLE pending_tenants ADD COLUMN membership_mode TEXT NOT NULL DEFAULT 'domain'
    CHECK (membership_mode IN ('domain', 'invite'));

CREATE TABLE invitations (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id        UUID NOT NULL REFERENCES tenants(id),
    email            TEXT NOT NULL,
    inviter_user_id  UUID REFERENCES users(id),
    token_hash       TEXT NOT NULL UNIQUE,
    status           TEXT NOT NULL DEFAULT 'pending'
                     CHECK (status IN ('pending', 'accepted', 'revoked', 'expired')),
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at       TIMESTAMPTZ NOT NULL,
    accepted_at      TIMESTAMPTZ,
    accepted_user_id UUID REFERENCES users(id)
);

-- At most one live invitation per (tenant, email).
CREATE UNIQUE INDEX invitations_pending_uidx
    ON invitations (tenant_id, email)
    WHERE status = 'pending';
CREATE INDEX invitations_tenant_idx ON invitations (tenant_id);
CREATE INDEX invitations_expires_idx ON invitations (expires_at)
    WHERE status = 'pending';

-- RLS to match the other tenant-scoped tables. FORCE is off across the app
-- (see the 20260529 migrations), so the table-owner connection bypasses this;
-- it is defence-in-depth for any future non-owner role.
ALTER TABLE invitations ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON invitations
    USING (
        current_setting('app.bypass_rls', true) = 'on'
        OR tenant_id::text = current_setting('app.current_tenant_id', true)
    )
    WITH CHECK (
        current_setting('app.bypass_rls', true) = 'on'
        OR tenant_id::text = current_setting('app.current_tenant_id', true)
    );
