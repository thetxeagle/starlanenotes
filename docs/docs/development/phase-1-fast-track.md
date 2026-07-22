# Phase 1 Fast-Track Backlog

This document is the durable Phase 1 slicing board. It separates:

- **Fast-track slices**: small, useful, testable increments worth doing soon.
- **Deferred scope**: real roadmap work that should not be accidentally pulled into a fast-track slice.
- **Already-covered scope**: work that should be verified or polished, not rebuilt.

Use this before starting Phase 1 work so completed foundations do not get reimplemented. Caboose is not allowed to “help” by widening scope.

## Current fast-track position

The broad Phase 1 core app is partially complete, but the current fast-track lane has covered the obvious stale-doc, route-evidence, and state-hygiene gaps for SCIM, MCP, templates, search, admin lifecycle, AI/webhooks/notifications, and OpenAPI/Scalar. The safest next move is **not** another automatic “one more tiny slice” pass. Pick a new slice only when inspection finds a concrete gap, or explicitly move the remaining work to a larger roadmap lane.

## Fast-track audit result

As of the latest v1.0.12 checkpoints:

- SCIM closeout is covered for endpoint behavior, token/setup docs, audit expectations, focused release-validation tests, and generated route evidence; remaining admin token-management/sync-history UI is intentionally deferred unless selected.
- OpenAPI/Scalar has broad route-family coverage plus full documented-route-surface regression evidence; add more only for a discovered implemented-route schema/detail gap.
- MCP local-vault fast-track read/request behavior is complete for accepted-token reads through the canonical dispatch gate, and server-side MCP HTTP execution guardrails now cover unknown tools, deferred write tools, non-admin users, and admin API keys missing the `admin` scope; listener-backed binding and write execution remain deferred.
- Template, search, admin lifecycle, AI document-action, webhook, and notification fast-track slices have current focused docs/tests/state evidence.
- Remaining items in this document are either explicit deferred scope or larger visual/manual QA/product lanes, not automatic Phase 1 fast-track work.

Use this audit result as a stop sign: if the next task cannot name a specific missing behavior, stale row, or untested state transition before editing, do not start a product slice.

## Completion-audit outcome

The 2026-07-14 completion audit reconciled this board against `SCRATCHPAD.md` and `docs/docs/development/phased-prompt-gap-audit.md`.

Outcome: **Phase 1 fast-track is sufficiently trimmed for now.**

- No automatic next implementation slice is queued.
- The remaining Phase 1 entries are either explicit deferred scope, larger manual/visual QA lanes, or broader roadmap work.
- Future Phase 1 work must start by naming the exact evidence gap it closes, or by explicitly selecting one deferred lane below.
- `SCRATCHPAD.md` should point back to this board instead of duplicating every completed fast-track slice.

## Super fast-track operating mode

Use this mode when the goal is speed without rework.

1. Pick exactly one row from the fast-track table.
2. Prove whether the foundation already exists before editing code.
3. If the foundation exists, close the smallest visible gap: stale docs, missing tests, stale UI state, route-schema evidence, or one backed loading/error/empty state.
4. Do not pull in deferred scope, even if the adjacent feature is tempting.
5. Validate the smallest relevant surface, update tracking docs, then commit and push before taking the next bite.

The current queue is now audit-gated:

| Order | Slice | Why this is fast-track safe | Stop condition |
|---|---|---|---|
| 1 | Concrete residual evidence only | Start only after inspection names one missing behavior, stale audit row, or untested state transition. Current AI, editor, search, admin, OpenAPI, template, webhook, and notification fast-track slices already have focused evidence. | One specific gap is fixed and validated, or no gap is documented. |
| 2 | Explicitly selected deferred item | Use only if the user selects a larger deferred item such as admin token UI, sync-history UI, full import/export, or MCP transport/write work. Editable Excalidraw has moved from deferred to implemented for the shared web/desktop editor surface. | A separate scoped goal is opened before implementation. |

If a candidate cannot fit this audit-gated table, it is not a super fast-track slice. Put it in “Left out for later” or open a new focused goal.

| Priority | Slice | Fast-track scope | Explicitly deferred |
|---|---|---|---|
| 1 | Phase 1 audit cleanup | Keep this file and `phased-prompt-gap-audit.md` reconciled against actual implemented SCIM, MCP, template, search, OpenAPI/Scalar, admin, AI, webhook, and notification work. Do not add new implementation prompts unless inspection finds a concrete gap. | Any product feature implementation. |
| 2 | SCIM closeout | Verify current SCIM Users/Groups endpoints, token/security expectations, audit events, and docs. Token operations, IdP setup mapping, audit review expectations, boundaries, focused release-validation tests, and generated route evidence are now documented or validated. Future admin token-management or sync-history UI work should start only from one exact operator workflow, release-validation failure, or selected SCIM/admin goal. | Rebuilding SCIM core endpoints, full IdP sync UI, broad enterprise policy. |
| 3 | OpenAPI/Scalar coverage | Cover implemented Phase 1 endpoints with route docs/schemas where missing. Health, public auth config, public page slug route coverage, auth token exchange/refresh, password reset request/confirm plus generated 429 rate-limit responses for rate-limited auth endpoints, OAuth/OIDC redirects and OAuth provider route coverage, current-user profile read/update, TOTP 2FA, auth session/API-key route annotations plus full documented-route-surface regression coverage, user-directory route coverage, admin-user create/list/update/bulk lifecycle/password reset/session route coverage, admin user CSV import/template route coverage, admin group create/member route coverage, admin config list/upsert/delete route coverage, admin search reindex route coverage, audit-log filter/default/lower-bound/capped pagination query parameters, audit CSV export route coverage, plugin runtime audit-event route coverage, AI config/chat/completion/document-action route coverage, search query parameters, Dataview max-length/default/lower-bound/cap query parameters, notification inbox query parameters, webhook delivery/redelivery and notification read-state route coverage, template/workflow-template route coverage, metadata tag nullable response fields, metadata tag/property/document-property/backlink route coverage, space/folder/folder-reorder/direct-member/group-grant/graph route coverage, core document list/read/update/content/starred/lock-conflict/publish/comment/resolve/reaction/version restore/attachment list-upload-delete/workflow/queue/deadline-escalation route coverage, implemented document workflow `{document_id}` route-parameter alignment, membership/grant mutations, document create/response/update/content/parent placement, document tag attach, and webhook redelivery now have dedicated generated schemas, explicit success responses, route-level annotations, or route-surface evidence. Future OpenAPI/Scalar work should start only from one concrete implemented-route family gap; do not use this row to justify new API behavior or endpoint redesign. | New API behavior or endpoint redesign. |
| 4 | Personal-space UI parity | Close profile username/display/personal-space UI consistency gaps against existing backend/local metadata. Local list/detail responses now preserve owner username/display metadata and desktop/mobile space UI surfaces personal-space owner identity. Future profile-editing visual QA or account UI polish should start only from one exact profile identity, owner-label, mirror-path, or username-conflict finding. | Broad account/profile redesign. |
| 5 | Admin lifecycle polish | Fill obvious empty/error/loading states and lifecycle controls already backed by APIs. Admin Users now shows a backed loading state while the lifecycle list API is pending, clears stale session rows while loading or failing a different user's session list, clears stale lifecycle rows/selection/session details after user-list load failures, clears stale session rows/loading state when the session panel closes, clears viewed session rows/loading state after password resets, and has focused lifecycle count, query/role/status filter, visible-selection state/toggle, bulk result-message, and bulk lifecycle request payload coverage. Future admin lifecycle work should start only from one named backed lifecycle edge or one exact stale/loading/error UI finding. | New admin policy engines, SCIM sync history UI unless explicitly selected. |
| 6 | AI/webhook/notification validation | Verify existing AI document actions, webhook retry UX, notification preferences/delivery states, and document any blockers. AI document-action OpenAPI now enumerates the documented built-ins; AI chat response rendering now has focused component coverage; the AI panel now exposes guarded one-click `summarize`, `improve`, and `extract_actions` controls plus bounded guarded `translate`/`custom` controls backed by `/ai/action` with focused request payload coverage; Admin Webhooks now clears stale delivery rows and redelivery state while refreshing webhook selections or handling delivery-list failures and has focused redelivery UX evidence for per-delivery queueing lockout plus row-level failure feedback; desktop/mobile notifications now clear stale rows and unread counts after failed full notification refreshes; mobile notification pagination now has focused de-dupe coverage for older pages; NotificationBell polling now has dependency-stable refresh, focused failure-state coverage, visible unread badge cap coverage, and mark-all-read state coverage for clearing visible unread rows; notification preference type/channel validation, generated type/channel enums, notification channel-to-delivery target mapping, and webhook exact/wildcard subscription matching now have focused server coverage and API docs. | New AI capabilities, new provider integrations, broad notification redesign. |
| 7 | Editor small UX gap | Bookmark cards are wired through the slash command and now have focused portable directive round-trip coverage; Excalidraw references now have focused persisted/rendered directive coverage, a slash command that inserts the portable directive, embedded edit/view modals that save scene JSON to document-owned `.excalidraw` sidecar assets when document context is available, and portable filename normalization for unsafe pasted/imported directives. Future editor work should start only from one exact directive, sidecar asset, round-trip, or device-rendering finding. | Live collaboration, full CRDT awareness, block-level editor model, export/import workflows, and broad visual/device QA. |
| 8 | Search/SNQL next thin slice | Add one bounded type-aware filter family or clarify SNQL docs/tests for existing syntax. Negative presence, unlocked-state, exact `created:`/`updated:` date filters plus `created_on:`/`updated_on:`/`modified:` aliases, `folder_id:` folder aliasing, and `title:` substring filtering now have focused local/server/parser/OpenAPI/docs coverage plus saved-filter formatting coverage for newer bounded filters. | Full Starlane query language, semantic/RAG search. |
| 9 | Template automation closeout | Validate existing template-backed creation and document deferred import/export automation boundaries. Same-space template tag/property matching, ignored unknown/null metadata, local replay queue ordering, deferred import/export automation boundaries, and renderer handling for unknown placeholders plus null/missing property values are now documented or covered. Future template automation work should start only when a selected goal names one exact package, parser, or automation gap. | Full import/export system, complex automation engine. |

## Already-covered foundations to preserve

These should be treated as existing foundations unless current tests or code contradict them.

- Auth/OIDC/2FA/session/API-key foundations exist, including refresh-token sessions, revocation, service-account keys, profile editing, password reset paths, documented JSON auth middleware/rate-limit responses, focused logout current-session deletion coverage, focused password-reset session-revocation/reset-token consumption coverage, and docs-validator coverage for logout/session plus cookie/CSRF migration guidance.
- Core SCIM Users/Groups endpoints exist under `/scim/v2`, including discovery, lifecycle mutation, group management for IdP-managed groups, and audit events.
- Personal-space backend/local metadata exists for stable usernames, local snapshot metadata, owner display names, and server response fallbacks.
- Personal-space UI now preserves local owner metadata in space responses and labels personal-space owner identity in desktop/mobile space surfaces.
- Local-vault MCP fast-track request path is complete for accepted-token reads through the canonical dispatch gate; confirmed writes remain journal-only.
- Template CRUD, workflow templates, document workflow activation, daily-note templates, template-backed document metadata application, and editor template insertion exist.
- Search has lightweight filter syntax and local/server parity for the existing first-pass filter families, including title substring, folder/folder-ID matching, and exact created/updated metadata date filters with API-field and modified-date aliases.
- Admin users, webhooks, SMTP notification settings, and several lifecycle controls have first-pass UI/API coverage.

## Deferred Phase 1 or cross-phase scope

Do not pull these into a fast-track slice unless the user explicitly selects them.

- Live collaborative editing UI, awareness, remote cursors, and CRDT conflict UX unless a selected collaboration goal names one exact presence, cursor, conflict-state, or validation gap.
- Full SNQL/Starlane query language and semantic/RAG search.
- Listener-backed MCP stdio/SSE process binding, MCP write execution, import/export/workflow/admin MCP execution, and full MCP route coverage.
- Full template import/export and advanced automation.
- New AI capabilities beyond validating current config/chat/completion/document-action paths.
- Broad admin redesign or enterprise policy engines.
- Full plugin runtime enforcement and registry install execution.
- Full cloud/provider sync, production Google OAuth validation, and OS background wake validation.

## Left out for later

These are intentionally parked so a fast-track goal can move without becoming a hidden rebuild.

| Area | Leave out for now | Safe later trigger |
|---|---|---|
| SCIM | Full IdP sync UI, enterprise policy engines, and broad provisioning redesign. | Select only after admin token docs, sync-audit expectations, setup guidance, and existing endpoint validation are closed. |
| MCP | Listener-backed process binding, write execution, import/export/workflow/admin audit tools, and full route coverage. | Select only after explicitly choosing a post-fast-track MCP transport/write goal. |
| Search/SNQL | Full Starlane query language, semantic search, embeddings, and RAG. | Select one type-aware filter family or one parser/docs validation at a time. |
| Templates | Full import/export system and advanced automation engine. | Select only after current template-backed creation and metadata behavior is validated. |
| Collaboration | Live collaborative editing UI, awareness, remote cursors, and rich conflict UX. | Treat as Phase 4-sized work, not a Phase 1 fast-track slice. |
| Admin | Broad admin redesign, enterprise governance, and provider-wide policy controls. | Select only after existing lifecycle/API-backed screens have empty/error/loading polish. |
| Providers | Default-enabled full cloud sync, production Google OAuth validation, and OS background wake validation. | Keep in the storage/app-readiness lane, outside Phase 1 fast-track unless explicitly pulled in. |
| Plugins | Full plugin sandbox/runtime enforcement, registry install execution, broad permission policy UX, and third-party package lifecycle. | Select only after a separate plugin-runtime goal is opened. |
| AI | New AI features, RAG, semantic workspace search, streaming polish beyond existing route validation, and provider expansion. | Select only after current document-action, config, chat, and completion paths are validated. |
| Desktop/mobile app readiness | Physical-device iPhone/iPad validation, production provider OAuth, native background wake checks, packaging, and store distribution. | Keep in the app-readiness lane unless the selected Phase 1 slice directly touches the same screen. |

## Super fast-track goal prompt

```text
Create a focused Phase 1 “slice slice baby” goal.

Scope:
- Use `docs/docs/development/phase-1-fast-track.md` as the source of truth.
- Pick exactly one super-fast-track queue item at a time.
- Before editing, inspect implementation evidence so completed foundations are not rebuilt.
- Keep deferred scope parked in the “Left out for later” section.
- Prefer docs/test/state-hygiene/API-schema slices over broad feature work.
- Update `SCRATCHPAD.md`, a session log, changelog entries, and this fast-track board when the slice changes scope or status.
- Validate with `git diff --check`; if code changes, run the smallest relevant test/type/design checks.
- Commit and push each clean checkpoint.

Hard exclusions unless explicitly reselected:
- No live collaboration.
- No full SNQL or semantic/RAG search.
- No MCP listener-backed binding or write execution.
- No broad provider sync.
- No full plugin runtime.
- No new AI capabilities beyond validating current document-action/config/chat/completion paths.
```

## Audit reconciliation status

| Area | Reconciled position | Next safe action |
|---|---|---|
| SCIM | Core Users/Groups endpoints and discovery are already implemented and audited. Token operations, IdP connector mapping, audit review expectations, current boundaries, focused release-validation tests, and generated Users list/filter plus User/Group item route OpenAPI coverage are documented or validated. | Future SCIM/admin work should start only from one exact admin token-management, sync-history, operator workflow, or release-validation finding; do not rebuild endpoints by default. |
| MCP | Local-vault read fast-track request path exists for accepted-token reads through the canonical dispatch gate; server MCP manifest/read-tool execution has generated OpenAPI route-parameter coverage, focused HTTP execution guardrail tests for unknown tools, deferred write tools, non-admin users, and admin API keys missing the `admin` scope, and those authenticated scope/admin denials now return the documented 403 `forbidden` envelope; writes remain journal-only. | Future MCP work should start only from a selected post-fast-track goal naming one transport-binding, write-execution, import/export/workflow/admin audit, route-coverage, docs, schema, or support gap. |
| Templates | CRUD, workflow templates, daily-note templates, insertion, and template-backed metadata application exist. Same-space metadata matching, ignored unknown/null metadata, local replay queue ordering, generated template/workflow-template route details, deferred import/export automation boundaries, and renderer resilience for unknown placeholders plus null/missing property values are now covered. | Future template work should start only from one exact import/export package, parser, automation, metadata application, or renderer finding. |
| Search | Lightweight filter syntax and local/server parity exist for the first-pass filters. Negative presence, unlocked-state, exact `created:`/`updated:` date filters plus `created_on:`/`updated_on:`/`modified:` aliases, `folder_id:` folder aliasing, and `title:` substring filtering now have focused parser/local/server/OpenAPI/docs coverage plus saved-filter formatting coverage for newer bounded filters. | Future search work should start only from one bounded parser, docs, test, saved-filter, or selected SNQL/semantic-search goal. |
| AI/webhook/notification | AI config/chat/completion/action routes, Admin Webhooks, webhook delivery history/redelivery, SMTP notification settings, notification inbox pagination/read-state, and notification preferences exist. AI config/chat/completion/document-action routes have generated OpenAPI schemas, and document-action OpenAPI now enumerates the documented built-ins; AI chat response rendering now has focused component coverage; the AI panel now exposes guarded one-click `summarize`, `improve`, and `extract_actions` controls plus bounded guarded `translate`/`custom` controls backed by `/ai/action` with focused request payload coverage; Admin Webhooks now clears stale delivery rows and redelivery state while refreshing webhook selections or handling delivery-list failures and has focused redelivery UX evidence for per-delivery queueing lockout plus row-level failure feedback; desktop/mobile notifications now clear stale rows and unread counts after failed full notification refreshes; mobile notification pagination now has focused de-dupe coverage for older pages; NotificationBell polling now has dependency-stable refresh and focused failure-state coverage for clearing stale rows/native notification IDs plus visible unread badge cap and mark-all-read state coverage; webhook delivery/redelivery and notification read-state routes have generated route-parameter coverage; notification preference type/channel validation, generated type/channel enums, notification channel-to-delivery target mapping, and webhook exact/wildcard subscription matching now have focused server coverage and API docs. | Future AI/webhook/notification work should start only from one exact state edge, delivery retry finding, channel-mapping gap, or selected provider-capability goal. |
| Admin lifecycle | First-pass lifecycle, invite/import, reset, sessions, webhooks, SMTP notifications, and lifecycle controls exist. Admin Users now distinguishes initial lifecycle-list loading from true empty/filter-empty states, clears stale session rows while switching users or handling session-list failures, clears stale lifecycle rows/selection/session details after user-list load failures, clears stale session rows/loading state when the session panel closes, clears viewed session rows/loading state after password resets, has backend route coverage for admin password-reset target-session revocation while preserving other users' sessions, and covers lifecycle count summaries, query/role/status filtering, visible-selection state and select/deselect transitions, bulk result-message formatting, and the bulk lifecycle request payload shape. | Future admin lifecycle work should start only from one concrete backed empty/error/loading edge, lifecycle request/response gap, or selected admin-policy goal. |
| OpenAPI/Scalar | First-pass JSON/YAML/Scalar routes exist with broad route-family coverage, including a shared required-field `Error` schema matching the documented runtime envelope, health schemas, SCIM basics plus Users list/filter, User/Group item route details, and SCIM user PATCH conflict response coverage, MCP manifest/read-tool execution route details, public auth config schema, public page slug route details, auth token exchange/refresh schemas, password reset request/confirm schemas, generated 409 conflict responses for implemented auth, create/import, document-lock, and SCIM user PATCH conflicts, generated 429 rate-limit responses for rate-limited auth endpoints, OAuth/OIDC redirect responses plus OAuth provider route details, current-user profile read/update schemas, TOTP 2FA schemas, auth session/API-key route details plus full documented-route-surface regression coverage, user-directory route details, admin-user create/list/update/bulk lifecycle/password reset/session route details, admin user CSV import/template route details, admin group create/member route details, admin config list/upsert/delete route details, admin search reindex route details, audit-log filter/default/lower-bound/capped pagination query parameters, audit CSV export route details, plugin runtime audit-event route details, AI config/chat/completion/document-action route details, search query parameters, Dataview max-length/default/lower-bound/cap query parameters, notification inbox query parameters, webhook delivery/redelivery and notification read-state route details, template/workflow-template create-conflict route details, metadata tag nullable response fields, metadata tag/property/create-conflict/document-property/backlink route details, space/folder/folder-reorder/direct-member/group-grant/graph route details, core document list/read/update/content/starred/lock/publish/comment/resolve/reaction/version restore/attachment list-upload-delete/workflow/queue/deadline-escalation route details, document workflow route paths aligned to the implemented `{document_id}` parameter, membership/grant mutation success responses, document create/response/update/content/parent placement schemas or success responses, document tag attach success response, and webhook redelivery success response coverage. The broad Phase 7 audit row is now reconciled to avoid treating OpenAPI as a rebuild task. Full plugin runtime enforcement remains deferred. | Future OpenAPI/Scalar work should start only from one concrete implemented-route schema, response, parameter, route-annotation, or generated-docs gap. |

## Next action rule

The completion audit is done. Do not queue another generic Phase 1 audit or “tiny slice” goal by default.

Use one of these paths instead:

1. If inspection finds a concrete stale row, missing documented behavior, or untested state transition, open a one-slice goal that names that exact gap.
2. If the user selects a deferred lane, use one of the parked implementation prompts below and keep its scope narrow.
3. If neither condition is true, leave Phase 1 fast-track parked and continue a different active lane such as app-readiness, auth hardening, provider validation, plugin runtime, or post-fast-track MCP.

## Next implementation goal prompts

These prompts are parked. Use them only after explicitly selecting the larger lane.

### Option A — SCIM admin token/sync-history UI

```text
Create a focused Phase 1 SCIM admin token/sync-history UI goal.

Scope:
- Build only the admin-facing token management or sync-history UI selected for SCIM.
- Do not rebuild core SCIM endpoints.
- Reuse existing SCIM Users/Groups endpoint behavior, token/setup docs, audit expectations, and release-validation evidence.
- Update `docs/docs/development/phase-1-fast-track.md`, `docs/docs/development/phased-prompt-gap-audit.md`, `SCRATCHPAD.md`, changelog, and session log.
- Validate with relevant frontend/backend checks and `git diff --check`.
- Commit and push a clean checkpoint.
```

### Option B — Residual OpenAPI/Scalar coverage

```text
Create a focused Phase 1 residual OpenAPI/Scalar coverage goal.

Scope:
- Audit current OpenAPI/Scalar coverage for implemented Phase 1 route families.
- Do not add new endpoint behavior.
- Add missing detailed schemas or route-level annotations only after inspection names a concrete implemented-route gap.
- Update durable docs, scratchpad, changelog, and session log.
- Validate with the existing OpenAPI/docs checks if available, plus `git diff --check`.
- Commit and push a clean checkpoint.
```

## Notes

- This document is intentionally conservative. It should prevent “one more thing” creep.
- If a slice cannot be completed and validated in a small checkpoint, split it again.
