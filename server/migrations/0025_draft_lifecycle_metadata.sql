ALTER TABLE document_drafts ADD COLUMN previous_folder_id TEXT;
ALTER TABLE document_drafts ADD COLUMN previous_parent_id TEXT;

CREATE INDEX IF NOT EXISTS idx_document_drafts_retention
    ON document_drafts(updated_at);
