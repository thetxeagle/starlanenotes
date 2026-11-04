# Release notes

## Unreleased

- Fix ARM64 server compilation by converting the native file link count to its portable stamp type.

- Update public repository links and Docker registry defaults for the transferred GitHub owner.

- Add a public contributor guide, sanitized agent guidance, optional Starlane interface skills and
  private-note template; validate their exact export paths and standalone design-check dependency.

- Refresh current UI captures and add a full-size repository gallery covering dark/light editing,
  document contents navigation and local workspace home.

- Refresh the product overview and Docker named-volume setup instructions.
- Public source snapshots omit private operations and documentation infrastructure.
- Automatic desktop updates are disabled in public snapshots until a public signed feed is configured.

## 1.5.6

- Use portable SQL bindings for the SQLite and PostgreSQL server workflows.
- Keep personal profile actions separate from workspace and site administration.
- Align application, server, desktop and shared-package version metadata.

PostgreSQL core workflows have isolated Linux validation. This does not establish
physical-device, live-provider or cross-platform acceptance. See the
[deployment guide](https://docs.starlanenotes.com/self-hosting/docker/) for scope and recovery limits.
