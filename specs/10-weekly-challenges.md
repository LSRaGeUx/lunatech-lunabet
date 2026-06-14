# 10. Defis hebdomadaires

Statut: a faire. Priorite: basse. Effort: M.

## Objectif

Offrir un objectif court terme renouvele chaque semaine, independant du classement general. Les retardataires au classement gardent une raison de jouer ("defi de la semaine: 5 pronos exacts"), ce qui soutient l'activite sur la duree d'une longue competition.

## User stories

- En tant que joueur, je vois le defi de la semaine et ma progression.
- En tant que joueur, je gagne un badge ou une mention quand je le reussis.
- En tant qu'admin, le defi se genere automatiquement, sans intervention.

## Modele de donnees

```sql
-- migrations/2026xxxx_challenges.sql
CREATE TABLE weekly_challenges (
    tenant_id   UUID NOT NULL REFERENCES tenants(id),
    week_start  DATE NOT NULL,
    kind        TEXT NOT NULL,
    target      INT  NOT NULL,
    PRIMARY KEY (tenant_id, week_start)
);

CREATE TABLE weekly_challenge_results (
    tenant_id   UUID NOT NULL REFERENCES tenants(id),
    week_start  DATE NOT NULL,
    user_id     UUID NOT NULL REFERENCES users(id),
    progress    INT  NOT NULL DEFAULT 0,
    completed_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, week_start, user_id)
);
```

`kind` parmi un catalogue fixe en code: `exact_count` (N scores exacts), `points_total` (N points), `bet_all` (parier sur tous les matchs de la semaine).

## Backend

- Module `src/challenges.rs`:
  - `ensure_week(pool, tenant, week_start)`: cree le defi de la semaine si absent (choix du `kind` rotatif deterministe par numero de semaine, pas d'aleatoire car non dispo dans certains contextes).
  - `recompute_progress(pool, tenant, week_start)`: recalcule la progression de chaque joueur, marque `completed_at` au passage du seuil.
- Branche dans la boucle de scoring de [src/main.rs](../src/main.rs).
- A la completion, attribuer un badge via [03-achievements-badges](03-achievements-badges.md) (`code = weekly_<kind>`), ce qui reutilise tout le mecanisme d'affichage et de notification.

## UI

- Encart "Defi de la semaine" sur [templates/today.html](../templates/today.html): libelle, barre de progression, badge a la cle.

## i18n

- Libelles par `kind`: "Reussis 5 scores exacts cette semaine" / "Land 5 exact scores this week", etc.

## Cas limites

- Semaine sans match: pas de defi, ne rien creer.
- Changement de fuseau: aligner `week_start` sur le lundi Amsterdam, coherent avec le digest.
- Defi `bet_all` quand le nombre de matchs varie: cible = nombre de matchs de la semaine, calculee a la creation.

## Criteres d'acceptation

- Un defi unique par tenant et par semaine, genere automatiquement.
- La progression et la completion sont exactes et idempotentes.
- La completion accorde le badge correspondant.
