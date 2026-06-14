# 12. Client mobile iOS et Android (Tauri)

Statut: en cours. Priorite: moyenne. Effort: L.

Fait: fondations backend multi-canal (migration `platform` + `device_token` sur
`push_subscriptions`, dispatch `src/push_channel.rs` appele par
`notifications.rs`, `POST /push/subscribe` accepte les tokens natifs), fichiers
d'association de domaine (`src/routes/well_known.rs`, vars `APPLE_APP_ID` /
`ANDROID_PACKAGE` / `ANDROID_CERT_FINGERPRINT`), et scaffold du projet Tauri 2
coque distante dans `mobile/` (voir `mobile/README.md`).

Reste (hors sandbox, machine + comptes requis): senders APNs / FCM reels,
plugin push cote Tauri, generation des projets iOS/Android (`cargo tauri
ios/android init`), icones, signature et soumission aux stores.

## Objectif

Distribuer LunaBet comme application native sur l'App Store et le Play Store, avec un client Tauri 2 (mobile). L'objectif n'est pas de reecrire l'app mais d'emballer l'experience web existante dans une coque native, en ajoutant ce que le web seul ne fait pas bien sur mobile: presence sur l'ecran d'accueil via les stores, notifications push natives fiables (notamment iOS), et integration systeme (icone, splash, partage).

## Pourquoi Tauri

- L'app est rendue cote serveur (Askama + htmx), donc legere et adaptee a un affichage en webview.
- Tauri 2 supporte iOS et Android et produit des binaires natifs avec un coeur Rust, coherent avec la stack du backend.
- Empreinte plus faible qu'Electron / React Native, et reutilisation du savoir-faire Rust de l'equipe.

## Architecture

**Decision actee: coque distante.** La webview charge directement le site deploye (apex ou sous-domaine du tenant). Le binaire mobile n'embarque pas l'UI; il fournit la coque native, le push, les deep links et la persistance de session. Avantage clef: mise a jour produit instantanee cote serveur, sans re-soumission au store pour un changement d'UI. On reutilise tel quel l'app server-rendered, sans avoir a extraire une API JSON.

Alternative ecartee pour le lancement: assets embarques dans le bundle parlant a une API JSON dediee. Cout eleve (il faudrait separer une API de l'app server-rendered) et perte de la mise a jour instantanee. A reconsiderer seulement si un besoin offline fort apparait.

Selection de l'espace: au premier lancement, l'utilisateur saisit ou choisit son espace (slug), ou se connecte via le login central de l'apex qui redirige vers le bon tenant ([src/routes/auth.rs](../src/routes/auth.rs), flux central deja existant). Le slug retenu est persiste cote app.

## Prerequis cote produit

- [08-pwa-push](08-pwa-push.md) doit etre livre d'abord: manifest, service worker et surtout l'infrastructure de souscriptions push (`push_subscriptions`, cles VAPID, envoi depuis [src/notifications.rs](../src/notifications.rs)). Le client Tauri reutilise ce socle et y ajoute le push natif.

## Composants

### Projet Tauri
- Nouveau dossier `mobile/` (ou depot dedie) avec la structure Tauri 2: `src-tauri/`, configuration `tauri.conf.json`, cibles iOS et Android.
- Webview pointant vers l'URL de l'espace, avec permissions reseau et stockage.
- Persistance du cookie de session `lb_session` entre lancements (la webview doit conserver les cookies; sinon stocker le slug et laisser le magic link recreer la session).

### Authentification et deep links
- Les magic links et liens d'invitation arrivent par email et s'ouvrent dans le navigateur systeme. Configurer des **universal links (iOS)** et **app links (Android)** sur le domaine pour que `/auth/callback` et `/invite/accept` ouvrent l'app plutot que le navigateur.
- Cote serveur: servir `apple-app-site-association` et `assetlinks.json` (nouvelle route statique, par exemple dans [src/routes/seo.rs](../src/routes/seo.rs) ou un module dedie).
- Au retour du deep link, la webview navigue vers la cible authentifiee et la session est posee comme sur le web.

### Notifications push natives
- iOS: APNs. Android: FCM. Le plugin push de Tauri (ou un plugin communautaire) fournit le token d'appareil.
- Reutiliser la table `push_subscriptions` de [08-pwa-push](08-pwa-push.md) en distinguant le canal:

```sql
-- migrations/2026xxxx_push_native.sql
ALTER TABLE push_subscriptions ADD COLUMN platform TEXT NOT NULL DEFAULT 'web'
    CHECK (platform IN ('web', 'ios', 'android'));
ALTER TABLE push_subscriptions ADD COLUMN device_token TEXT;
```

- Cote backend, l'envoi se ramifie selon `platform`: web-push (VAPID) pour `web`, APNs pour `ios`, FCM pour `android`. Encapsuler dans un trait `PushChannel` appele par [src/notifications.rs](../src/notifications.rs), de sorte que la logique de declenchement (rappels, alertes de rang) reste partagee.
- L'enregistrement du token natif passe par la meme route `POST /push/subscribe`, avec `platform` et `device_token`.

### Integration systeme
- Icone et splash screen aux couleurs LunaBet (reutiliser `favicon.svg`, palette du tenant par defaut).
- Barre de statut et zones sures (notch) gerees par la config Tauri.
- Lien de partage natif pour les codes de ligue ([04-private-leagues](04-private-leagues.md)) et invitations ([11-invite-based-orgs](11-invite-based-orgs.md)).

## Backend, impact

- Ajouter les fichiers d'association de domaine (`apple-app-site-association`, `assetlinks.json`).
- Generaliser l'envoi de push multi-canal (trait + implementations APNs / FCM / web-push).
- Aucune modification de la logique de jeu: paris, scoring, classement, cagnotte restent serveur.

## CI / distribution

- Pipeline de build mobile separe (signature iOS via Apple Developer, signature Android keystore).
- Comptes stores, fiches, captures (reutiliser [docs/screenshots](../docs/screenshots)).
- Politiques stores: une app de pronostics avec mise reelle d'argent (cagnotte) peut declencher des regles "jeux d'argent". Important: la cagnotte LunaBet fonctionne a l'honneur, l'app ne manipule aucun paiement (cf. [src/stakes.rs](../src/stakes.rs)). A documenter clairement dans la fiche store et a verifier tres en amont, car c'est le principal risque de rejet.

## Cas limites

- Session expiree dans la webview: retomber sur l'ecran de login, magic link via deep link.
- Multi-espace: si l'utilisateur appartient a plusieurs espaces, proposer un selecteur (le login central gere deja le cas multi-tenant).
- Refus des notifications: degrader vers l'email, ne pas reproposer en boucle.
- Revue store: prevoir un compte de demonstration et le mode dev ([src/routes/dev.rs](../src/routes/dev.rs)) pour les reviewers.

## Criteres d'acceptation

- L'app se lance, retient l'espace, et affiche l'experience web en standalone sur iOS et Android.
- Un magic link ou un lien d'invitation ouvre l'app via deep link et authentifie l'utilisateur.
- Les notifications push natives arrivent sur les deux plateformes via le meme declencheur que le web.
- Aucune regression cote serveur: le web continue de fonctionner a l'identique.
