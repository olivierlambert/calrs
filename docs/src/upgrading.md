# Upgrading to v2

This book covers the v2 prerelease line. Use the [release list](https://github.com/olivierlambert/calrs/releases) to select an available build and read its release notes. A beta or release candidate is not a stable release.

## Choose a container channel

| Tag | Tracks |
|---|---|
| `latest` | Stable release tags only; a v2 prerelease does not move it |
| `next` | Prerelease tags, including betas and release candidates |
| Exact version, such as `2.0.0-beta.4` | That release; recommended when you need a repeatable deployment |
| `main` | Builds from the main branch, which can contain unreleased changes |

Use an exact tag from the release you selected to pin an installation. `next` is convenient for a test instance but can advance without a configuration change. When stable v2 is released, select its actual release tag rather than assuming that `latest` already contains it.

## Back up and test the upgrade

1. Record the current version, container tag, environment settings and any service overrides.
2. Stop calrs for a consistent filesystem backup, then copy the **whole data directory**, including `calrs.db`, `secret.key` and uploaded assets. Preserve any SQLite WAL files if present. If `CALRS_SECRET_KEY` supplies the key, back up that secret separately. Protect the backup like the running instance.
3. Try the selected build against a copy of the backup in an isolated test environment. Stored calendar credentials and queued work remain active: restrict outbound access or use test integrations so a test copy cannot update real calendars or send real notifications.
4. Deploy the new image, or install the matching binary **and templates**, retaining the existing data directory and key. Startup applies pending database migrations automatically; no manual SQL conversion is needed.
5. Verify sign-in, calendar synchronization, available slots, a test booking, approval, cancellation and calendar write-back. Check notification delivery, public links and embeds, and any API/MCP clients you use.

Database migrations and UTC booking storage are not a supported downgrade path. To roll back, stop the new version and restore the pre-upgrade data, key, binary/templates or image together. Bookings and configuration changes made after that backup will not be in the restored database.

## Changes to review

### Calendar access and recurrence

Private CalDAV/EWS hosts, resource feeds and server-discovered calendar hosts are checked on outgoing connections. Add intended internal hosts to the [private-host allowlist](./security.md#allowing-private-caldav-hosts-self-hosting), including private IP literals. With an outbound proxy, destination filtering is the proxy's responsibility; see [outbound proxies](./security.md#behind-an-outbound-proxy).

Malformed CalDAV/EWS replies now count as sync failures instead of empty calendars. Calendar sources show errors and retain previously cached data. Unsupported recurrence rules conservatively block time, so an upgrade may reveal previously missing conflicts. Use **Troubleshoot** to inspect these blocks.

Microsoft calendars require a successful first sync before their hosts are offered as free. Read [Microsoft Calendar](./microsoft-calendar.md) for its sync window and shared-calendar behavior.

### Proxy trust

Set [trusted proxies](./security.md#trusted-proxies) for your actual deployment. Only trusted peers can supply `X-Forwarded-For` for rate limiting. For a directly exposed LAN service, use `CALRS_TRUSTED_PROXIES=none`; behind a reverse proxy, list its addresses or ranges. The default trusts private and local networks, which is broader than many installations need.

### Authentication and Exchange provisioning

OIDC identities already linked to an account cannot claim a different account just because an email address was recycled. Local accounts with MFA are not automatically linked to SSO; review [authentication](./authentication.md).

Existing users start without administrator-confirmed addresses for organization-wide Exchange impersonation. Verify mailbox ownership before confirming addresses or enabling provisioning. See [EWS impersonation](./ews-impersonation.md).

### New integration permissions

REST API use (personal tokens and OAuth), outgoing webhook management and per-event-type guest footers are administrator-controlled. They start restricted to admins unless the corresponding policy is enabled. OAuth clients request consent for specific scopes; a grant is not blanket access to the instance.

### SMS limits

The daily SMS limit counts **billed segments**, including paid test messages, rather than logical messages. A long or non-GSM message can consume several segments. Existing numeric limits stay in place, so review them against actual usage. Unknown gateway outcomes keep their reservation to avoid undercounting possible charges. See [SMS notifications](./sms.md).

### Booking and interface changes

The dashboard and public pages have new layouts, searchable paginated lists, and meeting sections. Existing booking URLs remain usable. Availability override management URLs now use event type IDs; use the current dashboard links if you bookmarked an old slug-based management URL.

New bookings store UTC instants; old bookings retain their legacy interpretation until rescheduled. See [timezone handling](./booking-flow.md#timezones-and-existing-installations). A pending request can hold a slot and expire, and approval re-checks availability; review those settings on event types that require confirmation.

For the complete feature set and setup links, see [Features in v2](./features.md).
