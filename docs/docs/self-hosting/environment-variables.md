# Environment Variables

All configuration is done via environment variables. Docker Compose loads `.env` beside `docker-compose.yml`; local server runs can use a `.env` file in the project root.

## Server

| Variable | Description | Default | Required |
|---|---|---|---|
| `STARLANE_IMAGE` | Container image used by Docker Compose | `ghcr.io/hbftechnologies/starlanenotes:latest` | No |
| `PORT` | HTTP listen port | `3000` | No |
| `BASE_URL` | Public-facing URL (used for CORS, OAuth redirects, and passkey/WebAuthn RP ID/origin verification) | `http://localhost:3000` | Yes (production) |
| `STARLANE_ENV` | Set to `production` to enforce JWT_SECRET and restrict CORS | _(empty)_ | Yes (production) |
| `STARLANE_CORS_ORIGINS` | Optional comma-separated extra production CORS origins for native wrappers, dev portals, or alternate hostnames. `BASE_URL` and built-in Tauri app origins are allowed automatically. Wildcard `*` is rejected in production. | _(empty)_ | No |
| `STARLANE_WEBAUTHN_ORIGINS` | Optional comma-separated extra origins allowed for passkey/WebAuthn verification. `BASE_URL` and built-in Tauri app origins are allowed automatically. Use only explicit trusted origins; wildcard `*` is rejected. | _(empty)_ | No |
| `DATA_DIR` | Root directory for SQLite data, documents, search index, and startup marker files | `data` | No |
| `MAX_UPLOAD_SIZE_MB` | Server-side maximum attachment upload size in megabytes. When set, this locks the Admin UI value. | `100` | No |
| `WEB_DIST_DIR` | Directory the Rust server uses when serving the built web app assets | `web-dist` | No |
| `ALLOW_PUBLIC_PAGES` | Set to `false` to disable unauthenticated public page publishing and reads | `true` | No |
| `RUST_LOG` | Tracing filter directive | `starlane_notes_server=debug,tower_http=debug` | No |

## Web and App Build

These variables are read by the Vite web build and must be set before building Docker, Tauri desktop, or Tauri mobile assets.

| Variable | Description | Default | Required |
|---|---|---|---|
| `VITE_MAX_UPLOAD_SIZE_MB` | Client-side picker guardrail for image and file imports. The server remains authoritative through `MAX_UPLOAD_SIZE_MB` or Admin UI `max_upload_size_mb`. | `30` | No |
| `VITE_GOOGLE_DRIVE_CLIENT_ID` | Optional browser-safe Google Drive provider client ID used by the manual provider setup UI | _(empty)_ | No |
| `VITE_GOOGLE_DRIVE_CLIENT_SECRET` | Optional Google Drive setup helper value for local/manual testing. Do not ship public browser builds with a real client secret. | _(empty)_ | No |
| `TAURI_DEV_HOST` | Tauri development host used by Vite when running native desktop/mobile dev builds | _(empty)_ | No |

## Database

| Variable | Description | Default | Required |
|---|---|---|---|
| `SQLITE_ENABLED` | Use SQLite and let the server create `${DATA_DIR}/starlane-notes.db` | `true` | No |
| `POSTGRES_HOST` | PostgreSQL host when `SQLITE_ENABLED=false` | _(empty)_ | Yes (PostgreSQL) |
| `POSTGRES_PORT` | PostgreSQL port when `SQLITE_ENABLED=false` | `5432` | No |
| `POSTGRES_USER` | PostgreSQL username when `SQLITE_ENABLED=false` | `starlane` | Yes (PostgreSQL) |
| `POSTGRES_PASSWORD` | PostgreSQL password when `SQLITE_ENABLED=false` | _(empty)_ | Yes (PostgreSQL) |
| `POSTGRES_DB` | PostgreSQL database name when `SQLITE_ENABLED=false` | `starlane_notes` | Yes (PostgreSQL) |
| `POSTGRES_SSLMODE` | PostgreSQL SSL mode when `SQLITE_ENABLED=false` | `disable` | No |
| `DATABASE_URL` | Optional legacy PostgreSQL URL override. Ignored when `SQLITE_ENABLED=true`. | _(empty)_ | No |

PostgreSQL example:

```env
SQLITE_ENABLED=false
POSTGRES_HOST=db
POSTGRES_PORT=5432
POSTGRES_USER=starlane
POSTGRES_PASSWORD=change-me
POSTGRES_DB=starlane_notes
POSTGRES_SSLMODE=disable
```

SQLite is automatic when `SQLITE_ENABLED=true` or omitted. The server creates `${DATA_DIR}/starlane-notes.db` and writes a `.starlane-notes-data-dir` marker file on startup so storage mounts are easy to verify. Migrations run automatically on startup.

## Authentication

Server-mode authentication settings can also be reviewed from **Admin -> Authentication**. Environment-backed values are locked there and must be changed in the deployment environment.

| Variable | Description | Default | Required |
| --- | --- | --- | --- |
| `SCIM_BEARER_TOKEN` | Enables SCIM discovery, User provisioning/lifecycle, and read-only Group directory endpoints; authenticates all SCIM bearer requests. | _(none)_ | No |

| Variable | Description | Default | Required |
|---|---|---|---|
| `JWT_SECRET` | Secret key for signing JWTs. Must be at least 32 characters. | Insecure dev default | Yes (production) |
| `REQUIRE_2FA` | When `true`, all server-mode users must enable TOTP 2FA; unenrolled signed-in users are redirected to Profile settings -> Security and enrolled users cannot disable TOTP until enforcement is turned off | `false` | No |
| `ALLOW_PUBLIC_SIGNUPS` | Enables public self-service account registration after the first admin user completes setup. Keep `false` for invite-only/self-hosted deployments. | `false` | No |

In production (`STARLANE_ENV=production`), the server will refuse to start without `JWT_SECRET`.

Generate and write a production-ready secret with:

```bash
JWT_SECRET="$(openssl rand -hex 32)"
sed -i.bak "s/^JWT_SECRET=.*/JWT_SECRET=${JWT_SECRET}/" .env
rm -f .env.bak
```

## OAuth -- Google

| Variable | Description | Default | Required |
|---|---|---|---|
| `GOOGLE_CLIENT_ID` | Google OAuth 2.0 client ID | _(none)_ | No |
| `GOOGLE_CLIENT_SECRET` | Google OAuth 2.0 client secret | _(none)_ | No |

Google OAuth is enabled automatically when both variables are set. The callback URL to register in Google Cloud Console is `{BASE_URL}/api/v1/auth/oauth/google/callback`.

## OAuth -- GitHub

| Variable | Description | Default | Required |
|---|---|---|---|
| `GITHUB_CLIENT_ID` | GitHub OAuth App client ID | _(none)_ | No |
| `GITHUB_CLIENT_SECRET` | GitHub OAuth App client secret | _(none)_ | No |

GitHub OAuth is enabled automatically when both variables are set. The callback URL to register in GitHub is `{BASE_URL}/api/v1/auth/oauth/github/callback`.

## Notifications

SMTP settings control email delivery for workflow and document notifications. They can also be managed from **Admin -> Notifications** when not locked by environment variables.

| Variable | Description | Default | Required |
|---|---|---|---|
| `SMTP_HOST` | SMTP relay host. Email delivery is disabled when empty. | _(empty)_ | No |
| `SMTP_PORT` | SMTP relay port | `587` | No |
| `SMTP_USER` | SMTP username and fallback sender address | _(empty)_ | No |
| `SMTP_PASSWORD` | SMTP password | _(empty)_ | No |
| `SMTP_FROM` | Sender address for notification email | `SMTP_USER` | No |

## Workflow Automation

Workflow deadline escalation automation is disabled unless an interval is configured. The worker reuses the admin overdue-delegation scan and records escalation events only for active workflows whose latest handoff is still a delegated event with an overdue `due` or `deadline` date.

| Variable | Description | Default | Required |
|---|---|---|---|
| `WORKFLOW_DEADLINE_ESCALATION_INTERVAL_MINUTES` | Enables the internal deadline escalation worker when set to a positive number of minutes | _(disabled)_ | No |
| `WORKFLOW_DEADLINE_ESCALATION_ACTOR_ID` | Active user ID recorded as the workflow event actor for automated escalations | _(empty)_ | Yes (if worker enabled) |
| `WORKFLOW_DEADLINE_ESCALATION_TARGET` | Handoff target used in automated escalation notes | `admin` | No |
| `WORKFLOW_REASSIGNMENT_POLICY` | Optional JSON policy for new workflow delegate/escalate events. Supports `allow_delegate`, `allow_escalate`, `allowed_targets`, `require_due_date`, and `max_due_days`. Imports are not blocked by this policy. | permissive | No |

Example reassignment policy:

```json
{"allowed_targets":["admin","role:owner"],"require_due_date":true,"max_due_days":14}
```

## User Provisioning Policy

Admin invite creation and CSV user import/preview can be constrained by optional server-side policy. These values can also be managed from Admin Configuration when they are not locked by environment variables.

| Variable | Description | Default | Required |
|---|---|---|---|
| `USER_PROVISIONING_ALLOWED_EMAIL_DOMAINS` | Comma-separated email domains allowed for newly provisioned users. Empty allows any domain. | _(empty)_ | No |
| `USER_PROVISIONING_ALLOWED_ROLES` | Comma-separated roles allowed for invite/import provisioning. | `admin,member,viewer` | No |

## OIDC (Generic OpenID Connect)

Database-backed OIDC settings can be managed from **Admin -> Authentication** when the corresponding environment variables are not set.

| Variable | Description | Default | Required |
|---|---|---|---|
| `OIDC_ENABLED` | Set to `true` to enable OIDC login | `false` | No |
| `OIDC_DISPLAY_NAME` | Label shown on the login button | `SSO` | No |
| `OIDC_ISSUER_URL` | OIDC issuer URL (must support auto-discovery) | _(none)_ | Yes (if OIDC enabled) |
| `OIDC_CLIENT_ID` | OIDC client ID | _(none)_ | Yes (if OIDC enabled) |
| `OIDC_CLIENT_SECRET` | OIDC client secret | _(none)_ | Yes (if OIDC enabled) |
| `OIDC_SCOPES` | Comma-separated list of scopes to request | `openid,profile,email` | No |
| `OIDC_USER_CLAIM` | Claim used to identify the user | `email` | No |
| `OIDC_ROLE_CLAIM` | Claim path for role mapping (dot-separated for nested claims) | _(empty)_ | No |
| `OIDC_ROLE_ADMIN_VALUE` | Value of the role claim that maps to admin | `admin` | No |
| `OIDC_AUTO_PROVISION` | Automatically create accounts on first OIDC login | `true` | No |

The OIDC callback URL is `{BASE_URL}/api/v1/auth/oidc/callback`.

## AI

| Variable | Description | Default | Required |
|---|---|---|---|
| `AI_ENABLED` | Set to `true` to enable AI features (chat, completions, doc actions) | `false` | No |
| `AI_DEFAULT_PROVIDER` | Which provider to use: `ollama`, `openai`, `anthropic`, `xai` | `ollama` | No |

### Ollama

| Variable | Description | Default |
|---|---|---|
| `OLLAMA_BASE_URL` | Ollama API base URL | `http://localhost:11434` |
| `OLLAMA_MODEL` | Model name to use | `llama3.2` |

### OpenAI

| Variable | Description | Default |
|---|---|---|
| `OPENAI_API_KEY` | OpenAI API key | _(none)_ |
| `OPENAI_MODEL` | Model name to use | `gpt-4o` |

### Anthropic

| Variable | Description | Default |
|---|---|---|
| `ANTHROPIC_API_KEY` | Anthropic API key | _(none)_ |
| `ANTHROPIC_MODEL` | Model name to use | `claude-sonnet-4-20250514` |

### xAI

| Variable | Description | Default |
|---|---|---|
| `XAI_API_KEY` | xAI API key | _(none)_ |
| `XAI_MODEL` | Model name to use | `grok-2-latest` |

## Docs and Wiki Tooling

These variables affect documentation generation and publishing scripts. They are not required for normal application runtime.

| Variable | Description | Default | Required |
|---|---|---|---|
| `DOCS_SOURCE_DIR` | Source directory used by `npm run docs:wiki` | `docs/docs` | No |
| `WIKI_OUTPUT_DIR` | Output directory used by `npm run docs:wiki` | `artifacts/gitea-wiki` | No |
| `WIKI_REMOTE_URL` | Optional Gitea Wiki remote URL override for `npm run docs:wiki:publish` | Derived from Git remote | No |
| `STARLANE_DOCS_PORT` | Host port for the standalone Starlight docs container | `9321` | No |
| `STARLANE_DOCS_IMAGE` | Docs container image used by `docker/docker-compose.docs.yml` | `ghcr.io/hbftechnologies/starlanenotes-docs:latest` | No |
| `STARLANE_DOCS_SITE_URL` | Public URL baked into the Starlight build for canonical links and sitemap output | `http://localhost:9321` | No; set it for public production builds |
| `WIKI_USERNAME` | Gitea Actions secret used by the wiki sync workflow when publishing generated wiki pages | _(empty)_ | Yes (wiki publish) |
| `WIKI_TOKEN` | Gitea Actions secret or token paired with `WIKI_USERNAME` for write access to the repository wiki remote | _(empty)_ | Yes (wiki publish) |

## Static Publishing Hooks

These values are read only by a downloaded static-site ZIP's deployment helper. They do not configure the Starlane server, are not embedded in the export, and should be supplied for one operator invocation rather than committed to an `.env` file.

| Variable | Description | Default | Required |
|---|---|---|---|
| `STARLANE_STATIC_DEPLOY_TARGET` | SSH rsync destination for `deploy-rsync.sh`, in the form `user@example.com:/absolute/publish-path`. Authentication and host verification must come from the invoking machine's SSH agent or SSH configuration. | _(empty)_ | Yes (rsync hook only) |

## Test and Development Fixtures

| Variable | Description | Default | Required |
|---|---|---|---|
| `STARLANE_SYNC_REPLAY_POSTGRES_URL` | Optional disposable PostgreSQL URL for the sync replay integration fixture; tests skip this fixture when unset | _(empty)_ | No |
