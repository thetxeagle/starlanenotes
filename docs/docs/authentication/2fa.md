# Two-Factor Authentication and Passkeys

Starlane Notes supports TOTP (Time-based One-Time Password) as a second authentication factor, compatible with apps like Aegis, Authy, Google Authenticator, and any RFC 6238-compliant client.

Starlane Notes also supports passkeys for server-mode accounts. Passkeys are managed from Profile settings in the shared web/desktop/mobile UI and use server-side WebAuthn challenge storage.

## TOTP Parameters

| Parameter  | Value         |
|-----------|---------------|
| Algorithm  | SHA1          |
| Digits     | 6             |
| Period     | 30 seconds    |
| Issuer     | Starlane Notes |

## Setup Flow

### 1. Initiate Setup

**Endpoint:** `POST /api/v1/auth/2fa/totp/setup`

Requires a valid access token. Generates a new TOTP secret, stores it on the user record (not yet enabled), and returns:

```json
{
  "qr_code_url": "otpauth://totp/Starlane%20Notes:user@example.com?secret=BASE32SECRET&issuer=Starlane%20Notes&algorithm=SHA1&digits=6&period=30",
  "recovery_codes": [
    "1234-5678",
    "2345-6789",
    "3456-7890",
    "4567-8901",
    "5678-9012",
    "6789-0123",
    "7890-1234",
    "8901-2345"
  ]
}
```

The `qr_code_url` is an `otpauth://` URI that can be rendered as a QR code by the client, opened directly from supported mobile/desktop authenticator apps, or copied into an authenticator app manually. Recovery codes are stored only as hashes on the server; the plaintext codes are shown once during setup.

### 2. Verify and Enable

**Endpoint:** `POST /api/v1/auth/2fa/totp/verify`

Requires a valid access token. Submit a TOTP code from the authenticator app to confirm setup:

```json
{
  "code": "123456"
}
```

On success, TOTP is enabled on the account:

```json
{
  "totp_enabled": true
}
```

If the code is invalid, the server returns `401 Unauthorized`. The user can retry with a new code or re-run setup to generate a fresh secret.

## Login with 2FA

Once TOTP is enabled, the login endpoint requires a `totp_code` field. This can be either a current 6-digit authenticator code or one unused recovery code:

```json
{
  "email": "user@example.com",
  "password": "securepassword",
  "totp_code": "123456"
}
```

If `totp_code` is omitted when 2FA is enabled, the server returns:

- **Status:** `403 Forbidden`
- **Error code:** `2fa_required`

The client should detect this error and prompt the user for their TOTP or recovery code, then resubmit the login request with the code included. Recovery codes may be entered with or without the dash.

## Disabling 2FA

**Endpoint:** `POST /api/v1/auth/2fa/totp/disable`

Requires a valid access token and a valid second-factor code to confirm the action. The code can be a current 6-digit authenticator code or one unused recovery code. If global 2FA enforcement is enabled with `REQUIRE_2FA=true`, the server rejects disable requests until an admin turns that requirement off:

```json
{
  "code": "123456"
}
```

On success, TOTP is disabled and the stored secret plus any remaining recovery codes are cleared:

```json
{
  "totp_enabled": false
}
```

## Recovery Codes

Eight recovery codes are generated during TOTP setup. Each code is an 8-digit number formatted as `XXXX-XXXX`.

Store these codes securely. They serve as backup access if the authenticator app is lost. Recovery codes are one-time use: when one is accepted during login, the server removes it from the account and it cannot be reused. A recovery code can also confirm disabling 2FA after the user is signed in when global 2FA enforcement is off. Disabling TOTP clears any remaining recovery codes.

## Enforcing 2FA

Set the `REQUIRE_2FA` environment variable to `true` to enforce 2FA for all users. When enabled, the `/api/v1/auth/config` endpoint reports `require_2fa: true`.

Signed-in users who have not enabled TOTP are redirected from protected app routes to **Profile settings -> Security**. The profile security page remains accessible so the user can complete TOTP setup. After TOTP is enabled, normal navigation is restored. Profile security also shows a warning notice while enrollment is required and incomplete. When TOTP is already enabled, Profile settings shows a policy notice instead of disable controls while `REQUIRE_2FA=true`; the API also returns `403 forbidden` for disable attempts.

```bash
REQUIRE_2FA=true
```

## Rate Limiting

The TOTP verification endpoint (`/auth/2fa/totp/verify`) is rate-limited along with other auth endpoints: 10 requests burst, refilling at 1 request per 6 seconds per IP.

## Passkeys

Passkeys let users sign in with platform authenticators such as Face ID, Touch ID, device unlock, password managers, or hardware security keys.

### Managing Passkeys

Signed-in users can manage passkeys from **Profile settings -> Security**:

- add a named passkey;
- review existing passkey metadata;
- delete passkeys they no longer use.

Passkey registration requires a secure browser or webview context that exposes `PublicKeyCredential` and the credential create/get APIs. When the current browser/webview or origin does not support passkeys, the UI keeps the security section visible and explains that passkeys are unavailable there.

### Passkey Login

The current login flow is email-first:

1. Enter the account email address.
2. Choose **Sign in with passkey**.
3. Complete the platform authenticator prompt.

Email-first login avoids account enumeration and keeps the initial implementation compatible with non-discoverable credentials. Fully discoverable passwordless login without entering an email remains future work.

### Server-Side Challenge Storage

Passkey registration and login challenges are stored in the server database as short-lived, single-use ceremony state. The client receives only the challenge ID and WebAuthn public-key options, then returns the browser credential response for validation.

## API Reference

| Method | Endpoint                        | Auth Required | Description                |
|--------|--------------------------------|---------------|----------------------------|
| POST   | `/api/v1/auth/2fa/totp/setup`   | Yes           | Generate TOTP secret and recovery codes |
| POST   | `/api/v1/auth/2fa/totp/verify`  | Yes           | Verify code and enable TOTP |
| POST   | `/api/v1/auth/2fa/totp/disable` | Yes           | Disable TOTP (requires valid code) |
| GET    | `/api/v1/auth/passkeys` | Yes | List passkey metadata |
| POST   | `/api/v1/auth/passkeys/register/start` | Yes | Start passkey registration |
| POST   | `/api/v1/auth/passkeys/register/finish` | Yes | Finish passkey registration |
| DELETE | `/api/v1/auth/passkeys/{passkey_id}` | Yes | Delete a passkey |
| POST   | `/api/v1/auth/passkeys/login/start` | No | Start email-first passkey login |
| POST   | `/api/v1/auth/passkeys/login/finish` | No | Finish passkey login and issue tokens |
