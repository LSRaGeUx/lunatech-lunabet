# LunaBet, specs produit: engagement et croissance

Ce dossier regroupe les specs des fonctionnalites visant deux objectifs:

1. Rendre l'app plus fun et plus addictive (boucles de recompense, social, urgence).
2. Ouvrir un nouveau mode de creation d'espace base sur l'invitation, sans dependance au domaine email (mode "amis").

Chaque spec est autonome: objectif, modele de donnees, backend, UI, i18n, cas limites, criteres d'acceptation. Les specs sont ecrites pour la stack actuelle: Rust + Axum, SQLx + PostgreSQL, templates Askama, htmx, multi-tenant.

## Etat des lieux (existant)

Deja en place et reutilisable comme socle:

- Pronostic de score exact, scoring auto 3 / 1 / 0 toutes les 5 min ([src/scoring.rs](../src/scoring.rs)).
- Classement avec cagnotte reelle et estimation de gains ([src/stakes.rs](../src/stakes.rs)).
- Avatars deterministes Captain Tsubasa ([src/characters.rs](../src/characters.rs)).
- Easter eggs ([static/easter-eggs.js](../static/easter-eggs.js)) et bouton "I feel lucky" ([static/lucky.js](../static/lucky.js)).
- Emails: rappel de match, digest quotidien, matchs du jour ([src/notifications.rs](../src/notifications.rs)).
- Multi-tenant avec resolution par sous-domaine, registre en cache ([src/tenant.rs](../src/tenant.rs)).
- Auth par magic link, gating par `allowed_email_pattern` ([src/routes/auth.rs](../src/routes/auth.rs)).
- Creation d'espace self-serve via `pending_tenants` ([src/routes/signup.rs](../src/routes/signup.rs)).
- Bilingue FR / EN ([src/i18n.rs](../src/i18n.rs)).

## Liste des specs

| # | Spec | Theme | Priorite | Effort |
|---|------|-------|----------|--------|
| 01 | [Series (streaks)](01-streaks.md) | Recompense / retention | Haute | S |
| 02 | [Joueur du jour](02-player-of-the-day.md) | Reconnaissance sociale | Haute | S |
| 03 | [Badges et hauts faits](03-achievements-badges.md) | Progression | Moyenne | M |
| 04 | [Ligues privees entre amis](04-private-leagues.md) | Social / retention | Haute | M |
| 05 | [Jokers et multiplicateurs](05-confidence-multipliers.md) | Strategie | Moyenne | M |
| 06 | [Celebration de score en temps reel](06-realtime-celebration.md) | Dopamine | Moyenne | S |
| 07 | [Compte a rebours et urgence](07-countdown-urgency.md) | Conversion | Haute | S |
| 08 | [PWA et notifications push](08-pwa-push.md) | Reengagement | Moyenne | L |
| 09 | [Profil, stats et rivalites](09-profile-rivalries.md) | Attachement | Moyenne | M |
| 10 | [Defis hebdomadaires](10-weekly-challenges.md) | Objectifs courts | Basse | M |
| 11 | [Espaces sur invitation (mode amis)](11-invite-based-orgs.md) | Croissance / onboarding | Haute | M |
| 12 | [Client mobile iOS et Android (Tauri)](12-mobile-tauri.md) | Distribution / reengagement | Moyenne | L |

Effort: S = 1 a 2 jours, M = 3 a 5 jours, L = 1 a 2 semaines (estimations indicatives, un developpeur).

## Phasage recommande

### Phase 1, quick wins a forte visibilite (1 a 2 semaines)
Brancher sur des donnees deja calculees, impact immediat.

- 01 Series
- 02 Joueur du jour
- 06 Celebration en temps reel
- 07 Compte a rebours et urgence

### Phase 2, croissance et social (2 a 3 semaines)
Les leviers de retention les plus forts.

- 11 Espaces sur invitation (mode amis)
- 04 Ligues privees entre amis

### Phase 3, profondeur de jeu (2 a 4 semaines)
A faire une fois la base sociale en place.

- 03 Badges et hauts faits
- 09 Profil, stats et rivalites
- 05 Jokers et multiplicateurs

### Phase 4, reengagement avance
- 08 PWA et notifications push
- 10 Defis hebdomadaires

### Phase 5, applications natives
A lancer une fois la PWA et le push web valides, le client Tauri reutilise ce socle.

- 12 Client mobile iOS et Android (Tauri)

## Principes transverses

- **Pas de regression sur le scoring existant.** Toute mecanique de points additionnelle (jokers, multiplicateurs) doit etre opt-in par tenant et reversible.
- **Tout est tenant-scoped.** Chaque nouvelle table porte un `tenant_id` et respecte le RLS deja en place.
- **Bilingue par defaut.** Tout texte visible passe par `loc.f("Francais", "English")`.
- **Pas de JS lourd.** On reste sur htmx + vanilla JS, un fichier par fonctionnalite dans `static/`.
- **Idempotence des jobs.** Les calculs periodiques (series, joueur du jour) suivent le pattern des tables d'idempotence existantes (`daily_digests`, `today_matches_emails`).
