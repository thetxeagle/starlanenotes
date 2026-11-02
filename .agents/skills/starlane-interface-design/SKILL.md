---
name: starlane-interface-design
description: Build or refactor Starlane Notes interfaces using the public design contract, shared components, themes and rendered evidence.
---

# Starlane Interface Design

Read [the contributor interface contract](../../../contributor-kit/reference/interface-contract.md)
and root AGENTS.md before starting. The contract link is relative to the installed public skill at
`.agents/skills/starlane-interface-design/`; canonical maintainers preview the exported tree to validate it.

Implement the requested interface with existing Starlane components and preserve behavior outside
the request. Inspect callers, tokens and tests first. Identify the affected object, primary action,
state transitions and failure/recovery behavior. Reuse shared control owners and implement complete
relevant states in both themes. Verify the integrated journey, keyboard interaction and responsive
behavior with realistic fictional content. Run `npm run validate:design` and affected tests.
Report changed behavior, rendered evidence and unverified coverage. Do not change data, permissions
or product semantics merely to improve a screenshot.

Follow CONTRIBUTING.md for validation and PR workflow. This skill grants no permission to commit,
push, deploy, publish, or rewrite history. No global or private skills are required.
