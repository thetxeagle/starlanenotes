# Google OAuth

Starlane Notes supports Google as an OAuth 2.0 provider with PKCE.

## Google Cloud Console Setup

1. Go to the [Google Cloud Console](https://console.cloud.google.com/).
2. Create a new project or select an existing one.
3. Navigate to **APIs & Services > Credentials**.
4. Click **Create Credentials > OAuth client ID**.
5. Select **Web application** as the application type.
6. Under **Authorized redirect URIs**, add:
   ```
   https://your-starlane-instance.example.com/api/v1/auth/oauth/google/callback
   ```
7. Copy the **Client ID** and **Client Secret**.

If you have not configured the OAuth consent screen yet, you will be prompted to do so. Set the user type to **External** (or **Internal** for Google Workspace orgs). Add the following scopes:

- `openid`
- `email`
- `profile`

## Environment Variables

```bash
GOOGLE_CLIENT_ID=your-client-id.apps.googleusercontent.com
GOOGLE_CLIENT_SECRET=your-client-secret
BASE_URL=https://your-starlane-instance.example.com
```

Both `GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET` must be set for Google OAuth to be enabled. The `/api/v1/auth/config` endpoint will report `google_enabled: true` when configured.

## Redirect URI

The callback URL is constructed automatically from `BASE_URL`:

```
{BASE_URL}/api/v1/auth/oauth/google/callback
```

This must exactly match the redirect URI registered in Google Cloud Console.

## Authentication Flow

1. Client directs the user to `GET /api/v1/auth/oauth/google`.
2. Server generates a PKCE challenge and CSRF token, then redirects to Google's authorization endpoint.
3. User authenticates with Google and grants consent.
4. Google redirects to the callback URL with an authorization code and state parameter.
5. Server validates the CSRF state, exchanges the code for tokens using the PKCE verifier.
6. Server fetches user info from `https://www.googleapis.com/oauth2/v3/userinfo`.
7. Server creates or links the user account and issues JWT tokens.
8. User is redirected to `{BASE_URL}/#access_token={...}&refresh_token={...}`.

The tokens are passed in the URL fragment (not query string) so they are not sent to the server in subsequent requests or logged in access logs.

## Account Linking

- If a user with the same email already exists, the Google account is linked to the existing user.
- If no user exists, a new account is created with the `member` role.
- The OAuth provider/user-ID mapping is stored in the `oauth_accounts` table to support multiple OAuth providers per user.

## Scopes Requested

| Scope     | Purpose                        |
|-----------|--------------------------------|
| `openid`  | OpenID Connect identification  |
| `email`   | User's email address           |
| `profile` | Display name and profile photo |
