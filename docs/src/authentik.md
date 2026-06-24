# Authentik (OIDC SSO)

calrs speaks standard OpenID Connect (Authorization Code flow with PKCE), so it
integrates with [Authentik](https://goauthentik.io/) out of the box. This page
walks through the full setup and — importantly — the one Authentik-specific
gotcha that will otherwise block every login with a generic
*"Authentication failed"* error: the **`email_verified` claim**.

A common deployment pairs OIDC login with calrs's
[global EWS impersonation](#tying-it-together-ews-impersonation): users sign in
through Authentik, and calrs uses their email to impersonate the matching
Exchange mailbox. In that setup Authentik is the single source of identity and
the email claim is the bridge to each user's calendar — so getting these claims
right matters.

> Tested against Authentik **2025.12**. The `email_verified` behaviour described
> below changed in **2025.10**; on older Authentik versions you can skip that
> step.

---

## 1. Create the OAuth2/OpenID provider

In the Authentik admin interface: **Applications → Providers → Create →
OAuth2/OpenID Provider**.

- **Authorization flow**: `default-provider-authorization-explicit-consent`
  (or the implicit-consent flow if you don't want a consent screen).
- **Client type**: **Confidential**.
- **Client ID** / **Client Secret**: note both — they go into calrs.
- **Redirect URIs** (Strict): the callback is **`/auth/oidc/callback`** on your
  public calrs URL:

  ```
  https://rdv.example.com/auth/oidc/callback
  ```

  This must match `CALRS_BASE_URL` exactly. A mismatch (wrong host, missing
  `/auth/` segment, http vs https) produces Authentik's *"Redirect URI Error"*.
- **Signing Key**: pick a certificate. calrs validates the ID token signature
  via the provider's JWKS, so the token **must** be signed.
- **Scopes**: leave the default mappings for `openid`, `email`, and `profile`
  selected — these are exactly the three scopes calrs requests. (The `email`
  mapping is the one we adjust in step 4.)

---

## 2. Create the application

**Applications → Create**.

- Give it a **Name** and a **Slug** (e.g. `calrs`). The slug becomes part of the
  issuer URL, so note it.
- Bind it to the provider from step 1.
- Add the policy/group bindings that decide who may sign in.

Your **issuer URL** is then:

```
https://<authentik-host>/application/o/<app-slug>/
```

for example `https://portal.example.com/application/o/calrs/`. calrs
auto-discovers everything else from `<issuer>/.well-known/openid-configuration`,
so you only ever configure the issuer URL — not the individual endpoints.

---

## 3. Configure calrs

Set the public base URL **before** starting the server, then enable OIDC:

```bash
# CALRS_BASE_URL must be the public URL — it prefixes the redirect URI and
# email links. The default (http://localhost:3000) will not match Authentik.
export CALRS_BASE_URL=https://rdv.example.com

calrs config oidc \
  --issuer-url https://portal.example.com/application/o/calrs/ \
  --client-id <CLIENT_ID> \
  --client-secret <CLIENT_SECRET> \
  --enabled true \
  --auto-register true
```

You can also do this from **Admin → OIDC**. Once enabled, the login page shows a
**"Sign in with SSO"** button.

---

## 4. Fix `email_verified` (Authentik 2025.10+)

This is the step that trips most people up.

calrs only links or auto-creates an account when the IdP asserts that the user
owns the email address — i.e. the ID token contains `email_verified: true`. This
is a deliberate security gate: without it, anyone able to register at the IdP
with an arbitrary email could squat on or hijack a calrs account keyed on that
address. There is **no toggle to disable it**.

The catch: **since Authentik 2025.10, the `email` scope returns
`email_verified: false` by default.** Authentik has no universal way to know
whether an address is verified, so it stopped asserting `true` unconditionally.
Before 2025.10 the claim was always `true`.

With the default mapping you'll see calrs reject the login:

```
WARN  calrs::auth: OIDC auto-register refused: IdP did not assert email_verified=true
WARN  calrs::auth: OIDC callback failed: account error
       error=The identity provider has not verified your email address.
```

and the browser lands on a generic *"Authentication failed"* page.

### Option A — assert `email_verified` for a trusted directory (recommended for EWS setups)

If your Authentik users are synced from a trusted source (Active Directory,
LDAP, your Exchange directory) the addresses are real by construction, so it's
safe to assert them as verified:

1. **Customization → Property Mappings → Create → Scope Mapping**
   - **Name**: `calrs email verified`
   - **Scope name**: `email` *(must be exactly `email` to replace the standard
     email scope)*
   - **Expression**:

     ```python
     return {
         "email": request.user.email,
         "email_verified": True,
     }
     ```
2. Edit your OAuth2 provider → **Advanced protocol settings → Scopes**:
   - **remove** `authentik default OAuth Mapping: OpenID 'email'`
   - **add** your `calrs email verified` mapping
   - leave `openid` and `profile` as they are
3. Save and retry the SSO login.

> Only do this when the directory is trusted. It marks **every** address as
> verified unconditionally, which is fine for a directory you control but not
> for an instance that allows open self-registration at the IdP.

### Option B — reflect a real verification status

If accounts can self-register in Authentik, base the claim on a user attribute
instead of hard-coding `true`:

```python
return {
    "email": request.user.email,
    "email_verified": bool(request.user.attributes.get("email_verified", False)),
}
```

Then set the `email_verified: true` attribute on users (or via a group) once
their address is actually verified.

---

## Tying it together: EWS impersonation

calrs links OIDC identities to local users **by email** (it first matches on the
stable OIDC `subject`, then falls back to email). Combined with **global EWS
impersonation**, this gives a zero-touch onboarding flow:

1. A user signs in through Authentik for the first time.
2. calrs auto-registers them with the email from the (now verified) `email`
   claim.
3. If global EWS impersonation has **auto-provision** enabled
   (**Admin → EWS**), calrs immediately provisions a managed Exchange source
   for that user, impersonating their email — their calendar starts syncing with
   no manual source setup.

### Domain mismatch

Impersonation targets the user's email as their Exchange SMTP address. If the
address Authentik sends differs from the mailbox domain (e.g. Authentik issues
`alice@example.com` but the mailbox is `alice@example.local`), set the
**Impersonation domain** override in **Admin → EWS**. calrs keeps the local part
and swaps the domain, so impersonation resolves to the real mailbox. Validate
this on one or two accounts before rolling out broadly.

---

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| Authentik shows **"Redirect URI Error"** | The redirect URI in the provider doesn't match `{CALRS_BASE_URL}/auth/oidc/callback` | Set the provider redirect URI to exactly `https://<host>/auth/oidc/callback` and make sure `CALRS_BASE_URL` matches (host, scheme, and the `/auth/` segment) |
| **"Authentication failed"** after returning from Authentik; logs show `email_verified=true` not asserted | Authentik 2025.10+ returns `email_verified: false` by default | Apply [step 4](#4-fix-email_verified-authentik-2025-10) |
| **"An account with this email already exists … not verified"** | A local account with that email exists, but `email_verified` is false so calrs won't auto-link it | Same as above — assert `email_verified: true` |
| Login works but no calendar appears | Global EWS auto-provision is off, or the impersonation target domain is wrong | Enable auto-provision and/or set the impersonation domain override in **Admin → EWS** |
| ID token signature / discovery errors | No signing key on the provider | Assign a certificate as the provider's **Signing Key** |

See also: [Authentication](./authentication.md) for the generic OIDC reference
and local-account options.
