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

> **On-premises Exchange only.** Exchange Online (Microsoft 365) has retired
> the `ApplicationImpersonation` role along with EWS, so this does not apply
> there: Microsoft 365 calendars go through Microsoft Graph (#229). The managed
> source model (one admin config, a per-user source provisioned from it) is not
> tied to EWS, so an app-only Graph variant could reuse it later.

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
| **EWS URL** | The endpoint, e.g. `https://mail.example.com/EWS/Exchange.asmx`. Validated on save: an invalid URL is rejected rather than silently stored. Enabling the feature without a URL, a service account and a password is refused too, with a message on the card. |
| **Service account username** | The impersonation account, e.g. `svc-calrs@example.com`. |
| **Service account password** | Encrypted at rest (AES-256-GCM, same scheme as CalDAV/OIDC secrets). **Keep-current**: leave blank on save to preserve the stored value. |
| **Lock user sources** | When on, non-admin users can no longer add their own calendar sources (CalDAV, EWS or Google): their Exchange calendar is managed centrally. Admins are exempt, and sources users added before stay until deleted. The Sources page shows an explanatory notice and hides the "Add" button. |
| **Auto-provision** | When on, a managed Exchange source is created for existing users (on save), for users an admin creates with `calrs user create`, and for users signing in through OIDC with a verified email. Self-registered accounts are not provisioned automatically: see [Security](#security--networking). |
| **Impersonation domain** | Optional override for the mailbox domain. See [Domain mismatch](#domain-mismatch) below. A leading `@` is stripped automatically. Requires **allowed email domains** to be set (Admin, Authentication): see the security notes. |

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

1. **Admin save / Provision now**: batch over all enabled users.
2. **`calrs user create`**: an account an admin creates.
3. **OIDC login**: new or returning user whose email the IdP marks verified,
   right after the account is linked or created. When that address changed at
   the IdP, the existing managed source follows it.

Managed sources cannot be edited (their endpoint and credentials come from this
card). Only an admin can remove one, and auto-provision recreates it at the
user's next OIDC login.

Users keep the actions that are theirs on a managed source: sync it, test it,
and choose which of its calendars their confirmed bookings are written to.
Until they choose, the first sync points bookings at the mailbox's **default
calendar**, found through Exchange's well-known calendar folder, since its name
depends on the mailbox's language ("Calendar", "Calendrier", ...). If the server
cannot name it, a mailbox with a single calendar uses that one; with several,
the user picks. Choosing "do not write" afterwards is respected: a later sync
never undoes it.

**When the mailbox a source reads changes** (an address change at the IdP, a
new mailbox-domain override, or a new endpoint), that source is reset: Exchange
folder ids belong to one mailbox, so the calendars and cached events of the old
one are dropped and its write calendar is cleared. The next sync discovers the
new mailbox, and each host picks a write calendar again (the sources page asks
for one).

When the feature is switched off, managed sources stop syncing and keep their
last synced events until they are removed or the feature is switched back on.

---

## Domain mismatch

Impersonation targets the user's email, as an SMTP address of their Exchange
mailbox: the primary address or any alias both work, so users with several
addresses or domains need nothing special as long as their calrs email is one
of them. If the address calrs knows (e.g. the AD UPN or the OIDC `email` claim)
is on a domain that is not one of the mailbox's addresses, impersonation will
fail to resolve, and that user alone is reported in the status table.

If **all** such users share the same mapping, set the **Impersonation domain**
override: calrs keeps the local part of the address and replaces the domain.
Leave it empty when users sit on several domains. For example, with the override set to
`example.com`:

```
alice@example.local   ->   alice@example.com
alice                 ->   alice@example.com
```

Validate this on one or two accounts before rolling out broadly.

The override swaps the domain of whatever address it is applied to, so on an
instance that accepts addresses from anywhere, `ceo@gmail.com` would
impersonate `ceo@example.com`. For that reason it only applies to addresses in
the **allowed email domains** list, and the card refuses to save an override
while that list is empty. List the domain your users sign in with (here
`example.local`).

---

## Security & networking

- **The service account is powerful**: it can read every mailbox in its RBAC
  scope. Restrict the scope with a management scope (step 1) and store its
  password only in calrs, where it is encrypted at rest.
- **Regular users never see the service account**: the sources page hides its
  name on managed sources. Admins see it, and the status table shows only
  the impersonated mailbox and the sync outcome.
- **A managed source opens the mailbox its user's email names.** calrs only
  provisions an address it can trust: one an admin entered, or one the OIDC
  provider verified. Local self-registration does not verify that the person
  owns the address they type, so self-registered accounts are never
  provisioned automatically, and the card warns while registration is open.
  "Provision now" does cover every enabled account, so review the user list
  before using it on an instance with open registration.
- **TLS certificate**: calrs trusts the bundled public CA roots only, not the
  operating system's store. An Exchange server whose certificate comes from an
  internal CA is refused (`UnknownIssuer`), whatever is installed on the calrs
  host. Use a certificate from a public CA for the EWS name, or publish EWS
  through a reverse proxy that presents one. Basic authentication must also be
  enabled on the EWS virtual directory:
  `Set-WebServicesVirtualDirectory -Identity "EWS (Default Web Site)" -BasicAuthentication $true`.
- **Private/internal EWS host** — if your Exchange server resolves to a private
  IP, calrs blocks the request by default (SSRF protection). Allow it explicitly
  via the `CALRS_ALLOW_PRIVATE_HOSTS` environment variable (comma-separated
  hostnames), e.g. `CALRS_ALLOW_PRIVATE_HOSTS=mail.example.com`.

---

See also: [Authentication](./authentication.md) for pairing this with OIDC SSO,
and the [Admin Dashboard](./admin.md) reference.
