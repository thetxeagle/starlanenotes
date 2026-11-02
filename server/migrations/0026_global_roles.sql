-- Reusable roles assigned to groups across all spaces.

CREATE TABLE IF NOT EXISTS global_roles (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    permissions TEXT NOT NULL DEFAULT '[]',
    built_in INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS group_global_roles (
    group_id TEXT NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    role_id TEXT NOT NULL REFERENCES global_roles(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (group_id, role_id)
);

CREATE INDEX IF NOT EXISTS idx_group_global_roles_group_id ON group_global_roles(group_id);
CREATE INDEX IF NOT EXISTS idx_group_global_roles_role_id ON group_global_roles(role_id);

INSERT INTO global_roles (id, name, description, permissions, built_in)
VALUES
    ('owner', 'Owner', 'Full administration of an assigned space.', '["view","comment","create_content","edit_content","move_content","duplicate_content","manage_folders","manage_templates","archive_content","delete_content","manage_members","manage_space"]', 1),
    ('admin', 'Admin', 'Manage content, folders, templates, archival, and deletion.', '["view","comment","create_content","edit_content","move_content","duplicate_content","manage_folders","manage_templates","archive_content","delete_content","manage_members"]', 1),
    ('editor', 'Editor', 'Create and modify pages, folders, and templates.', '["view","comment","create_content","edit_content","move_content","duplicate_content","manage_folders","manage_templates","archive_content","delete_content"]', 1),
    ('contributor', 'Contributor', 'Create, edit, move, and duplicate pages.', '["view","comment","create_content","edit_content","move_content","duplicate_content"]', 1),
    ('viewer', 'Viewer', 'Read-only access to space content.', '["view"]', 1)
ON CONFLICT(id) DO NOTHING;
