# Contributing to Starlane Notes

Contributions are welcome through GitHub issues and pull requests. This project is
[source-available under PolyForm Noncommercial 1.0.0](LICENSE), not OSI open source.
Read [TRADEMARKS.md](TRADEMARKS.md) before distributing a modified version.

## Fork, change, and review

1. Fork the public repository and create a topic branch from the target version branch, or from
   main when no target is specified. Check the issue or ask maintainers when the target is unclear.
2. Make a focused change with relevant regression tests and documentation. Preserve unrelated work.
3. Run the affected checks below and review the entire diff for credentials and personal artifacts.
4. Commit using `type(starlanenotes): description`, push your branch to your fork, and open a pull
   request describing the problem, resulting behavior, validation, and remaining limitations.

The public repository is a curated product snapshot with independent history. Maintainers reconcile
accepted changes into the canonical repository before the next export. Contributors do not need
access to the canonical host, its release automation, or personal workflow skills.

## Local setup and checks

Use Node.js matching package.json (`^20.19.0 || >=22.12.0`) and npm. From the repository root:

```sh
npm ci
npm run dev:isolated
```

The web development server uses http://127.0.0.1:5199. Local mode supports frontend work without
server credentials. For server-backed setup see the [deployment documentation](https://docs.starlanenotes.com/self-hosting/docker/).

```sh
npm run typecheck
npm test --workspace @starlane-notes/web -- --maxWorkers=2
npm run build
```

For UI changes also run `npm run validate:design` and verify the relevant journey in both themes,
keyboard navigation and responsive layouts. The exported design check scans application code;
maintainer-only cross-repository governance synchronization is not part of this command.
For notification changes run `npm run validate:notifications`.

Backend changes need Rust/Cargo, Python 3 for storage fixtures, and the native build prerequisites
in [docker/Dockerfile](docker/Dockerfile). Run `cargo test -p starlane-notes-server`; report any database
fixture or platform coverage not exercised. Preserve PostgreSQL and SQLite support. Desktop work
requires the relevant platform toolchain and Tauri CLI; `npm run desktop:dev` starts that workflow.
Do not assume a web build validates native behavior.

## Interface work and optional agent tools

Read the [interface contract](contributor-kit/reference/interface-contract.md).
The public [AGENTS.md](AGENTS.md) describes the optional Starlane design and
review skills. In exported snapshots it lives at the root and skills live under `.agents/skills/`.
Tools that do not discover those skills automatically can read their SKILL.md instructions directly.
No AI subscription or global skill installation is required to contribute.

## Documentation and release notes

Explain user-visible behavior and setup changes in the PR and update affected repository docs.
Product documentation is maintained separately; describe required guide changes and link the relevant
[documentation page](https://docs.starlanenotes.com/) in your PR so maintainers can reconcile them.
Public contributors do not need the documentation build infrastructure.

Add a concise bullet under Unreleased in the public CHANGELOG.md for features, fixes, security,
deployment changes, or release-relevant documentation. Describe the outcome, not the editing process.
Maintainers of the canonical checkout edit CHANGELOG.public.md for this exported file.
Do not change release versions or add speculative readiness claims as part of an unrelated fix.

Session logs are optional private working notes. The [template](contributor-kit/reference/session-template.md)
can help preserve decisions and evidence; put the useful summary in the PR, not personal logs in Git.
