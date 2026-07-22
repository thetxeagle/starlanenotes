# OpenID Connect (OIDC)

Starlane Notes supports any OpenID Connect-compliant identity provider. The server uses OIDC auto-discovery to fetch provider metadata from the issuer URL.

## Admin UI

Site admins can review and update database-backed OIDC and login policy settings from **Admin -> Authentication**. Values supplied by environment variables remain locked in the Admin UI and must be changed on the server or deployment environment.

Local standalone mode does not expose server authentication settings.

## Environment Variables

```bash
# Required
OIDC_ENABLED=true
OIDC_ISSUER_URL=https://auth.example.com/application/o/starlane-notes/
OIDC_CLIENT_ID=your-client-id
OIDC_CLIENT_SECRET=your-client-secret

# Optional
OIDC_DISPLAY_NAME=SSO                        # Button label in the login UI (default: "SSO")
OIDC_SCOPES=openid,profile,email             # Comma-separated (default: "openid,profile,email")
OIDC_USER_CLAIM=email                        # Claim used for user identification (default: "email")
OIDC_ROLE_CLAIM=                             # Dot-notation path to role in ID token claims
OIDC_ROLE_ADMIN_VALUE=admin                  # Value in role claim that maps to admin (default: "admin")
OIDC_AUTO_PROVISION=true                     # Auto-create users on first login (default: true)

BASE_URL=https://your-starlane-instance.example.com
```

| Variable               | Required | Default                   | Description |
|------------------------|----------|---------------------------|-------------|
| `OIDC_ENABLED`         | Yes      | `false`                   | Set to `true` to enable OIDC. |
| `OIDC_ISSUER_URL`      | Yes      | --                        | The issuer URL. Must serve `/.well-known/openid-configuration`. |
| `OIDC_CLIENT_ID`       | Yes      | --                        | Client ID from your identity provider. |
| `OIDC_CLIENT_SECRET`   | Yes      | --                        | Client secret from your identity provider. |
| `OIDC_DISPLAY_NAME`    | No       | `SSO`                     | Display name shown on the login button. |
| `OIDC_SCOPES`          | No       | `openid,profile,email`    | Scopes to request. `openid` is always included. |
| `OIDC_USER_CLAIM`      | No       | `email`                   | String claim used as the user's Starlane identity/email. Supports a top-level or dot-separated path. |
| `OIDC_ROLE_CLAIM`      | No       | (empty)                   | Optional string or string-array role claim path. Supports dot-separated nested claims. |
| `OIDC_ROLE_ADMIN_VALUE`| No       | `admin`                   | Claim value that maps the user to the Starlane `admin` role. Other/missing values map to `member`. |
| `OIDC_AUTO_PROVISION`  | No       | `true`                    | Auto-create user accounts on first OIDC login. |

## Redirect URI

The callback URL is constructed from `BASE_URL`:

```
{BASE_URL}/api/v1/auth/oidc/callback
```

Register this as an allowed redirect URI in your identity provider.

## Authentication Flow

1. Client directs the user to `GET /api/v1/auth/oidc`.
2. Server discovers the provider's endpoints via `{OIDC_ISSUER_URL}/.well-known/openid-configuration`.
3. Server generates a CSRF token and nonce, then redirects to the provider's authorization endpoint.
4. User authenticates with the identity provider.
5. Provider redirects to the callback URL with an authorization code and state parameter.
6. Server validates the CSRF state, exchanges the code for tokens, and verifies the ID token signature and nonce.
7. Server verifies the ID token signature, issuer, audience, expiry, and nonce, then extracts the configured identity and optional role claims from that verified token.
8. Server creates or links the user account and issues JWT tokens.
9. User is redirected to `{BASE_URL}/#access_token={...}&refresh_token={...}`.

## Claim Mapping

`OIDC_USER_CLAIM` defaults to the standard `email` claim. Set it to another string claim when the provider supplies the user's email/identity under a different name, such as `preferred_username`.

When `OIDC_ROLE_CLAIM` is set, its value becomes the role source of truth at each OIDC login: a matching `OIDC_ROLE_ADMIN_VALUE` produces `admin`; a missing or non-matching value produces `member`. The role claim may be a string or string array, so Keycloak-style paths such as `realm_access.roles` work. Do not enable role mapping until the provider sends the intended claim in the ID token.

`OIDC_AUTO_PROVISION=false` prevents creation of an unknown OIDC identity. Existing linked OAuth accounts and existing email-matched accounts can still sign in.

## Provider Examples

### Authentik

1. In Authentik, create an **OAuth2/OpenID Provider**:
   - **Client type**: Confidential
   - **Redirect URIs**: `https://starlane.example.com/api/v1/auth/oidc/callback`
   - **Scopes**: `openid`, `profile`, `email`
2. Create an **Application** and assign the provider.
3. Set environment variables:

```bash
OIDC_ENABLED=true
OIDC_ISSUER_URL=https://authentik.example.com/application/o/starlane-notes/
OIDC_CLIENT_ID=<client-id-from-authentik>
OIDC_CLIENT_SECRET=<client-secret-from-authentik>
OIDC_DISPLAY_NAME=Authentik
```

### Keycloak

1. In Keycloak, create a new **Client** in your realm:
   - **Client ID**: `starlane-notes`
   - **Client authentication**: On (confidential)
   - **Valid redirect URIs**: `https://starlane.example.com/api/v1/auth/oidc/callback`
2. Note the client secret from the **Credentials** tab.
3. Set environment variables:

```bash
OIDC_ENABLED=true
OIDC_ISSUER_URL=https://keycloak.example.com/realms/your-realm
OIDC_CLIENT_ID=starlane-notes
OIDC_CLIENT_SECRET=<client-secret-from-keycloak>
OIDC_DISPLAY_NAME=Keycloak
OIDC_ROLE_CLAIM=realm_access.roles
OIDC_ROLE_ADMIN_VALUE=starlane-admin
```

### Pocket-ID

1. In Pocket-ID, create a new OIDC client.
2. Set the redirect URI to `https://starlane.example.com/api/v1/auth/oidc/callback`.
3. Set environment variables:

```bash
OIDC_ENABLED=true
OIDC_ISSUER_URL=https://pocket-id.example.com
OIDC_CLIENT_ID=<client-id>
OIDC_CLIENT_SECRET=<client-secret>
OIDC_DISPLAY_NAME=Pocket-ID
```
