# 09. Profil, stats et rivalites

Statut: a faire. Priorite: moyenne. Effort: M.

## Objectif

Donner a chaque joueur une page personnelle qui raconte son parcours (precision, meilleurs et pires pronos, badges, serie) et introduire des rivalites nominatives ("tu mets 3-2 a Marie cette semaine"). Les stats personnelles et la rivalite creent un attachement emotionnel qui depasse le classement brut.

## User stories

- En tant que joueur, j'ai une page profil avec mes statistiques: points, precision (% de scores exacts), serie, badges.
- En tant que joueur, je vois mon meilleur et mon pire prono.
- En tant que joueur, je peux me comparer en tete-a-tete avec un autre membre (bilan de la semaine, de la competition).

## Donnees

Aucune nouvelle table obligatoire: tout se derive de `bets`, `matches`, et des fonctionnalites liees ([01-streaks](01-streaks.md), [03-achievements-badges](03-achievements-badges.md)).

Statistiques calculees:
- Points totaux, nombre de paris regles.
- Precision exacte = scores exacts / paris regles.
- Precision resultat = (exacts + bons resultats) / paris regles.
- Meilleur prono: le score exact sur le match le plus "improbable" (heuristique simple: ecart de buts eleve ou affiche).
- Pire prono: pari avec le plus gros ecart au resultat reel.

## Backend

- Module routes `src/routes/profile.rs`:
  - `GET /profile`: mon profil.
  - `GET /profile/:user_id`: profil public d'un membre du meme tenant (lecture seule, donnees deja publiques via le classement).
  - `GET /h2h/:user_id`: comparaison tete-a-tete avec moi.
- Requetes agregees sur `bets` filtrees par user et tenant. Reutiliser les helpers de [src/stakes.rs](../src/stakes.rs) la ou possible.

Tete-a-tete: sur l'ensemble des matchs termines, comparer les points de chaque joueur, compter qui a fait mieux match par match, en deduire un score "victoires-defaites-egalites".

## UI

- [templates/profile.html](../templates/profile.html): avatar, nom, stats clefs, badges, serie, meilleur et pire prono.
- [templates/h2h.html](../templates/h2h.html): deux colonnes, bilan, evolution semaine.
- Liens depuis [templates/leaderboard.html](../templates/leaderboard.html): cliquer un nom ouvre son profil; bouton "Defier" ouvre le tete-a-tete.

## i18n

- "Profil" / "Profile", "Precision" / "Accuracy", "Meilleur prono" / "Best call", "Tete-a-tete" / "Head to head".

## Cas limites

- Joueur sans pari regle: precision masquee, message "pas encore de resultats".
- Tete-a-tete entre joueurs n'ayant aucun match commun parie: afficher "pas assez de donnees".
- Respect du scope tenant: on ne compare jamais des joueurs de tenants differents.

## Criteres d'acceptation

- Les pourcentages de precision sont coherents avec les comptes du classement.
- Le profil public ne montre que des donnees deja exposees publiquement.
- Le tete-a-tete reflete correctement le bilan match par match.
