# GitHub OAuth

Starlane Notes supports GitHub as an OAuth 2.0 provider with PKCE.

## GitHub OAuth App Setup

1. Go to [GitHub Settings > Developer settings > OAuth Apps](https://github.com/settings/developers).
2. Click **New OAuth App**.
3. Fill in the form:
   - **Application name**: Starlane Notes (or any name)
   - **Homepage URL**: `https://your-starlane-instance.example.com`
   - **Authorization callback URL**:
     ```
     https://your-starlane-instance.example.com/api/v1/auth/oauth/github/callback
     ```
4. Click **Register application**.
5. Copy the **Client ID**.
6. Click **Generate a new client secret** and copy it immediately.

## Environment Variables

```bash
GITHUB_CLIENT_ID=your-client-id
GITHUB_CLIENT_SECRET=your-client-secret
BASE_URL=https://your-starlane-instance.example.com
```

Both `GITHUB_CLIENT_ID` and `GITHUB_CLIENT_SECRET` must be set for GitHub OAuth to be enabled. The `/api/v1/auth/config` endpoint will report `github_enabled: true` when configured.

## Redirect URI

The callback URL is constructed automatically from `BASE_URL`:

```
{BASE_URL}/api/v1/auth/oauth/github/callback
```

This must exactly match the callback URL registered in your GitHub OAuth App settings.

## Authentication Flow

1. Client directs the user to `GET /api/v1/auth/oauth/github`.
2. Server generates a PKCE challenge and CSRF token, then redirects to GitHub's authorization endpoint.
3. User authenticates with GitHub and grants consent.
4. GitHub redirects to the callback URL with an authorization code and state parameter.
5. Server validates the CSRF state, exchanges the code for tokens using the PKCE verifier.
6. Server fetches user info from `https://api.github.com/user` and the primary verified email from `https://api.github.com/user/emails`.
7. Server creates or links the user account and issues JWT tokens.
8. User is redirected to `{BASE_URL}/#access_token={...}&refresh_token={...}`.

## Email Resolution

GitHub does not guarantee an email in the user profile response. Starlane Notes fetches the user's email list from `https://api.github.com/user/emails` and selects the **primary, verified** email address. If no verified primary email is found, the login fails.

## Account Linking

- If a user with the same email already exists, the GitHub account is linked to the existing user.
- If no user exists, a new account is created with the `member` role.
- The display name is taken from the GitHub profile `name` field, falling back to the `login` (username) if `name` is not set.

## Scopes Requested

| Scope        | Purpose                                   |
|-------------|-------------------------------------------|
| `user:email` | Access the user's verified email addresses |
| `read:user`  | Read user profile data (name, avatar)      |
