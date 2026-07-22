-- ============================================================
-- Workflow Templates
-- ============================================================

CREATE TABLE IF NOT EXISTS workflow_templates (
    id TEXT PRIMARY KEY NOT NULL,
    space_id TEXT NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    stages TEXT NOT NULL,     -- JSON: ordered workflow stages
    approvers TEXT NOT NULL,  -- JSON: required approvers by user or role
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_workflow_templates_space_id ON workflow_templates(space_id);
