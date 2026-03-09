# Admin Dashboard

The admin dashboard is available at `/dashboard/admin` for users with the `admin` role.

## User management

Lists all registered users with:

- Name, email, username
- Role (admin/user)
- Status (enabled/disabled)
- Groups (if using OIDC group sync)

Actions per user:

- **Promote/Demote** — toggle admin role
- **Enable/Disable** — disabled users cannot log in or receive bookings

## Authentication settings

- **Registration** — toggle open registration on/off
- **Allowed domains** — restrict registration to specific email domains (comma-separated) or allow any

## OIDC configuration

- **Enabled** — toggle SSO login on/off
- **Issuer URL** — your OIDC provider's base URL
- **Client ID** — the client ID registered with your provider
- **Client secret** — update the secret (current value is never displayed)
- **Auto-register** — automatically create users on first OIDC login

## SMTP status

Shows whether SMTP is configured and the current sender address. SMTP is configured via CLI (`calrs config smtp`) or by editing the database directly.
