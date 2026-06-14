# 03. Badges et hauts faits

Statut: a faire. Priorite: moyenne. Effort: M.

## Objectif

Offrir une progression visible et collectionnable au-dela du classement. Les badges donnent des micro-objectifs ("encore 12 points avant le prochain palier") et recompensent des comportements varies, pas seulement etre premier.

## User stories

- En tant que joueur, je gagne un badge quand je realise un haut fait (premier score exact, journee parfaite, paliers de points).
- En tant que joueur, je vois mes badges sur mon profil et le prochain badge a atteindre.
- En tant que joueur, je vois une notification discrete quand je debloque un badge.

## Catalogue initial

| Code | Nom | Condition |
|------|-----|-----------|
| first_exact | Premier sans faute | Premier score exact |
| perfect_day | Journee parfaite | Tous les pronos d'une journee exacts (min 2 matchs) |
| pts_50 / pts_100 / pts_250 | Paliers | Atteindre 50 / 100 / 250 points cumules |
| streak_5 / streak_10 | En feu | Serie de 5 / 10 (voir [01-streaks](01-streaks.md)) |
| marathon | Marathonien | Parier sur tous les matchs d'une phase |
| underdog | Outsider | Predire correctement une victoire d'equipe non favorite (ecart de classement) |

Le catalogue est statique en code (pas de table de definitions), seules les obtentions sont persistees.

## Modele de donnees

```sql
-- migrations/2026xxxx_achievements.sql
CREATE TABLE achievements (
    tenant_id  UUID NOT NULL REFERENCES tenants(id),
    user_id    UUID NOT NULL REFERENCES users(id),
    code       TEXT NOT NULL,
    earned_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, user_id, code)
);
```

La cle primaire empeche le double octroi et rend l'evaluation idempotente.

## Backend

- Module `src/achievements.rs`: une fonction par regle, plus `evaluate_user(pool, tenant, user_id)` qui insere les badges manquants (`INSERT ... ON CONFLICT DO NOTHING`).
- Appel apres le scoring dans [src/main.rs](../src/main.rs), pour les users dont un pari vient d'etre regle.
- Les badges "outsider" et "marathon" ont besoin d'une notion de favori et de liste des matchs d'une phase: deriver depuis `matches` (stage, group_name) sans donnee externe nouvelle.

## UI

- Nouvelle page profil (voir [09-profile-rivalries](09-profile-rivalries.md)) qui liste les badges obtenus et grises ceux a venir, avec la condition.
- Petit rang de badges sur [templates/leaderboard.html](../templates/leaderboard.html) a cote du nom (3 max, le reste en "+N").
- Toast htmx "Badge debloque" au prochain chargement de page apres obtention.
- Icones SVG dediees dans `static/badges/`, style coherent avec les avatars existants.

## i18n

Nom et description de chaque badge dans les deux langues, table de correspondance en Rust.

## Cas limites

- Recalcul historique: `evaluate_user` doit pouvoir tourner sur tout l'historique sans creer de doublons.
- Ajout d'un badge au catalogue: un balayage unique attribue le badge retroactivement aux eligibles.
- Journee parfaite avec un seul match: exclue pour eviter la trivialite.

## Criteres d'acceptation

- Un badge ne peut etre obtenu qu'une fois.
- L'ajout d'une regle n'impacte pas les badges deja attribues.
- Le profil affiche obtenus et prochains paliers avec progression chiffree.
