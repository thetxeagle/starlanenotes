# Spaces

Spaces are the top-level organizational unit in Starlane Notes. Each space contains folders and documents, has its own member list, can receive group permission grants, and can be configured independently.

## Creating a Space

```
POST /api/v1/spaces
```

```json
{
  "name": "Engineering",
  "description": "Internal engineering docs",
  "icon": "rocket",
  "accent_color": "#58a6ff",
  "visibility": "private"
}
```

| Field | Type | Required | Constraints |
|---|---|---|---|
| `name` | string | yes | 1-100 characters, must produce a valid slug |
| `description` | string | no | Max 1000 characters |
| `icon` | string | no | Icon identifier |
| `accent_color` | string | no | Hex color code |
| `visibility` | string | no | `private` (default), `internal`, or `public` |

A URL-safe `slug` is auto-generated from the name. Slug must be unique across all spaces. The creating user is automatically added as the `owner`.

Space `icon` and `accent_color` values are stored as metadata and returned through space APIs. Current web and mobile navigation surfaces render the stored space icon or fall back to the space initial. A full icon-library picker and per-icon color customization remain roadmap work.

## Visibility Levels

| Level | Behavior |
|---|---|
| `private` | Only members can see or access the space |
| `internal` | All authenticated users can see the space in listings |
| `public` | Visible to all authenticated users |

Regardless of visibility, write access (creating documents, editing) requires explicit membership with an appropriate role.

## Listing Spaces

```
GET /api/v1/spaces
```

Returns spaces the authenticated user is a member of, can access through a group grant, plus any spaces with `internal` or `public` visibility. Each space includes the user's `member_role` when a direct or group role is available.

## Getting a Space

```
GET /api/v1/spaces/{space_id}
```

## Updating a Space

```
PUT /api/v1/spaces/{space_id}
```

Requires `owner` role. All fields are optional -- only provided fields are updated.

## Deleting a Space

```
DELETE /api/v1/spaces/{space_id}
```

Requires `owner` role. Deletes the space, all its members, folders, documents, and associated files from disk. This action is irreversible.

## Members

### Roles

| Role | Permissions |
|---|---|
| `owner` | Full control: manage members, update/delete space, all editor permissions |
| `editor` | Create, edit, and delete documents and folders; add members |
| `viewer` | Read-only access to space content |

System administrators (`admin` role on the user) bypass space role checks and have full access to all spaces.

For the full user, group, and permission model, see [Users, Groups, and Permissions](users-groups-permissions.md).

### List Members

```
GET /api/v1/spaces/{space_id}/members
```

Returns all members with their user ID, email, display name, avatar URL, role, and join date. Sorted by role then display name.

### Add Member

```
POST /api/v1/spaces/{space_id}/members
```

```json
{
  "user_id": "user-uuid",
  "role": "editor"
}
```

Requires `owner` or `editor` role. The `role` field defaults to `editor` if omitted.

### Update Member Role

```
PUT /api/v1/spaces/{space_id}/members/{user_id}
```

```json
{
  "user_id": "user-uuid",
  "role": "viewer"
}
```

Requires `owner` role.

### Remove Member

```
DELETE /api/v1/spaces/{space_id}/members/{user_id}
```

Requires `owner` role.

## Audit Trail

All space operations (create, update, delete, member changes, and group grant changes) are recorded in the audit log and can trigger [webhooks](../api/webhooks.md).
