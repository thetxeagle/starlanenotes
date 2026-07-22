# Local Authentication

Starlane Notes supports email/password authentication, passkey sign-in, OAuth/OIDC sign-in, and JWT-based sessions.

## Passkeys in the iOS app

The bundled iOS app can use passkeys for `starlane.2eagles.xyz`. Its signed target declares that relying-party domain as an Associated Domain (`webcredentials:starlane.2eagles.xyz`), and the hosted web build publishes the matching `/.well-known/apple-app-site-association` file for the `H9FVZ6838A.dev.starlane.notes` app ID.

After installing an updated app build, iOS may take time to refresh the association. If passkey setup still cannot open after reinstalling the app, use Safari temporarily and verify that `https://starlane.2eagles.xyz/.well-known/apple-app-site-association` serves the association JSON directly, rather than the Starlane HTML app shell.

For self-hosted servers, the native app opens Safari for passkey and SSO authentication. Safari retains only an opaque handoff ID across provider redirects, then returns a short-lived, single-use handoff code; the app exchanges it with a private PKCE verifier, so access and refresh tokens never appear in a callback URL. Password and TOTP authentication remain in the native app.

## Registration

**Endpoint:** `POST /api/v1/auth/register`

```json
{
  "email": "user@example.com",
  "display_name": "Jane Doe",
  "password": "securepassword"
}
```

The first user to register is automatically assigned the `admin` role and marks the instance setup as completed. After setup, public self-service registration is disabled by default; set `ALLOW_PUBLIC_SIGNUPS=true` or enable **Admin -> Authentication -> Allow public signups** to let subsequent users create `member` accounts without an invite/admin action.

### Password Requirements

| Constraint   | Value         |
|-------------|---------------|
| Minimum length | 8 characters  |
| Maximum length | 128 characters |

Passwords are hashed with Argon2id (default parameters) using a random salt.

### Input Validation

- `email`: required, max 254 characters
- `display_name`: required, max 100 characters
- `password`: required, 8--128 characters

### Response

On success, returns `201 Created` with an `AuthResponse`:

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

Duplicate email returns `409 Conflict`. When setup is complete and public signups are disabled, registration returns `403 Forbidden`.

## Login

**Endpoint:** `POST /api/v1/auth/login`

```json
{
  "email": "user@example.com",
  "password": "securepassword",
  "totp_code": "123456"
}
```

The `totp_code` field is optional. If the user has 2FA enabled and no code is provided, the server returns `403` with error code `2fa_required`. The client should prompt for a current authenticator code or one unused recovery code and resubmit the request with `totp_code` included. Accepted recovery codes are consumed immediately. After sign-in, either a current authenticator code or one unused recovery code can also confirm disabling 2FA when global 2FA enforcement is off; disabling clears the TOTP secret and remaining recovery codes.

Accounts created via OAuth/OIDC (with no password set) cannot use this endpoint and receive a `401 Unauthorized` with the message "This account uses OAuth/SSO login".

## Session Management

Each password, passkey, OAuth, or OIDC login creates a server-side session record with a hashed refresh token. Refresh requests must match an active session and rotate it to a new session record. Revoking a session prevents that refresh token from issuing further access tokens; already-issued access tokens remain valid only until their normal short expiry.

Signing out in server mode calls `POST /api/v1/auth/logout` to revoke the current refresh session. The client clears its local credentials immediately even if that request cannot reach the server.

### Token Lifecycle

Starlane Notes uses stateless JWT tokens:

| Token          | Lifetime    |
|---------------|-------------|
| Access token   | 15 minutes  |
| Refresh token  | 30 days     |

Both tokens are signed with HS256 using the `JWT_SECRET` environment variable.

### Refreshing Tokens

**Endpoint:** `POST /api/v1/auth/refresh`

```json
{
  "refresh_token": "eyJ..."
}
```

Returns a new access/refresh token pair. The server verifies the user still exists and is active before issuing new tokens.

### Logout

**Endpoint:** `POST /api/v1/auth/logout`

Requires a valid access token in the `Authorization` header. The server deletes the current session record matching the access token's `sid`, then returns `204 No Content`. The client also discards its stored access and refresh tokens so the signed-out session cannot refresh or continue through normal bearer-token validation.

### Listing Sessions

**Endpoint:** `GET /api/v1/auth/sessions`

Returns all sessions for the authenticated user, including device name, IP address, last seen timestamp, and creation time.

### Revoking a Session

**Endpoint:** `DELETE /api/v1/auth/sessions/{session_id}`

Deletes a specific session. Users can only revoke their own sessions.

### Current User

**Endpoint:** `GET /api/v1/auth/me`

Returns the authenticated user's profile (id, email, display name, avatar, role, TOTP status).

## Rate Limiting

Auth endpoints (`/auth/register`, `/auth/login`, `/auth/refresh`, `/auth/password-reset/request`, `/auth/password-reset/confirm`, `/auth/2fa/totp/verify`, `/auth/passkeys/login/start`, `/auth/passkeys/login/finish`) are rate-limited to 10 requests burst with a refill rate of 1 request every 6 seconds, keyed by client IP.

## Self-Service Password Reset

Signed-out users can request a password reset from `/reset-password` or through the API:

```http
POST /api/v1/auth/password-reset/request
```

The request response is intentionally generic to avoid revealing whether an email address exists. When an active account is found and SMTP is configured, the server creates a 30-minute one-time reset token and sends a link to `{BASE_URL}/reset-password?token=...`.

```http
POST /api/v1/auth/password-reset/confirm
```

Confirming a valid token updates the account password, consumes outstanding reset tokens for that user, and revokes existing refresh sessions.

## Environment Variables

| Variable        | Required | Description                                    |
|----------------|----------|------------------------------------------------|
| `JWT_SECRET`    | Yes (production) | Signing key for JWTs. Must be at least 32 characters. In dev mode, falls back to an insecure default. |
| `BASE_URL`      | No       | Base URL for the server. Defaults to `http://localhost:3000`. |
| `STARLANE_ENV`  | No       | Set to `production` to enforce `JWT_SECRET` and restrict CORS. |
| `REQUIRE_2FA`   | No       | Set to `true` to require TOTP enrollment for all server-mode users. Unenrolled signed-in users are redirected to Profile settings -> Security until setup is complete, and enrolled users cannot disable TOTP until enforcement is turned off. |
| `ALLOW_PUBLIC_SIGNUPS` | No | Set to `true` to allow public self-service registration after setup. Defaults to `false`, so new deployments are invite/admin-controlled unless explicitly opened. |

## Auth Configuration Endpoint

**Endpoint:** `GET /api/v1/auth/config` (public, no auth required)

Returns which authentication methods are enabled:

```json
{
  "google_enabled": false,
  "github_enabled": false,
  "oidc_enabled": false,
  "oidc_display_name": "SSO",
  "require_2fa": false,
  "allow_public_signups": false,
  "setup_completed": true
}
```

Use this endpoint to configure the login UI dynamically.
