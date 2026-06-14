//! Multi-channel push dispatch (spec 12).
//!
//! The trigger logic (kick-off reminders, rank climbs, live scores in
//! [crate::notifications]) is platform-agnostic: it asks this module to deliver
//! a payload to a stored subscription, and we branch on `platform` to the right
//! transport. Today only the `web` channel (VAPID / Web Push, in
//! [crate::webpush]) is wired; `ios` (APNs) and `android` (FCM) are recognised
//! and their device tokens are stored, but sending is a no-op until those
//! senders land (spec 12 phase C) — at which point only this file changes.
//!
//! This is a plain dispatch function rather than a `trait PushChannel` on
//! purpose: with a single real transport, an object-safe async trait would be
//! untested ceremony. Promote it to a trait when the second real channel
//! (APNs) arrives and there is something to abstract over.

use crate::webpush::{self, SendError, SendOutcome, Subscription, Vapid};

/// Default Web Push TTL: 24 h. A stale live-score or reminder is worthless, so
/// the push service may drop it after a day offline.
const PUSH_TTL_SECONDS: u32 = 24 * 60 * 60;

/// A subscription row as stored, for any platform.
pub struct StoredSubscription {
    pub platform: String,
    pub endpoint: Option<String>,
    pub p256dh: Option<String>,
    pub auth: Option<String>,
    /// Consumed by the APNs / FCM senders once they land (spec 12 phase C); the
    /// web channel ignores it.
    #[allow(dead_code)]
    pub device_token: Option<String>,
}

/// Deliver one payload to one subscription, dispatching on its platform.
///
/// Returns [`SendOutcome::Skipped`] (never an error) when the channel isn't
/// configured, so callers treat "not wired" and "nothing to do" identically and
/// only prune on a genuine [`SendOutcome::Gone`].
pub async fn deliver(
    http: &reqwest::Client,
    vapid: Option<&Vapid>,
    sub: &StoredSubscription,
    payload: &[u8],
) -> Result<SendOutcome, SendError> {
    match sub.platform.as_str() {
        "web" => {
            let Some(vapid) = vapid else {
                return Ok(SendOutcome::Skipped);
            };
            let (Some(endpoint), Some(p256dh), Some(auth)) = (
                sub.endpoint.as_ref(),
                sub.p256dh.as_ref(),
                sub.auth.as_ref(),
            ) else {
                // A web row missing its keys is malformed; skip rather than crash.
                return Ok(SendOutcome::Skipped);
            };
            let s = Subscription {
                endpoint: endpoint.clone(),
                p256dh: p256dh.clone(),
                auth: auth.clone(),
            };
            webpush::send(http, vapid, &s, payload, PUSH_TTL_SECONDS).await
        }
        "ios" | "android" => {
            // APNs / FCM not wired yet (spec 12 phase C). The token is stored so
            // these light up without any client-side change.
            tracing::debug!(
                platform = %sub.platform,
                "native push channel not configured yet; skipping"
            );
            Ok(SendOutcome::Skipped)
        }
        other => {
            tracing::warn!(platform = %other, "unknown push platform; skipping");
            Ok(SendOutcome::Skipped)
        }
    }
}
