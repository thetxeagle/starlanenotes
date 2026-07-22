# Notifications

Notification endpoints expose the current user's inbox, read state, and delivery preferences. All notification endpoints require user authentication.

The authenticated web, desktop, and mobile apps also record their system confirmations and errors in this inbox. Those records immediately show a temporary in-app banner, but dismissing or waiting out a banner never clears its inbox entry.

## List Notifications

```http
GET /api/v1/notifications
```

Returns the current user's notifications in descending creation order.

### Query Parameters

| Parameter | Type | Default | Limit | Description |
|---|---|---:|---|---|
| `limit` | integer | `50` | `1` to `100` | Page size. Values outside the range are clamped by the server. |
| `before` | string | none | n/a | Cursor from the final item of the previous page, formatted as `created_at|id`. |
| `unread_only` | boolean | `false` | n/a | When `true`, returns only unread notifications. |

Use the final item in a response to build the next `before` cursor. For example, if the last item has `created_at` of `2026-07-14T12:00:00Z` and `id` of `notification-1`, request:

```http
GET /api/v1/notifications?before=2026-07-14T12%3A00%3A00Z%7Cnotification-1
```

### Response

```json
[
  {
    "id": "notification-uuid",
    "notification_type": "comment",
    "title": "New comment",
    "body": "A teammate commented on your document.",
    "source_user": "Pat",
    "target_type": "document",
    "target_id": "document-uuid",
    "read": false,
    "created_at": "2026-07-14T12:00:00Z"
  }
]
```

## Mark One Notification Read

```http
POST /api/v1/notifications/{notification_id}/read
```

Marks one current-user notification as read. Returns `204 No Content` on success. Returns `404 Not Found` if the notification does not exist or does not belong to the authenticated user.

## Record System Activity

```http
POST /api/v1/notifications/system
```

The authenticated app uses this endpoint to retain user-visible system activity in the inbox. It accepts a `title` (1–160 characters) and optional `body` (up to 1000 characters), creates an unread `system` notification, and returns it with `201 Created`. System activity defaults to in-app delivery only, so it does not send confirmation emails unless the user explicitly changes that preference.

## Mark All Notifications Read

```http
POST /api/v1/notifications/read-all
```

Marks all unread notifications for the authenticated user as read. This operation is idempotent and returns `204 No Content`.

## Notification Preferences

```http
GET /api/v1/notifications/preferences
PUT /api/v1/notifications/preferences
```

Preferences control whether each supported notification type is delivered in-app, by email, both, or not at all.

### Supported Channels

| Channel | Delivery behavior |
|---|---|
| `both` | Inbox and email |
| `in_app` | Inbox only |
| `email` | Email only |
| `none` | Suppress delivery |

### Update Preferences

```json
{
  "preferences": [
    {
      "notification_type": "comment",
      "channel": "both"
    },
    {
      "notification_type": "doc_shared",
      "channel": "in_app"
    }
  ]
}
```

The server validates notification types and channels against the generated OpenAPI enums.
