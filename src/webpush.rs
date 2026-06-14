//! Pure-Rust Web Push sender (spec 08, part 2).
//!
//! Deliberately does not use the `web-push` crate: that pulls in `openssl`,
//! which would drag a native TLS stack into a project that is otherwise fully
//! on `rustls`. Everything here is RustCrypto (`p256`, `aes-gcm`) plus a small
//! hand-rolled HKDF/HMAC over `sha2`, so the binary keeps a single, static TLS
//! story and cross-compiles cleanly.
//!
//! Two pieces of the protocol live here:
//!
//! 1. **VAPID** (RFC 8292): an ES256-signed JWT identifying us to the push
//!    service, sent in the `Authorization` header alongside our public key.
//! 2. **Message encryption** (RFC 8291 with the `aes128gcm` content encoding of
//!    RFC 8188): the payload is encrypted to the subscription's public key so
//!    only the user's browser can read it; the push service just relays bytes.

use aes_gcm::aead::Aead;
use aes_gcm::{Aes128Gcm, KeyInit, Nonce};
use base64::Engine;
use chrono::Utc;
use p256::ecdh::EphemeralSecret;
use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use p256::elliptic_curve::sec1::ToEncodedPoint;
use p256::PublicKey;
use rand::rngs::OsRng;
use rand::RngCore;
use serde_json::json;
use sha2::{Digest, Sha256};

/// base64url, no padding — the encoding every Web Push field uses.
fn b64(input: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(input)
}

fn unb64(input: &str) -> anyhow::Result<Vec<u8>> {
    // Accept both padded and unpadded, URL-safe or standard: browsers and key
    // generators are inconsistent about padding and the +/- alphabet.
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(input.trim_end_matches('='))
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(input))
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(input))
        .map_err(|e| anyhow::anyhow!("invalid base64: {e}"))
}

/// The deployment's VAPID identity: a fixed P-256 keypair plus the contact
/// `sub` the push service can reach us at. Built once from config; cheap to
/// clone (just a few small byte buffers).
#[derive(Clone)]
pub struct Vapid {
    signing_key: SigningKey,
    /// Uncompressed SEC1 public key (65 bytes, `0x04` ‖ X ‖ Y), base64url —
    /// this is the `applicationServerKey` the browser subscribes with and the
    /// `k=` value in the Authorization header.
    pub public_key_b64: String,
    /// e.g. `mailto:ops@example.com`.
    subject: String,
}

impl Vapid {
    /// Build from the base64url-encoded private scalar (32 bytes) and public
    /// key (65 bytes) as produced by [`generate_keys`] or any standard VAPID
    /// generator. `subject` should be a `mailto:` or `https:` URL.
    pub fn from_config(
        private_b64: &str,
        public_b64: &str,
        subject: &str,
    ) -> anyhow::Result<Self> {
        let priv_bytes = unb64(private_b64)?;
        let signing_key = SigningKey::from_slice(&priv_bytes)
            .map_err(|e| anyhow::anyhow!("VAPID_PRIVATE_KEY is not a valid P-256 key: {e}"))?;
        // Normalise the public key to canonical unpadded base64url so the value
        // we hand the browser and the header are byte-identical regardless of
        // how it was written in the environment.
        let pub_bytes = unb64(public_b64)?;
        let _ = PublicKey::from_sec1_bytes(&pub_bytes)
            .map_err(|e| anyhow::anyhow!("VAPID_PUBLIC_KEY is not a valid P-256 point: {e}"))?;
        Ok(Self {
            signing_key,
            public_key_b64: b64(&pub_bytes),
            subject: subject.to_string(),
        })
    }

    /// Sign a VAPID JWT for the given audience (the `scheme://host` origin of
    /// the push endpoint). Valid for 12 h, well under the 24 h ceiling.
    fn jwt(&self, audience: &str) -> anyhow::Result<String> {
        let header = b64(br#"{"typ":"JWT","alg":"ES256"}"#);
        let claims = json!({
            "aud": audience,
            "exp": (Utc::now() + chrono::Duration::hours(12)).timestamp(),
            "sub": self.subject,
        });
        let claims = b64(claims.to_string().as_bytes());
        let signing_input = format!("{header}.{claims}");
        // ES256: ECDSA/P-256/SHA-256. `Signature::to_bytes` is the raw r‖s
        // (64 bytes) the JWS spec wants — no DER unwrapping needed.
        let sig: Signature = self.signing_key.sign(signing_input.as_bytes());
        Ok(format!("{signing_input}.{}", b64(&sig.to_bytes())))
    }
}

/// One push subscription as the browser reports it (`PushSubscription.toJSON()`).
pub struct Subscription {
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
}

/// Outcome of a single delivery attempt.
pub enum SendOutcome {
    /// Accepted by the push service (2xx).
    Delivered,
    /// The subscription is dead (404/410) and should be purged.
    Gone,
}

#[derive(Debug, thiserror::Error)]
pub enum SendError {
    #[error("push payload encryption failed: {0}")]
    Crypto(String),
    #[error("push endpoint has no origin: {0}")]
    BadEndpoint(String),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error("push service returned {status}: {body}")]
    Service { status: u16, body: String },
}

/// Send one encrypted push to one subscription.
///
/// `payload` is the cleartext (typically a small JSON blob the service worker
/// reads); it is encrypted to the subscription before it ever leaves the
/// process. `ttl` is how long the push service may hold the message if the
/// device is offline.
pub async fn send(
    http: &reqwest::Client,
    vapid: &Vapid,
    sub: &Subscription,
    payload: &[u8],
    ttl: u32,
) -> Result<SendOutcome, SendError> {
    let body = encrypt(sub, payload).map_err(|e| SendError::Crypto(e.to_string()))?;

    let audience = origin_of(&sub.endpoint)
        .ok_or_else(|| SendError::BadEndpoint(sub.endpoint.clone()))?;
    let jwt = vapid
        .jwt(&audience)
        .map_err(|e| SendError::Crypto(e.to_string()))?;
    let authorization = format!("vapid t={jwt}, k={}", vapid.public_key_b64);

    let resp = http
        .post(&sub.endpoint)
        .header("Authorization", authorization)
        .header("Content-Encoding", "aes128gcm")
        .header("Content-Type", "application/octet-stream")
        .header("TTL", ttl.to_string())
        .body(body)
        .send()
        .await?;

    let status = resp.status();
    if status.is_success() {
        Ok(SendOutcome::Delivered)
    } else if status == reqwest::StatusCode::NOT_FOUND || status == reqwest::StatusCode::GONE {
        Ok(SendOutcome::Gone)
    } else {
        let code = status.as_u16();
        let body = resp.text().await.unwrap_or_default();
        Err(SendError::Service { status: code, body })
    }
}

/// `scheme://host[:port]` of a URL, without the path — the VAPID `aud`.
fn origin_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let host = rest.split(['/', '?', '#']).next()?;
    if host.is_empty() {
        return None;
    }
    Some(format!("{scheme}://{host}"))
}

/// RFC 8291 + RFC 8188: produce the full `aes128gcm` body for `payload`,
/// encrypted to `sub`'s public key.
fn encrypt(sub: &Subscription, payload: &[u8]) -> anyhow::Result<Vec<u8>> {
    let ua_public_bytes = unb64(&sub.p256dh)?;
    let auth_secret = unb64(&sub.auth)?;
    let ua_public = PublicKey::from_sec1_bytes(&ua_public_bytes)
        .map_err(|e| anyhow::anyhow!("subscription p256dh invalid: {e}"))?;

    // Ephemeral server keypair for this one message (per RFC 8291).
    let server_secret = EphemeralSecret::random(&mut OsRng);
    let server_public = server_secret.public_key();
    let as_public_bytes = server_public.to_encoded_point(false).as_bytes().to_vec();

    let shared = server_secret.diffie_hellman(&ua_public);
    let shared_bytes = shared.raw_secret_bytes();

    // ikm = HKDF(salt=auth_secret, ikm=ecdh,
    //            info="WebPush: info\0" ‖ ua_public ‖ as_public, L=32)
    let mut key_info = b"WebPush: info\0".to_vec();
    key_info.extend_from_slice(&ua_public_bytes);
    key_info.extend_from_slice(&as_public_bytes);
    let prk_combine = hkdf_extract(&auth_secret, shared_bytes.as_slice());
    let ikm = hkdf_expand(&prk_combine, &key_info, 32);

    // Content-encryption salt + record size (RFC 8188).
    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);

    let prk = hkdf_extract(&salt, &ikm);
    let cek = hkdf_expand(&prk, b"Content-Encoding: aes128gcm\0", 16);
    let nonce = hkdf_expand(&prk, b"Content-Encoding: nonce\0", 12);

    // Single record: plaintext followed by the 0x02 last-record delimiter.
    let mut plaintext = payload.to_vec();
    plaintext.push(0x02);
    let cipher = Aes128Gcm::new_from_slice(&cek)
        .map_err(|e| anyhow::anyhow!("AES key setup failed: {e}"))?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext.as_ref())
        .map_err(|e| anyhow::anyhow!("AES-GCM encryption failed: {e}"))?;

    // Header: salt(16) ‖ record_size(4, BE) ‖ idlen(1) ‖ keyid(as_public).
    let record_size: u32 = 4096;
    let mut out = Vec::with_capacity(16 + 4 + 1 + as_public_bytes.len() + ciphertext.len());
    out.extend_from_slice(&salt);
    out.extend_from_slice(&record_size.to_be_bytes());
    out.push(as_public_bytes.len() as u8);
    out.extend_from_slice(&as_public_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

// --- HKDF-SHA256, hand-rolled ------------------------------------------------
//
// `hkdf`/`hmac` would only work against `sha2` 0.10's `digest 0.10` traits,
// but this project pins `sha2` 0.11 (`digest 0.11`). Rather than carry two
// `sha2` majors just for a few HMAC calls, we implement the (tiny) HMAC and
// HKDF primitives directly over the `sha2` already in the tree. Every output
// we need is <= 32 bytes, so HKDF-Expand only ever runs a single block.

const BLOCK: usize = 64; // SHA-256 block size

fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut block_key = [0u8; BLOCK];
    if key.len() > BLOCK {
        block_key[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block_key[..key.len()].copy_from_slice(key);
    }

    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= block_key[i];
        opad[i] ^= block_key[i];
    }

    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(data);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_hash);

    let mut out = [0u8; 32];
    out.copy_from_slice(&outer.finalize());
    out
}

fn hkdf_extract(salt: &[u8], ikm: &[u8]) -> [u8; 32] {
    hmac_sha256(salt, ikm)
}

/// HKDF-Expand for `len <= 32`. Panics above 32 — we never need more here.
fn hkdf_expand(prk: &[u8; 32], info: &[u8], len: usize) -> Vec<u8> {
    assert!(len <= 32, "hkdf_expand only supports a single SHA-256 block");
    let mut data = info.to_vec();
    data.push(0x01); // T(1) = HMAC(PRK, info ‖ 0x01)
    let block = hmac_sha256(prk, &data);
    block[..len].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::signature::Verifier;
    use p256::ecdsa::VerifyingKey;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// RFC 5869 test case 1: pins our hand-rolled HMAC/HKDF against the
    /// published SHA-256 vectors so a regression in the crypto can't slip by.
    #[test]
    fn hkdf_matches_rfc5869() {
        let ikm = hex("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b");
        let salt = hex("000102030405060708090a0b0c");
        let info = hex("f0f1f2f3f4f5f6f7f8f9");

        let prk = hkdf_extract(&salt, &ikm);
        assert_eq!(
            prk.to_vec(),
            hex("077709362c2e32df0ddc3f0dc47bba6390b6c73bb50f9c3122ec844ad7c2b3e5")
        );

        // First 32 bytes of the expected OKM (= T(1) for L <= 32).
        let okm = hkdf_expand(&prk, &info, 32);
        assert_eq!(
            okm,
            hex("3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf")
        );
    }

    /// Encrypt to a freshly generated subscription, then decrypt as that
    /// subscription's owner would. Exercises ECDH, the RFC 8291 key schedule,
    /// the RFC 8188 framing and AES-128-GCM end to end.
    #[test]
    fn encrypt_round_trips() {
        let ua_secret = p256::SecretKey::random(&mut OsRng);
        let ua_public = ua_secret.public_key();
        let ua_public_bytes = ua_public.to_encoded_point(false).as_bytes().to_vec();
        let mut auth = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut auth);

        let sub = Subscription {
            endpoint: "https://push.example.com/abc".into(),
            p256dh: b64(&ua_public_bytes),
            auth: b64(&auth),
        };
        let message = b"When I grow up, I want to be a watermelon";
        let body = encrypt(&sub, message).expect("encrypt");

        // --- parse the aes128gcm record (receiver side) ---
        let salt = &body[0..16];
        let idlen = body[20] as usize;
        let as_public_bytes = &body[21..21 + idlen];
        let ciphertext = &body[21 + idlen..];

        let as_public = PublicKey::from_sec1_bytes(as_public_bytes).unwrap();
        let shared = p256::ecdh::diffie_hellman(
            ua_secret.to_nonzero_scalar(),
            as_public.as_affine(),
        );

        let mut key_info = b"WebPush: info\0".to_vec();
        key_info.extend_from_slice(&ua_public_bytes);
        key_info.extend_from_slice(as_public_bytes);
        let prk_combine = hkdf_extract(&auth, shared.raw_secret_bytes().as_slice());
        let ikm = hkdf_expand(&prk_combine, &key_info, 32);

        let prk = hkdf_extract(salt, &ikm);
        let cek = hkdf_expand(&prk, b"Content-Encoding: aes128gcm\0", 16);
        let nonce = hkdf_expand(&prk, b"Content-Encoding: nonce\0", 12);

        let cipher = Aes128Gcm::new_from_slice(&cek).unwrap();
        let mut plaintext = cipher
            .decrypt(Nonce::from_slice(&nonce), ciphertext)
            .expect("decrypt");
        assert_eq!(plaintext.pop(), Some(0x02)); // last-record delimiter
        assert_eq!(plaintext, message);
    }

    /// The VAPID JWT must verify against the public key and carry the audience.
    #[test]
    fn vapid_jwt_verifies() {
        let (priv_b64, pub_b64) = generate_keys();
        let vapid = Vapid::from_config(&priv_b64, &pub_b64, "mailto:ops@example.com").unwrap();
        let jwt = vapid.jwt("https://push.example.com").unwrap();

        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3);
        let signing_input = format!("{}.{}", parts[0], parts[1]);
        let sig_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[2])
            .unwrap();

        let pub_bytes = unb64(&pub_b64).unwrap();
        let verifying = VerifyingKey::from_sec1_bytes(&pub_bytes).unwrap();
        let sig = Signature::from_slice(&sig_bytes).unwrap();
        verifying
            .verify(signing_input.as_bytes(), &sig)
            .expect("signature verifies");

        let claims = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[1])
            .unwrap();
        let claims: serde_json::Value = serde_json::from_slice(&claims).unwrap();
        assert_eq!(claims["aud"], "https://push.example.com");
        assert_eq!(claims["sub"], "mailto:ops@example.com");
    }
}

/// Generate a fresh VAPID keypair, returned as `(private_b64, public_b64)` in
/// the base64url form the config and browsers expect. Used by the `gen-vapid`
/// CLI subcommand so operators don't need a separate tool.
pub fn generate_keys() -> (String, String) {
    let secret = p256::SecretKey::random(&mut OsRng);
    let private_b64 = b64(&secret.to_bytes());
    let public_b64 = b64(secret.public_key().to_encoded_point(false).as_bytes());
    (private_b64, public_b64)
}
