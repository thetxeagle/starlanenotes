CREATE TABLE IF NOT EXISTS template_drafts (
    id TEXT PRIMARY KEY NOT NULL,
    template_id TEXT NOT NULL REFERENCES templates(id) ON DELETE CASCADE,
    space_id TEXT NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    owner_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    snapshot TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_autosaved_at TEXT,
    lock_owner_id TEXT,
    lock_expires_at TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_template_drafts_template_id ON template_drafts(template_id);
CREATE INDEX IF NOT EXISTS idx_template_drafts_space_id ON template_drafts(space_id);
CREATE INDEX IF NOT EXISTS idx_template_drafts_updated_at ON template_drafts(updated_at);

CREATE TABLE IF NOT EXISTS template_revisions (
    id TEXT PRIMARY KEY NOT NULL,
    template_id TEXT NOT NULL REFERENCES templates(id) ON DELETE CASCADE,
    space_id TEXT NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    revision_number INTEGER NOT NULL,
    snapshot TEXT NOT NULL,
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_template_revisions_number
    ON template_revisions(template_id, revision_number);
CREATE INDEX IF NOT EXISTS idx_template_revisions_template_id
    ON template_revisions(template_id, created_at DESC);
