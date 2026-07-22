export const AUTH_PROVIDER_SETUP_ROWS = [
  {
    name: "Google OAuth",
    vars: "GOOGLE_CLIENT_ID, GOOGLE_CLIENT_SECRET",
    note: "Server login only. Google Drive provider setup uses VITE_GOOGLE_DRIVE_CLIENT_ID with public PKCE and must not expose a browser client secret.",
  },
  { name: "GitHub OAuth", vars: "GITHUB_CLIENT_ID, GITHUB_CLIENT_SECRET" },
  { name: "Custom OIDC", vars: "OIDC_ISSUER_URL, OIDC_CLIENT_ID, OIDC_CLIENT_SECRET" },
] as const;

export const SIGNUP_POLICY_SETUP_ROWS = [
  {
    value: "invite",
    label: "Invite only",
    detail: "Block open sign-ups. Admins create or invite users.",
  },
  {
    value: "oidc",
    label: "OIDC auto-provision",
    detail: "Let known SSO users be created on first OIDC login.",
  },
  {
    value: "blocked",
    label: "Block all new users",
    detail: "Only existing accounts can sign in until an admin changes policy.",
  },
] as const;

export const AUTH_ERROR_ALERT_CLASS =
  "rounded-input border border-danger/30 bg-danger/10 px-3 py-2 text-sm text-danger";
