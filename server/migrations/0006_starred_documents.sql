-- Per-user starred document shortcuts.

CREATE TABLE IF NOT EXISTS starred_documents (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    starred_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (user_id, document_id)
);

CREATE INDEX IF NOT EXISTS idx_starred_documents_user_id ON starred_documents(user_id);
CREATE INDEX IF NOT EXISTS idx_starred_documents_document_id ON starred_documents(document_id);
