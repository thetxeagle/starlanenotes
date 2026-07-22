# CSV Import

Administrators can bulk-create user accounts by uploading a CSV file. Imported users are created without passwords -- they must authenticate via OAuth, OIDC, or a password reset flow.

## CSV Format

The CSV file must have a header row with the following columns:

| Column | Required | Description |
|---|---|---|
| `email` | yes | User's email address |
| `display_name` | yes | User's display name |
| `role` | no | `admin`, `member` (default), or `viewer` |
| `spaces` | no | Comma-separated space slugs to auto-add the user to |

### Example CSV

```csv
email,display_name,role,spaces
jane@example.com,Jane Doe,member,"engineering,design"
bob@example.com,Bob Smith,viewer,engineering
alice@example.com,Alice Zhang,admin,
```

Users added to spaces via CSV import receive the `editor` role in those spaces.

## Download Template

```
GET /api/v1/admin/users/import/template
```

Returns a downloadable CSV template file with example rows. The response has `Content-Type: text/csv` and a `Content-Disposition` header for browser download.

## Preview Import

```
POST /api/v1/admin/users/import/preview
```

Upload a CSV to preview the import results without making any changes.

**Request:** `multipart/form-data` with a `file` field containing the CSV.

**Response:**

```json
{
  "total": 3,
  "new_users": [
    {
      "email": "jane@example.com",
      "display_name": "Jane Doe",
      "role": "member",
      "spaces": ["engineering", "design"]
    }
  ],
  "skipped": [
    {
      "email": "bob@example.com",
      "reason": "User already exists"
    }
  ],
  "errors": [
    {
      "line": 4,
      "error": "Missing email"
    }
  ]
}
```

| Field | Description |
|---|---|
| `new_users` | Users that will be created |
| `skipped` | Users that already exist (matched by email) |
| `errors` | Rows with validation failures (line numbers are 1-indexed, accounting for the header) |

## Execute Import

```
POST /api/v1/admin/users/import
```

**Request:** Same as preview -- `multipart/form-data` with a `file` field.

**Response:**

```json
{
  "created": 2,
  "skipped": 1,
  "errors": []
}
```

### Behavior

- Existing users (matched by email) are silently skipped.
- Invalid roles (anything other than `admin`, `member`, `viewer`) are reported as errors and the row is skipped.
- Space slugs that do not match an existing space are silently ignored.
- Empty lines in the CSV are skipped.
- The CSV must be UTF-8 encoded.

## Requirements

- All import endpoints require the `admin` role.
- The CSV file must have at least the `email` and `display_name` header columns.
