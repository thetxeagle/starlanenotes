ALTER TABLE documents ADD COLUMN parent_id TEXT REFERENCES documents(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_documents_parent_id ON documents(parent_id);
