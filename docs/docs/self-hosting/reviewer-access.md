# Reviewer Access

Use this page to prepare reviewer access for TestFlight, App Store Review, or external testers. It turns the App Store readiness checklist into a concrete handoff plan without committing the project to one review path too early.

## Choose a review path

Pick one review path before submission. If you choose a demo server path, keep the exact access details current for the submitted build.

| Path | Use when | Required evidence |
| --- | --- | --- |
| Demo server | The review build should exercise server-backed accounts, users/groups, webhooks, notifications, or admin flows. | Public HTTPS URL, reviewer credentials, server uptime window, seeded sample workspace, and support contact. |
| Local mode | The review build can demonstrate core notes, folders, search, graph, and editor behavior without credentials. | First-launch local-mode flow, seeded or easily created sample content, no dependency on a private backend. |
| Both | You want reviewers to see server mode while keeping local mode as a fallback. | Demo server details plus clear local-mode fallback instructions in review notes. |

## Demo server checklist

- Use a public HTTPS URL reachable outside the VPN.
- Create reviewer credentials with the minimum role needed to test the submitted feature set.
- Seed a small workspace with non-private sample documents, folders, tags, search examples, graph links, and optional admin examples.
- Keep local/email authentication available if Google, GitHub, or OIDC login is enabled for the review environment.
- Keep AI-disabled defaults unless the review notes explicitly explain the configured provider and data flow.
- Keep the backend online for the full review window and document who can restart it.

## Local-mode checklist

- Confirm first launch can enter local mode without a server URL or reviewer credentials.
- Confirm reviewers can create a space, folder, document, and searchable content from a clean install.
- Confirm graph/search/editor flows work without internet access after setup.
- Avoid showing server-only settings as required actions in the local review path.

## Review notes template

Edit this for the exact submitted build:

```text
Starlane Notes can be reviewed in <server mode / local mode / both>.

Server mode:
URL: <demo-server-url>
Email: <reviewer-email>
Password: <reviewer-password>

Local mode:
On first launch, choose local/offline mode and create a local workspace. No credentials are required for this path.

AI-disabled default:
AI features are optional and disabled unless an administrator configures a provider. No third-party AI provider is required to test the core app.

Known review notes:
- <list any setup details, unavailable provider-sync features, or device-specific notes for this exact build>
```

## Related release docs

- [App Store review readiness](app-store-review-readiness.md)
- [Privacy](../privacy.md)
- [Support](../support.md)
