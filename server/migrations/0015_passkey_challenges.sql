-- Server-side WebAuthn/passkey ceremony state.
-- Challenge state must not be stored client-side; each row is single-use and short-lived.

CREATE TABLE IF NOT EXISTS passkey_challenges (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT REFERENCES users(id) ON DELETE CASCADE,
    email TEXT,
    kind TEXT NOT NULL, -- registration, authentication
    state_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    expires_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_passkey_challenges_user_id ON passkey_challenges(user_id);
CREATE INDEX IF NOT EXISTS idx_passkey_challenges_email ON passkey_challenges(email);
CREATE INDEX IF NOT EXISTS idx_passkey_challenges_expires_at ON passkey_challenges(expires_at);
