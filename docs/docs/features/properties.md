# Custom Properties

Custom properties add structured metadata to documents within a space. Properties are defined at the space level (schema) and values are set per document.

The document Properties panel supports first-pass management for both server-linked and standalone local spaces:

- Create text, number, date, date-time, checkbox, and URL properties for the current space.
- Create select and multi-select properties with comma-separated option lists.
- Create relation properties and pick another document in the same space as the value.
- Create user properties and pick an active user as the value.
- Rename, retarget, or delete custom properties from the current space.
- Reorder custom properties for the current space.
- Edit property values on the selected document.
- Create tags for the current space.
- Rename, recolor, or delete tags from the current space.
- Add existing tags to the selected document.
- Remove tags from the selected document.

Search supports first-pass tag and property filters in server-linked and local standalone modes.

## Property Types

| Type | Description |
|---|---|
| `text` | Single-line text |
| `long_text` | Multi-line text |
| `number` | Numeric value |
| `date` | Date (YYYY-MM-DD) |
| `datetime` | Date and time |
| `checkbox` | Boolean (true/false) |
| `select` | Single choice from predefined options |
| `multi_select` | Multiple choices from predefined options |
| `url` | URL/link |
| `user` | Reference to a user |
| `relation` | Reference to another document |

## Property Schema (Space-Level)

Property schemas are defined per space and apply to all documents within that space.

### List Property Schemas

```
GET /api/v1/spaces/{space_id}/properties
```

Returns all property schemas for the space, sorted by `sort_order`.

```json
[
  {
    "id": "prop-uuid",
    "name": "Status",
    "property_type": "select",
    "options": ["Draft", "In Review", "Published"],
    "sort_order": 1
  }
]
```

### Create Property Schema

```
POST /api/v1/spaces/{space_id}/properties
```

```json
{
  "name": "Priority",
  "property_type": "select",
  "options": ["Low", "Medium", "High", "Critical"]
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes | Display name |
| `property_type` | string | yes | One of the types listed above |
| `options` | JSON | no | Configuration for select/multi_select types |

Requires `owner` or `editor` role. New properties are appended to the end of the sort order.

### Update Property Schema

```
PUT /api/v1/spaces/{space_id}/properties/{property_id}
```

Accepts the same body as create. Requires `owner` or `editor` role.

### Reorder Property Schemas

```
PUT /api/v1/spaces/{space_id}/properties/reorder
```

```json
{
  "property_ids": ["prop-uuid-2", "prop-uuid"]
}
```

Requires `owner` or `editor` role. The submitted order becomes the space property display order.

### Delete Property Schema

```
DELETE /api/v1/spaces/{space_id}/properties/{property_id}
```

Requires `owner` role. Deleting a schema removes all associated values from documents.

## Property Values (Document-Level)

### List Property Values

```
GET /api/v1/spaces/{space_id}/documents/{doc_id}/properties
```

Returns all property schemas for the space with the document's current values. Properties without a set value return `null`.

```json
[
  {
    "property_id": "prop-uuid",
    "property_name": "Status",
    "property_type": "select",
    "options": ["Draft", "In Review", "Published"],
    "value": "In Review"
  },
  {
    "property_id": "prop-uuid-2",
    "property_name": "Due Date",
    "property_type": "date",
    "value": null
  }
]
```

### Set Property Values

```
PUT /api/v1/spaces/{space_id}/documents/{doc_id}/properties
```

```json
{
  "values": [
    { "property_id": "prop-uuid", "value": "Published" },
    { "property_id": "prop-uuid-2", "value": "2026-04-01" }
  ]
}
```

Requires `owner` or `editor` role. Uses upsert logic -- existing values are updated, new values are inserted. Set `value` to `null` to clear a property.

## Backlinks

```
GET /api/v1/spaces/{space_id}/documents/{doc_id}/backlinks
```

Returns documents in the current readable space that reference the given document via `[[wiki-style]]` or document-ID links. Owners, editors, and viewers can read backlinks; results are deliberately space-scoped so a backlink cannot expose documents from another space. The document right panel exposes the same list in server and standalone local-vault modes.

The OpenAPI reference documents this response as the `Backlink` schema.

```json
[
  {
    "document_id": "linking-doc-uuid",
    "title": "Architecture Overview",
    "space_id": "space-uuid",
    "space_name": "Engineering"
  }
]
```
