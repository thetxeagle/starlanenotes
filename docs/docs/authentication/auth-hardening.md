# Auth Hardening Notes

This page records the current Starlane Notes auth model and the next security-hardening lanes. It is intentionally source-backed and narrow so implementation work can move in small reviewable slices.

## Current model

- The Rust/Axum server issues HS256 JWT access and refresh tokens.
- Access tokens last 15 minutes.
- Refresh tokens last 30 days and are stored server-side as hashes in `sessions`.
- Refresh rotates the token pair and replaces the server session.
- Bearer access requests must map the token `sid` to an active, unexpired server session and active user.
- Password reset and admin password reset revoke existing sessions for the affected user.
- OAuth and OIDC use server-side state storage, PKCE/nonce validation, and redirect back to the web client with token values in the URL fragment.
- The current web client stores access and refresh tokens in browser `localStorage`.

## Invariants now enforced

- A bearer access token without a valid `sid` claim is rejected.
- A bearer access token whose session was revoked is rejected.
- A bearer access token for an inactive user is rejected.
- Current role and email are loaded from the database after session validation, so stale token role claims do not outlive server-side role changes.
- TOTP setup cannot overwrite an already enabled TOTP secret.
- OAuth/OIDC callback token parameters are removed from the browser URL fragment after processing.
- Incomplete callback token fragments clear local auth state instead of leaving partial tokens in the URL.

## Accepted current risk

The web client still stores server auth tokens in `localStorage`. This keeps the SPA/Tauri implementation simple, but it means a successful same-origin script injection could read bearer and refresh tokens. Current mitigations are:

- a restrictive CSP;
- short-lived access tokens;
- hashed and rotated refresh sessions;
- server-side session validation for bearer access requests;
- bounded password-reset confirmation input validation before token lookup;
- session revocation on logout, password reset, and admin reset.

Those mitigations reduce lifetime and blast radius, but they do not make `localStorage` equivalent to httpOnly cookies.

## Recommended next lane

The next structural hardening lane is cookie-backed refresh/session handling:

1. Keep access tokens short-lived.
2. Move refresh authority into a `Secure`, `HttpOnly`, `SameSite=Lax` or `SameSite=Strict` cookie where supported.
3. Add explicit CSRF protection for refresh, logout, and any cookie-authenticated mutation route.
4. Preserve bearer-token support for API keys, service automation, and Tauri/native flows where cookie handling is different.
5. Roll out behind a compatibility mode so existing self-hosted installs and desktop clients do not lose sessions abruptly.

This should be implemented as a dedicated migration slice, not mixed into ordinary auth cleanup.
