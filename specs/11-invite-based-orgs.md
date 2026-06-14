# 11. Espaces sur invitation (mode amis)

Statut: a faire. Priorite: haute. Effort: M.

## Objectif

Ajouter un second mode de creation et de jonction d'espace, oriente grand public ("amis"), ou l'appartenance ne depend plus du domaine email mais d'invitations explicites. Le createur devient administrateur, peut inviter qui il veut, et chaque membre invite peut a son tour inviter d'autres personnes. Un invite rejoint l'espace en cliquant un lien recu par email.

Le mode actuel base sur le domaine email ("entreprise") reste disponible et inchange.

## Contexte existant

- Un espace est un `tenant` ([src/tenant.rs](../src/tenant.rs)). L'appartenance est implicite: il existe une ligne `users (tenant_id, email)`.
- A la connexion ([src/routes/auth.rs](../src/routes/auth.rs), `tenant_request_magic_link`), l'acces est filtre par `allowed_email_pattern` sur le domaine de l'email.
- La creation self-serve passe par `pending_tenants` puis un lien de verification ([src/routes/signup.rs](../src/routes/signup.rs)), qui derive `allowed_email_pattern` du domaine du proprietaire.

Le mode amis remplace le gating par domaine par un gating par invitation, sans toucher au reste de la mecanique (paris, scoring, classement, cagnotte).

## Concepts

- **membership_mode** sur le tenant: `domain` (actuel, entreprise) ou `invite` (amis).
- **Invitation**: un enregistrement (tenant, email invite, inviteur, token, statut, expiration).
- **Membre**: une ligne `users` dans le tenant. Inviter ne cree pas le membre; accepter le cree.

## Regle de gating unifiee

Une tentative de connexion pour l'email E sur le tenant T est autorisee si l'une des conditions est vraie:

1. Il existe deja `users (tenant_id = T, email = E)` (membre etabli), OU
2. `T.membership_mode = 'domain'` ET `T.allowed_email_pattern` matche le domaine de E (auto-join entreprise), OU
3. Il existe une invitation **pending** non expiree pour (T, E).

Cette regle unifie les deux modes:
- En mode `domain`, la condition 2 conserve le comportement actuel; les invitations restent possibles en complement (inviter un externe au domaine).
- En mode `invite`, `allowed_email_pattern` ne matche rien (motif "match nothing"), donc seules les conditions 1 et 3 ouvrent l'acces.

Implementation dans `tenant_request_magic_link` ([src/routes/auth.rs](../src/routes/auth.rs)): remplacer le seul test de pattern par cette fonction `is_login_allowed(pool, tenant, email)`.

## Modele de donnees

```sql
-- migrations/2026xxxx_invite_mode.sql

-- Mode d'appartenance de l'espace.
ALTER TABLE tenants ADD COLUMN membership_mode TEXT NOT NULL DEFAULT 'domain'
    CHECK (membership_mode IN ('domain', 'invite'));

-- Autoriser ou non les membres non-admin a inviter (mode amis: TRUE par defaut).
ALTER TABLE tenants ADD COLUMN members_can_invite BOOLEAN NOT NULL DEFAULT TRUE;

CREATE TABLE invitations (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID NOT NULL REFERENCES tenants(id),
    email           TEXT NOT NULL,          -- invite, en minuscules
    inviter_user_id UUID REFERENCES users(id),  -- NULL si genere a la creation de l'espace
    token_hash      TEXT NOT NULL UNIQUE,   -- on ne stocke que le hash
    status          TEXT NOT NULL DEFAULT 'pending'
                    CHECK (status IN ('pending', 'accepted', 'revoked', 'expired')),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at      TIMESTAMPTZ NOT NULL,
    accepted_at     TIMESTAMPTZ,
    accepted_user_id UUID REFERENCES users(id)
);

-- Une seule invitation vivante par (tenant, email).
CREATE UNIQUE INDEX invitations_pending_uidx
    ON invitations (tenant_id, email)
    WHERE status = 'pending';

CREATE INDEX invitations_tenant_idx ON invitations (tenant_id);
CREATE INDEX invitations_expires_idx ON invitations (expires_at)
    WHERE status = 'pending';
```

Convention "match nothing": pour un tenant en mode `invite`, stocker `allowed_email_pattern = '(?!)'` (regex qui ne matche jamais). La fonction `Tenant::try_from` compile deja le pattern, `(?!)` est valide.

## Flux 1: creer un espace en mode amis

Etendre le signup existant ([src/routes/signup.rs](../src/routes/signup.rs)).

1. Le formulaire `/signup` ([templates/signup.html](../templates/signup.html)) gagne un choix de type d'espace:
   - "Entreprise (domaine email)": comportement actuel, demande le domaine via l'email du proprietaire.
   - "Amis (sur invitation)": pas de domaine; n'importe quel email de proprietaire est accepte.
2. `SignupForm` recoit un champ `space_kind` (`domain` ou `invite`).
3. En mode `invite`:
   - `allowed_email_pattern` stocke vaut `(?!)`.
   - `pending_tenants` doit memoriser le mode choisi: ajouter une colonne.

```sql
ALTER TABLE pending_tenants ADD COLUMN membership_mode TEXT NOT NULL DEFAULT 'domain'
    CHECK (membership_mode IN ('domain', 'invite'));
```

4. A la verification (`verify`), l'INSERT du tenant renseigne `membership_mode` et `members_can_invite = TRUE` pour le mode amis. Le proprietaire est cree comme membre admin (deja le cas).

Aucune invitation n'est requise pour le proprietaire: il est admin fondateur.

## Flux 2: inviter des personnes

Nouveau module routes `src/routes/invitations.rs`.

- `GET /members`: liste les membres de l'espace et les invitations en cours; formulaire d'invitation. Accessible a tout membre si `members_can_invite`, sinon admin seulement.
- `POST /invitations`: cree une ou plusieurs invitations (champ email, eventuellement liste). Pour chaque email:
  - normaliser en minuscules,
  - si deja membre: ne pas creer d'invitation, signaler "deja membre",
  - si invitation pending existante: renvoyer le lien existant (ne pas dupliquer),
  - sinon creer l'invitation (token aleatoire, hash stocke, `expires_at = NOW() + 7 jours`), envoyer l'email.
- `POST /invitations/:id/revoke`: passe `status = 'revoked'` (inviteur ou admin).
- `POST /invitations/:id/resend`: regenere le token si expire ou renvoie l'email.

Permissions:
- Tout membre peut inviter si `tenants.members_can_invite = TRUE`.
- Un admin peut toujours inviter et peut basculer `members_can_invite` dans [templates/admin_settings.html](../templates/admin_settings.html).
- Revocation: l'inviteur de l'invitation ou un admin.

Anti-abus:
- Rate limit par utilisateur (par exemple 20 invitations / jour) en reutilisant le mecanisme de [src/rate_limit.rs](../src/rate_limit.rs).
- Plafond d'invitations pending par tenant (configurable, defaut large).
- Honeypot non necessaire ici (action authentifiee), mais journaliser inviteur + email.

Email d'invitation: nouveau template [templates/emails/invitation.html](../templates/emails/invitation.html), bilingue selon la locale de l'inviteur ou la locale par defaut du tenant. Contenu: qui invite, nom de l'espace, bouton "Rejoindre", mention de l'expiration. Le lien pointe vers l'apex ou le sous-domaine du tenant: `{tenant_public_url}/invite/accept?token=...`.

## Flux 3: accepter une invitation

Route `GET /invite/accept?token=...` (dans `src/routes/invitations.rs`), servie sur le tenant cible.

1. Hasher le token, charger l'invitation par `token_hash` et `tenant_id` courant.
2. Rejets: token inconnu (lien invalide), `status != 'pending'` (deja utilisee ou revoquee), expiree (`expires_at < NOW()`, marquer `expired`).
3. Si valide, en une transaction:
   - upsert du membre: `INSERT INTO users (tenant_id, email, display_name) ... ON CONFLICT (tenant_id, email) DO NOTHING`, le `display_name` derive de l'email comme dans `callback`.
   - marquer l'invitation `accepted`, renseigner `accepted_at` et `accepted_user_id`.
   - creer une session et poser le cookie `lb_session` (meme logique que `auth::callback`, y compris `cookie_domain` en multi-tenant).
4. Rediriger vers `/today`.

Le token d'invitation fait donc office de premiere authentification: l'invite n'a pas besoin d'un magic link separe pour son premier acces. Les connexions suivantes passent par le magic link normal, desormais autorise par la regle de gating (condition 1, membre etabli).

Transitivite: une fois membre, l'utilisateur voit `/members` et peut inviter a son tour si `members_can_invite`. C'est ce qui realise "un invite peut inviter d'autres personnes".

## Job de maintenance

Etendre le job de nettoyage horaire existant ([src/main.rs](../src/main.rs)): passer les invitations `pending` expirees a `expired`. Idempotent.

## UI

- [templates/signup.html](../templates/signup.html): selecteur de type d'espace, aide contextuelle. Masquer le champ domaine en mode amis.
- [templates/members.html](../templates/members.html) (nouveau): membres, invitations en cours (statut, expiration), formulaire d'invitation, bouton copier le lien.
- [templates/emails/invitation.html](../templates/emails/invitation.html) (nouveau).
- [templates/_nav.html](../templates/_nav.html): entree "Membres" visible selon les droits.
- [templates/admin_settings.html](../templates/admin_settings.html): interrupteur "Autoriser les membres a inviter", et selecteur de mode d'appartenance (voir Settings admin ci-dessous).

## Settings admin: bascule de mode

Dans [src/routes/tenant_settings.rs](../src/routes/tenant_settings.rs), exposer:

- Un selecteur `membership_mode`: "Entreprise (domaine email)" / "Amis (sur invitation)".
- Quand `domain` est choisi: champ `allowed_email_pattern` editable (domaine autorise).
- Quand `invite` est choisi: champ domaine masque; a l'enregistrement, le serveur force `allowed_email_pattern = '(?!)'`.
- Avertissement affiche avant validation: "En mode Amis, seules les personnes invitees pourront rejoindre. Les membres actuels restent membres."
- Apres ecriture, invalider le cache du tenant via `TenantRegistry::invalidate` (deja le pattern utilise apres les autres edits de settings) pour que la regle de gating prenne effet immediatement.

## i18n

- "Inviter" / "Invite", "Membres" / "Members", "Invitation en cours" / "Pending invitation", "Rejoindre l'espace" / "Join the space", "Cette invitation a expire" / "This invitation has expired", "Type d'espace" / "Space type", "Entreprise" / "Company", "Amis" / "Friends".

## Securite

- Tokens: 32 octets aleatoires, seul le hash SHA-256 est stocke, comme magic links et signup.
- Les invitations sont strictement tenant-scoped; un token d'un tenant ne peut pas faire entrer dans un autre (jointure sur `tenant_id`).
- Le RLS existant doit couvrir `invitations` (ajouter la policy par tenant, cf. [migrations/20260525000007_rls.sql](../migrations/20260525000007_rls.sql)).
- Pas de divulgation: la page d'acceptation ne revele pas si un email est deja membre.

## Cas limites

- Email deja membre invite a nouveau: pas de nouvelle invitation, on peut lui renvoyer un lien de connexion classique.
- Invitation acceptee depuis un autre appareil / email casse: le cookie est pose pour la session du navigateur qui ouvre le lien, coherent avec le magic link.
- Espace en mode amis sans aucune invitation: seul le proprietaire peut entrer, normal.
- Bascule de mode `domain` <-> `invite` par un admin: **autorisee** (decision actee). Les membres existants restent toujours membres quel que soit le sens. En passant a `invite`, l'auto-join par domaine s'arrete, on remplace `allowed_email_pattern` par `(?!)`; les nouveaux entrants passent alors par invitation. En repassant a `domain`, l'admin re-saisit un motif de domaine et l'auto-join reprend. L'interrupteur vit dans [templates/admin_settings.html](../templates/admin_settings.html) avec un texte d'avertissement clair sur l'effet (cf. section Settings admin ci-dessous).
- Invitation vers un domaine qui matcherait deja un tenant entreprise: sans effet, chaque tenant a ses propres lignes `users`.

## Criteres d'acceptation

- Creer un espace en mode amis ne demande aucun domaine et rend le proprietaire admin.
- En mode amis, un email non invite ne peut pas obtenir de magic link valide; un email invite le peut, ou entre directement via le lien d'acceptation.
- Tout membre (si autorise) peut inviter, et un invite devenu membre peut inviter a son tour.
- Une invitation est unique tant qu'elle est pending, expire au bout de 7 jours, et peut etre revoquee.
- Le mode entreprise existant fonctionne exactement comme avant.
