# Google Calendar

calrs can connect to Google Calendar as a CalDAV source using Google's OAuth2 flow. Unlike a username/password source, Google requires you (the calrs operator) to register an OAuth2 application in Google Cloud and configure its client ID and secret in calrs once. After that, individual users connect their own Google accounts through the dashboard.

This page walks through the Google Cloud setup. The values you collect at the end go into **Admin → Auth → Google OAuth2** in the calrs dashboard.

---

## 1. Pick (or create) a Google Cloud project

Go to the [Google Cloud Console](https://console.cloud.google.com/) and either select an existing project or create a new one (e.g. `calrs-prod`). Everything below is scoped to this project.

The project is just a container for the OAuth2 client and the enabled APIs. You can use the same project for staging and production, or split them. It makes no functional difference to calrs.

---

## 2. Enable the required APIs

In **APIs & Services → Library**, enable:

- **Google Calendar API**: used for CalDAV access to the user's calendars (event read and write-back).
- **OIDC userinfo**: Google exposes this automatically when you request the `openid email` scopes; there is no separate "API" to toggle, but the OAuth consent screen must allow those scopes (see step 5).

Without the Calendar API enabled, every CalDAV request will fail with a 403 even though the OAuth2 handshake itself succeeds.

---

## 3. Create the OAuth2 client

In **APIs & Services → Credentials**, click **Create credentials → OAuth client ID**.

- **Application type:** *Web application*
- **Name:** anything (e.g. `calrs`)

You will get a **client ID** and **client secret**. Both are stored encrypted in calrs (`auth_config.google_oauth2_client_id` / `google_oauth2_client_secret`) once you paste them into the admin panel.

---

## 4. Authorized redirect URI

Under **Authorized redirect URIs**, add **exactly** one entry:

```
{CALRS_BASE_URL}/dashboard/sources/google/callback
```

Replace `{CALRS_BASE_URL}` with the public URL of your calrs instance, e.g.:

```
https://cal.example.com/dashboard/sources/google/callback
```

Notes:

- The URI must match byte-for-byte. No trailing slash, correct scheme (`https://` in production), correct host. Google rejects the callback otherwise.
- `CALRS_BASE_URL` is the same env var calrs uses for OIDC redirects and email links. Keep them consistent.
- If you run multiple environments (staging + prod), either register the same client with multiple redirect URIs or create one OAuth2 client per environment.

You do **not** need to set "Authorized JavaScript origins"; calrs performs the redirect server-side.

---

## 5. Scopes calrs requests

When a user connects their Google account, calrs requests these scopes:

| Scope | Why |
|---|---|
| `https://www.googleapis.com/auth/calendar` | Full read/write access to the user's calendars via Google's CalDAV endpoint. Needed both to read busy times and to push confirmed bookings back to the calendar. |
| `openid email` | OpenID Connect userinfo, used once at connect time to discover the account's email address. Google's CalDAV principal URL is `/caldav/v2/{userEmail}/user`, so calrs needs to know which email to scope the source to. |

calrs also passes `access_type=offline` and `prompt=consent` so that Google issues a long-lived refresh token. The refresh token is stored encrypted and used to mint new access tokens as needed (existing tokens are rotated automatically).

On the OAuth consent screen configuration (**APIs & Services → OAuth consent screen**), add the Calendar scope explicitly. The `openid` and `email` scopes are listed under the "non-sensitive" defaults and don't need additional review.

---

## 6. Test users vs. publishing the consent screen

While the OAuth consent screen is in **Testing** status, only accounts explicitly listed under **Test users** can complete the OAuth flow. Everyone else gets `Error 403: access_denied` at the Google consent page.

You have two options:

- **Keep it in Testing** if calrs is only used by a small, known group (say, a single team or family). Add each user's Google email to the Test users list. Refresh tokens issued to test users expire after 7 days, so users will need to reconnect their source weekly.
- **Publish the app** (button on the OAuth consent screen page) for any larger or longer-running deployment. Because the Calendar scope is marked **sensitive/restricted** by Google, publishing triggers Google's app verification process. They will ask for a homepage, privacy policy, branding assets, and (for restricted scopes) a security assessment. This can take weeks. Until verification completes, users see an "unverified app" warning but can still proceed via *Advanced → Go to {app} (unsafe)*.

For a self-hosted instance used by you and a handful of people, the Testing mode + Test users approach is usually fine; just remember the 7-day refresh token expiry.
