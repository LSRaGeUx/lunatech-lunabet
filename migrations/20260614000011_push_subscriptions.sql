-- Web Push subscriptions (spec 08, part 2). One row per (user, browser
-- endpoint): the endpoint is the push service URL the browser handed us, and
-- p256dh / auth are the client's encryption keys (base64url). Scoped per tenant
-- like everything else so a user's subscriptions never leak across spaces.
CREATE TABLE push_subscriptions (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL REFERENCES tenants(id),
    user_id     UUID NOT NULL REFERENCES users(id),
    endpoint    TEXT NOT NULL,
    p256dh      TEXT NOT NULL,
    auth        TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (user_id, endpoint)
);

CREATE INDEX idx_push_subscriptions_user ON push_subscriptions (tenant_id, user_id);

-- Master push toggle. Defaults TRUE so a user who opts in (by granting the
-- browser permission and subscribing) gets pushes immediately; turning it off
-- in the preferences silences every push without dropping the subscription.
ALTER TABLE users ADD COLUMN notify_push BOOLEAN NOT NULL DEFAULT TRUE;
