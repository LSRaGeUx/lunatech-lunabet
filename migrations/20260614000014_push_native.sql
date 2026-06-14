-- Native push channels (spec 12). The same push_subscriptions table now holds
-- web (VAPID), iOS (APNs) and Android (FCM) endpoints, distinguished by
-- `platform`. Web rows keep using endpoint + p256dh + auth; native rows carry a
-- `device_token` instead (and store that token in `endpoint` too, so the
-- UNIQUE (user_id, endpoint) key and the subscribe upsert keep working
-- unchanged across platforms).
ALTER TABLE push_subscriptions
    ADD COLUMN platform TEXT NOT NULL DEFAULT 'web'
        CHECK (platform IN ('web', 'ios', 'android')),
    ADD COLUMN device_token TEXT;

-- Native subscriptions have no Web Push encryption keys, so these become
-- nullable. Web rows still populate them (the route validates that).
ALTER TABLE push_subscriptions
    ALTER COLUMN p256dh DROP NOT NULL,
    ALTER COLUMN auth   DROP NOT NULL;
