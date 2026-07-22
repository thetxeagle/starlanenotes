-- Groups and space-level group permissions.

CREATE TABLE IF NOT EXISTS groups (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    description TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS group_members (
    id TEXT PRIMARY KEY NOT NULL,
    group_id TEXT NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(group_id, user_id)
);

CREATE TABLE IF NOT EXISTS space_group_permissions (
    id TEXT PRIMARY KEY NOT NULL,
    space_id TEXT NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    group_id TEXT NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    role TEXT NOT NULL DEFAULT 'viewer',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(space_id, group_id)
);

CREATE INDEX IF NOT EXISTS idx_group_members_group_id ON group_members(group_id);
CREATE INDEX IF NOT EXISTS idx_group_members_user_id ON group_members(user_id);
CREATE INDEX IF NOT EXISTS idx_space_group_permissions_space_id ON space_group_permissions(space_id);
CREATE INDEX IF NOT EXISTS idx_space_group_permissions_group_id ON space_group_permissions(group_id);
