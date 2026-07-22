# Privacy

Starlane Notes is self-hosted software. Most deployments are operated by the person or organization that installs the app, so the deployment administrator controls where the server runs, which authentication providers are enabled, which AI providers are configured, and how backups/logs are retained.

This page is a product privacy baseline for users, reviewers, and administrators. It is not a substitute for legal review before publishing a production privacy policy or App Store listing.

## Data stored by Starlane Notes

Depending on enabled features, a Starlane Notes deployment can store:

- account profile data such as email address, display name, role, avatar URL, and authentication metadata;
- spaces, folders, documents, comments, reactions, tags, properties, workflows, templates, attachments, and search indexes;
- audit records, webhook delivery records, notification records, API keys, sessions, and administrator configuration;
- local standalone vault data on desktop or mobile devices when local mode is used.

## Local and self-hosted behavior

- In self-hosted server mode, user data is stored on the server, database, and file storage configured by the deployment administrator.
- In local standalone mode, data is stored on the user device in the selected local vault location.
- Browser sessions are online-first and should not be treated as the canonical local vault. Browser storage is limited to preferences, temporary drafts, and caches.
- Backups, retention, monitoring, and access to server logs are controlled by the deployment administrator.

## Authentication and identity providers

Starlane Notes supports local authentication and optional OAuth/OIDC providers. When Google, GitHub, OIDC, SCIM, or other identity integrations are enabled, the identity provider may process login, profile, group, or provisioning data according to that provider's policies.

Administrators should document which providers are enabled for their deployment and keep local/email authentication available when required by App Store review or organizational policy.

## AI providers

AI features are optional and administrator-controlled. If enabled, document content or user prompts can be sent to the configured provider, such as Ollama, OpenAI, Anthropic, xAI, or another compatible service.

Administrators should disclose which AI providers are configured, whether data leaves the deployment, and whether provider-side retention or training settings apply. Users should not enable third-party AI providers for sensitive content unless that deployment's policy allows it.

## Webhooks and integrations

Webhooks, API keys, plugins, provider-sync flows, and future integration surfaces can send selected event or document-related data to configured destinations. Administrators should only enable integrations they trust and should rotate credentials when access changes.

## App Store privacy notes

Before submitting an iOS build, prepare App Store privacy answers for the exact review environment:

- whether the reviewer will use local mode, a demo server, or both;
- which authentication providers are enabled;
- whether AI providers are enabled or disabled by default;
- whether notifications, webhooks, provider sync, or external integrations are enabled;
- the public support URL and the public privacy policy URL.

For the release checklist, see [App Store review readiness](self-hosting/app-store-review-readiness.md).
