# Starlane Notes

Welcome to the Starlane Notes documentation.

## What is Starlane Notes?

Starlane Notes is a fully open-source, self-hosted document management system. It is offline-first, free forever, and designed for individuals and teams who want full ownership of their data.

## Quick Start

See [Docker setup](self-hosting/docker.md) to get started in minutes.

## Self-hosting

- [Docker setup](self-hosting/docker.md)
- [Starlight docs portal](self-hosting/docs-portal.md)
- [Environment variables](self-hosting/environment-variables.md)
- [Upgrading](self-hosting/upgrading.md)
- [Gitea Actions runner](self-hosting/gitea-actions-runner.md)
- [Gitea container registry](self-hosting/gitea-container-registry.md)
- [App readiness validation](self-hosting/app-readiness-validation.md)
- [App Store review readiness](self-hosting/app-store-review-readiness.md)
- [App Store metadata](self-hosting/app-store-metadata.md)
- [Reviewer access](self-hosting/reviewer-access.md)
- [Tauri mobile prerequisites](self-hosting/tauri-mobile-prerequisites.md)
- [Tauri updater signing](self-hosting/tauri-updater-signing.md)

## Authentication

- [Local authentication](authentication/local-auth.md)
- [Google OAuth](authentication/oauth-google.md)
- [GitHub OAuth](authentication/oauth-github.md)
- [OIDC](authentication/oidc.md)
- [2FA and passkeys](authentication/2fa.md)
- [SCIM](authentication/scim.md)
- [Auth hardening](authentication/auth-hardening.md)

## User provisioning

- [CSV user import](user-provisioning/csv-import.md)
- [SSO provisioning](user-provisioning/sso-provisioning.md)

## Features

- Markdown-first editor with live preview
- Local standalone vault mode for desktop/mobile app readiness
- Spaces, folders, and document organization
- Users, groups, and space permission controls
- Full-text search
- AI integration (Ollama, OpenAI, Anthropic)
- OAuth2, OIDC, 2FA, and passkey authentication
- Webhooks and REST API
- Space-level document templates
- First-pass plugin manifest contract
- Desktop and mobile apps

## Feature guides

- [Documents](features/documents.md)
- [Spaces](features/spaces.md)
- [Properties](features/properties.md)
- [Local-first mode](features/local-first-mode.md)
- [Themes](features/themes.md)
- [Users, groups, and permissions](features/users-groups-permissions.md)
- [AI integration](features/ai-integration.md)
- [Plugin system](plugins/overview.md)
- [Screenshots](screenshots.md)
- [Privacy](privacy.md)
- [Support](support.md)

## API

- [API overview and Scalar reference](api/overview.md)
- [API authentication](api/authentication.md)
- [Webhooks](api/webhooks.md)
- [Notifications](api/notifications.md)
- [MCP](api/mcp.md)

## Development

- [Contributing](contributing.md)
- [Development overview](development.md)
- [Phase 1 fast-track](development/phase-1-fast-track.md)
- [Phased prompt gap audit](development/phased-prompt-gap-audit.md)

## Feature Status Notes

- CRDT collaboration plumbing exists in the stack, but the current app-readiness focus is local vault persistence and server-backed document workflows.
- Document template CRUD, desktop/sidebar/mobile picking, command-menu insertion, daily-note templates, workflow templates, document workflow activation, and stage-transition foundations are implemented.
- Plugin runtime is not active yet; [Plugin System](plugins/overview.md) documents the first-pass manifest contract and remaining sandbox/runtime work.
