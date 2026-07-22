# Screenshots

This page tracks the screenshots that should appear in the README, docs portal, Gitea Wiki, and release metadata. It intentionally separates documentation screenshots from App Store screenshots so marketing assets do not drift from the actual app.

## Current asset status

The first reusable documentation/README capture set is available under `images/screenshots/`. These desktop web captures use a disposable local server and fictional workspace content; they do not replace the physical-device App Store capture set.

## Documentation capture set

Place reusable docs/README images under `images/screenshots/`. Use linked thumbnails so the page renders cleanly in both Gitea and GitHub:

```markdown
[![Dashboard](../../images/screenshots/dashboard-thumb.png)](../../images/screenshots/dashboard.png)
```

Required first-pass documentation scenes:

- [Document editor](../../images/screenshots/document-editor.png)
- [Spaces and folder navigation](../../images/screenshots/spaces-and-folders.png)
- [Search results](../../images/screenshots/search.png)
- [Site Administration](../../images/screenshots/site-administration.png)

## App Store capture set

App Store screenshots must show the actual iOS or iPadOS app, not desktop screenshots or marketing mockups. Capture these from the same build family that will be submitted:

- iPhone portrait: setup or home screen
- iPhone portrait: document editor with mobile toolbar visible
- iPhone portrait: spaces/folders navigation
- iPad portrait or landscape: document editor
- iPad landscape or Split View: graph/search or admin/settings surface

Before capture, confirm the [App Store review readiness](self-hosting/app-store-review-readiness.md) checklist is current and that the screenshots do not imply unsupported full-provider sync, plugin execution, or unreleased collaboration features.

## Capture notes

- Use the Starlane Notes icon, product name, and current theme tokens.
- Avoid debug data, private server URLs, test email addresses, or unfinished feature flags.
- Prefer sample content that demonstrates real document, folder, search, graph, and local/server setup value.
- Keep alt text descriptive when images are added.
- Re-capture the set when the visible app shell or core workspace interactions materially change.
