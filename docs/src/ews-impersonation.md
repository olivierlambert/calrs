# EWS Impersonation (Exchange)

calrs can connect to an on-premises Microsoft Exchange server (tested with
Exchange 2019) over **EWS** (Exchange Web Services). Instead of asking every
user for their mailbox credentials, you configure **one service account** that
holds the `ApplicationImpersonation` role, and calrs impersonates each user by
their email address to read their calendar.

This is **global EWS impersonation**: one Exchange server + one service account,
with a "managed" calendar source auto-provisioned for every user. It pairs
naturally with [OIDC SSO](./authentication.md) — the user signs in, and calrs
uses the email it gets from the IdP as the impersonation target, so a new user's
calendar starts syncing with zero manual setup.

> When global impersonation is disabled (the default), calrs behaves exactly as
> before: users add their own per-user EWS or CalDAV sources.

---

## 1. Prepare the Exchange service account

Create a dedicated service account (e.g. `svc-calrs@example.com`) and grant it
the **ApplicationImpersonation** RBAC role. From the Exchange Management Shell:

```powershell
New-ManagementRoleAssignment -Name "calrs-impersonation" `
  -Role ApplicationImpersonation `
  -User svc-calrs@example.com
```

To limit blast radius, scope the assignment to a specific set of mailboxes with
a management scope instead of granting it tenant-wide:

```powershell
New-ManagementScope -Name "calrs-mailboxes" `
  -RecipientRestrictionFilter "MemberOfGroup -eq 'CN=calrs-users,OU=Groups,DC=example,DC=com'"

New-ManagementRoleAssignment -Name "calrs-impersonation" `
  -Role ApplicationImpersonation `
  -User svc-calrs@example.com `
  -CustomRecipientWriteScope "calrs-mailboxes"
```

Note your EWS endpoint URL — usually:

```
https://mail.example.com/EWS/Exchange.asmx
```

---

## 2. Configure calrs (Admin → EWS)

Go to **Admin dashboard → EWS** (the global impersonation section). Fields:

| Field | Notes |
|---|---|
| **Enabled** | Master switch. When off, the feature is fully inert and per-user sources are used. |
| **EWS URL** | The endpoint, e.g. `https://mail.example.com/EWS/Exchange.asmx`. Validated on save — an invalid URL is rejected rather than silently stored. |
| **Service account username** | The impersonation account, e.g. `svc-calrs@example.com`. |
| **Service account password** | Encrypted at rest (AES-256-GCM, same scheme as CalDAV/OIDC secrets). **Keep-current**: leave blank on save to preserve the stored value. |
| **Lock user sources** | When on, non-admin users can no longer add their own calendar sources — their Exchange calendar is managed centrally. Admins are exempt. The Sources page shows an explanatory notice and hides the "Add" button. |
| **Auto-provision** | When on, a managed Exchange source is created for **every** user — both existing users (on save) and each new user as they are created or first sign in via OIDC. |
| **Impersonation domain** | Optional override for the mailbox domain. See [Domain mismatch](#domain-mismatch) below. A leading `@` is stripped automatically. |

There is also a **"Provision now"** button: it runs a one-shot batch that
provisions a managed source for every enabled user immediately, regardless of
the auto-provision toggle. Saving with auto-provision enabled does the same
thing.

---

## 3. How provisioning works

For each user, calrs inserts a single **managed** row into `caldav_sources`:

- `provider_type = 'ews'`, `managed = 1`, `auth_type = 'basic'`
- `impersonate_email = <user's email>` — the SOAP layer injects a
  `t:ExchangeImpersonation` header with this address on every request
- `password_enc = NULL` — the managed source carries no secret of its own; the
  sync path reads the live service-account credentials from the in-memory config

Provisioning is **idempotent and race-safe**: a partial unique index allows only
one managed EWS source per account, so an OIDC login racing the admin's
"Provision now" can never create duplicates.

Provisioning runs at three moments (all gated on auto-provision, except the
explicit button):

1. **Admin save / Provision now** — batch over all enabled users.
2. **New account creation** — local registration.
3. **OIDC login** — new or returning user, right after the account is
   linked/created.

---

## Domain mismatch

Impersonation targets the user's email as their Exchange **PrimarySmtpAddress**.
If the address calrs knows (e.g. the AD UPN or the OIDC `email` claim) is on a
different domain than the mailbox, impersonation will fail to resolve.

Set the **Impersonation domain** override: calrs keeps the local part of the
address and replaces the domain. For example, with the override set to
`example.com`:

```
alice@example.local   ->   alice@example.com
alice                 ->   alice@example.com
```

Validate this on one or two accounts before rolling out broadly.

---

## Security & networking

- **The service account is powerful** — it can read every mailbox in its RBAC
  scope. Restrict the scope with a management scope (step 1) and store its
  password only in calrs, where it is encrypted at rest.
- **Private/internal EWS host** — if your Exchange server resolves to a private
  IP, calrs blocks the request by default (SSRF protection). Allow it explicitly
  via the `CALRS_ALLOW_PRIVATE_HOSTS` environment variable (comma-separated
  hostnames), e.g. `CALRS_ALLOW_PRIVATE_HOSTS=mail.example.com`.

---

See also: [Authentication](./authentication.md) for pairing this with OIDC SSO,
and the [Admin Dashboard](./admin.md) reference.
