# 06. Celebration de score en temps reel

Statut: a faire. Priorite: moyenne. Effort: S.

## Objectif

Donner un retour gratifiant au moment ou un pari est gagne. Aujourd'hui le scoring tourne en silence toutes les 5 min; on veut un effet visuel (confettis, burst manga, tigre) la premiere fois que l'utilisateur revoit un match qu'il a juste, pour ancrer la boucle de recompense.

## User stories

- En tant que joueur, quand je reviens sur l'app apres qu'un de mes pronos a ete valide, je vois une animation de celebration sur les matchs gagnes.
- L'animation ne se rejoue pas a chaque rechargement: une fois vue, elle s'eteint.

## Approche

Reutiliser l'esthetique existante: `manga-burst.svg`, le tigre de [static/easter-eggs.js](../static/easter-eggs.js). Pas de websocket: on detecte cote serveur les paris "nouvellement vus comme gagnes".

## Modele de donnees

```sql
-- migrations/2026xxxx_bet_seen.sql
ALTER TABLE bets ADD COLUMN result_seen_at TIMESTAMPTZ;
```

Un pari regle (`points` non nul) avec `result_seen_at IS NULL` est "a celebrer".

## Backend

- [src/routes/today.rs](../src/routes/today.rs) et [src/routes/matches.rs](../src/routes/matches.rs): a la lecture, selectionner les paris regles non encore vus de l'utilisateur, les exposer au template, puis les marquer `result_seen_at = NOW()` (apres rendu, ou via un petit POST de confirmation pour eviter de marquer si la page n'est pas reellement affichee).
- Distinguer le niveau de celebration: exact (3 pts, gros effet), outcome (1 pt, effet leger).

## UI

- [templates/match_card.html](../templates/match_card.html): attribut `data-celebrate="exact|outcome"` sur les cartes concernees.
- Nouveau `static/celebrate.js`: au chargement, scanne les cartes a celebrer et declenche confettis ou tigre, en s'appuyant sur les helpers existants des easter eggs (factoriser le code du tigre).
- Sons optionnels desactives par defaut.

## i18n

- Messages courts "Score exact ! +3" / "Exact score! +3", "Bien vu ! +1" / "Nice! +1".

## Cas limites

- Beaucoup de matchs gagnes d'un coup (retour apres plusieurs jours): limiter a une celebration agregee ("5 pronos gagnes, +11 pts") plutot que cinq animations.
- Pari perdu: pas d'animation, juste l'affichage existant.
- Le marquage `result_seen_at` doit etre robuste au double chargement (idempotent).

## Criteres d'acceptation

- L'animation apparait une seule fois par match gagne et par utilisateur.
- L'intensite reflete exact vs outcome.
- Aucune animation pour les paris deja vus ou perdus.
