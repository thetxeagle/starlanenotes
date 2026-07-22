-- Passkey credential storage for upgraded databases.
-- Fresh installs create this table from 0001; existing installs that already ran
-- the original initial schema need this idempotent forward migration.

CREATE TABLE IF NOT EXISTS passkeys (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    credential_id TEXT NOT NULL UNIQUE,
    public_key TEXT NOT NULL,
    name TEXT NOT NULL,
    sign_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    last_used_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_passkeys_user_id ON passkeys(user_id);
