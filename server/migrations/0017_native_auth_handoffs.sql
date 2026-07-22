-- One-time, PKCE-bound browser-to-native authentication handoffs.
CREATE TABLE IF NOT EXISTS native_auth_handoffs (
    id TEXT PRIMARY KEY NOT NULL,
    code_challenge TEXT NOT NULL,
    user_id TEXT REFERENCES users(id) ON DELETE CASCADE,
    code_hash TEXT,
    expires_at TEXT NOT NULL,
    consumed_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_native_auth_handoffs_expires_at ON native_auth_handoffs(expires_at);
