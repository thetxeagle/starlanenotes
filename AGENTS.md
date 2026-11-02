# Contributor agent guidance

Starlane Notes is source-available under [PolyForm Noncommercial 1.0.0](LICENSE).
Read [CONTRIBUTING.md](CONTRIBUTING.md) before changing code. AI tooling is optional.

## Project map

- `apps/web`: React + TypeScript web application.
- `server`: Rust/Axum backend; SQLite and PostgreSQL behavior must remain compatible.
- `apps/desktop/src-tauri`: Tauri shell and platform integrations.
- `packages/shared-types`: shared TypeScript types.
- `docker`: application build and deployment assets.

## Working agreement

Inspect current code and tests, preserve unrelated work, and keep changes scoped to the request.
Use the commands and affected-layer checks in CONTRIBUTING.md. Report checks actually run and
coverage that remains unverified. Do not claim runtime or device success from a compile alone.

For interfaces, read the [public interface contract](contributor-kit/reference/interface-contract.md)
and use the relevant optional skill:

- [Starlane interface design](.agents/skills/starlane-interface-design/SKILL.md)
- [Starlane interface review](.agents/skills/starlane-interface-review/SKILL.md)

Use the fork/branch/pull-request workflow. This guidance does not authorize an agent to commit,
push, deploy, publish images, or rewrite history. Follow the contributor's explicit instructions.
Do not add credentials, personal captures, machine configuration, or session records to a PR.
Optional private work notes can use the [session template](contributor-kit/reference/session-template.md).
Update the public changelog for user-visible changes; see CONTRIBUTING.md.
