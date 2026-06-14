# 05. Jokers et multiplicateurs

Statut: a faire. Priorite: moyenne. Effort: M.

## Objectif

Ajouter une couche de strategie et de tension: permettre de miser un "joker" qui double les points d'un match choisi. Un joker est une mise de confiance: le joueur designe le match sur lequel il est le plus sur, et si ce pari rapporte des points (score exact ou bon resultat), ces points sont doubles. Cela cree un dilemme avant le coup d'envoi (ou placer sa confiance) et de l'euphorie ou du regret apres.

Fonctionnalite opt-in par espace pour ne pas perturber les espaces qui veulent garder le scoring simple.

## User stories

- En tant que joueur, je peux marquer un match a venir comme mon "joker" pour la phase en cours.
- En tant que joueur, mes points sur ce match sont doubles.
- En tant qu'admin, j'active ou desactive les jokers pour mon espace.

## Regles

- **Un joker par phase de competition.** La phase est portee par `matches.stage` (phase de groupes, huitiemes, quarts, demies, finale). Un joueur pose au plus un joker parmi les matchs d'une meme phase.
- Le joker doit etre pose avant le `kickoff_at` du match, comme un pari.
- Multiplicateur applique au calcul: `points_effectifs = points_base * multiplier`.
- Modifiable tant que le match cible n'a pas commence et tant qu'aucun match de la phase n'a verrouille le choix (voir cas limites).

## Modele de donnees

```sql
-- migrations/2026xxxx_multipliers.sql
ALTER TABLE tenants ADD COLUMN jokers_enabled BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE bets    ADD COLUMN multiplier INT NOT NULL DEFAULT 1
    CHECK (multiplier IN (1, 2));
```

Le joker est porte par le pari lui-meme (`bets.multiplier = 2`). Contrainte applicative: au plus un pari avec `multiplier = 2` par user et par phase (`matches.stage`).

## Backend

- [src/scoring.rs](../src/scoring.rs): `compute_points` reste inchange pour la base, mais `recompute_all` multiplie par `bets.multiplier` avant d'ecrire `bets.points`. Garder une trace claire: stocker `points` comme valeur finale (deja x multiplier).
- [src/routes/bets.rs](../src/routes/bets.rs): nouvelle action `POST /bets/:match_id/joker` (toggle) qui verifie:
  - jokers actives pour le tenant,
  - match encore ouvert,
  - aucun autre joker deja pose dans la meme phase (sinon le deplacer, avec confirmation).
- Determiner la phase via `matches.stage` du match cible, et chercher un eventuel joker existant sur les autres matchs de cette phase pour le meme user.
- Validation de l'unicite par phase en transaction.

## UI

- [templates/match_card.html](../templates/match_card.html): bouton "x2 joker" sur les matchs ouverts si la fonctionnalite est active. Etat visuel distinct quand pose.
- Bandeau d'aide la premiere fois ("Un joker par phase, double tes points").
- [templates/admin_settings.html](../templates/admin_settings.html): interrupteur "Activer les jokers".

## i18n

- "Joker" / "Joker", "Double tes points" / "Double your points", "Un joker par phase" / "One joker per phase".

## Cas limites

- Deplacer un joker deja pose dans la phase: retirer l'ancien, poser le nouveau, en transaction. Possible tant que le nouveau match cible n'a pas commence.
- Joker sur un match deja commence: refuse.
- Joker pose sur un match deja joue de la phase: le choix est fige des le coup d'envoi de ce match; on ne peut plus le deplacer vers un autre match de la meme phase, sinon on permettrait de changer d'avis apres coup.
- Phase a un seul match (finale): le joker y est trivial mais autorise.
- Desactivation des jokers par l'admin en cours de competition: les jokers deja poses restent honores, plus aucun nouveau possible.

## Criteres d'acceptation

- Un seul joker actif par periode et par joueur.
- Les points du match joker sont effectivement doubles dans le classement.
- Espace sans jokers actives: aucun changement de comportement ni d'UI.
