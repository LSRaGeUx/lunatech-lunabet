# 02. Joueur du jour

Statut: a faire. Priorite: haute. Effort: S.

## Objectif

Mettre en avant chaque jour le meilleur pronostiqueur de la veille. La reconnaissance publique cree une raison de revenir voir "qui a gagne" et valorise les joueurs sans monopoliser le classement general.

## User stories

- En tant que joueur, je vois en haut du tableau de bord qui a marque le plus de points sur les matchs de la veille.
- En tant que joueur du jour, je vois une mise en avant de mon avatar et un libelle "Joueur du jour".
- L'email digest mentionne le joueur du jour.

## Definition

- Periode: les matchs `FINISHED` dont le `kickoff_at` tombe dans la journee calendaire precedente (fuseau Amsterdam, comme le digest existant).
- Score du jour: somme des `bets.points` du user sur ces matchs.
- Gagnant: score du jour le plus eleve. Egalite departagee par nombre de scores exacts, puis ordre alphabetique du nom.
- Si aucun match termine la veille: pas de joueur du jour.

## Modele de donnees

```sql
-- migrations/2026xxxx_player_of_the_day.sql
CREATE TABLE player_of_the_day (
    tenant_id   UUID NOT NULL REFERENCES tenants(id),
    day         DATE NOT NULL,
    user_id     UUID NOT NULL REFERENCES users(id),
    points      INT  NOT NULL,
    exact_count INT  NOT NULL,
    computed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, day)
);
```

Cle primaire `(tenant_id, day)` qui rend le calcul idempotent, comme `daily_digests`.

## Backend

- Calcul dans `src/streaks.rs` ou un nouveau `src/highlights.rs`, fonction `compute_player_of_the_day(pool, tenant, day)`.
- Branche dans le meme planificateur que le digest quotidien dans [src/notifications.rs](../src/notifications.rs): le calcul precede l'envoi du digest, ainsi l'email peut citer le gagnant.
- Route de lecture: helper appele par [src/routes/today.rs](../src/routes/today.rs) pour charger l'entree du jour courant (qui reflete la veille).

## UI

- [templates/today.html](../templates/today.html): bandeau "Joueur du jour" en tete, avec avatar ([src/characters.rs](../src/characters.rs)), nom, points marques. Style festif reutilisant `manga-burst.svg`.
- [templates/emails/daily_digest.html](../templates/emails/daily_digest.html): une ligne "Joueur du jour: X (N pts)".

## i18n

- "Joueur du jour" / "Player of the day", "a marque" / "scored".

## Cas limites

- Egalite: depart deterministe (exacts puis nom) pour eviter un gagnant qui change entre deux calculs.
- Espace a un seul joueur: il est joueur du jour des qu'il marque, acceptable.
- Aucun match la veille: ne rien afficher, ne pas inserer de ligne.

## Criteres d'acceptation

- Le joueur du jour correspond au plus haut total de points de la veille.
- Le calcul est idempotent (rejouer ne cree pas de doublon).
- Le bandeau disparait les jours sans matchs termines.
