# Webhooks

Webhooks let you receive HTTP POST notifications when events occur in Starlane Notes. All webhook management endpoints require the `admin` role.

## Event Types

| Event | Trigger |
|---|---|
| `space.created` | A new space is created |
| `space.updated` | A space's settings are modified |
| `space.deleted` | A space is deleted |
| `document.created` | A new document is created |
| `document.updated` | A document's metadata (title, icon, folder) is modified |
| `document.content_updated` | A document's body content is saved |
| `document.deleted` | A document is deleted |
| `*` | Wildcard -- matches all events |

Webhooks can be scoped to a specific space (only receive events from that space) or global (receive events from all spaces).
Subscription matching is exact for event names from the table above. Use `*` to receive all events.

Admins can manage webhooks from **Admin -> Webhooks** in server mode. The admin surface can create webhooks, list configured endpoints, delete endpoints, inspect the 50 most recent delivery attempts for the selected webhook, and queue a redelivery for a recorded delivery attempt.

## Payload Format

Each delivery sends a JSON POST with these headers:

| Header | Description |
|---|---|
| `Content-Type` | `application/json` |
| `X-Starlane-Event` | The event type (e.g. `document.created`) |
| `X-Starlane-Delivery` | Unique delivery ID (UUID) |
| `X-Starlane-Signature` | `sha256=<hex>` HMAC signature (only if a secret is configured) |

### Example Payload

```json
{
  "document_id": "550e8400-e29b-41d4-a716-446655440000",
  "space_id": "660e8400-e29b-41d4-a716-446655440000",
  "title": "Meeting Notes",
  "created_by": "770e8400-e29b-41d4-a716-446655440000"
}
```

The payload fields vary by event type but always include the relevant resource IDs and the user who triggered the action.

## Signature Verification

If a webhook has a `secret` configured, every delivery includes an `X-Starlane-Signature` header containing an HMAC-SHA256 signature of the request body. Verify it on your server:

```python
import hmac, hashlib

def verify(payload_bytes, secret, signature_header):
    expected = "sha256=" + hmac.new(
        secret.encode(), payload_bytes, hashlib.sha256
    ).hexdigest()
    return hmac.compare_digest(expected, signature_header)
```

## Delivery and Retry

- Deliveries are dispatched asynchronously in background tasks.
- A delivery is considered successful if the target returns an HTTP 2xx status code.
- Failed deliveries are retried up to **3 attempts** with exponential backoff (2s, 4s, 8s).
- Each attempt is recorded with its response status, success flag, and attempt number.

## Management Endpoints

All endpoints require `admin` role authentication.

### List Webhooks

```
GET /api/v1/webhooks
```

### Create Webhook

```
POST /api/v1/webhooks
```

```json
{
  "name": "CI Notifier",
  "url": "https://ci.example.com/webhook",
  "secret": "optional-shared-secret",
  "events": ["document.created", "document.updated"],
  "space_id": null
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes | Display name for the webhook |
| `url` | string | yes | Target URL for POST delivery |
| `secret` | string | no | Shared secret for HMAC signatures |
| `events` | string[] | yes | Event types to subscribe to (or `["*"]`) |
| `space_id` | string | no | Scope to a specific space; `null` for global |

The secret is stored as a SHA-256 hash and cannot be retrieved after creation.

### Get Webhook

```
GET /api/v1/webhooks/{webhook_id}
```

### Update Webhook

```
PUT /api/v1/webhooks/{webhook_id}
```

Accepts the same body as create. Updates name, URL, and subscribed events.

### Delete Webhook

```
DELETE /api/v1/webhooks/{webhook_id}
```

### List Deliveries

```
GET /api/v1/webhooks/{webhook_id}/deliveries
```

Returns the 50 most recent deliveries, ordered newest first.

```json
[
  {
    "id": "delivery-uuid",
    "event_type": "document.created",
    "response_status": 200,
    "success": true,
    "attempt_number": 1,
    "delivered_at": "2026-03-26T12:00:00Z"
  }
]
```

### Redeliver

```
POST /api/v1/webhooks/{webhook_id}/deliveries/{delivery_id}/redeliver
```

Queues a replay of the recorded delivery payload against the webhook's current URL and secret.
The replay creates a new delivery record and follows the normal retry behavior. Returns
`202 Accepted` after the delivery record is found and queued.
