# SCIM Provisioning

Starlane Notes exposes a narrow SCIM 2.0 provisioning boundary for User lifecycle, Group creation, and directory discovery.

## Configure the bearer token

```
GET /scim/v2/ServiceProviderConfig
```

Set a dedicated server-side bearer token before calling the endpoint:

```bash
SCIM_BEARER_TOKEN=<long-random-secret>
```

Send it with every request:

```http
Authorization: Bearer <SCIM_BEARER_TOKEN>
```

When no token is configured, the endpoint returns a SCIM `404` error and does not expose provisioning metadata. Invalid or missing bearer tokens return a SCIM `401` error.

### Token operations

- Generate a long random value for `SCIM_BEARER_TOKEN`; do not reuse a user password, JWT secret, OAuth client secret, or personal API key.
- Store the value only in server deployment configuration or the secret store used by the deployment. The Admin UI does not currently display, rotate, or revoke SCIM bearer tokens.
- Rotate the token by updating `SCIM_BEARER_TOKEN` on the server, restarting/redeploying the service, and then updating the IdP SCIM connector before the old token is used again.
- Treat token rotation as a short maintenance window unless the deployment can run old and new app instances separately behind the load balancer. The current implementation accepts exactly one configured token at a time.
- Use HTTPS in front of the server for every SCIM request. The token is a bearer secret and should not be sent over plain HTTP outside local development.

## Discovery

All discovery endpoints require the configured SCIM bearer token:

| Endpoint | Purpose |
| --- | --- |
| `GET /scim/v2/ServiceProviderConfig` | Supported SCIM capabilities and pagination limit. |
| `GET /scim/v2/ResourceTypes` | User and Group resource types. |
| `GET /scim/v2/ResourceTypes/{User\|Group}` | One resource-type contract. |
| `GET /scim/v2/Schemas` | User and Group schemas. |
| `GET /scim/v2/Schemas/{schema-id}` | One schema contract. |

The generated API reference also includes this surface at `/api/docs`, with a dedicated SCIM bearer security scheme and `/scim/v2` path base.

## IdP connector setup

Use the same base URL that users use to reach the server, then append `/scim/v2` as the SCIM base URL:

```text
https://notes.example.com/scim/v2
```

Configure the IdP connector with bearer-token authentication and the exact `SCIM_BEARER_TOKEN` value. Validate the connector with:

1. `GET /ServiceProviderConfig`
2. `GET /ResourceTypes`
3. `GET /Schemas`
4. `GET /Users?filter=userName eq "person@example.com"`

Recommended IdP attribute mapping:

| SCIM attribute | Starlane Notes behavior |
| --- | --- |
| `userName` | Required email identity; normalized to lowercase for create/replace/filter paths. |
| `displayName` | Optional user display name; falls back to `userName` when omitted. |
| `externalId` | Optional immutable IdP correlation ID for Users and Groups. |
| `active` | Optional user lifecycle state; `false` deactivates the account without deleting content. |
| Group `displayName` | Required group name for SCIM-managed groups. |
| Group `members[].value` | Existing Starlane user IDs, not email addresses. |

Provider-specific setup screens vary, but keep the connector to the supported SCIM surface: Users, Groups, single-resource lookup, `userName`/`externalId` filters, replace-style lifecycle updates, and full Group membership replacement.

## User provisioning and lifecycle

`GET /scim/v2/Users` lists core users and `GET /scim/v2/Users/{id}` retrieves one user by Starlane Notes ID. User responses contain `id`, `userName`, `displayName`, `externalId`, `active`, primary email, and SCIM metadata. A missing user receives a SCIM `404` error response. The generated OpenAPI/Scalar reference documents the `{user_id}` path parameter for User item read, replace, patch, and deprovision routes.

The collection supports IdP lookup filters `filter=userName eq "person@example.com"` (case-insensitive) and `filter=externalId eq "idp-user-123"`, plus SCIM `startIndex`/`count` pagination. `startIndex` is one-based, and `count` is capped at 100; unsupported filters or invalid pagination receive a SCIM `400` error response. The generated OpenAPI/Scalar reference documents this list response and the supported lookup filter forms.

`POST /scim/v2/Users` provisions a `member` account with a personal space. It accepts `userName` (required), optional `displayName`, optional immutable `externalId`, and optional `active` (default `true`). It returns `201 Created` with a `Location` header. Duplicate email or external ID returns SCIM `409`.

`PATCH /scim/v2/Users/{id}` supports one intentionally constrained `replace` operation for SCIM-provisioned users. It accepts `active` (boolean), `displayName` (non-empty string up to 100 characters), or `userName` (a valid email, normalized to lowercase):

```json
{
  "schemas": ["urn:ietf:params:scim:api:messages:2.0:PatchOp"],
  "Operations": [{ "op": "replace", "path": "active", "value": false }]
}
```

Each accepted operation records an audit event. Other PATCH paths and multiple operations are rejected with SCIM `400`; duplicate email identities return `409`. Starlane-created users without a SCIM `externalId` cannot be changed by this endpoint.

`PUT /scim/v2/Users/{id}` replaces the SCIM-provisioned user's `userName`, `displayName`, and `active` state, returning the updated SCIM resource. `externalId` remains immutable; if present in a replacement request, it must match the existing provisioned identity. Duplicate email identities return `409`.

`DELETE /scim/v2/Users/{id}` deprovisions a SCIM-provisioned user by deactivating their account. It does not delete documents, spaces, or other content, and it records an audit event. Repeating the request is safe: it leaves the account inactive and returns `204` while the provisioned user exists.

## Group provisioning and directory

`GET /scim/v2/Groups` lists existing Starlane groups, and `GET /scim/v2/Groups/{id}` returns one group. Group responses include `displayName`, optional description, and ordered member IDs/display names. The generated OpenAPI/Scalar reference documents the `{group_id}` path parameter for Group item read, replace, membership patch, and delete routes.

`POST /scim/v2/Groups` creates a SCIM-managed group and may assign existing Starlane user IDs as initial members. It accepts a required `displayName`, optional immutable `externalId`, optional `description`, and optional `members` values. Member values must be non-empty, unique, and refer to existing users. The request is atomic: an invalid member leaves no group or membership row behind. It returns `201 Created` with a `Location` header; duplicate display names or external IDs return SCIM `409`.

`PATCH /scim/v2/Groups/{id}` supports one constrained reconciliation operation: a single `replace` operation at the `members` path replaces the complete membership with validated existing-user IDs. The change is transactional, updates Group metadata, and records an audit event.

```json
{
  "schemas": ["urn:ietf:params:scim:api:messages:2.0:PatchOp"],
  "Operations": [{ "op": "replace", "path": "members", "value": [{ "value": "user-id" }] }]
}
```

`PUT /scim/v2/Groups/{id}` atomically replaces a SCIM-managed group's `displayName`, optional description, and complete membership list. If supplied, `externalId` must match the group's immutable correlation identity. `DELETE /scim/v2/Groups/{id}` removes only SCIM-managed groups and their memberships/linked group grants; locally administered groups are never mutable through the SCIM lifecycle routes. Both operations record audit events.

## Audit and sync review

Accepted SCIM mutations record audit events for user lifecycle/profile changes and group lifecycle/membership changes. The current closeout expectation is:

| Operation family | Audit event examples |
| --- | --- |
| User create/replace/profile update/lifecycle update/deprovision | `user.scim_*` |
| Group create/replace/delete/membership replace | `group.scim_*` |

Use Admin Audit Log filtering or exported audit records to review SCIM activity after an IdP sync run. A dedicated SCIM sync-history screen is not implemented yet; if an IdP reports a failed sync, correlate the IdP timestamp and target resource with Starlane audit events plus server logs.

## Current boundaries

- The SCIM bearer token is deployment configuration only. Admin Authentication does not yet manage or rotate it.
- User `externalId` is immutable through the SCIM API. SCIM `PUT` can replace `userName`, `displayName`, and `active` for provisioned users; granular PATCH is deliberately limited to one safe `replace` operation for `active`, `displayName`, or `userName`.
- SCIM Group PATCH is intentionally restricted to full membership replacement; use PUT for full Group replacement.
- Bulk SCIM, password management, sorting, ETags, multi-token rotation, provider-specific screenshots, and a dedicated sync-history UI remain future work.
