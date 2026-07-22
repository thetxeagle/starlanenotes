# App Readiness Validation

Use this checklist before tagging a desktop/mobile app-readiness release or handing a build to testers.

## Repository Checks

Run from the repository root:

```bash
npm run --workspace apps/web typecheck
npm run --workspace apps/web lint
npm test --workspace apps/web -- --run
npm run --workspace apps/web build
cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml
npm run validate:design
npm run docs:validate
npm run docs:wiki
```

Expected status for the active v1.0.12 readiness branch:

- Web typecheck, lint, tests, and build should pass.
- Vite may still warn about the large TipTap editor chunk.
- Starlane design validation should pass; treat design-token, shared-control, banned-pattern, and semantic-color drift as readiness blockers for UI changes.
- Documentation completeness validation should pass; treat missing feature, environment-variable, Docker/deployment, wiki section/index-link, CI/wiki workflow, wiki publish-secret, agent workflow inventory, legacy docs-tooling coverage, or docs-tooling regression coverage as readiness blockers for relevant changes.
- `npm run docs:wiki` should render generated wiki pages under `artifacts/gitea-wiki/`.
- Native device readiness still requires explicit simulator or physical-device validation for iOS/iPadOS keyboard, properties-menu, iCloud vault, background-provider-sync, and production Google OAuth flows.

## App-Surface Checks

Validate these flows in the web app and, where applicable, the Tauri shell:

- First-run choice between standalone vault and linked server.
- Personal Space naming in local mode.
- Local-mode top bar hides sync/profile controls and keeps settings available.
- Settings gear opens site administration or local-mode storage/system settings.
- Server-mode System settings expose the maximum upload size setting unless `MAX_UPLOAD_SIZE_MB` locks it from the environment.
- Space selector dialog is centered and visually consistent with search.
- New-space dialog creates the space and switches to it.
- Sidebar row menus expose rename/delete for folders and notes.
- Site administration uses the admin sidebar, not the normal space sidebar.
- The top-left sidebar toggle collapses/expands the admin sidebar.

## Editor Checks

- Markdown mode is the default editing mode.
- Page View is read-only.
- Split mode is absent.
- The desktop/web formatting toolbar is absent; block and insert actions are reachable from `Ctrl+/` and context menus.
- Code block language selector appears only inside a code block and opens above other editor chrome.
- Code block indentation and blank lines survive mode switches/reload.
- Table insertion uses the grid picker.
- Table row/column add/delete actions are available from table context menus.
- Right-click context menu exposes copy, copy as Markdown, paste as Markdown, paste plain text, table actions, command menu, and block delete/duplicate where applicable.
- Block handle is hidden for tables and dismisses when clicking outside.

## Local Vault Checks

- Browser local mode can create spaces, folders, and documents.
- Browser local mode can export/import JSON snapshots.
- Tauri desktop can choose, reset, and reveal a vault folder.
- Tauri desktop can discover sibling vault folders, create a new vault folder, and switch the active vault from Admin Storage.
- Tauri desktop can export/import `.starlane-vault` archives with assets.
- Tauri vault folders include a generated `spaces/` Markdown mirror with per-space `documents/`, `attachments/`, and `templates/` folders.
- Tauri mobile shows sandbox-safe storage copy instead of desktop folder/archive controls, checks vault status without initializing the App-folder vault, restores and validates the last configured vault before mounting, returns to the three-section create/open/connect chooser with a "Needs repair" marker when startup validation fails, surfaces native storage action errors, keeps the default App folder rename path disabled in favor of creating or switching to a custom app-visible vault, exposes long-press vault rename/safe-trash actions for visible non-active vault folders, keeps the server-mode App Settings vault-switch submenu visible below Sync so users can create/open/connect another vault, and on iOS persists custom vault selection through a Documents-side fallback file if the native settings store is empty.

## Native Build Checks

### macOS

- `npm run desktop:build:app` builds `target/release/bundle/macos/Starlane Notes.app`.
- Distribution builds still require Developer ID signing, hardened runtime, notarization, and stapling.

### iOS

- `npm run mobile:ios:doctor` should pass Xcode, simulator, signing identity, and provisioning profile checks.
- Physical-device builds should use `APPLE_DEVELOPMENT_TEAM=H9FVZ6838A npm run mobile:ios:build`, then install the exported IPA payload with `xcrun devicectl device install app`.

### Android

- `npm run mobile:android:build` should produce an unsigned APK and AAB.

## Documentation Checks

- User, group, and permission behavior is documented.
- Local-first status is current and does not call implemented v1.0.12 behavior merely planned.
- Template documentation is explicit that document template CRUD/local persistence, desktop/sidebar/mobile picking, command-menu insertion, daily-note templates, workflow-template CRUD, document workflow activation, stage-transition foundations, and approval enforcement are implemented while template import/export depth and advanced template automation remain.
- `CHANGELOG.md`, `changelogs/v1.0.12.md`, and the active `sessions/` file are updated.

## Guideline Sources

Before a UI-heavy release, compare changed screens against:

- `repo-docs/guidelines/starlane-brand-baseline.md`
- `repo-docs/guidelines/starlane-design-system.md`
- `repo-docs/guidelines/starlane-app-overview.md`
- `repo-docs/guidelines/starlane-agent-isolation.md`
