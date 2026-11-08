# EWS Impersonation (Exchange)

calrs can connect to an on-premises Microsoft Exchange server (tested with
Exchange 2019) over **EWS** (Exchange Web Services). Instead of asking every
user for their mailbox credentials, you configure **one service account** that
holds the `ApplicationImpersonation` role, and calrs impersonates each user by
their email address to read their calendar.

This is **global EWS impersonation**: one Exchange server + one service account,
with a "managed" calendar source auto-provisioned for every user. It pairs
naturally with [OIDC SSO](./authentication.md): the user signs in, and calrs
uses the email the IdP verified as the impersonation target, so a new user's
calendar starts syncing with zero manual setup.

A managed source keeps **busy times only**. For an event synced from a managed
mailbox calrs stores its times, time zone, all-day flag, recurrence, status and
transparency, and nothing about what the meeting is: no title, description,
location or iCal text (a recurring event keeps only the start, exception
dates and moved-occurrence ids its expansion needs). Calendar folder names are
not stored either: a managed source's calendars are called "Exchange calendar
1", "Exchange calendar 2" and so on. The user's own troubleshoot view shows
those events as "Unavailable (your calendar)", and an admin cannot read
anyone's mailbox content through calrs, impersonating the user included.

> When global impersonation is disabled (the default), calrs behaves exactly as
> before: users add their own per-user EWS or CalDAV sources.

> **On-premises Exchange only.** Exchange Online (Microsoft 365) is not
> covered: its calendars go through Microsoft Graph (#229).

---

## 1. Prepare the Exchange service account

Create a dedicated service account (e.g. `svc-calrs@example.com`) and grant it
the **ApplicationImpersonation** RBAC role from the Exchange Management Shell.
Choose **one** of the two options below; do not run both. Running option A and
then option B leaves the unscoped assignment in place, so the account can
still impersonate every mailbox in the organization.

**Option A (recommended): only the mailboxes calrs should read.** Create a
management scope for those mailboxes and assign the role within it:

```powershell
New-ManagementScope -Name "calrs-mailboxes" `
  -RecipientRestrictionFilter "MemberOfGroup -eq 'CN=calrs-users,OU=Groups,DC=example,DC=com'"

New-ManagementRoleAssignment -Name "calrs-impersonation" `
  -Role ApplicationImpersonation `
  -User svc-calrs@example.com `
  -CustomRecipientWriteScope "calrs-mailboxes"
```

**Option B: every mailbox in the organization.** Only if calrs should be able
to reach all mailboxes:

```powershell
New-ManagementRoleAssignment -Name "calrs-impersonation" `
  -Role ApplicationImpersonation `
  -User svc-calrs@example.com
```

If you already ran option B and want option A instead, remove the unscoped
assignment first (`Get-ManagementRoleAssignment -Role ApplicationImpersonation
-RoleAssignee svc-calrs@example.com`, then `Remove-ManagementRoleAssignment`
on it).

Note your EWS endpoint URL, usually:

```
https://mail.example.com/EWS/Exchange.asmx
```

---

## 2. Configure calrs (Admin → Sign-in and security)

Go to the admin dashboard, **Sign-in and security** tab, Exchange card (organization-wide Exchange through impersonation). Fields:

| Field | Notes |
|---|---|
| **Enabled** | Master switch. When off, the feature is fully inert and per-user sources are used. |
| **EWS URL** | The endpoint, e.g. `https://mail.example.com/EWS/Exchange.asmx`. Must be `https://` (the service account password goes out with every request; an `http://` endpoint left in an older database leaves the feature off, except on the loopback interface for local testing). Validated on save: an invalid URL is rejected rather than silently stored. Enabling the feature without a URL, a service account and a password is refused too, with a message on the card. |
| **Service account username** | The impersonation account, e.g. `svc-calrs@example.com`. |
| **Service account password** | Encrypted at rest (AES-256-GCM, same scheme as CalDAV/OIDC secrets). **Keep-current**: leave blank on save to preserve the stored value, as long as the endpoint and the account stay the same. Saving a new endpoint or a new service account without typing the password clears the stored one (the card says so), so it is never sent to a server it was not typed for; the feature then idles until it is entered again. |
| **Lock user sources** | When on, non-admin users can no longer add their own calendar sources (CalDAV, EWS or Google): their Exchange calendar is managed centrally. Admins are exempt, and sources users added before stay until deleted. The Sources page shows an explanatory notice and hides the "Add" button. |
| **Auto-provision** | When on, a managed Exchange source is created for every enabled user whose address is trusted (see [Who gets a mailbox](#who-gets-a-mailbox)): existing ones on save, then each new one as it appears. |
| **Users' domain** and **Mailbox domain** | Optional override, both or neither. See [Domain mismatch](#domain-mismatch) below. A leading `@` is stripped automatically. |

There is also a **"Provision now"** button: it runs a one-shot batch that
provisions a managed source for every enabled, trusted user immediately,
regardless of the auto-provision toggle. Saving with auto-provision enabled
does the same thing.

The card also lists **addresses to confirm** (enabled accounts in the allowed
email domains whose address is not trusted yet), each with a **Confirm**
button, and offers **Remove managed sources and clear the service password**
(see [Switching it off](#switching-it-off)).

---

## Who gets a mailbox

A managed source opens the mailbox its user's address names, so calrs only
provisions an address it can trust (`users.email_trusted`):

- an account an admin created with `calrs user create`;
- an SSO (OIDC) account whose email the identity provider marks verified
  (`email_verified`). This is checked again at **every login**: when the IdP
  sends `email_verified: false`, or reports a new address it does not verify,
  the account is no longer trusted and its managed source is deleted on the
  spot, with its calendars and cached busy times. Many identity providers in
  front of Exchange 2019 (ADFS, Entra ID by default) never send the claim at
  all: then an address an admin confirmed stays trusted as long as it is
  unchanged (letter case aside), and a changed one is not trusted until it is
  confirmed again;
- an account an admin confirmed by hand, with **Confirm** on the card or
  `calrs user trust-email <email>` (which exits non-zero for an unknown
  address).

A locally **self-registered** account is never trusted on its own: local
sign-up does not verify that the person owns the address they type. It gets
no managed source from auto-provision, "Provision now" or a login, until an
admin confirms it. Registration also refuses an address that differs from an
existing one only in letter case (`CEO@example.com` when `ceo@example.com`
exists), since Exchange treats them as one mailbox.

Accounts that existed before this check was added start untrusted, and any
managed source they had was removed by the upgrade. They stay untrusted until
an admin confirms them (the card lists them), or, for SSO accounts, until a
login where the identity provider marks the address verified.

One mailbox serves at most one user. Addresses are compared without regard to
(ASCII) letter case and surrounding spaces, and the impersonated mailbox is
stored lowercased. When a second user's address maps to a mailbox another
user's managed source already reads, the second user gets no source and the
status table shows the conflict, with one exception: an owner whose address
the identity provider verified takes the mailbox over from an account that is
only admin-confirmed (that account's managed source is removed and the event
logged). An address that does not have exactly one `@`, or that holds a
non-ASCII character (Exchange 2019 has no SMTPUTF8), names no mailbox and is
never provisioned. The list of addresses to confirm shows the mailbox each
confirmation would open, and a confirmation covers exactly that address and
mailbox: if either changed since the page was shown (an SSO login brought a
new address, the override changed), nothing is confirmed and the card asks to
review the list again. `calrs user trust-email` confirms the stored address it
prints. When a new domain override changes the mailbox an
admin-confirmed address maps to, that address must be confirmed again (an
address the identity provider verified keeps its trust); the card says how
many.

"One mailbox, one user" holds for the exact address calrs impersonates.
Exchange also resolves proxy addresses (aliases), so two calrs accounts whose
addresses are two aliases of one mailbox would both read it: give users their
mailbox's primary SMTP address in calrs (and at the identity provider).

A suspended account keeps its managed source but is neither synced nor
written to until it is enabled again.

---

## 3. How provisioning works

For each user, calrs inserts a single **managed** row into `caldav_sources`:

- `provider_type = 'ews'`, `managed = 1`, `auth_type = 'basic'`
- `password_enc = NULL`: the managed source carries no secret of its own; the
  sync path reads the live service-account credentials from the in-memory config

Provisioning is **idempotent and race-safe**: partial unique indexes allow only
one managed EWS source per account and one per mailbox, so an OIDC login racing
the admin's "Provision now" can never create duplicates.

- `impersonate_email = <mailbox>`: the user's address, lowercased, after the
  domain override. The SOAP layer injects a `t:ExchangeImpersonation` header
  with this address on every request, and the user's current address is
  checked against it before every sync or write.

Provisioning runs at four moments (all gated on auto-provision, except the
explicit button and the confirmation), and only ever for trusted users:

1. **Admin save / Provision now**: batch over all enabled, trusted users.
2. **`calrs user create`**: an account an admin creates.
3. **OIDC login**: new or returning user whose email the IdP marks verified,
   right after the account is linked or created. When that address changed at
   the IdP, the existing managed source follows it.
4. **Confirming an address** on the card or with `calrs user trust-email`.

Managed sources cannot be edited (their endpoint and credentials come from this
card). Only an admin can remove one, with **Remove** in the per-user status
table; auto-provision recreates it at the next save, confirmation or SSO
login, so suspend the account to keep someone out.

Users keep the actions that are theirs on a managed source: sync it, test it,
and choose which of its calendars their confirmed bookings are written to.
Until they choose, the first sync points bookings at the mailbox's **default
calendar**, found through Exchange's well-known calendar folder, since its name
depends on the mailbox's language ("Calendar", "Calendrier", ...). If the server
cannot name it, a mailbox with a single calendar uses that one; with several,
the user picks. Choosing "do not write" afterwards is respected: a later sync
never undoes it. Only one of the source's own calendars can be chosen.

**When the mailbox a source reads changes** (an address change at the IdP, a
mailbox-domain override that maps the address elsewhere, or an endpoint on
another server), that source is reset: Exchange folder ids belong to one
mailbox, so the calendars and cached events of the old one are dropped and its
write calendar is cleared. Writing the same endpoint differently (another path
case on the same server) resets nothing. A user's own EWS source pointed at
another account or server is reset the same way when it is saved. Event types that had selected one of
those calendars lose that selection: each is named in the server log, and the
card says how many. The next sync discovers the new mailbox, and each host
picks a write calendar again (the sources page asks for one).

Calendar folders are listed page by page, so a mailbox with many folders is
read whole. A calendar folder the server stops listing is only dropped once it
is missing from **two listings in a row**, and never on a listing that is empty or that
is not a proper EWS answer (an error page, a proxy's sign-in page), which
fails the sync instead. Dropping it removes its cached events and the event
types' selection of it. This applies to users' own EWS sources too.

When an owner the identity provider verified takes a mailbox over from an
account that was only admin-confirmed, the status table shows "Taken over
from" that account, with the time, and the server log has a warning.

The status table lists 50 sources a page, failing ones first, and the
addresses waiting for confirmation 20 a page. Once either list runs over one
page, a search box finds a user or a mailbox.

**Sync cadence on large organizations.** The background loop syncs one
calendar source a minute, the stalest one, and every provisioned user has a
source of their own. With N sources, each one is synced in the background
about every N minutes, and its daily full resync only runs when its turn
comes: past about 1,440 sources (one a minute for a day), full resyncs come
less often than daily. Booking pages still refresh a host's calendars on
demand once they are 5 minutes old, so hosts whose pages are visited stay
current. This is tracked in
[#388](https://github.com/olivierlambert/calrs/issues/388).

## Switching it off

While the feature is off (or its configuration incomplete), managed sources are
kept but skipped: they are not synced, no booking is written to them, and
neither counts as a failure, so there are no retries and no alert emails. They
keep their last synced busy times until the feature is switched back on or
they are removed.

To take the feature out of service, use **Remove managed sources and clear the
service password** on the card (it asks for confirmation). It deletes every
managed source with its calendars and cached busy times, clears the stored
service password and switches global Exchange off. **Bookings already written
into the mailboxes stay there**: calrs no longer has a way to reach them, so
remove them in Exchange if needed.

---

## Domain mismatch

Impersonation targets the user's email, as an SMTP address of their Exchange
mailbox: the primary address or any alias both work, so users with several
addresses or domains need nothing special as long as their calrs email is one
of them. If the address calrs knows (e.g. the AD UPN or the OIDC `email` claim)
is on a domain that is not one of the mailbox's addresses, impersonation will
fail to resolve, and that user alone is reported in the status table.

If such users share one login domain, set the override: **Users' domain** is
the domain calrs knows them by, **Mailbox domain** the domain of their
mailboxes. calrs then rewrites only addresses in exactly the users' domain,
keeping the local part; an address in any other domain is used as it is. Fill
in both or neither. For example, with `example.local` and `example.com`:

```
alice@example.local        ->   alice@example.com
bob@example.com            ->   bob@example.com      (not the users' domain)
jsmith@subsidiary.com      ->   jsmith@subsidiary.com
```

So `jsmith@subsidiary.com` can never be mapped onto `jsmith@example.com`. If
an address the override rewrites lands on a mailbox another user's source
already reads, that user is refused and shown as a conflict.

Validate this on one or two accounts before rolling out broadly.

---

## Security & networking

- **The service account is powerful**: it can read every mailbox in its RBAC
  scope. Restrict the scope with a management scope (step 1) and store its
  password only in calrs, where it is encrypted at rest.
- **Regular users never see the service account**: the sources page hides its
  name on managed sources. Admins see it, and the status table shows only
  the impersonated mailbox and the sync outcome.
- **A managed source opens the mailbox its user's email names.** calrs only
  provisions a trusted address (see [Who gets a mailbox](#who-gets-a-mailbox)).
  Self-registered accounts are never provisioned, not by auto-provision and not
  by "Provision now", until an admin confirms them; the card warns while
  registration is open. Confirm an address only once you know it belongs to
  that person.
- **Busy times only**: nothing about the meetings in a managed mailbox is
  stored, so neither the user's pages nor an admin (impersonation included)
  can read mailbox content through calrs.
- **TLS certificate**: calrs trusts the bundled public CA roots only, not the
  operating system's store. An Exchange server whose certificate comes from an
  internal CA is refused (`UnknownIssuer`), whatever is installed on the calrs
  host. Use a certificate from a public CA for the EWS name, or publish EWS
  through a reverse proxy that presents one. Basic authentication must also be
  enabled on the EWS virtual directory:
  `Set-WebServicesVirtualDirectory -Identity "EWS (Default Web Site)" -BasicAuthentication $true`.
- **Private/internal EWS host**: if your Exchange server resolves to a private
  IP, calrs blocks the request by default (SSRF protection). Allow it explicitly
  via the `CALRS_ALLOW_PRIVATE_HOSTS` environment variable (comma-separated
  hostnames), e.g. `CALRS_ALLOW_PRIVATE_HOSTS=mail.example.com`.

---

See also: [Authentication](./authentication.md) for pairing this with OIDC SSO,
and the [Admin Dashboard](./admin.md) reference.
