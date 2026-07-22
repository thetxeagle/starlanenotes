# Plugin System

Starlane Notes does not run third-party plugins yet. v1.0.12 adds the first contract layer: a manifest validator, Admin Plugins review screen, registry index browser, permission review summary, local install-record ledger, local install-management audit events, and server-backed audit logging for blocked runtime attempts.

## Manifest Shape

Plugins will be distributed as a directory or ZIP containing `plugin.json`, optional server/client entry points, and documentation:

```text
my-plugin/
├── plugin.json
├── server/
│   └── index.ts
├── client/
│   └── index.tsx
└── README.md
```

The current validator accepts this manifest shape:

```json
{
  "id": "com.example.my-plugin",
  "name": "My Plugin",
  "version": "1.0.0",
  "description": "Does something useful",
  "author": "Jane Smith",
  "license": "MIT",
  "homepage": "https://github.com/example/my-plugin",
  "starlane_version": ">=0.1.0",
  "permissions": {
    "server": ["read_docs", "http_fetch"],
    "client": ["editor_commands", "sidebar_panel"]
  },
  "http_fetch": ["api.example.com"],
  "settings_schema": [
    {
      "key": "api_key",
      "label": "API Key",
      "type": "secret",
      "required": true
    }
  ]
}
```

## Permission Contract

Server permissions:

| Permission | Intent |
|---|---|
| `read_docs` | Read document content and metadata |
| `write_docs` | Create and update documents |
| `read_spaces` | Read space structure and member lists |
| `write_spaces` | Create or modify folders, excluding destructive delete behavior |
| `read_users` | Read user display names and emails |
| `http_fetch` | Make outbound HTTP requests to explicitly allowed domains |
| `search` | Query the search index |
| `db_read` | Read from the plugin's isolated database namespace |
| `db_write` | Write to the plugin's isolated database namespace |

Client permissions:

| Permission | Intent |
|---|---|
| `editor_commands` | Register slash commands |
| `editor_blocks` | Register custom block renderers |
| `sidebar_panel` | Add a right-sidebar panel |
| `toolbar_button` | Add editor toolbar actions |
| `properties` | Register custom property types |
| `context_menu` | Add document context-menu actions |
| `settings_page` | Add a plugin settings page |

When `permissions.server` includes `http_fetch`, the manifest must also include a non-empty `http_fetch` domain allowlist. Entries must be hostnames such as `api.example.com`; URL schemes, paths, and wildcards are rejected.

The Admin Plugins validation panel now summarizes requested access before saving an install record. The review labels read/write document-space access, user-directory access, outbound network allowlists, client extension points, and secret settings as low/medium/high review items. Saving a reviewed install record still does not execute plugin code; runtime enforcement remains future work.

## Current Status

Implemented:

- Manifest TypeScript types.
- `plugin.json` validation and normalization.
- Permission allowlist checks.
- Settings schema validation for `string`, `number`, `boolean`, `select`, and `secret` fields.
- Admin Plugins screen for pasting a manifest and reviewing validation output.
- Admin Plugins registry index browser for filtering validated `registry.json` entries and preparing local manifest-review stubs without fetching remote sources.
- Permission review summaries for validated manifests, including network allowlists and secret settings.
- Local Admin Plugins install records with validated manifest save, global enable/disable state, per-space enablement state, source provenance metadata, matched pasted-registry provenance, and remove controls.
- Local Admin Plugins install audit events for saved install records, enable/disable actions, per-space scope changes, and removals.
- Local Admin Plugins runtime audit events for blocked runtime enablement attempts while execution remains unavailable, including action filtering and JSON/CSV export payloads.
- Server-backed audit logging for blocked plugin runtime enablement attempts through `/api/v1/admin/plugin-runtime-audit`.
- Focused validator tests.

Remaining:

- ZIP/Git/registry source fetching and installers beyond local registry browsing/review stubs.
- Server-side Deno sandbox.
- Client iframe sandbox and SDK.
- Runtime permission enforcement.
- Runtime use of per-space enablement beyond blocked-attempt audit logging.

## Runtime Guard

`guardPluginRuntimeAccess(record, capability, spaceId?)` is the deny-by-default boundary every future plugin host must call before dispatch. It rejects disabled plugins, spaces that are not enabled for the plugin, undeclared client/server permissions, non-HTTPS fetch URLs, and fetch hosts outside the manifest allowlist. Valid declarations remain denied with `runtime_unavailable` until a sandbox host is added; the guard does not execute plugin code.

The first client host is `startPluginClientSandbox(...)`: it creates a hidden `sandbox="allow-scripts"` iframe with an opaque origin and a frozen postMessage-only API. It grants no same-origin, navigation, popup, form, or direct Starlane API access. Only installer-verified source may be passed to it, and startup reuses the runtime guard.
