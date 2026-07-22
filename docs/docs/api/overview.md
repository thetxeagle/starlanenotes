# API Overview

Starlane Notes exposes a REST API under the `/api/v1` prefix. All endpoints return JSON and accept JSON request bodies unless otherwise noted.

## Base URL

```
http://localhost:3000/api/v1
```

Set `BASE_URL` in your environment to change the canonical base URL (used for CORS, OAuth callbacks, and public page links).

## Interactive Reference

The server exposes a first-pass OpenAPI 3.1 reference for the current route surface:

| Endpoint | Description |
|---|---|
| `GET /api/docs` | Scalar API reference |
| `GET /api/openapi.json` | OpenAPI JSON document |
| `GET /api/openapi.yaml` | OpenAPI YAML document |
| `GET /api/v1/docs` | Versioned Scalar API reference |
| `GET /api/v1/openapi.json` | Versioned OpenAPI JSON document |
| `GET /api/v1/openapi.yaml` | Versioned OpenAPI YAML document |

Scalar is served by the Rust API server itself through `utoipa-scalar`; it does **not** require a separate Docker container for local development. Run the server and open `/api/docs` or `/api/v1/docs` to view the interactive reference. If Starlane Notes is deployed in Docker, the same routes are available through the containerized server. The optional [Starlight Docs Portal](../self-hosting/docs-portal.md) is a separate static guide/wiki container; it complements Scalar and does not replace it.

The Gitea Wiki and `docs/docs/` Markdown remain the narrative documentation source for setup guides, feature notes, and roadmap context. Scalar should point at the generated OpenAPI document (`/api/openapi.json`, `/api/v1/openapi.json`, or the YAML equivalents) rather than trying to convert wiki pages into API operations. When a wiki page describes API behavior, link back to the Scalar reference for the executable contract and keep the Markdown page focused on workflow and explanation.

The first-pass spec prioritizes route discoverability, tags, auth requirements, and shared error contracts. Detailed request and response schemas are expanding incrementally; health, SCIM Users list/filter and User/Group item route coverage, public auth config, auth token exchange/refresh, password reset request/confirm, OAuth/OIDC redirects and OAuth provider route coverage, current-user profile, TOTP 2FA, passkey registration/login/management, auth session/API-key route coverage, user-directory route coverage, admin-user create/lifecycle/list/session and CSV import/template route coverage, admin group create/member route coverage, admin config list/upsert/delete and search reindex route coverage, audit CSV export and plugin runtime audit-event route coverage, webhook management/delivery/redelivery route coverage, notification preferences and read-state route coverage including supported type/channel enums, MCP manifest/read-tool execution, AI config/chat/completion/document-action, core document list/read/update/content/starred/lock/publish/comment/resolve/reaction/version restore/attachment list-upload-delete/workflow/queue/deadline-escalation route coverage plus create/response/parent placement schemas, public-page slug route coverage, space/folder/folder-reorder/graph/direct-member/group-grant route coverage, template/workflow-template route coverage, metadata tag/property/document-property/backlink route coverage, and document lifecycle no-content response route families now include concrete schemas, explicit success responses, or route-level annotations.

## Authentication

Most endpoints require a valid JWT access token passed via the `Authorization` header:

```
Authorization: Bearer <access_token>
```

API keys can also authenticate requests with `X-API-Key: <key>`. Personal keys inherit the creator's role. Admin-managed service account keys authenticate as admin service principals and must include the `admin` scope. Scoped keys can be limited to read, write, or admin API access.

See [Authentication](authentication.md) for details on obtaining and refreshing tokens.

### Public Endpoints

The following endpoints do not require authentication:

| Endpoint | Description |
|---|---|
| `GET /api/v1/auth/config` | Auth provider configuration |
| `POST /api/v1/auth/register` | User registration |
| `POST /api/v1/auth/login` | User login |
| `POST /api/v1/auth/passkeys/login/start` | Start passkey login |
| `POST /api/v1/auth/passkeys/login/finish` | Finish passkey login |
| `POST /api/v1/auth/refresh` | Token refresh |
| `GET /api/docs` | Scalar API reference |
| `GET /api/openapi.json` | OpenAPI JSON document |
| `GET /api/openapi.yaml` | OpenAPI YAML document |
| `GET /api/v1/p/{slug}` | Public page content |
| `GET /api/v1/auth/oauth/{provider}` | OAuth redirect |
| `GET /api/v1/auth/oidc` | OIDC redirect |

## Response Format

Successful responses return the resource directly as JSON. Error responses use a consistent structure:

```json
{
  "error": "bad_request",
  "message": "Human-readable error description"
}
```

### Error Codes

| HTTP Status | `error` Value | Meaning |
|---|---|---|
| 400 | `bad_request` | Invalid input or validation failure |
| 401 | `unauthorized` | Missing, expired, or invalid token |
| 403 | `forbidden` | Authenticated principal lacks the required role or API-key scope |
| 403 | `2fa_required` | Login succeeded but a TOTP code is required |
| 404 | `not_found` | Requested resource was not found |
| 409 | `conflict` | Resource already exists or is locked |
| 429 | `too_many_requests` | Authentication rate limit exceeded |
| 500 | `internal_error` | Server-side failure |

## Rate Limiting

Authentication endpoints (`/auth/register`, `/auth/login`, `/auth/refresh`, `/auth/password-reset/request`, `/auth/password-reset/confirm`, `/auth/2fa/totp/verify`, `/auth/passkeys/login/start`, `/auth/passkeys/login/finish`) are rate-limited per IP address:

- **Burst**: 10 requests
- **Refill**: 1 request every 6 seconds

Exceeding the limit returns `429 Too Many Requests`.

## CORS

- **Development** (`STARLANE_ENV` unset or not `production`): all origins allowed.
- **Production** (`STARLANE_ENV=production`): the origin matching `BASE_URL`, built-in Tauri native-app origins, and comma-separated origins in `STARLANE_CORS_ORIGINS` are allowed. Wildcard `*` is rejected in production. Allowed methods: GET, POST, PUT, DELETE, OPTIONS. Allowed headers: Authorization, Content-Type, Accept.

## Pagination and Limits

- Search results default to 20 and Dataview results default to 50; both cap at 100 results via the `?limit=` query parameter. Search supports first-pass title, tag, folder, property, attachment, cover, workflow, metadata-presence, and lock-state filters; presence and lock filters accept both `true` and `false`. Dataview accepts `GET /api/v1/dataview?q=...` and returns matching readable documents as search-result records.
- Notification inbox results default to 50, maximum 100. Use `before=created_at|id` for cursor pagination and `unread_only=true` for unread-only pages.
- Webhook delivery history returns the most recent 50 entries.
- Document content is capped at 5 MB.
- Comment body is capped at 10,000 characters.
- `POST /api/v1/notifications/{notification_id}/read` returns `404` when that notification is not owned by the current user; `POST /api/v1/notifications/read-all` remains an idempotent no-content action.
- Notification preference updates accept only the current notification types and the `both`, `in_app`, `email`, or `none` channels.
- Notification channels map to delivery targets as follows: `both` sends inbox and email, `in_app` sends inbox only, `email` sends email only, and `none` suppresses delivery.

## API Sections

| Section | Prefix | Description |
|---|---|---|
| [Authentication](authentication.md) | `/auth/*`, `/admin/api-keys` | Login, registration, passkeys, OAuth, OIDC, 2FA, sessions, personal API keys, and admin-managed service account keys |
| [Spaces](../features/spaces.md) | `/spaces/*` | Space CRUD, members, group grants, permissions |
| Documents | `/spaces/{id}/documents/*` | Document CRUD, content, locking, publishing |
| Bookmark preview | `/bookmarks/metadata` | Authenticated, on-demand Open Graph/title snapshot lookup for bookmark cards; private-network targets and redirects are blocked. |
| Templates | `/spaces/{id}/templates/*` | Space-level reusable document templates |
| Workflow Templates | `/spaces/{id}/workflow-templates/*` | Space-level approval workflow template definitions |
| Document Workflows | `/spaces/{id}/documents/{id}/workflow*` | Activate, inspect, transition, list history, and remove a document workflow instance |
| Workflow Maintenance | `/admin/workflows/deadline-escalations` | Admin scan and schedulable escalation run for overdue delegated workflow candidates |
| Folders | `/spaces/{id}/folders/*` | Folder hierarchy within spaces |
| Comments | `/spaces/{id}/documents/{id}/comments/*` | Threaded comments and reactions |
| Properties | `/spaces/{id}/properties/*` | Custom property schemas and values |
| Search | `/search`, `/dataview` | Full-text search via tantivy and first-pass Dataview query execution |
| Graph | `/spaces/{id}/graph` | Space graph data |
| [Webhooks](webhooks.md) | `/webhooks/*` | Webhook management and delivery history |
| [Notifications](notifications.md) | `/notifications/*` | Current-user notification inbox, read state, and delivery preferences |
| AI | `/ai/*` | AI chat, completion, and document actions |
| [SCIM Provisioning](../authentication/scim.md) | `/scim/v2/*` | Bearer-token-protected User and Group provisioning, discovery, and lifecycle operations |
| [MCP Server Plan](mcp.md) | `/mcp/manifest`, `/mcp/tools/{tool_name}` | Authenticated MCP manifest plus conservative read-only HTTP tool execution, planned transports, scopes, safety boundaries, and implementation order |
| [Plugin System](../plugins/overview.md) | `/admin/plugin-runtime-audit` | First-pass plugin manifest contract plus server-backed audit logging for blocked runtime attempts; runtime execution remains planned |
| [Users, Groups, Permissions](../features/users-groups-permissions.md) | `/users/directory`, `/admin/users`, `/admin/groups`, `/spaces/{id}/members`, `/spaces/{id}/groups` | User directory, admin user listing, reusable groups, and direct/group space permissions |
| Admin | `/admin/*` | User management, site configuration, audit log/export, blocked plugin runtime audit logging, search reindex, CSV import |

## Known API Gaps

- The OpenAPI reference still favors route discoverability for some endpoints; detailed request and response schemas are being added route family by route family.
- Some future roadmap surfaces such as full publishing, plugin runtime execution, guest access, richer imports, SCIM sync-history/admin token-management UI, and non-read-only MCP transports/tools are not implemented yet.
