-- Track a session class so native applications can remain inactive for longer
-- than browser sessions. Existing rows are conservative browser sessions.
ALTER TABLE sessions ADD COLUMN session_kind TEXT NOT NULL DEFAULT 'browser';

CREATE INDEX IF NOT EXISTS idx_sessions_expiry_kind ON sessions(expires_at, session_kind);
