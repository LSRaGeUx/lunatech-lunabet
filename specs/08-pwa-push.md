# 08. PWA et notifications push

Statut: a faire. Priorite: moyenne. Effort: L.

## Objectif

Rendre l'app installable sur l'ecran d'accueil et envoyer des notifications push web, bien plus immediates que l'email. Cible: rappels avant coup d'envoi et alertes de gains ("tu viens de gagner 3 pts, tu passes 4e"). Ce socle web est aussi le prerequis du client mobile Tauri ([12-mobile-tauri](12-mobile-tauri.md)).

## User stories

- En tant que joueur, j'installe LunaBet comme une app (PWA).
- En tant que joueur, j'autorise les notifications et je recois un push "tu n'as pas encore parie, coup d'envoi dans 1h".
- En tant que joueur, je recois un push quand mes points changent mon rang.
- En tant que joueur, je gere mes preferences de notification.

## Composants

### PWA
- `static/manifest.webmanifest`: nom, icones (reutiliser `favicon.svg`), couleurs du tenant, `display: standalone`, `start_url: /today`.
- `static/sw.js`: service worker, cache applicatif minimal (offline shell) et reception des push.
- Balises dans [templates/base.html](../templates/base.html): lien manifest, enregistrement du service worker.
- Le manifest peut etre servi dynamiquement par tenant pour reprendre les couleurs et le logo (route legere ou template).

### Push web (VAPID)
Crate Rust: `web-push`.

```sql
-- migrations/2026xxxx_push_subscriptions.sql
CREATE TABLE push_subscriptions (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL REFERENCES tenants(id),
    user_id     UUID NOT NULL REFERENCES users(id),
    endpoint    TEXT NOT NULL,
    p256dh      TEXT NOT NULL,
    auth        TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (user_id, endpoint)
);

ALTER TABLE users ADD COLUMN notify_push BOOLEAN NOT NULL DEFAULT TRUE;
```

Cles VAPID en variables d'environnement ([src/config.rs](../src/config.rs)): `VAPID_PUBLIC_KEY`, `VAPID_PRIVATE_KEY`.

## Backend

- Routes `src/routes/push.rs`:
  - `POST /push/subscribe`: enregistre une souscription.
  - `POST /push/unsubscribe`.
  - `GET /push/public-key`: expose la cle publique VAPID.
- [src/notifications.rs](../src/notifications.rs): a cote des emails existants, envoyer un push quand le canal est dispo. Reutiliser l'idempotence par match (`match_reminders`).
- Nettoyer les souscriptions invalides (410 Gone) renvoyees par le service de push.

## UI

- Bouton "Activer les notifications" sur [templates/today.html](../templates/today.html) ou une page parametres, qui declenche la demande de permission et l'abonnement.
- Section preferences: cases rappels de match, alertes de rang.

## i18n

- Titres et corps des push localises selon `users.lang` (deja persiste).

## Cas limites

- iOS Safari: push web supporte uniquement en PWA installee (iOS 16.4+). Documenter la limite; le client Tauri ([12-mobile-tauri](12-mobile-tauri.md)) la contourne via le push natif.
- Permission refusee: retomber sur l'email, ne pas reproposer en boucle.
- Souscription expiree: purge a la premiere erreur d'envoi.

## Criteres d'acceptation

- L'app s'installe et s'ouvre en standalone.
- Un push de rappel arrive avant le coup d'envoi pour les abonnes non encore pariants.
- Les preferences coupent effectivement les push correspondants.
