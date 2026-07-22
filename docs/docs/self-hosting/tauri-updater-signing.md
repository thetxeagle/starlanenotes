# Tauri Updater Signing (Manual/Future Release Flow)

Starlane Notes has Tauri updater configuration in the desktop shell, but the current release workflow does not publish signed desktop updater artifacts automatically. Desktop and mobile packages are built manually with local platform tooling until signing/notarization/device-release automation is intentionally reintroduced.

Use this page when preparing a trusted local desktop release or when re-enabling automated desktop updater artifact publication.

## 1) Generate signer keys (once)

Run on your trusted machine:

```bash
mkdir -p ~/.tauri
cargo tauri signer generate --write-keys ~/.tauri/starlane-notes.key
```

This creates:

- Private key: `~/.tauri/starlane-notes.key` (never commit)
- Public key: `~/.tauri/starlane-notes.key.pub` (safe to share)

## 2) Prepare signing secrets

For manual local builds, keep these values on your trusted build machine or in a password manager:

- `TAURI_SIGNING_PRIVATE_KEY` (contents of `~/.tauri/starlane-notes.key`)
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (passphrase used during key generation)

If desktop updater automation is reintroduced later, add the same values as repository secrets in Gitea Actions.

Important:

- Gitea repository secret names must not start with `GITEA_` or `GITHUB_`.
- Keep the private key and passphrase in a password manager.

## 3) Confirm desktop updater config

`apps/desktop/src-tauri/tauri.conf.json` should include:

- `plugins.updater.pubkey` set to the generated public key
- `plugins.updater.endpoints` set to the Gitea releases latest JSON URL:

`https://github.com/hbftechnologies/starlanenotes/releases/latest/download/latest.json`

## 4) Release flow requirements

For desktop auto-update to work, tagged releases must publish signed Tauri bundles and `latest.json`.

Current repository release workflows build web/server/container artifacts and container images. If desktop updater artifacts are added to release automation later, use the same signing secrets above.
