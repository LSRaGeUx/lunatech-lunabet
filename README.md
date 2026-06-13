# LunaBet

FIFA World Cup 2026 betting app for Lunatech employees.
Exact-score predictions, leaderboard, stake-weighted prize pot. Fully bilingual EN / FR.

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)

## Preview

| Landing | Predictions album |
|---|---|
| ![Home page](docs/screenshots/01-home.png) | ![Matches page](docs/screenshots/04-matches.png) |

| Leaderboard with 3D scene | My stake in the pot |
|---|---|
| ![Leaderboard](docs/screenshots/05-leaderboard.png) | ![Stake](docs/screenshots/06-stake.png) |

| Admin (stake management) | English version |
|---|---|
| ![Admin Stakes](docs/screenshots/07-admin-stakes.png) | ![Leaderboard EN](docs/screenshots/08-leaderboard-en.png) |

### Emails

All transactional emails are **multipart HTML** (with a plain-text fallback), match the app's branding (navy header, pitch stripe, red "stamp" CTA button), embed the **Lunatech logo**, and come in two flavors:

- **Magic link** — rendered in the visitor's locale (FR or EN, based on the `lb_lang` cookie at the time of the request).
- **Match reminder** — bilingual side-by-side (FR + EN in the same email, since per-user language is not stored).

| Magic link (FR) | Magic link (EN) |
|---|---|
| ![Magic Link FR](docs/screenshots/09-email-magic-link-fr.png) | ![Magic Link EN](docs/screenshots/10-email-magic-link-en.png) |

| Match reminder (bilingual) |
|---|
| ![Match Reminder](docs/screenshots/11-email-reminder.png) |

<details>
<summary>Auth & dev pages</summary>

| Sign-in (magic link) | Dev mode |
|---|---|
| ![Login](docs/screenshots/02-login.png) | ![Dev page](docs/screenshots/03-dev.png) |

</details>

> Screenshots are produced by [`scripts/screenshots.sh`](scripts/screenshots.sh).
> The server must be running at `http://127.0.0.1:3000` with fixtures loaded
> (`cargo run -- seed && cargo run`). The `?shot=1` query param freezes the
> 3D goal animation on the leaderboard at a known frame so the capture is clean.

## Stack

- **Rust + Axum** (async web framework)
- **PostgreSQL** via SQLx (runtime queries, migrations auto-applied at startup)
- **Askama** (templates compiled into the binary)
- **htmx** (client-side interactivity, no JS framework)
- **Three.js** loaded via importmap CDN (3D football, mini goal-shot scene)
- **lettre** (SMTP sender for magic links and reminders)
- **football-data.org** (auto-pulls fixtures and results; competition code `WC`)

## Features

### Betting
- **Magic-link** sign-in, restricted to `@lunatech.com` emails
- One bet type only: **exact score** (e.g. 2-1)
- Bets close **at kick-off**
- Scoring:
  - **3 points** for an exact score
  - **1 point** for the right winner (or correct draw)
  - **0 points** otherwise
- Leaderboard tiebreak: points → exact scores → settled bets → account age

### Prize pot and stakes
- Three tiers: **2€ / 5€ / 10€** (chosen on `/stake`)
- **Honor system**: the app never handles money. Payment is made to the administrator if needed, and the admin marks players "paid" on `/admin/stakes`.
- Default deadline: **end of the WC2026 group stage** (2026-06-27 23:59 UTC), configurable
- Pot split between the **top 3 paid players**:
  - `payout_i = pot × (base_i × stake_i) / Σ(base × stake)` with `base = [0.5, 0.3, 0.2]`
  - When all three stakes are equal it degenerates to the classic 50 / 30 / 20 split
  - A higher stake gets a larger slice of its position's share
- **Unpaid** players stay in the leaderboard with a "not eligible" badge; their payout slot rolls down to the next paid player

### Notifications
- Background job every 5 min — for every match starting within `REMINDER_LEAD_MINUTES` minutes that hasn't been announced yet:
  - Per-user **email** to anyone who hasn't placed a bet on that match
  - **Slack** message to the configured channel (if `SLACK_WEBHOOK_URL` is set)
- Each match is announced exactly once (`matches.reminded_at`)
- **Daily recap email**: from `DAILY_DIGEST_HOUR` (UTC) each morning, every user gets a digest of the previous day's results, the points they earned that day, and the current leaderboard. Localised per user (FR/EN), sent at most once per day per tenant (`daily_digests` table), and skipped on days with no finished match.

### Internationalisation
- Fully bilingual **FR / EN** UI
- Language switcher in the topbar (cookie `lb_lang`, 1-year TTL)
- First-visit detection via `Accept-Language`, defaults to FR

### Look & feel
- **Retro Panini sticker-album theme** — cream paper, navy + red + gold accents
- Bebas Neue (headings) + Lora (body), loaded from Google Fonts
- Football touches: striped pitch under the topbar and in the footer, goal-net pattern behind titles, ball watermarks on cards, center-circle artwork in the footer
- **3D football** (Three.js) — a small spinning ball in the topbar and a larger floating one on the landing page
- Procedural soccer-ball texture (12 black pentagons centered on the vertices of an icosahedron, plus the 30 seam arcs)
- **3D goal-shot mini-scene** animated above the leaderboard — sky + pitch backdrop, full goal cage with posts/crossbar/supports, translucent white net, parabolic top-corner shot, flash and bounce, looping every 3.8 s
- Lunatech logo in the top-right of the topbar and in the footer (drop the real SVG into `static/lunatech-logo.svg` to override the placeholder)

## Environment variables

All variables are declared in `.env` (copied from `.env.example`).

### Network & database
| Variable | Default | Description |
|---|---|---|
| `DATABASE_URL` | _(required)_ | PostgreSQL URL, e.g. `postgres://postgres:postgres@localhost:5434/lunatech_betting` |
| `BIND_ADDR` | `127.0.0.1:3000` | HTTP listen address + port |
| `BASE_URL` | `http://localhost:3000` | Public URL of the app — magic links use it |

### Sessions (cookie signing)
| Variable | Default | Description |
|---|---|---|
| `COOKIE_KEY` | _(required unless `DEV_MODE`)_ | Base64-encoded **64+ byte** secret used to sign private cookies. Generate with `openssl rand -base64 64`. If missing and `DEV_MODE=true`, the app generates a random key at startup (sessions are wiped on every restart). |

### SMTP (magic links + reminders)
| Variable | Default | Description |
|---|---|---|
| `SMTP_HOST` | `localhost` | SMTP host (Mailpit in dev, SES/Gmail/etc. in prod) |
| `SMTP_PORT` | `1025` | SMTP port |
| `SMTP_USERNAME` | _(empty)_ | Optional, when SMTP auth is required |
| `SMTP_PASSWORD` | _(empty)_ | Optional, when SMTP auth is required |
| `SMTP_STARTTLS` | `false` | Set to `true` to upgrade the connection with STARTTLS |
| `MAIL_FROM` | `lunatech-betting@lunatech.com` | "From" address used on outgoing emails |

### football-data.org
| Variable | Default | Description |
|---|---|---|
| `FOOTBALL_DATA_API_KEY` | _(empty)_ | Free API key from https://www.football-data.org/client/register. Empty → fixture sync disabled (handy in dev with seeded data). |
| `FOOTBALL_DATA_COMPETITION` | `WC` | Competition code. `WC` = FIFA World Cup (free tier). |

### Sign-up & admin
| Variable | Default | Description |
|---|---|---|
| `ALLOWED_EMAIL_DOMAIN` | `lunatech.com` | Email domain allowed to sign in via magic link |
| `ADMIN_EMAILS` | _(empty)_ | Comma-separated list of admin emails (whitespace ignored, case-insensitive). On every sign-in these users are promoted to admin (access to `/admin/stakes`). Remove an email from the list and the user loses admin on their next sign-in. |
| `STAKE_DEADLINE` | `2026-06-27T23:59:00Z` | RFC 3339 timestamp after which no new players can join the pot |

### Notifications
| Variable | Default | Description |
|---|---|---|
| `SLACK_WEBHOOK_URL` | _(empty)_ | Slack Incoming Webhook URL. Empty → Slack disabled. Docs: https://api.slack.com/messaging/webhooks |
| `REMINDER_LEAD_MINUTES` | `120` | How many minutes before kick-off the reminder is sent |
| `DAILY_DIGEST_HOUR` | `8` | UTC hour (0-23) from which the daily recap email goes out, covering the previous day |

### Development
| Variable | Default | Description |
|---|---|---|
| `DEV_MODE` | `false` | Enables `/dev` (one-click sign-in), allows a missing `COOKIE_KEY`, doesn't crash if SMTP is unreachable. **Never enable this in production.** |
| `RUST_LOG` | `lunatech_betting=info,tower_http=info` | Log level (`tracing-subscriber`) |

## Running in production

1. Provision a Postgres database and a working SMTP relay, and grab a `football-data.org` API key.

2. Generate a stable `COOKIE_KEY`:
   ```sh
   openssl rand -base64 64 | tr -d '\n'
   ```

3. Set at least these variables:
   ```sh
   DATABASE_URL=postgres://...
   BASE_URL=https://lunabet.example.com
   COOKIE_KEY=<64+ bytes base64>
   SMTP_HOST=...
   SMTP_PORT=587
   SMTP_USERNAME=...
   SMTP_PASSWORD=...
   SMTP_STARTTLS=true
   FOOTBALL_DATA_API_KEY=...
   ADMIN_EMAILS=nicolas.leroux@lunatech.com,...
   ```

4. Build in release mode:
   ```sh
   cargo build --release
   ./target/release/lunatech-betting
   ```

   Migrations are applied at startup. A background job syncs fixtures, recomputes scoring, and sends reminders every 5 minutes.

## Running locally (dev)

1. Start Postgres and Mailpit through docker-compose:
   ```sh
   docker compose up -d
   ```

   - Postgres listens on **`localhost:5434`** (the port is shifted to avoid clashing with other projects)
   - Mailpit web UI: http://localhost:8025

2. Copy the config:
   ```sh
   cp .env.example .env
   ```

   The example already ships with `DEV_MODE=true` and `ADMIN_EMAILS=nicolas.leroux@lunatech.com`. Tweak as needed.

3. Load the fixtures:
   ```sh
   cargo run -- seed
   ```

   This creates 5 fake Lunatech users (Nicolas as admin with 10€ paid, Alice 5€ paid, Bruno 2€ paid, Céline 5€ unpaid, David with no stake), 8 matches (3 finished, 5 upcoming), and 17 bets already placed.

4. Start the app:
   ```sh
   cargo run
   ```

5. Open http://localhost:3000/dev — pick a user and click "Sign in" to explore as that person without needing a magic link.

### Testing emails locally

- **Magic link** — go to http://localhost:3000/login, type any `@lunatech.com` email, then open http://localhost:8025 (Mailpit) to see the rendered email.
- **Match reminder** — run `cargo run -- notify` to fire the reminder job once manually (no need to wait 5 minutes for the next tick). Reminders go out for every match starting within `REMINDER_LEAD_MINUTES`.
- **Daily recap** — run `cargo run -- daily-digest` to send the recap once (defaults to yesterday). Pass a date to target a specific day, e.g. `cargo run -- daily-digest 2026-06-10`.

In dev mode:
- `COOKIE_KEY` is auto-generated when missing
- Magic links are printed to the console when SMTP is unreachable
- The `/dev` page returns 404 when `DEV_MODE=false`

**Never enable `DEV_MODE=true` in production.**

## Routes

| Route | Access | Description |
|---|---|---|
| `GET /` | public | Landing page with 3D football and sign-in buttons (redirects to `/matches` when signed in) |
| `GET /login`, `POST /login` | public | Request a magic link |
| `GET /login/sent` | public | Confirmation page after a magic link is sent |
| `GET /auth/callback?token=...` | public | Magic-link verification and session creation |
| `POST /logout` | authenticated | Sign out |
| `GET /matches` | authenticated | Upcoming + finished matches with the bet form |
| `POST /matches/:id/bet` | authenticated | Place or update a bet |
| `GET /leaderboard` | authenticated | Leaderboard with pot, payouts, and the 3D goal-shot mini-scene |
| `GET /stake`, `POST /stake` | authenticated | Pick a stake tier (2 / 5 / 10€) |
| `GET /admin/stakes` | admin | List of pot sign-ups, mark payment received |
| `POST /admin/stakes/:user_id/paid` | admin | Mark a player as paid |
| `POST /admin/stakes/:user_id/unpaid` | admin | Undo the payment |
| `GET /lang/:code` | public | Switch language (`fr` or `en`) via cookie |
| `GET /dev` | dev mode | List of seed users with one-click sign-in |
| `GET /dev/login?email=...` | dev mode | Direct sign-in (no magic link) |
| `GET /static/*` | public | Static assets (CSS, JS, SVG) |

## Code layout

```
migrations/                SQL migrations (sqlx, applied at startup)
  20260525000001_init.sql           users, sessions, magic_links, matches, bets
  20260525000002_match_reminders    matches.reminded_at
  20260525000003_stakes             stake_eur, stake_chosen_at, paid_at, paid_by
  20260525000004_stakes_2_5_10      CHECK constraint (2, 5, 10)

src/
  main.rs                bootstrap, migrations, background jobs
  config.rs              environment-variable parsing
  state.rs               AppState (pool, cookie key, http client, config)
  models.rs              User, Match, Bet
  i18n.rs                Locale enum (FR/EN), extractor from cookie/header
  error.rs               AppError wrapping anyhow
  football_data.rs       football-data.org client
  scoring.rs             scoring rules + SQL recompute, unit tests
  stakes.rs              pot, top-3, payout formula, unit tests
  notifications.rs       reminder dispatch (email + Slack)
  mail.rs                SMTP wrapper + multipart HTML rendering (magic link + reminders)
  fixtures.rs            `cargo run -- seed` command
  routes/
    mod.rs               router assembly
    auth.rs              magic link + sessions + AuthUser extractor
    home.rs              landing page
    matches.rs           matches list
    bets.rs              bet placement
    leaderboard.rs       leaderboard + pot + payouts
    stake.rs             stake tier selection
    admin.rs             /admin/stakes + AdminUser extractor
    dev.rs               /dev page (dev mode only)
    lang.rs              FR/EN switch (cookie)

templates/               Askama (compiled into the binary)
  base.html              layout (topbar, language switcher, pitch footer)
  home.html, login.html, login_sent.html
  matches.html, leaderboard.html, stake.html
  admin_stakes.html, dev.html
  emails/
    magic_link.html      HTML magic-link email (Locale-dependent)
    match_reminder.html  HTML reminder email (bilingual side-by-side)

static/                  assets served by tower-http ServeDir
  style.css              Panini theme, paper/navy/red/gold palette
  ball.js                Three.js — 3D ball + goal-shot mini-scene
  lunatech-logo.svg      placeholder (replace with the real logo)
```

## Tests

```sh
cargo test
```

Covers:
- `scoring::compute_points` (4 cases: exact, right winner, right draw, missed)
- `stakes::compute_payouts` (6 cases: equal stakes, sum equals pot, larger stake gets a larger share, empty pot, fewer than 3 winners, single winner)

## Production checklist

- `BASE_URL` must point at the real public URL — magic links depend on it.
- `COOKIE_KEY` must be stable (rotating it invalidates every active session).
- Use a real SMTP relay (Gmail SMTP, SES, etc.), not Mailpit.
- Snapshot the Postgres database during the tournament — bets must be preserved.
- Free tier of football-data.org is rate-limited to 10 requests/minute; the job ticks every 5 minutes so there's plenty of headroom.
- **Legal note** — a real-money pool between colleagues, even when settled out-of-app ("honor system"), can fall under French ANJ gambling regulation. Run the project past HR or legal before any public rollout. The app never touches the money (payments happen outside the app), which limits exposure but does not eliminate it.

## License

Distributed under the [Apache 2.0](LICENSE) license — © 2026 Lunatech Labs.
