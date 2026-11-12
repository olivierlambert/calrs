# Microsoft Exchange (EWS)

Use the EWS provider for **on-premises Exchange**. The implementation targets Exchange 2013/2016/2019 and uses the SOAP endpoint at `/EWS/Exchange.asmx`. For Microsoft 365 / Exchange Online or Outlook.com, use [Microsoft Calendar](./microsoft-calendar.md), which connects through Microsoft Graph and OAuth2.

EWS calendar access is separate from logging into calrs. Use [OIDC](./authentication.md#oidc--sso) for SSO. For centrally managed calendars without asking each user for a password, see [EWS impersonation](./ews-impersonation.md).

## Connect an individual mailbox

1. In **Calendar sources**, add a source and choose the Exchange/EWS provider.
2. Enter the EWS endpoint, for example `https://mail.example.com/EWS/Exchange.asmx`, and the mailbox credentials.
3. Test the connection and save the source.
4. Sync it, then choose which calendars should be synced and which writable calendar receives bookings.
5. Check an event type's slots against a known busy event and make a test booking to verify write-back.

The client uses HTTP Basic authentication over TLS. It does not negotiate NTLM or Kerberos. Your Exchange deployment must permit the supported authentication method or provide an appropriately configured authentication proxy. A Microsoft OAuth client configured for Graph does not authenticate this provider.

## CLI setup

```bash
calrs source add --provider ews \
  --url https://mail.example.com/EWS/Exchange.asmx \
  --username alice@example.com --name Exchange

# Or ask Autodiscover for the endpoint using the mailbox address
calrs source add --provider ews --email alice@example.com \
  --username alice@example.com --name Exchange

calrs sync
calrs source list
```

The password is prompted without echo. For automation, use `--password-stdin`; see the [CLI reference](./cli-reference.md#calrs-source). Use the web dashboard to connect the correct user on a multi-user instance.

## Private networks and failures

If Exchange resolves to a private address, add its hostname to the instance's [private-host allowlist](./security.md#allowing-private-caldav-hosts-self-hosting). Autodiscover and any returned endpoint must satisfy the same policy. For an outbound HTTP proxy, review [proxy behavior](./security.md#behind-an-outbound-proxy).

Calendar discovery, events, recurring occurrences and write-back use EWS operations behind calrs's common calendar interface. A malformed or failed EWS reply fails the sync and retains cached events; it is not interpreted as an empty calendar. Check source errors in the dashboard and the server log. Booking write failures are shown on the booking and retried; see [write-back recovery](./caldav.md#when-a-write-back-fails).
