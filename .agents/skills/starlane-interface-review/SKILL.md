---
name: starlane-interface-review
description: Review Starlane Notes interfaces using the public design contract, shared components, themes and rendered evidence.
---

# Starlane Interface Review

Read [the contributor interface contract](../../../contributor-kit/reference/interface-contract.md)
and root AGENTS.md before starting. The contract link is relative to the installed public skill at
`.agents/skills/starlane-interface-review/`; canonical maintainers preview the exported tree to validate it.

Review only unless implementation is explicitly requested. Resolve the target diff or flow, inspect
shared component ownership and current behavior, then render relevant states when tooling is available.
Run applicable read-only checks. Group repeated symptoms under the shared root cause. Report severity,
location, current behavior, smallest correction and evidence. Separate confirmed defects from design
preferences and unverified risks. Block for inaccessible controls, data loss, broken permissions or
unusable journeys; distinguish major recurring drift from minor isolated polish.
End with a verdict, verified coverage and Not verified coverage. Do not mutate files or external
systems during a review-only task.

Follow CONTRIBUTING.md for validation and PR workflow. This skill grants no permission to commit,
push, deploy, publish, or rewrite history. No global or private skills are required.
