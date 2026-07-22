# SSO Provisioning

Starlane Notes supports automatic user provisioning through OAuth providers and, when enabled, OpenID Connect (OIDC). Google and GitHub OAuth create unknown users on first sign-in; OIDC follows `OIDC_AUTO_PROVISION`.

For IdP-driven directory lifecycle management after login setup, use [SCIM Provisioning](../authentication/scim.md). SCIM covers bearer-token-protected User and Group lifecycle endpoints, discovery metadata, audit expectations, and current sync boundaries.

## Auto-Provisioning on First Login

When a user logs in via OAuth (Google, GitHub) or OIDC and no matching account exists:

1. The server checks for an existing `oauth_accounts` record matching the provider and provider user ID.
2. If not found, it checks for an existing user with the same email address. If found, the OAuth account is linked to the existing user.
3. If no user exists with that email, Google/GitHub create a new `member` user. OIDC does the same only when `OIDC_AUTO_PROVISION=true`.

New users created via SSO have no password set. They can only authenticate through their linked SSO provider unless an admin sets a password for them.

## OIDC Configuration

| Variable | Default | Description |
|---|---|---|
| `OIDC_ENABLED` | `false` | Enable OIDC authentication |
| `OIDC_DISPLAY_NAME` | `SSO` | Button label shown on the login page |
| `OIDC_ISSUER_URL` | -- | OIDC provider issuer URL (must support auto-discovery) |
| `OIDC_CLIENT_ID` | -- | Client ID registered with the provider |
| `OIDC_CLIENT_SECRET` | -- | Client secret |
| `OIDC_SCOPES` | `openid,profile,email` | Comma-separated list of scopes |
| `OIDC_USER_CLAIM` | `email` | String identity/email claim; dot-separated nested paths are supported |
| `OIDC_ROLE_CLAIM` | (empty) | Optional string or string-array role claim path; nested paths are supported |
| `OIDC_ROLE_ADMIN_VALUE` | `admin` | Role-claim value that maps to Starlane `admin`; all other values map to `member` |
| `OIDC_AUTO_PROVISION` | `true` | Create users on first SSO login |

### Callback URL

Register the following callback URL with your OIDC provider:

```
{BASE_URL}/api/v1/auth/oidc/callback
```

## OIDC Claim Mapping

The server verifies the ID token before reading configured identity and role claims. `OIDC_USER_CLAIM` defaults to `email`; use another top-level or dot-separated string claim only when it contains the identity you want Starlane to store. When `OIDC_ROLE_CLAIM` is set, every OIDC login maps a matching `OIDC_ROLE_ADMIN_VALUE` to `admin` and maps missing or non-matching values to `member`. Set `OIDC_AUTO_PROVISION=false` to reject unknown OIDC identities while allowing existing linked and email-matched accounts to sign in.

## OAuth Providers

### Google

| Variable | Description |
|---|---|
| `GOOGLE_CLIENT_ID` | Google OAuth client ID |
| `GOOGLE_CLIENT_SECRET` | Google OAuth client secret |

Callback URL: `{BASE_URL}/api/v1/auth/oauth/google/callback`

### GitHub

| Variable | Description |
|---|---|
| `GITHUB_CLIENT_ID` | GitHub OAuth client ID |
| `GITHUB_CLIENT_SECRET` | GitHub OAuth client secret |

Callback URL: `{BASE_URL}/api/v1/auth/oauth/github/callback`

Both providers use PKCE for the authorization code exchange.

## Auth Configuration Endpoint

```
GET /api/v1/auth/config
```

Returns which providers are enabled (public, no auth required):

```json
{
  "google_enabled": true,
  "github_enabled": false,
  "oidc_enabled": true,
  "oidc_display_name": "Login with Pocket-ID",
  "require_2fa": false,
  "setup_completed": true
}
```

The frontend uses this endpoint to determine which login buttons to display.

## Security Considerations

- OIDC state (CSRF tokens, nonces) is stored in-memory and expires after 10 minutes.
- OAuth uses PKCE to protect the authorization code exchange.
- After successful SSO authentication, tokens are passed to the frontend via URL fragment (`#access_token=...`) rather than query parameters, preventing them from appearing in server access logs.
