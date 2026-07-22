# App Store Review Readiness

Use this checklist before submitting Starlane Notes to Apple App Review. It is based on Apple App Review Guidelines risk areas that commonly affect self-hosted, Tauri, offline-first, and AI-capable apps.

Primary reference: [Apple App Review Guidelines](https://developer.apple.com/app-store/review/guidelines/).

## Current Status

| Area | Status | Notes |
| --- | --- | --- |
| Minimum functionality | In progress | App has real document, local-first, graph, search, desktop/mobile, and self-hosted value. Validate that the iOS build does not feel like a thin web wrapper. |
| App completeness | In progress | Signed iOS device-architecture build now succeeds and the placeholder app icon has been replaced. Use [Screenshots](../screenshots.md) and [App Store Metadata](app-store-metadata.md) to close release assets before submission. |
| Privacy | In progress | AI features are optional and server-controlled. App Store privacy answers and any third-party AI disclosure still need final review. |
| Login/reviewer access | Needs prep | Use [Reviewer Access](reviewer-access.md) to choose demo server, local mode, or both before submission. |
| Native device QA | In progress | Tauri iOS simulator and signed device-architecture builds succeed, and paired iPhone/iPad installs have passed across v1.0.12 checkpoints. Prior iPhone launch smokes have passed, but the latest signed v1.0.12 phone install still needs a manual launch smoke because the device launch command was not approved. iPad launch/install attempts can still be blocked when the device is locked. Deeper native workflow QA remains. |

## Guideline Risk Map

| Apple guideline | Risk | Starlane Notes control |
| --- | --- | --- |
| 2.1 App Completeness | Rejection for crashes, placeholders, broken URLs, unavailable backend, or incomplete features. | Submit only after device smoke tests, working setup flow, valid support/privacy URLs, and review notes are ready. |
| 2.3 Accurate Metadata | Rejection if screenshots/description promise unsupported features. | Metadata must describe the actual current build: self-hosted notes, optional local mode, document management, graph view, and optional AI. |
| 2.5 Software Requirements | Rejection for private APIs, deprecated tech, broken IPv6, or inappropriate web behavior. | Keep Tauri/WebKit usage standard, avoid private APIs, and smoke test on current iOS/iPadOS. |
| 4.2 Minimum Functionality | Rejection if app is just a repackaged website or lacks lasting utility. | Highlight native/mobile-specific value: offline access, local vault path, push/local notifications if enabled, share/deep-link behavior, mobile editor UX, and graph/search utility. |
| 4.3 Spam | Rejection for low-quality, generic, template-like apps. | Keep product name, icon, UI, docs, and review notes specific to Starlane Notes. Avoid generic AI/app-builder claims. |
| 4.8 Login Services | If third-party login is offered for primary account creation, Apple may require an equivalent login option. | Local auth exists. If Google/GitHub OAuth is enabled in the review environment, ensure local/email auth remains available or add Sign in with Apple before public App Store submission. |
| 5.1 Privacy | Rejection for unclear data use, unnecessary permission requests, or undisclosed third-party sharing. | Request only needed permissions. Disclose self-hosted/server data flow and require consent before sending user content to third-party AI providers. |

## Pre-Submission Checklist

### Product Completeness

- Verify all iOS icon slots render cleanly after the Notes logo replacement.
- Remove placeholder copy, debug labels, test URLs, and unfinished feature toggles from the submitted build.
- Confirm first launch gives users a clear choice between connecting to a server and using available local/offline behavior.
- Confirm empty states are helpful and do not look like broken screens.
- Confirm all links in Settings, About, Support, Privacy Policy, and documentation open correctly.

### Native iOS Readiness

- Run `npm run mobile:ios:doctor` and resolve every warning except optional `APPLE_BUNDLE_IDENTIFIER`.
- Continue signed device-architecture builds at release checkpoints.
- Run fresh physical install/launch smoke on paired iPhone and iPad before tester handoff, skipping locked devices without blocking unrelated validation.
- Run an iOS simulator smoke test for launch, navigation, editor, graph view, local mode, and server connection setup.
- Confirm the app works on the currently shipping iOS/iPadOS versions targeted by Xcode.
- Confirm orientation behavior matches App Store screenshots and does not expose broken layouts.

### Privacy And AI

- Do not request protected permissions unless the submitted build uses them.
- If notifications are enabled, explain the purpose before requesting permission.
- If AI is enabled, disclose whether document content can be sent to Ollama, OpenAI, Anthropic, xAI, or another provider.
- Require explicit user/admin configuration before third-party AI providers receive content.
- Keep App Store privacy labels aligned with the submitted review server and default app behavior.

### Reviewer Access

- Provide one of:
  - a stable demo server URL with reviewer credentials,
  - a built-in demo mode approved in review notes,
  - or a fully functional local/offline path that demonstrates core functionality without credentials.
- Include clear review notes for server mode, local mode, AI-disabled defaults, and any feature that depends on admin setup.
- Use [Reviewer Access](reviewer-access.md) to prepare the demo server, reviewer credentials, local-mode fallback, and review notes for the exact submitted build.
- Keep the backend online for the full review window if a demo server is provided.

### Metadata

- Screenshots must show the actual iOS app, not desktop or marketing mockups.
- Description should focus on concrete user value: self-hosted notes, document organization, markdown editing, graph view, search, and offline-first/local-first direction. Use [App Store Metadata](app-store-metadata.md) for the detailed metadata checklist.
- Do not claim full iCloud or Google Drive sync until those providers are production-ready.
- Do not imply Apple endorsement or use third-party logos in icons/screenshots without permission.
- Support URL and privacy policy URL must be public, stable, and accurate. Use [Support](../support.md) and [Privacy](../privacy.md) as the documentation baseline before publishing final URLs.

## Suggested Review Notes

Use this as a starting point and edit it for the exact submitted build:

```text
Starlane Notes is a self-hosted, offline-first document management app.

The submitted iOS build supports document browsing/editing, spaces/folders, search, graph view, and local/offline app flows. Server mode can be tested with the demo environment below.

Demo server:
URL: <demo-server-url>
Email: <reviewer-email>
Password: <reviewer-password>

AI features are optional and disabled unless configured by an administrator. If enabled, user document content may be sent to the configured provider. No third-party AI provider is required to test the core app.

If testing without the demo server, use local mode from first launch to create and manage local documents.
```

## Preflight Commands

Run these before creating the archive for App Review:

```bash
npm run typecheck --workspace=apps/web
npm run lint --workspace=apps/web
npm test --workspace apps/web -- --run
npm run build --workspace=apps/web
cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml
npm run validate:design
npm run docs:validate
npm run docs:wiki
npm run mobile:ios:doctor
```

After signing is configured, run:

```bash
APPLE_DEVELOPMENT_TEAM=<team-id> npm run mobile:ios:doctor
APPLE_DEVELOPMENT_TEAM=<team-id> ./scripts/mobile/ios-tauri.sh build --debug -t aarch64-sim --ci
APPLE_DEVELOPMENT_TEAM=<team-id> ./scripts/mobile/ios-tauri.sh build --debug -t aarch64 --ci
```

## Known Open Items

- Run fresh paired iPhone/iPad physical install/launch smoke before tester handoff.
- Publish public support and privacy policy URLs from the current [Support](../support.md) and [Privacy](../privacy.md) baselines.
- Prepare final App Store description, keywords, screenshots, support URL, and privacy policy URL using [App Store Metadata](app-store-metadata.md).
- Decide whether first App Store submission supports server-only review, local-mode review, or both.
- Prepare reviewer demo credentials or a complete local/demo workflow using [Reviewer Access](reviewer-access.md).
