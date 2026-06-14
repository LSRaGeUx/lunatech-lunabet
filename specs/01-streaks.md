# 01. Series (streaks)

Statut: a faire. Priorite: haute. Effort: S.

## Objectif

Recompenser la regularite en comptant les matchs consecutifs ou l'utilisateur a marque des points. La serie est le ressort d'addiction le moins couteux: elle se calcule entierement a partir de `bets.points` deja persiste, et la peur de "casser sa serie" ramene l'utilisateur chaque jour.

## User stories

- En tant que joueur, je vois ma serie en cours ("3 matchs de suite avec des points") sur le tableau de bord.
- En tant que joueur, je recois un rappel "ne casse pas ta serie" quand j'ai une serie active et que je n'ai pas encore parie sur le prochain match.
- En tant que joueur, je vois la meilleure serie de l'espace sur le classement.

## Definition

- Serie courante: nombre de matchs termines consecutifs (par ordre de `kickoff_at`) sur lesquels l'utilisateur avait un pari avec `points > 0`.
- Un match termine sur lequel l'utilisateur n'avait pas parie, ou a fait 0 point, remet la serie a zero.
- Meilleure serie: plus longue sequence historique.

## Modele de donnees

Aucune table obligatoire: la serie peut se deriver a la volee depuis `bets` et `matches`. Pour eviter de recalculer a chaque page, on materialise sur `users`:

```sql
-- migrations/2026xxxx_streaks.sql
ALTER TABLE users ADD COLUMN current_streak INT NOT NULL DEFAULT 0;
ALTER TABLE users ADD COLUMN best_streak    INT NOT NULL DEFAULT 0;
ALTER TABLE users ADD COLUMN streak_updated_match_id BIGINT;
```

`streak_updated_match_id` retient le dernier match termine deja pris en compte, pour rendre la mise a jour idempotente.

## Backend

Nouveau module `src/streaks.rs`:

- `recompute_for_tenant(pool, tenant_id)`: pour chaque user, parcourt ses matchs termines par `kickoff_at`, met a jour `current_streak` / `best_streak`.
- Appel branche dans la boucle de scoring existante de [src/main.rs](../src/main.rs), juste apres `scoring::recompute_all`, donc toutes les 5 min. Pas de nouveau job.
- Helper `streak_of(user)` lisible par les routes.

Requete de calcul (esquisse, par user):

```sql
SELECT m.id, b.points
FROM matches m
JOIN bets b ON b.match_id = m.id AND b.user_id = $1
WHERE m.status = 'FINISHED' AND b.tenant_id = $2
ORDER BY m.kickoff_at ASC;
```

On replie en Rust pour calculer la serie suffixe (current) et le max (best).

## UI

- [templates/today.html](../templates/today.html): badge "Serie: 3 (record 5)" pres du nom, avec une flamme. Reutiliser le style des badges de tier existants.
- [templates/leaderboard.html](../templates/leaderboard.html): colonne "Serie" avec icone flamme, triable visuellement.
- CSS dans [static/style.css](../static/style.css): classe `.streak-badge` qui s'intensifie selon la longueur (3, 5, 10).

## i18n

- "Serie" / "Streak", "record" / "best", "Ne casse pas ta serie !" / "Don't break your streak!".

## Notifications

Etendre le rappel de match dans [src/notifications.rs](../src/notifications.rs): si le destinataire a `current_streak >= 3` et n'a pas encore parie sur le match a venir, ajouter une ligne d'accroche dans l'email. Pas de nouvel email, juste une variante de contenu.

## Cas limites

- Premier match jamais pari: serie 0, pas de badge.
- Match sans pari au milieu: coupe la serie.
- Recalcul rejoue: idempotent grace au parcours complet, `best_streak` ne descend jamais.

## Criteres d'acceptation

- La serie s'affiche correctement apres au moins deux matchs termines consecutifs avec points.
- Un 0 point casse la serie courante mais conserve le record.
- La page classement liste les series sans requete N+1 (un seul SELECT agrege ou colonnes materialisees).
