-- ============================================================
-- Document Workflow Instances
-- ============================================================

CREATE TABLE IF NOT EXISTS document_workflows (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    workflow_template_id TEXT NOT NULL REFERENCES workflow_templates(id) ON DELETE CASCADE,
    current_stage_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(document_id)
);

CREATE INDEX IF NOT EXISTS idx_document_workflows_document_id ON document_workflows(document_id);
CREATE INDEX IF NOT EXISTS idx_document_workflows_template_id ON document_workflows(workflow_template_id);
