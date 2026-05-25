# LunaBet

App de paris sur la Coupe du Monde 2026 pour les employés de Lunatech.
Score exact, leaderboard, pot pondéré par mise. Bilingue FR / EN.

## Aperçu

| Accueil | Album de pronos |
|---|---|
| ![Page d'accueil](docs/screenshots/01-home.png) | ![Page Matches](docs/screenshots/04-matches.png) |

| Classement avec scène 3D | Mise dans le pot |
|---|---|
| ![Leaderboard](docs/screenshots/05-leaderboard.png) | ![Stake](docs/screenshots/06-stake.png) |

| Admin (gestion des mises) | English version |
|---|---|
| ![Admin Stakes](docs/screenshots/07-admin-stakes.png) | ![Leaderboard EN](docs/screenshots/08-leaderboard-en.png) |

### Emails

Tous les emails sont en **HTML multipart** (avec fallback texte brut), reprennent le branding de l'app (header navy, pelouse, bouton tampon-encreur rouge), embarquent le **logo Lunatech**, et sont :
- **Magic link** : adapté à la locale du visiteur (FR ou EN selon le cookie `lb_lang` au moment de la requête)
- **Rappel de match** : bilingue side-by-side (FR + EN dans le même email, vu qu'on ne stocke pas la langue par utilisateur)

| Magic link (FR) | Magic link (EN) |
|---|---|
| ![Magic Link FR](docs/screenshots/09-email-magic-link-fr.png) | ![Magic Link EN](docs/screenshots/10-email-magic-link-en.png) |

| Rappel de match (bilingue) |
|---|
| ![Match Reminder](docs/screenshots/11-email-reminder.png) |

<details>
<summary>Pages d'authentification & mode dev</summary>

| Connexion (magic link) | Mode développement |
|---|---|
| ![Login](docs/screenshots/02-login.png) | ![Dev page](docs/screenshots/03-dev.png) |

</details>

> Les captures sont générées par [`scripts/screenshots.sh`](scripts/screenshots.sh).
> Le serveur doit tourner sur `http://127.0.0.1:3000` avec les fixtures chargées (`cargo run -- seed && cargo run`).
> Le paramètre `?shot=1` fige l'animation 3D du classement sur une frame nette pour la capture.

## Stack

- **Rust + Axum** (web framework async)
- **PostgreSQL** via SQLx (runtime queries, migrations auto au démarrage)
- **Askama** (templates compilés à la compilation)
- **htmx** (interactivité côté client, zéro framework JS)
- **Three.js** via importmap CDN (ballon de foot 3D, mini-scène tir au but)
- **lettre** (envoi d'emails SMTP pour les magic links et rappels)
- **football-data.org** (récupération automatique des matches et résultats, code compétition `WC`)

## Fonctionnalités

### Paris
- Connexion par **magic link** envoyé par email, limitée à `@lunatech.com`
- Un seul type de pari : **score exact** (ex: 2-1)
- Les paris ferment **au coup d'envoi** du match
- Barème :
  - **3 points** pour un score exact
  - **1 point** pour le bon vainqueur (ou bon match nul)
  - **0 point** sinon
- Tiebreak du classement : points → scores exacts → paris résolus → ancienneté du compte

### Pot et mises
- 3 paliers : **2€ / 5€ / 10€** (page `/stake`)
- **Honor system** : l'app ne touche pas à l'argent. Le joueur verse sa mise à l'admin (Lydia, virement, etc.) qui marque "payé" dans la page `/admin/stakes`
- Date limite par défaut : **fin de la phase de groupes WC2026** (2026-06-27 23:59 UTC), configurable
- Répartition du pot entre les **3 premiers payeurs** du classement :
  - `payout_i = pot × (base_i × stake_i) / Σ(base × stake)` avec `base = [0.5, 0.3, 0.2]`
  - Quand les 3 paliers sont identiques → répartition 50 / 30 / 20 classique
  - Mise plus élevée = part plus grande de sa tranche
- Les joueurs **non payés** restent dans le classement avec un badge "non éligible" ; le payout passe au suivant qui a payé

### Notifications
- Job de fond toutes les 5 min : pour chaque match qui démarre dans `REMINDER_LEAD_MINUTES` minutes et n'a pas encore été annoncé :
  - **Email** personnalisé à chaque joueur sans pari sur ce match
  - **Message Slack** dans le canal cible (si `SLACK_WEBHOOK_URL` est défini)
- Chaque match n'est annoncé qu'une seule fois (`matches.reminded_at`)

### Internationalisation
- Interface entièrement bilingue **FR / EN**
- Switcher de langue dans la topbar (cookie `lb_lang` valable 1 an)
- Détection automatique via `Accept-Language` au premier accès, fallback FR

### Look & feel
- Thème **album Panini rétro** : papier crème, navy + rouge + accents or
- Typo Bebas Neue (titres) + Lora (corps), Google Fonts via CDN
- Effets foot : pelouse rayée sous la topbar et dans le footer, filet derrière les titres, watermarks ballon, rond de coup d'envoi au footer
- **Ballon 3D** Three.js dans la topbar (mini, tourne en permanence) et sur la home (grand, flotte)
- Procedural soccer-ball texture (12 pentagones noirs aux sommets d'un icosaèdre + 30 lignes de couture)
- **Mini-scène tir au but** animée au-dessus du classement : décor pelouse + ciel, cage avec poteaux/barre/supports, filet en lignes blanches translucides, tir parabolique en lucarne, flash et rebond, en boucle de 3.8 s
- Logo Lunatech en haut à droite de la topbar et au footer (placeholder SVG à remplacer par le vrai logo dans `static/lunatech-logo.svg`)

## Variables d'environnement

Toutes définies dans `.env` (copié depuis `.env.example`).

### Réseau & base de données
| Variable | Défaut | Description |
|---|---|---|
| `DATABASE_URL` | _(requis)_ | URL PostgreSQL, ex: `postgres://postgres:postgres@localhost:5434/lunatech_betting` |
| `BIND_ADDR` | `127.0.0.1:3000` | Adresse + port d'écoute HTTP |
| `BASE_URL` | `http://localhost:3000` | URL publique de l'app — les magic links en dépendent |

### Sécurité (sessions)
| Variable | Défaut | Description |
|---|---|---|
| `COOKIE_KEY` | _(requis sauf en `DEV_MODE`)_ | Clé base64 de **64+ bytes** pour signer les cookies privés. Générer avec `openssl rand -base64 64`. Si absente et `DEV_MODE=true`, l'app génère une clé aléatoire au démarrage (sessions invalidées à chaque redémarrage). |

### SMTP (magic links + rappels)
| Variable | Défaut | Description |
|---|---|---|
| `SMTP_HOST` | `localhost` | Hôte SMTP (Mailpit en local, SES/Gmail/etc. en prod) |
| `SMTP_PORT` | `1025` | Port SMTP |
| `SMTP_USERNAME` | _(vide)_ | Optionnel, si auth |
| `SMTP_PASSWORD` | _(vide)_ | Optionnel, si auth |
| `SMTP_STARTTLS` | `false` | `true` pour activer STARTTLS |
| `MAIL_FROM` | `lunatech-betting@lunatech.com` | Adresse expéditeur des emails |

### football-data.org
| Variable | Défaut | Description |
|---|---|---|
| `FOOTBALL_DATA_API_KEY` | _(vide)_ | Clé API gratuite obtenue sur https://www.football-data.org/client/register. Vide → pas de sync (utile en dev avec fixtures) |
| `FOOTBALL_DATA_COMPETITION` | `WC` | Code de la compétition. `WC` = Coupe du Monde FIFA (free tier) |

### Inscriptions & admin
| Variable | Défaut | Description |
|---|---|---|
| `ALLOWED_EMAIL_DOMAIN` | `lunatech.com` | Domaine email autorisé pour la connexion magic link |
| `ADMIN_EMAILS` | _(vide)_ | Liste d'emails admin séparés par des virgules. À la connexion, ces utilisateurs sont automatiquement promus admin (accès à `/admin/stakes`). |
| `STAKE_DEADLINE` | `2026-06-27T23:59:00Z` | Timestamp RFC3339 après lequel les joueurs ne peuvent plus s'inscrire au pot |

### Notifications
| Variable | Défaut | Description |
|---|---|---|
| `SLACK_WEBHOOK_URL` | _(vide)_ | URL d'un incoming webhook Slack. Vide → désactivé. Doc : https://api.slack.com/messaging/webhooks |
| `REMINDER_LEAD_MINUTES` | `120` | Combien de minutes avant le coup d'envoi envoyer les rappels |

### Développement
| Variable | Défaut | Description |
|---|---|---|
| `DEV_MODE` | `false` | Active la page `/dev` (login en un clic), autorise `COOKIE_KEY` absent, ne plante pas si SMTP indispo. **À ne jamais activer en prod.** |
| `RUST_LOG` | `lunatech_betting=info,tower_http=info` | Niveau de logs (`tracing-subscriber`) |

## Lancer en prod

1. Préparer un Postgres et un SMTP fonctionnels, obtenir une clé `football-data.org`.

2. Générer un `COOKIE_KEY` stable :
   ```sh
   openssl rand -base64 64 | tr -d '\n'
   ```

3. Définir au minimum ces variables :
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

4. Builder en release :
   ```sh
   cargo build --release
   ./target/release/lunatech-betting
   ```

   Les migrations sont appliquées au démarrage. Un job de fond synchronise les fixtures, recalcule le scoring et envoie les rappels toutes les 5 minutes.

## Lancer en local (dev)

1. Démarrer Postgres et Mailpit via docker-compose :
   ```sh
   docker compose up -d
   ```

   - Postgres écoute sur **`localhost:5434`** (port décalé pour éviter les conflits avec d'autres projets)
   - Mailpit : interface web sur http://localhost:8025

2. Copier la configuration :
   ```sh
   cp .env.example .env
   ```

   Le `.env.example` a déjà `DEV_MODE=true` et `ADMIN_EMAILS=nicolas.leroux@lunatech.com`. Adapte si besoin.

3. Charger les fixtures :
   ```sh
   cargo run -- seed
   ```

   Crée 5 utilisateurs Lunatech fictifs (Nicolas admin avec 10€ payés, Alice 5€ payés, Bruno 2€ payés, Céline 5€ non payés, David sans mise), 8 matches (3 terminés, 5 à venir), 17 paris déjà placés.

4. Lancer l'app :
   ```sh
   cargo run
   ```

5. Ouvrir http://localhost:3000/dev — choisir un utilisateur et cliquer "Se connecter" pour explorer sans magic link.

### Tester les emails en local

- **Magic link** : aller sur http://localhost:3000/login, taper un email `@lunatech.com`, puis ouvrir http://localhost:8025 (Mailpit) pour voir l'email rendu.
- **Rappel de match** : exécuter `cargo run -- notify` pour déclencher le job une fois manuellement (sans attendre les 5 min de la boucle). Les rappels sont envoyés pour tous les matches qui démarrent dans `REMINDER_LEAD_MINUTES`.

En mode dev :
- `COOKIE_KEY` est auto-généré si absent
- Magic links loggués dans la console si SMTP est indispo
- La page `/dev` renvoie 404 si `DEV_MODE=false`

**N'active jamais `DEV_MODE=true` en production.**

## Pages

| Route | Accès | Description |
|---|---|---|
| `GET /` | public | Landing avec ballon 3D et boutons de connexion (ou redirect `/matches` si connecté) |
| `GET /login`, `POST /login` | public | Demande de magic link |
| `GET /login/sent` | public | Confirmation après envoi du magic link |
| `GET /auth/callback?token=...` | public | Validation du magic link et création de session |
| `POST /logout` | authentifié | Déconnexion |
| `GET /matches` | authentifié | Liste des matches à venir + terminés, formulaire de pari |
| `POST /matches/:id/bet` | authentifié | Place ou met à jour un pari |
| `GET /leaderboard` | authentifié | Classement avec pot et payouts, mini-scène tir au but 3D |
| `GET /stake`, `POST /stake` | authentifié | Choix du palier de mise (2/5/10€) |
| `GET /admin/stakes` | admin | Liste des inscriptions au pot, marquage paiement |
| `POST /admin/stakes/:user_id/paid` | admin | Marquer un joueur comme ayant payé |
| `POST /admin/stakes/:user_id/unpaid` | admin | Annuler le paiement |
| `GET /lang/:code` | public | Bascule la langue (`fr` ou `en`) via cookie |
| `GET /dev` | dev mode | Liste des utilisateurs de test avec login en un clic |
| `GET /dev/login?email=...` | dev mode | Login direct sans magic link |
| `GET /static/*` | public | Assets statiques (CSS, JS, SVG) |

## Structure du code

```
migrations/                SQL migrations (sqlx, appliquées au démarrage)
  20260525000001_init.sql           users, sessions, magic_links, matches, bets
  20260525000002_match_reminders    matches.reminded_at
  20260525000003_stakes             stake_eur, stake_chosen_at, paid_at, paid_by
  20260525000004_stakes_2_5_10      contrainte CHECK (2, 5, 10)

src/
  main.rs                bootstrap, migrations, jobs de fond
  config.rs              parsing des variables d'environnement
  state.rs               AppState (pool, cookie key, http client, config)
  models.rs              User, Match, Bet
  i18n.rs                Locale enum (FR/EN), extractor depuis cookie/header
  error.rs               AppError wrapping anyhow
  football_data.rs       client API football-data.org
  scoring.rs             barème + SQL recompute, tests unitaires
  stakes.rs              pot, top3, formule de payout, tests unitaires
  notifications.rs       envoi rappels email + Slack
  mail.rs                wrapper SMTP, rendu HTML multipart (magic link + rappels)
  fixtures.rs            commande `cargo run -- seed`
  routes/
    mod.rs               agrégation du router
    auth.rs              magic link + sessions + extractor AuthUser
    home.rs              landing
    matches.rs           liste des matches
    bets.rs              placement de pari
    leaderboard.rs       classement + pot + payouts
    stake.rs             choix du palier
    admin.rs             /admin/stakes + extractor AdminUser
    dev.rs               page /dev (mode développement uniquement)
    lang.rs              switch FR/EN (cookie)

templates/               Askama (compilés à la compilation Rust)
  base.html              layout (topbar, switcher de langue, footer pelouse)
  home.html, login.html, login_sent.html
  matches.html, leaderboard.html, stake.html
  admin_stakes.html, dev.html
  emails/
    magic_link.html      email HTML magic link (bilingue selon Locale)
    match_reminder.html  email HTML rappel (bilingue FR+EN side-by-side)

static/                  assets servis par tower-http ServeDir
  style.css              thème Panini, palette papier/navy/rouge/or
  ball.js                Three.js : ballon 3D + mini-scène tir au but
  lunatech-logo.svg      placeholder (à remplacer par le vrai logo)
```

## Tests

```sh
cargo test
```

Couvre :
- `scoring::compute_points` (4 cas : exact, bon vainqueur, bon nul, raté)
- `stakes::compute_payouts` (6 cas : mises égales, somme = pot, plus grosse mise = plus grosse part, pot vide, ≤2 winners, 1 winner)

## Points d'attention pour la prod

- `BASE_URL` doit pointer sur l'URL publique (les magic links en dépendent).
- `COOKIE_KEY` doit être stable (rotation = invalide toutes les sessions).
- Configurer un vrai SMTP (Gmail SMTP, SES, etc.) au lieu de Mailpit.
- Snapshot Postgres pendant la compétition (les paris doivent être préservés).
- Plan gratuit football-data.org : 10 requêtes/minute, le job tourne toutes les 5 min → marge confortable.
- **Cadre légal** : un pot d'argent réel entre collègues, même via "honor system", peut tomber sous la régulation de l'ANJ en France. Faire valider par les RH ou le service juridique avant ouverture publique. L'app ne touche jamais à l'argent (les paiements se font hors-app), ce qui limite l'exposition mais ne l'élimine pas.
