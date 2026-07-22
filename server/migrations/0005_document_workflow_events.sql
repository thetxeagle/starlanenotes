-- ============================================================
-- Document Workflow History
-- ============================================================

CREATE TABLE IF NOT EXISTS document_workflow_events (
    id TEXT PRIMARY KEY NOT NULL,
    document_workflow_id TEXT NOT NULL REFERENCES document_workflows(id) ON DELETE CASCADE,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    from_stage_id TEXT,
    to_stage_id TEXT NOT NULL,
    action TEXT NOT NULL,
    note TEXT,
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_document_workflow_events_workflow_id ON document_workflow_events(document_workflow_id);
CREATE INDEX IF NOT EXISTS idx_document_workflow_events_document_id ON document_workflow_events(document_id);
