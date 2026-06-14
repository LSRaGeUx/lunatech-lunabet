# 07. Compte a rebours et urgence

Statut: a faire. Priorite: haute. Effort: S.

## Objectif

Augmenter le taux de pari en creant un sentiment d'urgence avant le coup d'envoi. Un compte a rebours visible ("plus que 23 min pour parier") pousse a l'action immediate, surtout sur les matchs ou l'utilisateur n'a pas encore mise.

## User stories

- En tant que joueur, je vois sur chaque match ouvert le temps restant avant le coup d'envoi.
- Quand il reste moins d'une heure et que je n'ai pas parie, l'indicateur passe en mode urgent (couleur accent, pulsation).
- A l'expiration, la carte bascule automatiquement en "verrouille" sans rechargement manuel.

## Approche

Tout cote client a partir du `kickoff_at` deja rendu dans le HTML. Pas de nouveau champ ni de requete serveur.

## Backend

- S'assurer que `match_card.html` expose `kickoff_at` en ISO 8601 dans un attribut `data-kickoff` (verifier l'existant, sinon l'ajouter). [src/models.rs](../src/models.rs) a deja `is_open_for_bets()`.
- Aucune route nouvelle.

## UI

- [templates/match_card.html](../templates/match_card.html): element `.countdown[data-kickoff]` sur les matchs ouverts.
- Nouveau `static/countdown.js`: tick chaque seconde, formate "2h 14m", "23 min", "moins d'1 min". Sous un seuil (60 min) ajoute la classe `.countdown-urgent`. A zero, desactive les inputs du formulaire et ajoute `.locked` sans rechargement.
- CSS dans [static/style.css](../static/style.css): `.countdown`, `.countdown-urgent` (pulsation, couleur accent du tenant), etat verrouille.
- S'appuyer sur [static/timezone-converter.js](../static/timezone-converter.js) existant pour la coherence des fuseaux.

## i18n

- Formats "h" / "h", "min" / "min", "Coup d'envoi imminent" / "Kickoff imminent", "Paris clos" / "Bets closed". Passer les libelles via `data-*` ou un petit dictionnaire injecte selon `loc`.

## Cas limites

- Horloge client decalee: tolerance acceptable, le serveur reste l'autorite pour accepter ou refuser un pari (deja le cas via `is_open_for_bets`).
- Onglet en arriere-plan longtemps: recalculer a partir de l'heure courante, pas d'accumulation de derive.
- Match deja commence au chargement: rendre directement verrouille.

## Criteres d'acceptation

- Le compte a rebours est exact a la minute pres et passe en urgent sous 60 min.
- A l'expiration, la carte se verrouille sans action utilisateur.
- Aucune regression sur l'acceptation serveur des paris.
