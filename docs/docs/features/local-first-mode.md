# Local-First Mode

Starlane Notes supports a server-backed mode and an in-progress local standalone mode for desktop/mobile app readiness. Local mode lets a user start without connecting to a server and keep a vault snapshot on the device or in a chosen native folder when the platform allows it.

## Goals

- Allow Starlane Notes to run without connecting to a server.
- Preserve offline-first behavior as the default.
- Support optional personal-cloud sync providers for single-user workflows.
- Keep current server-backed mode fully supported for teams and self-hosting.

## Non-Goals (Initial Scope)

- Replacing team collaboration workflows in server mode.
- Real-time multi-user permissions in no-server mode.
- Shipping all sync providers at once.

## Product Modes

### 1) Server Mode

- Uses backend API for auth, data, search, notifications, and collaboration.
- Best fit for shared/team spaces.
- Native server sessions keep a device-local cache of successful server reads, including the signed-in user, auth configuration, page lists, and any page content opened on the device. If the linked server cannot be reached later, the app opens that downloaded workspace instead of blocking at startup.
- Offline edits to already-downloaded documents and folders, plus new pages/folders with client-generated source IDs, are retained as a local, ordered write queue. The app retries those workspace operations when connectivity returns. A server rejection or conflict stays queued for explicit recovery; the client never silently overwrites a server-side result.
- When a native server-linked shell is offline, its workspace bar confirms whether the session-scoped downloaded workspace is ready and shows the count of locally queued changes. This is an availability signal, not a claim that server conflicts have been resolved.
- First connection still requires the server. The app cannot safely create an offline server workspace until it has downloaded the authenticated session and at least one successful server response. Creating spaces, invites, account changes, and other administration remains online-only.

### 2) Local Vault Mode

- No server required.
- Local storage is the primary source of truth.
- App opens directly into local vault workflows.
- Browser local mode uses IndexedDB/local storage.
- Tauri desktop local mode can persist the vault snapshot and native assets under a selected vault folder. Admin Storage includes a first-pass vault switcher for sibling vault folders and can create a new vault folder from the app.
- Tauri mobile local mode checks whether a vault is explicitly configured before initializing local storage. If a last configured vault exists, the app restores local mode and validates that vault before mounting the workspace. If no vault is configured, startup returns to a three-section chooser for creating a local vault, opening an existing local/iCloud/server vault copy, or connecting to a server instead of silently creating a phantom App-folder vault. First-run setup can name the vault folder, and mobile settings can rename it later or switch among app-visible vault folders.
- Tauri vaults generate a user-readable mirror under `spaces/`: each space has `documents/`, `attachments/`, and `templates/` folders, and pages/templates are written as Markdown files with metadata frontmatter. The JSON snapshot remains the authoritative app index, while desktop reconciliation can pull supported Markdown edits and newly added Markdown documents/templates back into the snapshot.
- Mobile settings expose the current vault folder, cache reset, app reset, and first-pass iCloud/provider selection without showing server administration features.
- Server-linked app shells expose Admin Storage controls for changing the self-hosted server URL and resetting setup back to the local/server chooser.

### 3) Local Vault + Provider Sync (Planned)

- Same local vault as above.
- Optional sync providers mirror vault state across personal devices.

## Sync Provider Rollout

### Phase A: Local Vault Baseline (v1.0.12 continuing)

- Local-only vault mode with no provider account needed.
- Import/export + backup restore.
- Basic vault health checks (schema/version migration).
- First-run local/server choice.
- Desktop vault folder controls and first-pass sibling-vault switching.
- JSON snapshot import/export.
- Desktop `.starlane-vault` archive import/export with native assets.
- Generated Tauri vault Markdown mirror for Files/Finder browsing.

### Phase B: iCloud Drive Provider (v2 candidate)

- Apple-platform first provider for personal sync.
- iOS/macOS vault replication using iCloud container/file sync semantics.
- Conflict handling and recovery UX for interrupted sync.

### Phase C: Google Drive Provider (post-v2)

- Cross-platform provider option.
- OAuth onboarding, token refresh, and background sync policy.
- Provider-specific retry/rate-limit behavior and conflict resolution.

## Technical Architecture Direction

- Add a storage/sync provider abstraction above persistence operations.
- Keep a canonical local data model; providers replicate local state rather than replacing it.
- Store to the local device first in native app shells, then sync outward when a server or personal provider is configured.
- Treat self-hosted server installs as SQLite-first because most deployments are expected to use the default embedded database. Keep PostgreSQL as the compatible scale-out option for larger or multi-user deployments.
- Use a native local database for desktop/mobile offline operation. SQLite is the preferred local app store because it fits macOS/iOS deployment, aligns with the server default, and can persist documents, metadata, sync cursors, and queued write operations without requiring a server connection.
- Route native local persistence through a stable native-local-database command boundary. The current Tauri implementation writes a SQLite payload store for vault collections, sync metadata, and queued operations, then maintains the JSON snapshot/Markdown mirror as a compatibility bridge. A future migration should normalize the SQLite tables further before treating the JSON snapshot as non-canonical.
- Keep the web app online-first by default. Web sessions should modify the server through the Starlane API, which writes to the server database directly; browser storage should remain limited to preferences, temporary drafts, and non-canonical caches unless a separate PWA/offline mode is explicitly designed.
- Keep Personal Space available offline by default once it has been opened from a linked server. Shared/server spaces and pages become available offline after their list/content has been downloaded; a full explicit space-download control remains a follow-up.
- Track sync state via operation logs/checkpoints, not only file timestamps.
- Build deterministic conflict resolution for concurrent edits and interrupted sessions.

## Current Storage Map

| Data | Server mode | Browser local mode | Tauri local mode |
|---|---|---|---|
| Documents, folders, spaces, tags, properties, templates | Backend database plus document content files. SQLite is the default; PostgreSQL is the compatible larger-install option. | Online-first API reads/writes against the server; local browser state is non-canonical unless a future offline/PWA mode is scoped. | Vault-scoped native SQLite payload store plus the portable `starlane-vault-*.json` snapshot/Markdown mirror as the active compatibility bridge |
| User-visible Markdown files | Server document content path | Not generated | Generated mirror under `spaces/<space>/documents/` and `spaces/<space>/templates/` with frontmatter |
| Attachments and images | Server attachment storage under managed data dir | Browser asset records and data URLs/object URLs where native filesystem access is unavailable | Files copied into `spaces/<space>/attachments/<document>/` when document context is known; legacy fallback remains `assets/` |
| App settings | Backend config and browser client preferences | Browser storage keys for launch mode, theme, toolbar layout, and local cache state | Browser storage keys plus Tauri store value for the active vault path |
| Search | Tantivy-backed server index plus API filters | Persisted local `search_index` cache inside the vault snapshot plus tag/property filters | Persisted local `search_index` cache inside the native vault snapshot plus tag/property filters |
| Sync state | Server API/session state | Snapshot sync-state metadata with current state, timestamp, durable queued local write capture/list/clear, and replay-manifest export | Native SQLite metadata plus snapshot sync-state metadata with the same durable queued-operation ledger and replay-manifest export; manifests now carry a replay contract and readiness block for queued-at ordering, operation-id idempotency, server API transport, required payload availability, and SQLite/PostgreSQL target compatibility |

Server-linked shells store the active self-hosted server URL in browser storage. Successful server GET responses are also cached in device-local WebView storage, scoped to the linked server API base. Admin Storage can replace that URL after validation and can clear the launch/storage selection so the next reload returns to the first-run local/server chooser. The product rule for server-connected native shells is local-copy first: use the downloaded workspace when the server is unavailable, refresh it with server responses when online, and show the connection-recovery screen only if neither a server connection nor a signed-in local workspace cache exists.

The current native-app rule is: local data is written first, then mirrored or synced outward. The web app remains online-first and writes through server APIs. The generated Markdown tree is intentionally readable and portable. Desktop vaults can reconcile edits made to generated document/template Markdown files back into the JSON snapshot when the files are still tracked by `.starlane-mirror-manifest.json`.

Desktop reconciliation can also import newly added `.md` files placed under an existing mirrored space's `documents/` or `templates/` folder. Frontmatter is optional for new imports. Document paths below `documents/` create or reuse matching snapshot folders, and template files under `templates/` import as space templates.

## Migration and Portability Strategy

- Browser local mode supports JSON snapshot export/import for backup and testing. JSON imports show a preflight summary with spaces, documents, folders, assets, templates, sync state, and queued-operation count before replacing the current vault.
- Local mode runs a startup storage structure check before mounting the authenticated app. The check confirms the local vault can be initialized and normalizes safe relationship drift by detaching invalid folder/page parent references instead of deleting records. Records that reference a missing space block startup for manual review rather than being guessed or discarded.
- Tauri desktop supports `.starlane-vault` archive export/import, including native assets.
- Admin Storage can export local vaults and selected server spaces as static publishing artifacts: a self-contained HTML file, or a ZIP bundle containing `index.html`, `styles.css`, `site.json`, `deploy.json`, `deploy-local.sh`, `deploy-rsync.sh`, one generated HTML page per document, and extracted `assets/` files for exportable asset bytes. Native Tauri exports hydrate eligible vault-backed asset bytes before bundling, while remote or unavailable binaries remain listed for manual follow-up. `site.json` records site metadata, document paths, tags/properties, extracted asset entries, and per-document skipped-asset references; `deploy.json` records a credential-free manual upload plan with the entrypoint, manifest, publish root, upload file list, skipped asset summary, and deploy-hook metadata. `deploy-local.sh` copies publishable files into an explicit local directory argument after the ZIP is unpacked. `deploy-rsync.sh` uploads only the generated publishable files to an SSH destination supplied as `STARLANE_STATIC_DEPLOY_TARGET=user@example.com:/absolute/publish-path`; it relies on the caller's SSH agent/config for credentials and host verification, and never stores a destination or secret in the ZIP. ZIP document pages rewrite embedded `data:` URLs plus matching vault-path and `starlane://asset/...` references to relative asset paths when bytes are available. Export settings support a custom title, description metadata, and light/dark/paper themes.
- Tauri mobile supports app-visible vault folders and first-pass vault switching. Vault status checks are non-initializing, so settings and startup can show an unconfigured state without creating the default App folder. If the restored vault fails the startup health check, the app clears the active vault pointer, marks that vault as needing repair in the chooser, and does not delete vault files. The mobile vault submenu exposes create/connect actions above the vault list, and long-press/context actions can rename a visible vault or move a non-active vault into the app `.Trash` folder instead of permanently deleting it. iOS persists custom vault choices through both the native settings store and a Documents-side `.starlane-vault-location.json` fallback so an empty settings store can be repaired without deleting vault folders. Mobile archive import/export and share-sheet provider flows remain future work.
- Standalone-to-server migration can upload local spaces, folders, documents, templates, property schemas/values, tags, document tag assignments, active workflow state, and document-linked asset binaries into server APIs. Imported spaces, folders, documents, templates, property schemas, property values, tags, workflow templates, active document workflows, and workflow events preserve source IDs where the snapshot carries them; spaces/documents/templates/workflow templates/property values/tags/active document workflows/workflow events preserve source timestamps where present; handoff event authorship is preserved when the source actor exists on the target server; uploaded assets rewrite migrated document Markdown to server attachment URLs. Assets without available binary data or an inferable document reference are skipped with a migration warning.
- Server-to-standalone migration can download selected spaces from Admin Storage into standalone vault snapshots. Browser-side per-space download checkpoints record the latest snapshot time and counts after each selected download, and queued replay manifests include those checkpoints as recovery context. Server replay execution records authenticated replay-run history plus checkpoint rows keyed by user/provider/space, and local-mode Admin Storage shows recent replay history with checkpoint summaries. Failed or unsupported replay rows retain their retry count and last server error in the local queue, so retry/clear recovery remains available after the app restarts. Permissions provenance and attachment copy guarantees still need follow-up.
- Conflict-safe provider sync has durable local write-intent capture, queued replay-manifest export with captured entity payloads and tombstones, a narrow replay contract/readiness block for SQLite/PostgreSQL-backed servers, an authenticated server-side replay preflight endpoint, replay execution for `document:create`, `document:update_content`, `folder:create`, `folder:update`, `folder:delete`, `tag:create`, `tag:update`, `tag:delete`, `document_tag:add`, `document_tag:remove`, `template:create`, `template:update`, `template:delete`, `property_schema:create`, `property_schema:update`, `property_schema:delete`, `property_schema:reorder`, `document_property_values:set`, `workflow_template:create`, `workflow_template:update`, `workflow_template:delete`, `document_workflow:create`, `document_workflow:update`, `document_workflow:delete`, `document_workflow:transition`, `document_workflow_event:*`, and `asset:create` with operation-id ledger idempotency, core replay payload ID matching, in-manifest dependency-order blocking for later-created prerequisites, shared-space recovery checkpoints, selected shared-space snapshot downloads, browser-side per-space download checkpoints, server-authenticated replay history, Admin Storage recent-history controls, manual linked-server health/preflight-gated replay retry, failed/unsupported subset retry and selected clear controls, provider pull/push readiness blockers for connection state, replay payload availability, structured unavailable-payload details for missing attachment bytes, native byte-pack errors, byte sources checked, recovery actions, and pending conflicts, explicit contract-only iCloud/Google Drive root/auth/scope/write/conflict boundaries, dry-run/blocked provider mutation plans for remote pull/push status, provider remote enablement plans for explicit transport readiness, persisted explicit provider remote sync enable/disable state guarded by manual-session readiness, typed provider default-sync policy gates with readiness-gate reporting for manual sessions, explicit enablement, background transport wake eligibility, and missing device-executed wake validation, provider remote mutation envelopes, transport-attempt results, manual provider pull/push blocked-attempt reporting when live transport is unconfigured, request specs for future Google Drive file-upsert and iCloud native file-upsert execution, a native iCloud provider-sync manifest write command, a native iCloud transport executor for enabled envelopes, and an explicit Google Drive API transport executor for enabled envelopes when a bearer token and API transport are supplied, with explicit manual Google Drive push now uploading typed provider-sync manifest wrappers carrying queued replay manifests through the fetch transport when enabled and explicit manual pull discovering, downloading, validating, preflighting, dry-running, apply-planning, and exposing an explicit execution boundary for those root-scoped provider-sync manifests. Local provider import merge now covers core local-vault families including documents, spaces, folders, tags, document tags, templates, workflow templates, property schemas, document property values, browser-backed assets, document workflows, and workflow events through existing snapshot/native SQLite paths while blocking same-entity local queued conflicts and server-only or recovery-required families. Admin System exposes import review/apply visibility. Tauri local background/lifecycle wakes can attempt explicitly enabled provider pushes after queued server replay work, with manual-session connected/scope/expiry validation, optional selected Drive root metadata, OAuth authorization request planning, PKCE/state bootstrap preview, browser-facing public-PKCE secret blocking, validated Admin OAuth launch handoff, state-validated callback exchange-request derivation, Admin callback-preview validation, callback-to-manual-connection composition, manual completion-plan visibility, fixture-backed Admin manual-session completion, explicit Google OAuth/API fetch completion, injected code-exchange execution, token-response session mapping, refresh-session mapping, root lookup/create request contracts, selected-root session mapping, injected root lookup/create execution, composed manual connection persistence, Admin System manual-readiness visibility, and manual setup-plan visibility, Tauri app-open replay scheduling with retry/backoff/last-error state plus online/focus/visible lifecycle wake triggers, durable native wake-intent registration for future OS schedulers, native OS-scheduler descriptors with stable task identifiers, a wake-recording execution command, a wake-receipt import bridge, Apple/Android OS-registration plans, Android WorkManager hook scaffolding, iOS BGTaskScheduler metadata, and iOS BGTask handler scaffolding, normalized native SQLite tables populated from snapshot writes, native readback from normalized SQLite collection/queue/cursor/checkpoint tables, direct native SQLite per-record upsert/delete commands, Tauri adapter-owned collection routing through per-record mutations, full snapshot-save diff routing for metadata/search records, reduced redundant native full-snapshot bridge writes after successful record mutations, SQLite-backed replay execution fixture coverage for portable SQL paths, an opt-in PostgreSQL replay fixture via `STARLANE_SYNC_REPLAY_POSTGRES_URL`, and JSON import preflight summaries. Asset replay writes server attachments and rewrites matching local asset references in document content when embedded bytes are available; Tauri local vault assets can be byte-packed into replay payloads when their `asset://` URL or vault path still resolves under the active vault. It still needs remaining provider import edge-case validation, default-enabled full provider sync, production Google OAuth deployment validation, plus device-executed OS background task validation before server, iCloud, or Google Drive sync can be treated as full sync providers.

## Risks and Constraints

- Conflict correctness is higher risk than raw storage access.
- Background sync behavior differs by platform and OS lifecycle rules.
- Encryption/key management must be defined before broad provider rollout.
- Provider APIs and quotas require robust retry/backoff strategies.

## Delivery Milestones

1. Define mode and provider interfaces in client codebase. Done.
2. Add launch-time mode selection plumbing. Done.
3. Implement local vault baseline. Continuing through v1.0.12 with provider sync and native validation follow-up.
4. Promote the generated Markdown tree from mirror-only to import/reconciliation support. Done for tracked edits and new document/template Markdown imports.
5. Add standalone/server migration flows. In progress: snapshot preview, server import/export, mapped metadata import, broad source preservation, document-linked asset binary upload, tag timestamp preservation, handoff event authorship for matching server users, non-handoff workflow history replay, snapshot sync-state metadata preservation, durable queued local write capture/list/clear, replay-manifest export, explicit shared-space snapshot downloads, browser-side per-space download checkpoints, and JSON import preflight summaries are implemented; automatic server replay remains part of the larger sync roadmap.
6. Introduce the native app-local SQLite store behind the existing local data client so desktop/mobile can read/write offline without treating the JSON snapshot as the long-term canonical runtime database. In progress: the Tauri command boundary, frontend adapter preference path, vault-scoped SQLite payload store, normalized write tables, normalized readback for all current snapshot collections plus sync queues/checkpoints, direct per-record native upsert/delete commands, Tauri adapter-owned collection routing, full snapshot-save diff routing for metadata/search records, and reduced native full-snapshot bridge writes after successful record mutations exist; the snapshot bridge remains as compatibility fallback for older shells and failed record-command batches.
7. Add mobile document-provider/share-sheet import/export.
8. Add iCloud sync adapter and recovery UX.
9. Add Google Drive sync adapter and provider parity checks.

## Current Status

- Local standalone mode is wired through the web client data layer.
- Desktop local vault folder selection, sibling-vault switching, and archive import/export are implemented for Tauri desktop.
- Server-linked Admin Storage can update the active server URL or reset the app back to setup selection.
- Standalone-to-server import maps property schemas/values, tags/document tags, workflow templates, and active workflow state into server records while preserving source content relationships.
- Mobile local vaults use an explicit choice when no valid last vault is available: create a new app-visible folder, open a visible local/iCloud/server vault copy, or connect to a server URL that detects standard login, OAuth, or SSO. The visible app Documents folder can be browsed through Files/Finder when iOS document sharing is enabled, and the first-pass vault picker has an iOS vault-location fallback for custom folder persistence plus a local "Needs repair" marker for vaults that fail startup validation.
- Mobile storage actions surface native filesystem/iCloud failure messages from the Tauri shell instead of replacing them with a generic fallback. The default App folder remains the safe built-in vault location and cannot be renamed directly; create or switch to a custom app-visible vault folder before renaming.
- Tauri local vaults now write a generated `spaces/` Markdown mirror beside the canonical JSON snapshot and can import newly added document/template Markdown files during desktop reconciliation.
- Self-hosted Inter and JetBrains Mono font assets are bundled through the web app build so Docker, Tauri, and offline installs do not need external font CDNs.
- The mobile settings surface is intentionally streamlined: local mode hides users, groups, API, and site administration, while sync/server mode keeps advanced administration desktop-oriented.
- Android build compatibility has been validated.
- iOS build/install smoke has passed on simulator and physical-device lanes with the `H9FVZ6838A` Apple development team.
- iCloud Drive and Google Drive have explicit provider contracts plus manually enabled push transports. Google Drive manual pull now discovers, downloads, validates, preflights, dry-runs, apply-plans, and exposes review/apply handling for root-scoped provider-sync manifests. Remaining work is provider import edge-case validation, default-enabled full provider sync, production Google OAuth deployment validation, native iCloud folder relocation, and device-proven background execution.
