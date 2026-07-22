# Users, Groups, and Permissions

Starlane Notes has two permission layers:

- **System role**: controls instance-wide administration.
- **Space role**: controls access to a specific space.

## System Roles

| Role | Access |
|---|---|
| `admin` | Site administration, user list, groups, global configuration, webhook management, audit log, search reindex, and all space access checks |
| `member` | Normal authenticated user access |
| `viewer` | Limited authenticated user role used by provisioning flows |

The first registered user becomes an `admin`. Later users default to `member` unless a provisioning flow assigns a different role.

On a server-connected desktop or mobile app, administrators can open **Site administration** from the app settings/menu. The mobile entry opens the same iOS-settings-style administration home and subpages used by the web interface. Phone-width browsers use that same mobile shell and navigation model, so administration remains usable without installing the native app. It is intentionally hidden in local standalone mode and for non-admin accounts.

## User Administration

Admins can list users through:

```
GET /api/v1/admin/users
```

Admins can create password-backed users through:

```
POST /api/v1/admin/users
```

```json
{
  "email": "new-user@example.com",
  "display_name": "New User",
  "role": "member"
}
```

The response includes a one-time `temporary_password`. Copy it before leaving the Admin Users screen.

The Admin Users screen can also import a CSV with `email`, `display_name` or `name`, and optional `role` columns. Rows without a role default to `member`; valid roles are `member` and `admin`. The list includes lifecycle summary counts plus email/name search, role filters, and active-status filters for larger teams.

Admin invite creation and server CSV import/preview share the same provisioning policy. By default the policy is permissive. Operators can restrict new user email domains with `USER_PROVISIONING_ALLOWED_EMAIL_DOMAINS` and restrict provisionable roles with `USER_PROVISIONING_ALLOWED_ROLES`; environment-backed values appear locked in Admin Configuration.

Admins can reset an active user's password from the Admin Users screen. The reset action generates
a one-time temporary password, revokes that user's active refresh sessions, and displays the
temporary password once for copy-out.

Admins can also select users in the Admin Users screen and bulk activate or deactivate them. The same capability is available through the server API:

```
POST /api/v1/admin/users/bulk-lifecycle
```

```json
{
  "user_ids": ["user-id-1", "user-id-2"],
  "action": "deactivate"
}
```

The bulk lifecycle endpoint deduplicates targets, reports skipped rows with reasons, blocks self-deactivation, and preserves the last-active-admin safeguard across the batch. It updates lifecycle state only; role changes still use the single-user lifecycle route.

SSO/OIDC provisioning is documented in [SSO Provisioning](../user-provisioning/sso-provisioning.md).

Signed-out users can request a self-service password reset from `/reset-password` when SMTP is configured. Reset links are one-time tokens that expire after 30 minutes; confirming a reset updates the password and revokes existing refresh sessions.

Admins can inspect and revoke a user's refresh sessions from the Admin Users screen:

```
GET    /api/v1/admin/users/{user_id}/sessions
DELETE /api/v1/admin/users/{user_id}/sessions/{session_id}
```

Current user administration covers lifecycle summaries, list filtering, invite-style creation, CSV import, role changes, single-user deactivate/reactivate controls, selected-user bulk activate/deactivate workflows, admin password reset actions, self-service password reset initiation/confirmation, shared invite/import provisioning policy enforcement, and session revocation with self-demotion and last-active-admin safeguards. Signed-in users can update their display name and avatar URL from Profile settings. SCIM now supports User provisioning, external-ID correlation, active-state lifecycle updates, and SCIM-managed Group create/replace/membership-replace/delete operations for existing Starlane user IDs; Admin UI token management and dedicated SCIM sync-history review remain app-readiness gaps.

## User Directory

Signed-in users can list a narrow active-user directory for user pickers and mentions:

```
GET /api/v1/users/directory
```

The response includes `id`, `email`, `display_name`, and `avatar_url` for active users. It does not expose role, lifecycle, or administration fields.

## Groups

Groups are reusable collections of users that can be granted space permissions.

### List Groups

```
GET /api/v1/admin/groups
```

### Create Group

```
POST /api/v1/admin/groups
```

```json
{
  "name": "Engineering",
  "description": "Product and platform engineers"
}
```

### Group Members

```
GET    /api/v1/admin/groups/{group_id}/members
POST   /api/v1/admin/groups/{group_id}/members
DELETE /api/v1/admin/groups/{group_id}/members/{user_id}
```

Adding a member accepts:

```json
{
  "user_id": "user-uuid"
}
```

Group management requires an `admin` user.

## Space Roles

Spaces support direct user membership and group grants. Both use the same role names.

| Role | Permissions |
|---|---|
| `owner` | Full space control, including visibility, members, group grants, and destructive actions |
| `editor` | Create, edit, and delete documents/folders; can add direct members |
| `viewer` | Read-only access |

System admins bypass space role checks.

Current permissions are space-scoped. Folder-level overrides, a dedicated commenter role, guest links, and before/after audit diffs for permission changes remain roadmap work; do not model external IdP or self-hosted setups as if those narrower permission layers already exist.

## Direct User Grants

```
GET    /api/v1/spaces/{space_id}/members
POST   /api/v1/spaces/{space_id}/members
PUT    /api/v1/spaces/{space_id}/members/{user_id}
DELETE /api/v1/spaces/{space_id}/members/{user_id}
```

Listing direct members requires an `owner`, `editor`, or `viewer` grant for that same space. Membership mutations retain their existing role requirements.

Adding or updating a member uses:

```json
{
  "user_id": "user-uuid",
  "role": "editor"
}
```

## Group Grants

```
GET    /api/v1/spaces/{space_id}/groups
POST   /api/v1/spaces/{space_id}/groups
PUT    /api/v1/spaces/{space_id}/groups/{group_id}
DELETE /api/v1/spaces/{space_id}/groups/{group_id}
```

Granting or updating a group uses:

```json
{
  "group_id": "group-uuid",
  "role": "viewer"
}
```

When a user has both direct and group access, direct membership is reported first in space listings. Space access checks also consider group grants and allow the strongest matching group role.

## Local Mode

In local standalone mode, server-admin actions are unavailable. The UI keeps storage and sync-provider controls available, but users, groups, audit-log, and server configuration controls are disabled or informational.

## Templates Status

Document template CRUD is available at `/api/v1/spaces/{space_id}/templates` and uses normal space roles:

- `owner`, `editor`, and `viewer` can list and read templates.
- `owner` and `editor` can create, update, and delete templates.

The desktop sidebar and mobile new-document flows can create documents from templates when templates exist. The editor command palette can insert template content at the cursor. Daily-note templates are supported through reserved names or a `daily` tag. Workflow template CRUD and document workflow activation are available through the workflow-template and document-workflow APIs; full template package import/export and advanced automation remain roadmap work.
