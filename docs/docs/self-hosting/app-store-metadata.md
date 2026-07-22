# App Store Metadata

Use this page to prepare App Store product metadata for Starlane Notes. Keep the metadata aligned with the submitted build and the currently validated feature set.

## Required metadata fields

Prepare these fields before submission:

- app name and subtitle;
- short promotional text, if used;
- full description;
- keywords;
- support URL;
- privacy policy URL;
- screenshots for each submitted device class;
- review notes and reviewer access details.

Use [Support](../support.md), [Privacy](../privacy.md), [Screenshots](../screenshots.md), and [Reviewer Access](reviewer-access.md) as the source docs before publishing final App Store values.

## Description boundaries

The description should focus on concrete current value:

- self-hosted notes and document management;
- spaces, folders, markdown editing, graph view, and search;
- optional local/offline mode where validated for the submitted build;
- optional AI only when the review environment explains provider configuration and data flow.

Do not claim unsupported or deferred capabilities as shipped:

- full iCloud or Google Drive sync;
- live collaborative cursors or presence;
- plugin runtime execution or third-party plugin installation;
- full semantic/RAG search;
- production provider background sync unless that exact build has device evidence.

## Metadata review checklist

- Screenshots show the actual iOS or iPadOS app, not desktop captures or mockups.
- Support URL and privacy policy URL are public, stable, and match the submitted review environment.
- Keywords describe the app honestly and do not imply unrelated SaaS, AI, or Apple endorsement.
- Review notes match the selected reviewer path: demo server, local mode, or both.
- Any unavailable provider-sync, AI, plugin, or collaboration feature is omitted from customer-facing claims or clearly marked as future work outside App Store metadata.

For the full release gate, see [App Store review readiness](app-store-review-readiness.md).
