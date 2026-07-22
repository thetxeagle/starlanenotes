# Documents

Documents are the core content unit in Starlane Notes. Each document belongs to a space, optionally inside a folder, and stores its content as a file on disk with metadata in the database.

Documents can also be nested as child pages. Child pages render in the sidebar beneath their parent page and use the same document content, metadata, permissions, comments, attachment, and version surfaces as top-level pages. Create and move dialogs use slash-style destinations such as `Personal Space / Folder / Parent Page` so child-page placement is explicit on desktop and mobile.

The editor chrome shows live document stats, including word count and estimated reading time, in both Markdown and Page View modes.

Desktop editing includes a Focus mode that collapses heavier page chrome and keeps the document title plus writing surface visible for quieter writing sessions.

The right panel includes Contents and Backlinks tabs. Contents lists Markdown headings for the current document; Backlinks lists same-space documents that link to it, with empty and retryable error states in both server and standalone local-vault modes.

In server-linked mode, the right panel also includes a History tab that lists saved document versions when version records exist. Editors and owners can use the version Restore action from this tab to create a new restore checkpoint from a selected version. Local standalone version history remains planned follow-up work.

## Creating a Document

```
POST /api/v1/spaces/{space_id}/documents
```

```json
{
  "title": "Meeting Notes",
  "folder_id": null,
  "icon": "file-text",
  "content": "# Meeting Notes\n\nAgenda items..."
}
```

| Field | Type | Required | Constraints |
|---|---|---|---|
| `title` | string | yes | Max 500 characters |
| `folder_id` | string | no | UUID of parent folder, or `null` for space root |
| `icon` | string | no | Icon identifier |
| `content` | string | no | Initial document body, max 5 MB |

The document is written to disk with YAML frontmatter (title, icon, locked status, public flag) followed by the content body. The creator is automatically subscribed to document notifications.

Document icons are stored as document metadata and rendered inline in document lists, starred/recent surfaces, mobile navigation, and page chrome when present. A full page-icon picker/library and icon color customization remain roadmap work.

## Listing Documents

```
GET /api/v1/spaces/{space_id}/documents?folder_id={folder_id}
```

Returns documents in a specific folder. If `folder_id` is omitted, returns documents at the space root (not in any folder). Results are sorted by `sort_order` then `title`.

## Getting a Document

```
GET /api/v1/spaces/{space_id}/documents/{doc_id}
```

Returns document metadata including lock status, public sharing state, and timestamps.

## Updating Document Metadata

```
PUT /api/v1/spaces/{space_id}/documents/{doc_id}
```

```json
{
  "title": "Updated Title",
  "folder_id": "new-folder-uuid",
  "icon": "bookmark",
  "cover_image": "/uploads/cover.jpg"
}
```

All fields are optional. Requires `owner` or `editor` role. Fails if the document is locked by another user.

## Content Operations

### Read Content

```
GET /api/v1/spaces/{space_id}/documents/{doc_id}/content
```

Returns the document body:

```json
{
  "id": "doc-uuid",
  "title": "Meeting Notes",
  "content": "# Meeting Notes\n\n..."
}
```

### Update Content

```
PUT /api/v1/spaces/{space_id}/documents/{doc_id}/content
```

```json
{
  "content": "Updated document body..."
}
```

Content is capped at 5 MB. Requires `owner` or `editor` role. Fails if the document is locked by another user. Triggers search re-indexing.

## Attachments

Server-linked documents support attachment listing, upload, and deletion at:

```
GET    /api/v1/spaces/{space_id}/documents/{doc_id}/attachments
POST   /api/v1/spaces/{space_id}/documents/{doc_id}/attachments
DELETE /api/v1/spaces/{space_id}/documents/{doc_id}/attachments/{attachment_id}
```

Readers can list attachments. Owners and editors can upload or delete them. Uploads require one multipart field named `file` with a non-blank filename of at most 255 bytes and must stay within the configured upload-size limit. Every route first verifies that the routed document belongs to the routed space; attachment deletion also requires that the attachment belongs to that same document and space. This prevents cross-space metadata exposure and cross-document deletion. Standalone vault attachments remain local assets until an explicit server migration/replay path is used.

## Editor

Bookmark cards use the portable Markdown form `[bookmark:Title](https://example.com "Optional description")`. The toolbar and slash command open a small form for the URL, title, and optional description; the editor renders a compact linked card with its domain tag and preserves the directive through autosave. In server mode, **Fetch preview** makes one explicit, authenticated server request and saves an Open Graph/title snapshot into the page; page viewing and offline use never refetch it. The server blocks localhost, private, link-local, and redirected targets, caps responses, and never forwards user credentials. Standalone vaults retain manual bookmark details.

When a table cell is active, the editor shows contextual table controls for adding or deleting rows and columns, or deleting the table. On mobile, the same controls open immediately above the keyboard-attached toolbar.

Document Markdown is deliberately not an HTML execution surface. Raw HTML is displayed as text; generated links are limited to `http`, `https`, `mailto`, and intentional `starlane` links; images are limited to approved HTTP(S), local `starlane`, or image-only data URLs. Scripts, event handlers, arbitrary CSS, iframes, and executable data URLs are not rendered.

Excalidraw canvas references use the portable Markdown form `![[excalidraw:name.excalidraw]]`. The editor slash command can insert a starter `untitled.excalidraw` canvas reference. Saved drawings render as a fitted, clickable page thumbnail; opening the thumbnail shows the embedded Excalidraw editor in edit mode and a read-only Excalidraw viewer in page view. When a document context is available, the full drawing remains a document-owned `.excalidraw` sidecar asset and the saved scene is also retained as a portable page-preview fallback, so the thumbnail still renders when the sidecar is temporarily unavailable. Older embedded `excalidraw-json` fenced blocks remain readable. Unsafe drawing names are normalized into portable sidecar filenames so pasted or imported directives cannot preserve path separators or hidden relative-path names.

The web frontend uses TipTap (ProseMirror-based) as the rich text editor. It supports:

- Headings (H1-H6)
- Bold, italic, strikethrough, inline code
- Bullet lists, ordered lists, task lists
- Blockquotes
- Code blocks with syntax highlighting
- Tables
- Images
- Links
- Horizontal rules
- Highlighted text (mark)

Content is stored as the editor's serialized format on disk.

## Frontmatter

Each document file on disk begins with YAML frontmatter:

```yaml
title: "Document Title"
icon: "file-text"
locked: false
public: false
```

The `.md` file on disk is always the canonical source. The database serves as an index and cache.

## Document Locking

Owners can lock a document to prevent edits by other users.

### Lock

```
POST /api/v1/spaces/{space_id}/documents/{doc_id}/lock
```

Requires `owner` role. Sets the document as locked and records who locked it.

### Unlock

```
POST /api/v1/spaces/{space_id}/documents/{doc_id}/unlock
```

Requires `owner` role.

While locked, any attempt to update the document metadata or content by a different user returns a `409 Conflict` error.

## Public Sharing

Documents can be published as public pages accessible without authentication.

### Publish

```
POST /api/v1/spaces/{space_id}/documents/{doc_id}/publish
```

Requires `owner` or `editor` role. Returns a `public_slug` (16-character random string). Public pages can be disabled instance-wide by setting `ALLOW_PUBLIC_PAGES=false`.

### Unpublish

```
POST /api/v1/spaces/{space_id}/documents/{doc_id}/unpublish
```

### Access Public Page

```
GET /api/v1/p/{slug}
```

No authentication required. Returns the document content along with space metadata and author information:

```json
{
  "title": "Meeting Notes",
  "icon": "file-text",
  "cover_image": null,
  "content": "...",
  "space_name": "Engineering",
  "space_accent_color": "#58a6ff",
  "author": "Jane Doe",
  "updated_at": "2026-03-26T12:00:00Z"
}
```

The bundled web app also renders the public reader at `/p/{slug}`. It stays outside the authenticated app shell, uses the same safe Markdown renderer as page view, and deliberately treats an unpublished slug and an unavailable public page the same way. This lets a self-hosted instance share a focused knowledge-base page without exposing workspace navigation or account controls.

## Deleting a Document

```
DELETE /api/v1/spaces/{space_id}/documents/{doc_id}
```

Requires `owner` or `editor` role. Removes the document from the database and deletes the file from disk. Also removes the document from the search index.

## Comments and Reactions

### List Comments

```
GET /api/v1/spaces/{space_id}/documents/{doc_id}/comments
```

Returns top-level comments with nested replies. Comments support text selections (anchored to character offsets in the document).

### Create Comment

```
POST /api/v1/spaces/{space_id}/documents/{doc_id}/comments
```

```json
{
  "content": "This section needs a citation.",
  "parent_id": null,
  "selection_start": 150,
  "selection_end": 200,
  "selection_text": "selected passage"
}
```

| Field | Constraints |
|---|---|
| `content` | Required, max 10,000 characters |
| `parent_id` | UUID of parent comment for replies, or `null` |
| `selection_start` / `selection_end` | Character offsets for inline comments |
| `selection_text` | The selected text at time of commenting |

Creating a comment auto-subscribes the commenter to document notifications.

### Update Comment

```
PUT /api/v1/spaces/{space_id}/documents/{doc_id}/comments/{comment_id}
```

Only the comment author can edit their own comments.

### Delete Comment

```
DELETE /api/v1/spaces/{space_id}/documents/{doc_id}/comments/{comment_id}
```

Only the comment author can delete their own comments.

### Resolve Comment

```
POST /api/v1/spaces/{space_id}/documents/{doc_id}/comments/{comment_id}/resolve
```

Marks a comment thread as resolved, recording who resolved it.

### Reactions

```
GET /api/v1/spaces/{space_id}/documents/{doc_id}/reactions
```

Returns reaction counts grouped by emoji, with a flag indicating whether the current user has reacted.

```
POST /api/v1/spaces/{space_id}/documents/{doc_id}/reactions
```

```json
{
  "emoji": "thumbsup"
}
```

Toggle behavior: if the user already reacted with this emoji, it is removed; otherwise it is added.

## Folders

Documents can be organized into nested folders within a space.

### List Folders

```
GET /api/v1/spaces/{space_id}/folders
```

Returns all folders in the space, sorted by `sort_order` then `name`.

### Create Folder

```
POST /api/v1/spaces/{space_id}/folders
```

```json
{
  "name": "Architecture",
  "parent_id": null,
  "icon": "folder"
}
```

| Field | Type | Required | Constraints |
|---|---|---|---|
| `name` | string | yes | 1-100 characters |
| `parent_id` | string | no | UUID of parent folder for nesting |
| `icon` | string | no | Icon identifier |

Requires `owner` or `editor` role.

### Get, Update, Delete Folder

```
GET    /api/v1/spaces/{space_id}/folders/{folder_id}
PUT    /api/v1/spaces/{space_id}/folders/{folder_id}
DELETE /api/v1/spaces/{space_id}/folders/{folder_id}
```

Update accepts optional `name`, `icon`, and `parent_id` (to move a folder). Delete cascades to child folders and documents. Both require `owner` or `editor` role.

Folder icons are stored as folder metadata and rendered inline in web/mobile navigation when present. Folder icon library selection and richer color customization remain roadmap work.

### Reorder Folder

```
PUT /api/v1/spaces/{space_id}/folders/{folder_id}/reorder
```

```json
{
  "sort_order": 3
}
```

Requires `owner` or `editor` role.

## Search Indexing

Documents are automatically indexed in the tantivy full-text search engine on creation, content update, and title change. Deleted documents are removed from the index. Admins can trigger a full reindex via `POST /api/v1/admin/search/reindex`.

### Search

```
GET /api/v1/search?q=term&space_id=optional&tag=optional&property_name=optional&property_value=optional&has_workflow=optional&has_tag=optional&has_property=optional&is_locked=optional&created_on=optional&updated_on=optional&limit=20
```

| Parameter | Type | Default | Description |
|---|---|---|---|
| `q` | string | optional when filters are present | Search query (max 500 characters) |
| `space_id` | string | all spaces | Limit search to a specific space |
| `tag` | string | none | Limit results to documents with an exact tag name match |
| `property_name` | string | none | Limit results to documents with a matching property name |
| `property_value` | string | none | Limit results to documents whose property value contains this text |
| `has_attachment` | boolean | none | Include only documents with (`true`) or without (`false`) attachments |
| `has_cover` | boolean | none | Include only documents with (`true`) or without (`false`) a cover image |
| `has_workflow` | boolean | none | Include only documents with (`true`) or without (`false`) a workflow instance |
| `has_tag` | boolean | none | Include only documents with (`true`) or without (`false`) at least one tag |
| `has_property` | boolean | none | Include only documents with (`true`) or without (`false`) at least one stored property value |
| `is_locked` | boolean | none | Include only currently locked (`true`) or unlocked (`false`) documents |
| `created_on` | date | none | Include only documents created on the exact `YYYY-MM-DD` date |
| `updated_on` | date | none | Include only documents updated on the exact `YYYY-MM-DD` date |
| `limit` | integer | 20 | Max results (capped at 100) |

Returns matching documents with title, space info, excerpt, author, and last updated timestamp.

The web and mobile search surfaces expose first-pass SNQL terms including `title:`, `tag:`, `space:`, `folder:`, `folder_id:`, `property:`, `created:YYYY-MM-DD`, `updated:YYYY-MM-DD`, `has:attachment`/`has:noattachment`, `has:cover`/`has:nocover`, `has:workflow`/`has:noworkflow`, `has:tag`/`has:notag`, `has:property`/`has:noproperty`, `is:locked`/`is:unlocked`, and `is:starred`. The equivalent boolean aliases `attachment:true|false`, `cover:true|false`, `workflow:true|false`, `tag:true|false`, `property:true|false`, `properties:true|false`, and `locked:true|false` are accepted for saved-view and typed-query compatibility. Exact-date aliases `created_on:YYYY-MM-DD`, `updated_on:YYYY-MM-DD`, and `modified:YYYY-MM-DD` map to the same `created:`/`updated:` filters, while `folder_id:` maps to the existing folder filter for API-field compatibility. Server-linked search applies the same filters after Tantivy text matching; local standalone search applies them against local vault metadata.

Tags shown in the document Properties panel can open global search pre-filtered to the clicked tag and current space.

Tag and property-schema directory reads require access to the routed space. Document tag and property-value reads/writes also verify that the routed document belongs to that same space; tag assignment is limited to tags from that space. This keeps metadata from being exposed or attached across spaces.

Global search can save the current query, tag/property filters, or Starred filter as a local saved view. Saved views appear inside the search modal and can be applied or deleted from the same surface.

The main search box accepts the first SNQL (Starlane Notes Query Language) terms: `title:"Launch Plan"`, `tag:Planning`, `space:space-id`, `folder:Projects`, `folder_id:folder-id`, `property:Status=Open`, `property.Priority:High`, `created:2026-07-14`, `updated:2026-07-14`, `modified:2026-07-14`, `has:noattachment`, `has:nocover`, `has:noworkflow`, `has:notag`, `has:noproperty`, `is:locked`, `is:unlocked`, and `is:starred`. `title:` matches a case-insensitive document title substring, and `folder:`/`folder_id:` match an exact folder name or folder ID through the existing folder filter. `created:`, `created_on:`, `updated:`, `updated_on:`, and `modified:` require exact `YYYY-MM-DD` dates; `modified:` is an alias for `updated:`. Filter terms are removed from the free-text query before the search is sent. The web header and native/fullscreen search keep this as one persistent entry field; typing opens the web result popover rather than duplicating the query in a second filter field. Saved views created with older structured filters are expanded into visible SNQL terms when opened.

### Dataview Foundation

v1.0.12 adds a first-pass local Dataview query engine for document-driven views. The engine parses boolean expressions, quoted title matches, wildcard `*` searches, fuzzy `~` searches, direct equality, `is` / `is not` predicates, tag slugs, camelCase property names, and normalized `YYYY-MM-DD` dates.

The current implementation exposes local and server data-client execution for readable documents, the global search modal can run Dataview queries with `dv:` or `dataview:` prefixes, and Page View renders fenced `dataview` / `dv` blocks. Blocks default to a results table and also accept first-pass display options such as `name: Open work`, `view: list`, `view: cards`, `limit: 5`, `columns: title, space, updated, excerpt`, `sort: updated desc`, `sort: author asc`, `group: space`, or `group: author`; results open the matched document when selected. A named block is the portable saved collection form: its name, property query, and display settings stay in the page Markdown and render the same way in web, desktop, and mobile page view. The editor `/collection` command inserts a property-backed table starter using `Status = "Open"`; change the property expression before saving to match the space schema. Table blocks can add first-pass computed columns with `column: Age = daysSince(updated)`, `column: Year = year(updated)`, `column: Week = week(updated)`, `column: Month = month(updated)`, `column: Link = link`, `column: Owner = author`, `column: Space = space`, `column: Title chars = length(title)`, `column: Excerpt chars = length(excerpt)`, `column: Title words = words(title)`, or `column: Excerpt words = words(excerpt)`. Computed columns can also drive result sorting with labels or expressions, such as `sort: Age desc`, `sort: Title chars asc`, `sort: Excerpt words desc`, `sort: week(updated) desc`, or `sort: month(updated) asc`. Server mode uses `GET /api/v1/dataview?q=...&limit=50`, caps results at 100, and returns the same search-result record shape as full-text search. Richer card templates and deeper computed expressions remain roadmap work.

## Templates Status

Space-level document templates are reusable page blueprints. A template stores a document body plus optional description, pre-filled properties, and tags; users can create a new document from that template on demand without changing the original template.

### List Templates

```
GET /api/v1/spaces/{space_id}/templates
```

Requires `owner`, `editor`, or `viewer` access.

### Create Template

```
POST /api/v1/spaces/{space_id}/templates
```

```json
{
  "name": "Project Brief",
  "description": "Reusable kickoff template",
  "content": "# {{project}}\n\n## Goals",
  "properties": {
    "status": "draft"
  },
  "tags": ["project", "brief"]
}
```

Requires `owner` or `editor` access. Template names are capped at 100 characters, descriptions at 1,000 characters, and content at 5 MB.

### Get, Update, and Delete Template

```
GET /api/v1/spaces/{space_id}/templates/{template_id}
PUT /api/v1/spaces/{space_id}/templates/{template_id}
DELETE /api/v1/spaces/{space_id}/templates/{template_id}
```

Template creation, updates, and deletes are written to the audit log. Local standalone mode preserves templates in local vault snapshots and desktop archive import/export.

The desktop sidebar and mobile new-document flows show a template picker when templates exist. In the editor, the command palette lists each available template and inserts its content at the cursor.

### Template-Backed Document Creation

When a document is created with `template_id`, Starlane applies only template metadata that already exists in the same space:

- Template tags are matched case-insensitively against same-space tags. Unknown tag names are ignored instead of creating new tags.
- Template properties are matched case-insensitively against same-space property schemas. Unknown properties and `null` template property values are ignored.
- Matching property values are serialized into the new document property records.
- Server-backed creation inserts the document, matched tags, and matched property values in one transaction.
- Local standalone creation applies the same matching behavior, then queues document, tag, and property-value replay operations in dependency order for later server replay.

Template content insertion and template-backed document creation do not implement a full template import/export automation engine. Template records are included in vault snapshots and selected space exports, but richer cross-space template package import, conflict resolution, and automation flows remain roadmap work.

### Template Placeholders

Template content supports these placeholders when a document is created from a template or when a template is inserted from the editor command palette:

| Placeholder | Output |
|---|---|
| `{{title}}` | New document title or insertion context title |
| `{{space}}` | Current space name |
| `{{author}}` | Current user display name when available |
| `{{date}}` | Localized long date, for example `Thursday, July 2, 2026` |
| `{{isoDate}}` | Local date stamp, for example `2026-07-02` |
| `{{yesterday}}` | Previous local calendar date in long localized form |
| `{{tomorrow}}` | Next local calendar date in long localized form |
| `{{yesterdayIsoDate}}` | Previous local date stamp, for example `2026-07-01` |
| `{{tomorrowIsoDate}}` | Next local date stamp, for example `2026-07-03` |
| `{{weekday}}` | Localized weekday name |
| `{{month}}` | Localized month name |
| `{{year}}` | Four-digit year |
| `{{time}}` | Localized hour and minute |
| `{{datetime}}` | Localized date and time |
| `{{property.status}}` | Value from the template property named `status` |
| `{{cursor}}` | Removes the placeholder and places the editor cursor there after command-palette insertion |

Unknown placeholders are preserved in the inserted content so future template fields do not silently disappear.

For normal page editing, use the editor command palette to insert common dynamic values such as current date, current time, current date and time, current space name, and author. Raw `{{...}}` placeholders are not automatically expanded in ordinary page text.

### Daily Note Templates

The Daily note action uses the first template that matches one of these conventions:

- Template name is `_daily`, `Daily`, or `Daily Note` (case-insensitive)
- Template has a `daily` tag

Daily notes are stored in a reserved `_daily` folder. If the folder does not exist, the Daily note action creates it before creating the note.

Daily-note template content supports the same placeholder set above. The most common daily placeholders are:

| Placeholder | Output |
|---|---|
| `{{date}}` | Localized long date, for example `Thursday, July 2, 2026` |
| `{{isoDate}}` | Local date stamp, for example `2026-07-02` |
| `{{yesterdayIsoDate}}` | Previous local date stamp |
| `{{tomorrowIsoDate}}` | Next local date stamp |

The document title remains the local ISO date stamp (`YYYY-MM-DD`) so the Daily note action can reopen the existing note for that day.

### Starred Documents

Documents can be starred from the document header. In server mode, stars are stored per user on the server and sync across signed-in devices. In local standalone mode, stars stay in the per-device quick-access cache. Starred documents appear in the desktop sidebar for the active space, in global search through the Starred filter, and on mobile from the active Space screen and Recent screen.

Mobile Recent keeps the same gesture behavior: swipe right to star or unstar a document. Starred rows sort ahead of other recent documents.

The global search Starred filter lists cached starred documents across spaces, supports title/space filtering through the search box, and allows inline unstarring for quick cleanup.

The desktop sidebar's Starred section links to an active-space Starred manager. The manager refreshes server-backed stars for that space when available, searches cached starred shortcuts across spaces, sorts by recently starred, recently updated, or title, groups results by space, opens documents, and can unstar one document or all visible filtered results.

### Source View

Documents can switch between Page view, Edit, and Source from the document header. Source opens the Markdown body in a plain monospaced textarea, uses the same autosave path as the rich editor, and becomes read-only when a document is locked.

### Journal View

The Journal route (`/spaces/{space_id}/journal`) gives daily notes a calendar and feed surface. It reads date-stamped documents from the `_daily` folder when present, groups entries into a month grid, and shows a searchable feed with Recent, This week, and Missed modes.

Recurring prompt cards appear when a daily, weekly, or early-month journal cadence is due. Selecting a prompt opens the matching journal entry if it already exists or creates the missing date-stamped note from the daily template.

Prompt cards can be customized with document templates. A template can opt in by setting a `journal-prompt` tag, a `daily`, `weekly`, or `monthly` tag, or a template property named `journal_prompt_cadence`, `journalPromptCadence`, or `cadence`. Prompt templates can also set `journal_prompt_title` / `journalPromptTitle` and `journal_prompt_description` / `journalPromptDescription` properties. When a prompt template is selected, its rendered content seeds the created journal note for that cadence.

On mobile, the active Space screen exposes Today and Journal actions together. It also shows a compact recent daily-note strip when daily notes exist, so phone and iPad users can open recent entries or the full Journal without expanding the `_daily` folder manually.

From the Journal view, users can:

- Open or create today's daily note.
- Open an existing journal entry from the calendar or feed.
- Create a missing daily note for a selected calendar date.
- Switch the feed between recent entries, current-week entries, and missed days.
- Use daily, weekly, and monthly prompt cards to recover recurring journaling rhythms.
- Customize prompt card copy and seeded note content through tagged/propertied templates.
- Review monthly entry counts and the current consecutive-day streak.

## Workflow Templates

Workflow templates define reusable document approval flows at the space level. They are the first foundation for document approval workflows: a template stores ordered stages plus required approvers by user or role.

Documents can activate one workflow template at a time. The current workflow and stage are visible in the document Properties panel. Editors can move the workflow between stages when their user or role matches the stage approver rules. Transition history is preserved in the document side panel, workflow-aware document filters power queue views, and assigned approvers can receive in-app and email notifications.

### List Workflow Templates

```
GET /api/v1/spaces/{space_id}/workflow-templates
```

Requires `owner`, `editor`, or `viewer` access.

### Create Workflow Template

```
POST /api/v1/spaces/{space_id}/workflow-templates
```

```json
{
  "name": "Draft Review",
  "description": "Default document approval flow",
  "stages": [
    { "id": "draft", "name": "Draft", "sort_order": 0 },
    { "id": "review", "name": "Review", "sort_order": 1 },
    { "id": "approved", "name": "Approved", "sort_order": 2 }
  ],
  "approvers": [
    {
      "approver_type": "role",
      "approver_id": "owner",
      "stage_id": "review"
    }
  ]
}
```

Requires space `owner` access. Notes Admins are also allowed through the normal space-role guard. Workflow names are capped at 100 characters, descriptions at 1,000 characters, and templates must have 2 to 12 named stages.

Approvers use `approver_type: "user"` with a user ID or `approver_type: "role"` with a role such as `owner`, `editor`, or `admin`.

### Get, Update, and Delete Workflow Template

```
GET /api/v1/spaces/{space_id}/workflow-templates/{template_id}
PUT /api/v1/spaces/{space_id}/workflow-templates/{template_id}
DELETE /api/v1/spaces/{space_id}/workflow-templates/{template_id}
```

Get requires `owner`, `editor`, or `viewer` access. Update and delete require space `owner` access. Create, update, and delete actions are written to the audit log. Local standalone mode preserves workflow templates in local vault snapshots.

### Activate a Document Workflow

```
PUT /api/v1/spaces/{space_id}/documents/{document_id}/workflow
```

```json
{
  "template_id": "workflow-template-id",
  "stage_id": "draft"
}
```

`stage_id` is optional. When it is omitted, the workflow starts on the template's first stage.

Requires `owner` or `editor` access. Enabling a workflow creates or replaces the document's current workflow instance and writes a `document.workflow_enable` audit action.

### Get or Remove a Document Workflow

```
GET /api/v1/spaces/{space_id}/documents/{document_id}/workflow
DELETE /api/v1/spaces/{space_id}/documents/{document_id}/workflow
```

`GET` requires `owner`, `editor`, or `viewer` access and returns `null` when no workflow is active. `DELETE` requires `owner` or `editor` access and removes the active workflow instance. Local standalone mode preserves active document workflows in local vault snapshots.

### Transition and Inspect Workflow History

```
POST /api/v1/spaces/{space_id}/documents/{document_id}/workflow/transitions
GET /api/v1/spaces/{space_id}/documents/{document_id}/workflow/events
POST /api/v1/spaces/{space_id}/documents/{document_id}/workflow/events
```

```json
{
  "stage_id": "review",
  "note": "Ready for review"
}
```

Transitioning requires `owner` or `editor` access. The workflow is marked `completed` when it moves to the template's final stage; otherwise it remains `active`. The transition writes a `document.workflow_transition` audit action and appends a lightweight event record.

History listing requires `owner`, `editor`, or `viewer` access. Local standalone mode preserves workflow transition events in local vault snapshots.

`POST /workflow/events` records non-transition workflow handoff history without changing the active stage. It requires `owner` or `editor` access and accepts `action: "delegate"` or `action: "escalate"` plus a required note. These records appear in the Properties panel and right-panel History tab.

### Workflow Queue

```
GET /api/v1/spaces/{space_id}/documents?include_nested=true&workflow_status=active
```

The app shell exposes a space workflow queue at `/spaces/{space_id}/workflows`. The queue can filter by active/completed status, workflow template, workflow stage, approval scope, updated-age bucket, handoff attention, and local search text. Approval scopes include all workflow documents, documents assigned directly to the current user, documents assigned to the current user's role, and workflow documents without a required approver.

Queue rows show document title, workflow, current stage, required approver, latest handoff, deadline urgency, age, and updated time. Rows are keyboard-openable with Enter or Space. Users can save queue views locally per space, select visible rows, copy selected document links, and open the first selected document for triage.

When selected documents share the same workflow template, the queue can move them together to another stage with an optional transition note. Server and local approval rules are still enforced per document, and any failed transition is reported for manual review.

Selected queue documents can also record delegation or escalation handoff notes in bulk. Handoff events preserve the current workflow stage and approval enforcement, but add durable workflow history so reviewers can capture who should take over, why an item is escalated, the due date for follow-up, and which documents still need manual follow-up.

The bulk handoff controls include structured target rules derived from the selected documents: current stage approvers, the previous handoff target, the current user, and an admin escalation fallback. Choosing a rule fills the handoff target with the same parseable `user`, `email`, or `role` string used by server notifications and deadline filters while still allowing a manual override. Operators can also save reusable reassignment rules per space to reapply common delegate/escalate actions, targets, notes, and due dates from the queue.

Standard handoff notes such as `Delegated to editor@example.com: due 2026-01-12` or `Escalated to owner: deadline 2026-01-12` are parsed by the queue for delegated/escalated filters, due-soon/overdue filters, deadline-first sorting, and target search. In server mode, recognized user IDs, emails, admin targets, and space-role targets receive a workflow handoff notification.

The queue can bulk-escalate visible overdue delegated documents. The action records escalation events against the current workflow stage, targets the delegated owner when one can be parsed, and falls back to the admin role for unparseable overdue handoffs.

Admins can also inspect or run server-side overdue delegated workflow escalation:

```
GET /api/v1/admin/workflows/deadline-escalations?before=2026-01-13
POST /api/v1/admin/workflows/deadline-escalations?before=2026-01-13&target=admin
```

The optional `before` date uses `YYYY-MM-DD` and defaults to the server's current date. `GET` returns active workflow documents whose latest handoff event is still a delegation with a parsed due/deadline date before that cutoff. `POST` records escalation events for the same candidates, preserves the current workflow stage, targets `admin` by default, supports `dry_run=true`, and becomes idempotent once the latest handoff is no longer a delegation.

Self-hosted servers can run the same escalation path automatically by setting `WORKFLOW_DEADLINE_ESCALATION_INTERVAL_MINUTES` and `WORKFLOW_DEADLINE_ESCALATION_ACTOR_ID`. The actor must be an active user because workflow history records preserve `created_by` with a database foreign key. `WORKFLOW_DEADLINE_ESCALATION_TARGET` optionally overrides the escalation target and defaults to `admin`.
