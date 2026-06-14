# 04. Ligues privees entre amis

Statut: a faire. Priorite: haute. Effort: M.

## Objectif

Permettre, a l'interieur d'un espace, de creer des mini-ligues regroupant un sous-ensemble de joueurs avec leur propre classement. La comparaison sociale rapprochee (mes amis, mes collegues d'equipe) est un levier de retention bien plus fort que le classement global ou les retardataires decrochent vite.

A ne pas confondre avec les espaces (tenants): une ligue est un groupe interne a un espace, qui partage les memes matchs et les memes paris. Aucun nouveau pari n'est cree, on filtre seulement le classement.

## User stories

- En tant que joueur, je cree une ligue et j'obtiens un code de partage.
- En tant que joueur, je rejoins une ligue avec un code.
- En tant que joueur, je vois un classement filtre sur les membres de ma ligue, reutilisant les points existants.
- En tant que createur, je renomme ou supprime ma ligue et je retire des membres.

## Modele de donnees

```sql
-- migrations/2026xxxx_leagues.sql
CREATE TABLE leagues (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL REFERENCES tenants(id),
    name        TEXT NOT NULL,
    join_code   TEXT NOT NULL,
    owner_user_id UUID NOT NULL REFERENCES users(id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, join_code)
);

CREATE TABLE league_members (
    league_id  UUID NOT NULL REFERENCES leagues(id) ON DELETE CASCADE,
    user_id    UUID NOT NULL REFERENCES users(id),
    joined_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (league_id, user_id)
);
```

Le `join_code` est court et lisible (par exemple 6 caracteres base32 sans ambiguites). Unicite par tenant.

## Backend

Nouveau module routes `src/routes/leagues.rs`:

- `GET /leagues`: mes ligues + formulaire de creation / jonction.
- `POST /leagues`: creer (genere `join_code`, ajoute le createur comme membre).
- `POST /leagues/join`: rejoindre via code.
- `GET /leagues/:id`: classement de la ligue.
- `POST /leagues/:id/leave`, `POST /leagues/:id/remove` (createur), `POST /leagues/:id/rename`, `DELETE /leagues/:id`.

Le classement reutilise la logique de [src/stakes.rs](../src/stakes.rs) en ajoutant un filtre `user_id IN (SELECT user_id FROM league_members WHERE league_id = $1)`. Refactorer `load_leaderboard` pour accepter un filtre optionnel de membres.

Garde-fous:
- Toutes les routes exigent `AuthUser` et verifient que la ligue appartient au tenant courant.
- Seul le createur supprime ou retire des membres.
- Plafond raisonnable de ligues par user (par exemple 20) pour limiter l'abus.

## UI

- Nouvelle entree de menu "Ligues" dans [templates/_nav.html](../templates/_nav.html).
- [templates/leagues.html](../templates/leagues.html): liste de mes ligues, bouton creer, champ code pour rejoindre, partage du code (copier dans le presse-papier).
- [templates/league.html](../templates/league.html): classement filtre, reutilise le composant visuel du classement global.

## Stakes et cagnotte

Les ligues sont purement ludiques au depart: pas de cagnotte par ligue. La cagnotte reste au niveau de l'espace ([src/stakes.rs](../src/stakes.rs)). Une cagnotte par ligue est une extension future, hors scope.

## i18n

- "Ligues" / "Leagues", "Creer une ligue" / "Create a league", "Code d'invitation" / "Join code", "Rejoindre" / "Join".

## Cas limites

- Code en collision: regenerer jusqu'a unicite.
- Rejoindre deux fois: `ON CONFLICT DO NOTHING`.
- Createur qui quitte: transferer la propriete au plus ancien membre, ou supprimer si vide.
- Ligue vide apres depart: suppression automatique optionnelle.

## Criteres d'acceptation

- Un classement de ligue n'affiche que ses membres, avec les memes points que le classement global.
- Un code permet de rejoindre, un mauvais code renvoie une erreur claire.
- Les droits createur sont respectes (rename, remove, delete).
