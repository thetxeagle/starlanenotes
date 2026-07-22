# Development

## Prerequisites

- Node.js 20+
- Rust 1.82+
- Docker, optional for container validation
- Android Studio + Android SDK for Android targets
- Xcode + Command Line Tools for iOS targets

## Local Web and Server

```bash
npm install
npm run dev
```

In another terminal:

```bash
cd server
cargo run
```

The Vite dev server proxies API and WebSocket requests to the backend on `localhost:3000`.

For isolated agent work, use:

```bash
npm run dev:isolated
npm run docs:validate
npm run docs:wiki
```

`npm run docs:validate` checks documentation completeness, environment-variable coverage, Docker/deployment docs, required wiki sections and index links, CI/wiki workflow wiring, wiki publish-secret docs, agent workflow inventories, legacy docs-tooling drift, and docs-tooling regressions such as wiki metadata filtering.
`npm run docs:wiki` renders the Markdown docs into Gitea Wiki pages under `artifacts/gitea-wiki` for local review.

See [Phased Prompt Gap Audit](development/phased-prompt-gap-audit.md) for the current implementation status against the original Starlane Notes phased prompts.

## Validation

Before release, tester handoff, or app-readiness review, use the full checklist in
[App Readiness Validation](self-hosting/app-readiness-validation.md). It includes web typecheck,
lint, tests, production build, Tauri cargo check, design validation, docs completeness validation,
and wiki rendering.

For focused day-to-day changes, run the smallest relevant subset:

```bash
npm run typecheck --workspace=apps/web
npm run lint --workspace=apps/web
npm test --workspace apps/web -- --run
npm run build --workspace=apps/web

cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml
npm run validate:design
npm run docs:validate
npm run docs:wiki
```

Use server `cargo test` commands for backend/API changes and keep tests scoped to the
changed surface when a full backend suite is unnecessary.

## Desktop and Mobile

```bash
npm run desktop:dev
npm run desktop:build

npm run mobile:android:init
npm run mobile:android:studio
npm run mobile:android:dev
npm run mobile:android:build

npm run mobile:ios:init
npm run mobile:ios:dev
npm run mobile:ios:doctor
```

Direct Xcode builds can fail because Tauri injects temporary build context. Prefer the Tauri wrapper commands for build and run workflows.
