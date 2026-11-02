ALTER TABLE documents ADD COLUMN published_at TEXT;

UPDATE documents
SET published_at = updated_at
WHERE published_at IS NULL;

CREATE TABLE IF NOT EXISTS document_drafts (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    space_id TEXT NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    owner_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    icon TEXT,
    cover_image TEXT,
    content TEXT NOT NULL DEFAULT '',
    base_updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_document_drafts_document_id
    ON document_drafts(document_id);
CREATE INDEX IF NOT EXISTS idx_document_drafts_space_owner
    ON document_drafts(space_id, owner_id);
CREATE INDEX IF NOT EXISTS idx_document_drafts_updated_at
    ON document_drafts(updated_at);

CREATE TABLE IF NOT EXISTS document_draft_access (
    draft_id TEXT NOT NULL REFERENCES document_drafts(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('viewer', 'editor')),
    granted_by TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (draft_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_document_draft_access_user
    ON document_draft_access(user_id);
