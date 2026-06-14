# 13. Push live: score et top 5 a chaque changement de score

Statut: fait. Priorite: moyenne. Effort: M.

Implemente: detection du delta de score dans `notifications::send_live_score_updates`
(colonnes `matches.last_pushed_*`), diffusion par tenant avec le top 5, polling
adaptatif dans [src/main.rs](../src/main.rs). Sous-commande de test:
`cargo run -- live-score`.

## Objectif

Pousser une notification a chaque fois que le score d'un match change (but
marque, coup de sifflet final), avec le nouveau score et le top 5 du classement
du tenant. Le but est de transformer chaque but en evenement collectif et de
ramener les joueurs dans l'app pendant les matchs, pas seulement avant le coup
d'envoi (rappels, spec 08) ou apres (digest).

S'appuie sur le socle Web Push pur-Rust deja livre ([08-pwa-push](08-pwa-push.md),
`src/webpush.rs`, `push_subscriptions`, `push_to_user`) et complete la
celebration en page ([06-realtime-celebration](06-realtime-celebration.md)) par
un canal hors-app.

## User stories

- En tant que joueur, je recois un push "But ! France 2 - 1 Bresil" pendant le
  match, avec le top 5 actuel du classement.
- En tant que joueur, je vois le classement bouger en direct sans avoir l'app
  ouverte.
- En tant que joueur qui a coupe les notifications, je ne recois rien.

## Cadence de polling (adaptatif)

Les conditions d'utilisation de football-data.org imposent d'adapter la
frequence des requetes a l'activite des matchs, sous peine de bannissement IP.
La boucle de scoring de [src/main.rs](../src/main.rs) fait donc du **polling
adaptatif**:

- **1 min** quand un match est en cours ou imminent;
- **10 min** sinon (rien ne se passe).

"En cours ou imminent" = `any_match_active`: un match `IN_PLAY` / `PAUSED`, ou
un match `SCHEDULED` / `TIMED` dont le coup d'envoi tombe dans la fenetre [10 min
avant, 3 h apres]. La fenetre autour du kickoff garde un polling rapide meme si
le champ `status` du fournisseur est en retard sur le coup d'envoi reel, donc le
push live reste prompt sans interroger l'API en continu.

Implementation: la boucle calcule son prochain delai en fin d'iteration
(`LIVE_INTERVAL` 60 s, `IDLE_INTERVAL` 600 s) au lieu d'un `interval` fixe.
Toutes les taches de la boucle (sync, scoring, streaks, achievements, badges,
push live, alertes de rang, rappels) sont idempotentes, donc la cadence variable
ne change que la fraicheur des donnees.

Consequence: pendant un match, "des que le score change" veut dire "dans la
minute qui suit". Un quota free tier (~10 req/min, un sync fait peu de requetes)
est largement respecte.

## Detection du changement de score

Les matchs ne sont pas scopes par tenant (une seule ligne `matches` partagee),
donc la detection se fait une seule fois, par comparaison aux colonnes
`last_pushed_*` plutot que dans `sync_fixtures` (qui reste inchange):

```sql
-- migrations/20260614000013_live_score_push.sql
ALTER TABLE matches
    ADD COLUMN last_pushed_home   INT,
    ADD COLUMN last_pushed_away   INT,
    ADD COLUMN last_pushed_status TEXT;
```

`send_live_score_updates` selectionne les matchs ou `(home_score, away_score,
status)` differe de `(last_pushed_home, last_pushed_away, last_pushed_status)`,
puis pour chacun:

- **but** = le score a change, les deux scores sont presents, et ce n'est pas
  l'ouverture `NULL -> 0-0` (simple coup d'envoi);
- **final** = passage a `FINISHED`;
- on ne pousse que si le match a deja ete observe (`last_pushed_status` non
  NULL): le premier passage ne fait que poser la baseline, donc activer la
  feature (ou un premier sync qui backfill des matchs deja joues) ne declenche
  aucun envoi en masse;
- les simples changements de `status` (coup d'envoi, mi-temps) ne poussent pas.

Apres examen, tous les candidats (pousses ou non) voient leurs colonnes
`last_pushed_*` mises a jour en une requete, donc un meme (match, score) n'est
jamais pousse deux fois, meme apres un redemarrage.

## Backend

- [src/notifications.rs](../src/notifications.rs): `send_live_score_updates(state)`
  detecte les deltas (ci-dessus), pose la baseline, puis pour chaque tenant
  appelle `live_score_for_tenant`:
  - construit le top 5 du tenant via `stakes::load_leaderboard`;
  - diffuse a tous les abonnes opt-in (`notify_push = TRUE`) via `push_to_user`
    (qui charge les souscriptions et purge les 410 / 404);
  - localise titre et corps selon `users.lang`.
- Appel dans la boucle de scoring de [src/main.rs](../src/main.rs), apres
  `scoring::recompute_all` (pour que le top 5 reflete les points fraichement
  calcules sur un coup de sifflet final).

Payload push (JSON lu par `static/sw.js`):

```json
{
  "title": "But ! France 2 - 1 Bresil",
  "body": "Classement: 1. Alice 24  2. Bob 21  3. ...",
  "url": "/leaderboard"
}
```

Pour un match termine, titre "Score final: France 2 - 1 Bresil".

## Destinataires

Diffusion a **tous les joueurs opt-in du tenant** (pas seulement le top 5, ni
seulement ceux qui ont parie sur ce match): l'evenement est collectif et le but
est de faire revenir tout le monde. Alternatives a trancher si le volume gene:

- limiter aux joueurs ayant parie sur ce match;
- limiter aux N premiers du classement.

## Preferences

Aujourd'hui un seul interrupteur `users.notify_push` coupe tous les push. Le
push live etant plus frequent que les rappels, prevoir (optionnel) une
preference dediee `notify_live_scores` pour le couper sans perdre les rappels.
A defaut, `notify_push` gouverne tout.

## UI

- Rien de nouveau cote opt-in: la carte Notifications du profil
  ([templates/profile.html](../templates/profile.html)) gere deja l'abonnement.
- Si preference dediee: ajouter une case dans cette carte.

## i18n

- Titres et corps localises selon `users.lang` (FR / EN), comme les rappels.
- "But !" / "Goal!", "Score final" / "Final score", "Classement" / "Standings".

## Cas limites

- Volume: un match a buts nombreux x beaucoup d'abonnes = rafale d'envois.
  Acceptable pour des tenants de taille bureau; a surveiller sinon.
- Buts rapproches: avec un polling de 5 min, on peut ne voir que le delta net
  (ex: 0-0 vu directement a 2-0). Le polling adaptatif reduit le risque.
- Score corrige par football-data.org (but annule): le score "recule"; on
  pousse quand meme la correction (le score affiche reste juste).
- Match a l'arret / postponed: traiter le changement de `status` comme un
  evenement a notifier ou l'ignorer (a trancher; par defaut, notifier seulement
  les changements de score et le passage a FINISHED).

## Criteres d'acceptation

- Un but detecte au sync declenche un push contenant le nouveau score et le top
  5 du tenant, a tous les abonnes opt-in.
- Le coup de sifflet final declenche un push "score final" avec le classement
  mis a jour.
- Aucun double envoi pour un meme (match, score) entre deux syncs ou apres un
  redemarrage.
- Couper les notifications stoppe effectivement ces push.
