# Contributing

## Project Structure

```
StarlaneNotes/
  apps/
    web/          # React 18 + TypeScript frontend
    desktop/      # Tauri v2 desktop wrapper
  server/         # Rust/Axum backend
    src/
      auth/       # JWT, OAuth, OIDC, TOTP, middleware
      spaces/     # Spaces, documents, folders, comments, properties
      search/     # Tantivy full-text search
      ai/         # AI providers (Ollama, OpenAI, Anthropic, xAI)
      webhooks/   # Webhook management and dispatch
      provisioning/ # CSV user import
      notifications/ # Notification system
      audit/      # Audit logging
      sync/       # CRDT sync via WebSocket
      storage/    # File I/O, path sanitization
      db/         # Database initialization
    migrations/   # SQL migrations (SQLx)
  packages/
    shared-types/ # Shared TypeScript types
  docs/           # Markdown docs synced to Gitea Wiki
  docker/         # Docker Compose setup
```

## Development Setup

### Prerequisites

- Rust (stable toolchain)
- Node.js 18+
- npm

### Backend

```bash
cd server
cp ../docker/.env.example ../.env   # optional local server env
cargo run
```

The server starts on port 3000 by default (override with `PORT` env var). SQLite is the default database -- no external services needed. Migrations run automatically on startup.

Key environment variables for development:

| Variable | Default | Notes |
|---|---|---|
| `SQLITE_ENABLED` | `true` | Set to `false` to use PostgreSQL |
| `POSTGRES_HOST` | empty | Required when `SQLITE_ENABLED=false` |
| `POSTGRES_PORT` | `5432` | PostgreSQL port |
| `POSTGRES_USER` | `starlane` | PostgreSQL username |
| `POSTGRES_PASSWORD` | empty | Required when `SQLITE_ENABLED=false` |
| `POSTGRES_DB` | `starlane_notes` | PostgreSQL database name |
| `JWT_SECRET` | dev-only default | Auto-set in dev, required in production |
| `PORT` | `3000` | Server listen port |
| `BASE_URL` | `http://localhost:3000` | Used for CORS, OAuth callbacks |

### Frontend

```bash
cd apps/web
npm install
npm run dev
```

The frontend dev server proxies API requests to the backend at `localhost:3000`.

### Desktop + Mobile (Tauri v2)

From repo root:

```bash
# Desktop dev shell
npm run desktop:dev

# Desktop package build
npm run desktop:build

# macOS .app-only build for local manual packaging checks
npm run desktop:build:app

# Android (requires Android Studio + SDK + NDK + JDK)
npm run mobile:android:init
npm run mobile:android:studio
npm run mobile:android:dev
npm run mobile:android:build

# iOS (requires Xcode on macOS)
npm run mobile:ios:init
npm run mobile:ios:dev
npm run mobile:ios:build
```

Notes:

- Mobile targets are managed from `apps/desktop/src-tauri/` in this repository.
- Desktop/mobile packages are manual local builds today; Gitea Actions currently publish web/server artifacts and containers, not signed desktop/mobile installers.
- Use a physical device or emulator/simulator for runtime validation.
- Android setup guardrails:
  - Install Android SDK via Android Studio.
  - Set `ANDROID_HOME` to your SDK path (commonly `~/Library/Android/sdk` on macOS).
  - Run `npm run mobile:android:init` before first `mobile:android:dev`.
  - Open `apps/desktop/src-tauri/gen/android` directly in Android Studio, or use `npm run mobile:android:studio`.
  - Use Android Studio for resource edits, emulator/device tools, and Gradle diagnostics; use Tauri commands for app build/run validation.
- iOS setup guardrails:
  - Run `npm run mobile:ios:init` before first `mobile:ios:dev` to generate Apple project files.
  - Ensure Xcode simulator is available (`xcrun simctl list` works).
  - Configure signing before device builds:
    - set `APPLE_DEVELOPMENT_TEAM` (consumed by `mobile:ios:*` wrapper scripts), or
    - set Tauri iOS config/Xcode project signing directly.
  - Optional: set `APPLE_BUNDLE_IDENTIFIER` to override bundle id for local device builds.
- If signing/updater keys are needed, generate and store them outside the repo.
- Repository secrets must not start with `GITEA_` or `GITHUB_`.
- For updater key + secret workflow, see `self-hosting/tauri-updater-signing.md`.

### Documentation

```bash
npm run docs:validate
npm run docs:wiki
```

`npm run docs:validate` checks documentation completeness, environment-variable coverage, Docker/deployment docs, required wiki sections and index links, CI/wiki workflow wiring, wiki publish-secret docs, agent workflow inventories, legacy docs-tooling drift, and docs-tooling regressions such as wiki metadata filtering. The generated wiki files are written to `artifacts/gitea-wiki`. Maintainers can publish them with `npm run docs:wiki:publish`; CI also syncs the Gitea Wiki when docs change on `main` or version branches.

## Code Style

### Rust

- Format with `rustfmt` (config in `rustfmt.toml` at project root)
- Follow existing patterns for route handlers: extract auth, validate input, execute query, dispatch webhooks/audit, return response
- Use `AppError` variants for error responses
- All database queries use parameterized bindings (`?1`, `?2`, etc.)

### TypeScript/React

- Tailwind CSS for styling, using theme tokens (`var(--primary)`, `var(--bg)`, etc.)
- shadcn/ui components
- No glassmorphism, no oversized rounded corners, no decorative gradients
- Design references: Linear, Raycast, Stripe, GitHub

## Pull Request Process

1. Create a branch from `main` with the naming convention: `v{X.X.X}` (e.g., `v1.0.5`)
2. Make changes and test locally
3. Run the smallest relevant checks for the changed surface. Before release, tester handoff, or
   app-readiness review, follow the full [App Readiness Validation](self-hosting/app-readiness-validation.md)
   checklist: web typecheck, lint, tests, production build, Tauri cargo check, Starlane design
   validation, docs completeness validation, and wiki rendering.
4. For backend/API changes, run the focused Rust tests that cover the changed route, storage path,
   sync path, or schema contract.
5. Write a clear commit message using Conventional Commits format: `type(starlanenotes): description`
6. Open a PR against `main` with a summary of changes and the checks you ran.

## Focused Checks

- Sync replay tests run with `cargo test -p starlane-notes-server sync::tests`.
- To exercise the gated PostgreSQL replay fixture, set `STARLANE_SYNC_REPLAY_POSTGRES_URL` to a disposable PostgreSQL database URL before running the same command. The test creates and drops its own temporary schema.

## Architecture Notes

- **File-first**: The `.md` file on disk is the canonical source of truth. The database is an index and cache.
- **Offline-first**: CRDT sync via automerge-rs allows the app to work fully offline.
- **Single binary**: The Rust server embeds the React static files via `include_dir!` for production deployment.
- **DB flexibility**: SQLite is automatic under `DATA_DIR`, swap to PostgreSQL with `SQLITE_ENABLED=false` and the `POSTGRES_*` variables.

## License

PolyForm Noncommercial 1.0.0 -- no paywalled features, no vendor lock-in. Contributions must remain compatible with the repository license and the Starlane trademark policy.
