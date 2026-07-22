# API Authentication

Starlane Notes uses JWT (JSON Web Tokens) for API authentication. Tokens are issued via email/password login, passkey login, OAuth (Google, GitHub), or OIDC.

For the current hardening posture and planned cookie/CSRF migration lane, see [Auth Hardening Notes](../authentication/auth-hardening.md).

## Token Types

| Token | Lifetime | Purpose |
|---|---|---|
| Access token | 15 minutes | Authenticates API requests |
| Refresh token | 30 days | Obtains new access/refresh token pairs |

Both tokens are HS256-signed JWTs containing the user's `sub` (user ID), `sid` (server session ID), `email`, `role`, and `token_type`. The server requires these claims during validation.

## Login

```
POST /api/v1/auth/login
```

```json
{
  "email": "user@example.com",
  "password": "secret",
  "totp_code": "123456"
}
```

The `totp_code` field is only required when the user has 2FA enabled. If 2FA is enabled and the code is omitted, the server returns `403` with `"error": "2fa_required"`.

### Response

```json
{
  "access_token": "eyJ...",
  "refresh_token": "eyJ...",
  "expires_in": 900,
  "user": {
    "id": "uuid",
    "email": "user@example.com",
    "display_name": "Jane Doe",
    "avatar_url": null,
    "role": "member",
    "totp_enabled": false
  }
}
```

## Registration

```
POST /api/v1/auth/register
```

```json
{
  "email": "user@example.com",
  "display_name": "Jane Doe",
  "password": "minimum8chars"
}
```

The first user registered automatically receives the `admin` role and marks instance setup as completed. Subsequent users receive the `member` role.

### Validation

- Email: required, max 254 characters, must be unique
- Password: 8-128 characters
- Display name: required, max 100 characters

## Token Refresh

```
POST /api/v1/auth/refresh
```

```json
{
  "refresh_token": "eyJ..."
}
```

Returns a new access/refresh token pair. Refresh tokens are checked against a hashed server-side session record and rotated on successful refresh. The server verifies the user is still active before issuing new tokens.

## Using the Access Token

Include the access token in the `Authorization` header for all authenticated requests:

```
Authorization: Bearer eyJ...
```

Bearer access tokens are accepted only while their `sid` still maps to an active, unexpired server session and active user. Revoking a session, logging out, resetting a password, or deactivating a user invalidates subsequent bearer-token requests without waiting for the access token's 15-minute expiration.

Personal API keys can also authenticate API requests with:

```
X-API-Key: stln_...
```

API keys are hashed at rest and displayed only once when created. Personal keys inherit the role of the user who created them. Service account keys are admin-managed credentials for server-to-server automation and authenticate as admin service principals only when they include the `admin` scope. New keys can be scoped:

- `read`: allows safe read methods such as `GET`, `HEAD`, and `OPTIONS`.
- `write`: allows mutating non-admin API requests.
- `admin`: allows admin API access for users who also have the `admin` role.

Legacy personal keys without stored scopes are treated as full-access keys for compatibility.

## Current User

```
GET /api/v1/auth/me
```

Returns the authenticated user's profile (id, email, display_name, avatar_url, role, totp_enabled).

### Update Profile

```
PUT /api/v1/auth/me
```

```json
{
  "display_name": "Jane Doe",
  "avatar_url": "https://example.com/avatar.png"
}
```

`display_name` is required and `avatar_url` can be omitted or set to an empty string to clear the profile image.

## OAuth Login

Starlane Notes supports Google and GitHub OAuth with PKCE.

### Initiate OAuth

```
GET /api/v1/auth/oauth/{provider}
```

Where `{provider}` is `google` or `github`. Redirects the user to the provider's authorization page.

### Callback

```
GET /api/v1/auth/oauth/{provider}/callback?code=...&state=...
```

After successful authentication, the server redirects to `{BASE_URL}/#access_token=...&refresh_token=...`. The tokens are passed in the URL fragment (not the query string) so they are not sent to the server in subsequent requests. The web client strips auth token parameters from the fragment immediately after processing and clears stored tokens if the callback is incomplete or `/auth/me` rejects the issued access token.

If no user exists with the provider's email, a new account is created with the `member` role.

### OAuth Environment Variables

| Variable | Description |
|---|---|
| `GOOGLE_CLIENT_ID` | Google OAuth client ID |
| `GOOGLE_CLIENT_SECRET` | Google OAuth client secret |
| `GITHUB_CLIENT_ID` | GitHub OAuth client ID |
| `GITHUB_CLIENT_SECRET` | GitHub OAuth client secret |

## OIDC Login

Generic OpenID Connect support for any compliant provider (Pocket-ID, Authentik, Keycloak, etc.).

### Initiate OIDC

```
GET /api/v1/auth/oidc
```

### Callback

```
GET /api/v1/auth/oidc/callback?code=...&state=...
```

Same redirect behavior as OAuth. See [SSO Provisioning](../user-provisioning/sso-provisioning.md) for auto-provisioning and role mapping configuration.

## Two-Factor Authentication (TOTP)

### Setup

```
POST /api/v1/auth/2fa/totp/setup
```

Returns a QR code URL and one-time recovery codes. The TOTP secret is stored but not activated until the first successful verification. Recovery codes are stored only as hashes and are shown in plaintext once during setup.
If TOTP is already enabled, setup is rejected; disable the existing authenticator before enrolling a new secret.

### Verify and Enable

```
POST /api/v1/auth/2fa/totp/verify
```

```json
{
  "code": "123456"
}
```

Verifies the TOTP code and enables 2FA on the account.

After TOTP is enabled, `/auth/login` accepts either a current authenticator code or one unused recovery code in the `totp_code` field. Accepted recovery codes are consumed immediately and cannot be reused.

### Disable

```
POST /api/v1/auth/2fa/totp/disable
```

Requires a valid second-factor code in the request body. This can be a current authenticator code or one unused recovery code. Disabling TOTP clears the stored secret and any remaining recovery codes. When `REQUIRE_2FA=true`, the server rejects disable requests with `403 forbidden` until enforcement is turned off.

## Passkeys

Passkeys use WebAuthn ceremonies. Challenge state is stored server-side, expires quickly, and is single-use; clients should never store or replay ceremony state.

### Native browser handoff

Self-hosted native clients use `POST /api/v1/auth/native-handoff/start`, `POST /api/v1/auth/native-handoff/complete`, and `POST /api/v1/auth/native-handoff/exchange` to let Safari complete passkey or SSO authentication. The start request supplies a PKCE SHA-256 challenge; authenticated Safari completion returns a one-time code, and the native client exchanges that code only with its matching private verifier. Codes expire in five minutes and access or refresh tokens are never placed in the callback URL.

The browser-facing JSON returned by the API contains WebAuthn `publicKey` options. Web, desktop, and mobile clients convert base64url challenge/credential fields to `ArrayBuffer` values before calling `navigator.credentials.create()` or `navigator.credentials.get()`.

### List passkeys

```
GET /api/v1/auth/passkeys
```

Requires a valid access token. Returns passkey metadata only:

```json
[
  {
    "id": "uuid",
    "name": "MacBook Touch ID",
    "created_at": "2026-07-16T00:00:00Z",
    "last_used_at": null
  }
]
```

### Register a passkey

Start registration while signed in:

```
POST /api/v1/auth/passkeys/register/start
```

```json
{
  "name": "iPhone Face ID"
}
```

The response contains a `challenge_id` and WebAuthn `public_key` creation options. After `navigator.credentials.create()` succeeds, submit the JSON-serializable credential:

```
POST /api/v1/auth/passkeys/register/finish
```

```json
{
  "challenge_id": "uuid",
  "name": "iPhone Face ID",
  "credential": {
    "id": "credential-id",
    "rawId": "base64url",
    "type": "public-key",
    "response": {
      "clientDataJSON": "base64url",
      "attestationObject": "base64url"
    }
  }
}
```

The server validates the registration response, rejects duplicate credential IDs, and stores the serialized passkey credential for future sign-in.

### Sign in with a passkey

The current flow is email-first passkey login:

```
POST /api/v1/auth/passkeys/login/start
```

```json
{
  "email": "user@example.com"
}
```

After `navigator.credentials.get()` succeeds, finish the login:

```
POST /api/v1/auth/passkeys/login/finish
```

```json
{
  "challenge_id": "uuid",
  "credential": {
    "id": "credential-id",
    "rawId": "base64url",
    "type": "public-key",
    "response": {
      "clientDataJSON": "base64url",
      "authenticatorData": "base64url",
      "signature": "base64url",
      "userHandle": null
    }
  }
}
```

On success, the response is the same `AuthResponse` shape returned by password login. Discoverable/passwordless login without entering an email first remains a future enhancement.

### Delete a passkey

```
DELETE /api/v1/auth/passkeys/{passkey_id}
```

Requires a valid access token. Deletes only passkeys owned by the signed-in user.

## Session Management

### List Sessions

```
GET /api/v1/auth/sessions
```

Returns active sessions with device name, IP address, last seen, and creation timestamps.
Signed-in users can also review and revoke their own sessions from mobile **Profile -> Active sessions**.

### Revoke Session

```
DELETE /api/v1/auth/sessions/{session_id}
```

Admins can inspect and revoke sessions for any user:

```
GET    /api/v1/admin/users/{user_id}/sessions
DELETE /api/v1/admin/users/{user_id}/sessions/{session_id}
```

Admins can also reset an active user's password:

```
POST /api/v1/admin/users/{user_id}/password-reset
```

The response includes a one-time `temporary_password`. The reset revokes active refresh sessions for that user.

## API Keys

API keys provide script and integration access. Personal keys belong to the signed-in user. Service account keys are managed by admins for server-to-server automation and must include the `admin` scope.

### List API Keys

```
GET /api/v1/auth/api-keys
```

Returns key metadata only. Secret values are never returned after creation.

### Create API Key

```
POST /api/v1/auth/api-keys
```

```json
{
  "name": "Automation",
  "scopes": ["read", "write"],
  "expires_at": "2026-12-31T23:59:59Z"
}
```

`scopes` defaults to `["read", "write"]` for personal keys. `expires_at` is optional and must be a future RFC3339 timestamp when provided.

### Create Response

```json
{
  "id": "uuid",
  "name": "Automation",
  "key_prefix": "stln_abcd123",
  "is_service_account": false,
  "scopes": ["read", "write"],
  "last_used_at": null,
  "expires_at": "2026-12-31T23:59:59Z",
  "created_at": "2026-07-06T00:00:00Z",
  "key": "stln_full-secret-shown-once"
}
```

### Revoke API Key

```
DELETE /api/v1/auth/api-keys/{key_id}
```

### Service Account API Keys

Admins can list, create, and revoke service account keys:

```
GET    /api/v1/admin/api-keys
POST   /api/v1/admin/api-keys
DELETE /api/v1/admin/api-keys/{key_id}
```

```json
{
  "name": "Nightly import worker",
  "scopes": ["admin"],
  "expires_at": "2026-12-31T23:59:59Z"
}
```

Service account key creation fails unless the normalized scopes include `admin`. These keys have no user owner, are listed separately in **Admin -> API Keys**, and should be reserved for trusted automation.

## Auth Configuration (Public)

```
GET /api/v1/auth/config
```

Returns which auth providers are enabled, whether 2FA is required, and whether initial setup is complete. Used by the frontend to show the correct login options.

## Password Reset (Public)

```
POST /api/v1/auth/password-reset/request
```

Accepts an email address and returns a generic response. If an active account exists and SMTP is configured, the server sends a 30-minute one-time reset link.

```
POST /api/v1/auth/password-reset/confirm
```

Accepts a reset token and new password. A valid token updates the password, consumes outstanding reset tokens for that user, and revokes existing refresh sessions.

## Logout

```
POST /api/v1/auth/logout
```

Logout deletes the current server-side session and the client discards its stored tokens. Because bearer access tokens are checked against active sessions, the logged-out session is rejected on subsequent requests.

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `JWT_SECRET` | dev-only default | Signing key for JWTs (min 32 chars, required in production) |
| `STARLANE_ENV` | (empty) | Set to `production` to enforce JWT_SECRET requirement |
| `REQUIRE_2FA` | `false` | Require TOTP enrollment for all server-mode users; unenrolled signed-in users are redirected to Profile settings -> Security |
