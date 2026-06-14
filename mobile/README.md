# LunaBet mobile (Tauri 2)

Native iOS / Android shell around the deployed LunaBet web app (spec
[12-mobile-tauri](../specs/12-mobile-tauri.md)). It is a **remote shell**: the
webview loads the live site, so product changes ship server-side without a store
resubmission. This folder is a standalone Cargo project; it is intentionally
outside the backend workspace, so the root `cargo build` never touches it.

## What is committed vs generated

Committed: the Tauri core (`src-tauri/src`, `Cargo.toml`, `build.rs`,
`tauri.conf.json`, `capabilities/`) and the fallback `dist/index.html`.

Generated locally (git-ignored), because they need the Tauri CLI and your
toolchain / signing identity:

- `src-tauri/icons/*` — `cargo tauri icon <png>` (see `src-tauri/icons/README.md`).
- `src-tauri/gen/` — the Xcode / Gradle projects from `cargo tauri ios init` and
  `cargo tauri android init`.

## Prerequisites (your machine)

- Rust + the Tauri CLI: `cargo install tauri-cli --version '^2'`.
- iOS: macOS, Xcode, an Apple Developer account (for signing + APNs).
- Android: Android Studio / SDK + NDK, a signing keystore.

## Run / build

```sh
cd mobile/src-tauri

# Desktop preview of the shell (fast sanity check):
cargo tauri dev

# iOS:
cargo tauri ios init     # once, generates src-tauri/gen/apple
cargo tauri ios dev      # on simulator/device
cargo tauri ios build    # release IPA (needs signing)

# Android:
cargo tauri android init # once, generates src-tauri/gen/android
cargo tauri android dev
cargo tauri android build
```

## Pointing at an environment

`tauri.conf.json` -> `app.windows[0].url` is set to `https://lunabet.eu` (the
apex, which runs the central multi-tenant login so a user can reach their
space). Change it to a specific tenant subdomain for a single-space build, or
to a staging URL for testing.

## Deep links (universal / app links)

The backend already serves the association files when configured (spec 12,
`src/routes/well_known.rs`):

- iOS: set `APPLE_APP_ID=<TeamID>.com.lunatech.lunabet` so
  `/.well-known/apple-app-site-association` is served, then declare the
  `applinks:lunabet.eu` associated domain in the generated iOS project.
- Android: set `ANDROID_PACKAGE=com.lunatech.lunabet` and
  `ANDROID_CERT_FINGERPRINT=<sha256>` so `/.well-known/assetlinks.json` is
  served, then declare the intent filter in the generated Android project.

With these in place, magic-link (`/auth/callback`) and invite
(`/invite/accept`) URLs open the app instead of the browser.

## Native push (spec 12 phase C)

Not wired yet. The backend already stores native tokens: `POST /push/subscribe`
accepts `{ "platform": "ios"|"android", "device_token": "..." }` and the
`push_subscriptions` table carries `platform` + `device_token`. The remaining
work is a Tauri push plugin to obtain the APNs/FCM token and call that endpoint,
plus the server-side APNs/FCM senders behind `src/push_channel.rs` (today it
dispatches web and skips native).

## Store review note

LunaBet's pot is honour-based; the app handles **no payments** (see
`src/stakes.rs`). State this clearly in the store listings to avoid the
gambling-policy rejection path, and give reviewers the dev one-click login
(`src/routes/dev.rs`) / a demo account.
