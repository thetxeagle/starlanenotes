-- Starlane Notes: Initial Schema
-- Compatible with both SQLite and PostgreSQL via sqlx AnyPool

-- ============================================================
-- Users & Authentication
-- ============================================================

CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY NOT NULL,
    email TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    password_hash TEXT,  -- NULL for OAuth/OIDC-only users
    avatar_url TEXT,
    role TEXT NOT NULL DEFAULT 'member',  -- admin, member, viewer
    is_active INTEGER NOT NULL DEFAULT 1,
    totp_secret TEXT,
    totp_enabled INTEGER NOT NULL DEFAULT 0,
    email_otp_enabled INTEGER NOT NULL DEFAULT 0,
    recovery_codes TEXT,  -- JSON array of hashed codes
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    refresh_token_hash TEXT NOT NULL,
    device_name TEXT,
    ip_address TEXT,
    user_agent TEXT,
    last_seen_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON sessions(expires_at);

CREATE TABLE IF NOT EXISTS oauth_accounts (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,  -- google, github, oidc
    provider_user_id TEXT NOT NULL,
    provider_email TEXT,
    access_token TEXT,
    refresh_token TEXT,
    token_expires_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(provider, provider_user_id)
);

CREATE INDEX IF NOT EXISTS idx_oauth_accounts_user_id ON oauth_accounts(user_id);

CREATE TABLE IF NOT EXISTS passkeys (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    credential_id TEXT NOT NULL UNIQUE,
    public_key TEXT NOT NULL,
    name TEXT NOT NULL,
    sign_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    last_used_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_passkeys_user_id ON passkeys(user_id);

CREATE TABLE IF NOT EXISTS api_keys (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT REFERENCES users(id) ON DELETE CASCADE,  -- NULL for service account keys
    name TEXT NOT NULL,
    key_hash TEXT NOT NULL UNIQUE,
    key_prefix TEXT NOT NULL,  -- First 8 chars for identification
    is_service_account INTEGER NOT NULL DEFAULT 0,
    last_used_at TEXT,
    expires_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_api_keys_user_id ON api_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_api_keys_key_hash ON api_keys(key_hash);

-- ============================================================
-- Spaces & Organization
-- ============================================================

CREATE TABLE IF NOT EXISTS spaces (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    description TEXT,
    icon TEXT,
    accent_color TEXT,
    visibility TEXT NOT NULL DEFAULT 'private',  -- private, internal, public
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS space_members (
    id TEXT PRIMARY KEY NOT NULL,
    space_id TEXT NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role TEXT NOT NULL DEFAULT 'editor',  -- owner, editor, viewer
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(space_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_space_members_space_id ON space_members(space_id);
CREATE INDEX IF NOT EXISTS idx_space_members_user_id ON space_members(user_id);

-- ============================================================
-- Folders & Documents
-- ============================================================

CREATE TABLE IF NOT EXISTS folders (
    id TEXT PRIMARY KEY NOT NULL,
    space_id TEXT NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    parent_id TEXT REFERENCES folders(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    icon TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_folders_space_id ON folders(space_id);
CREATE INDEX IF NOT EXISTS idx_folders_parent_id ON folders(parent_id);

CREATE TABLE IF NOT EXISTS documents (
    id TEXT PRIMARY KEY NOT NULL,
    space_id TEXT NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    folder_id TEXT REFERENCES folders(id) ON DELETE SET NULL,
    title TEXT NOT NULL,
    icon TEXT,
    cover_image TEXT,
    content_path TEXT NOT NULL,  -- Relative path to .md file on disk
    locked INTEGER NOT NULL DEFAULT 0,
    locked_by TEXT REFERENCES users(id),
    public INTEGER NOT NULL DEFAULT 0,
    public_slug TEXT UNIQUE,
    created_by TEXT NOT NULL REFERENCES users(id),
    last_updated_by TEXT NOT NULL REFERENCES users(id),
    automerge_state BLOB,  -- Full CRDT document snapshot for instant cold-open
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_documents_space_id ON documents(space_id);
CREATE INDEX IF NOT EXISTS idx_documents_folder_id ON documents(folder_id);
CREATE INDEX IF NOT EXISTS idx_documents_public_slug ON documents(public_slug);
CREATE INDEX IF NOT EXISTS idx_documents_created_by ON documents(created_by);

CREATE TABLE IF NOT EXISTS doc_versions (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    version_number INTEGER NOT NULL,
    change_set BLOB,  -- CRDT change set (automerge binary)
    lines_added INTEGER NOT NULL DEFAULT 0,
    lines_removed INTEGER NOT NULL DEFAULT 0,
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_doc_versions_document_id ON doc_versions(document_id);

-- ============================================================
-- Tags
-- ============================================================

CREATE TABLE IF NOT EXISTS tags (
    id TEXT PRIMARY KEY NOT NULL,
    space_id TEXT NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    color TEXT NOT NULL DEFAULT '#6b7280',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(space_id, name)
);

CREATE INDEX IF NOT EXISTS idx_tags_space_id ON tags(space_id);

CREATE TABLE IF NOT EXISTS doc_tags (
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (document_id, tag_id)
);

-- ============================================================
-- Properties
-- ============================================================

CREATE TABLE IF NOT EXISTS properties_schema (
    id TEXT PRIMARY KEY NOT NULL,
    space_id TEXT NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    property_type TEXT NOT NULL,  -- text, long_text, number, date, datetime, checkbox, select, multi_select, url, user, relation
    options TEXT,  -- JSON for select/multi_select options
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(space_id, name)
);

CREATE INDEX IF NOT EXISTS idx_properties_schema_space_id ON properties_schema(space_id);

CREATE TABLE IF NOT EXISTS property_values (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    property_id TEXT NOT NULL REFERENCES properties_schema(id) ON DELETE CASCADE,
    value TEXT,  -- Stored as text, parsed by type
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(document_id, property_id)
);

CREATE INDEX IF NOT EXISTS idx_property_values_document_id ON property_values(document_id);

-- ============================================================
-- Attachments & Excalidraw
-- ============================================================

CREATE TABLE IF NOT EXISTS attachments (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    space_id TEXT NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    filename TEXT NOT NULL,
    original_name TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    storage_path TEXT NOT NULL,
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_attachments_document_id ON attachments(document_id);
CREATE INDEX IF NOT EXISTS idx_attachments_space_id ON attachments(space_id);

CREATE TABLE IF NOT EXISTS excalidraw_files (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    space_id TEXT NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    filename TEXT NOT NULL,
    storage_path TEXT NOT NULL,
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_excalidraw_files_document_id ON excalidraw_files(document_id);

-- ============================================================
-- Comments & Reactions
-- ============================================================

CREATE TABLE IF NOT EXISTS comments (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    parent_id TEXT REFERENCES comments(id) ON DELETE CASCADE,  -- NULL for top-level, set for replies
    user_id TEXT NOT NULL REFERENCES users(id),
    content TEXT NOT NULL,
    selection_start INTEGER,  -- Character offset for inline comments
    selection_end INTEGER,
    selection_text TEXT,  -- Original selected text (for display if content changes)
    resolved INTEGER NOT NULL DEFAULT 0,
    resolved_by TEXT REFERENCES users(id),
    resolved_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_comments_document_id ON comments(document_id);
CREATE INDEX IF NOT EXISTS idx_comments_parent_id ON comments(parent_id);

CREATE TABLE IF NOT EXISTS reactions (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    emoji TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(document_id, user_id, emoji)
);

CREATE INDEX IF NOT EXISTS idx_reactions_document_id ON reactions(document_id);

-- ============================================================
-- Notifications
-- ============================================================

CREATE TABLE IF NOT EXISTS notifications (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    type TEXT NOT NULL,  -- mention, comment, comment_reply, doc_shared, space_invite, doc_subscription, doc_locked, doc_unlocked
    title TEXT NOT NULL,
    body TEXT,
    source_user_id TEXT REFERENCES users(id),
    target_type TEXT,  -- document, space, comment
    target_id TEXT,
    read INTEGER NOT NULL DEFAULT 0,
    read_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_notifications_user_id ON notifications(user_id);
CREATE INDEX IF NOT EXISTS idx_notifications_read ON notifications(user_id, read);

CREATE TABLE IF NOT EXISTS notification_prefs (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    notification_type TEXT NOT NULL,
    channel TEXT NOT NULL DEFAULT 'both',  -- in_app, email, both, none
    UNIQUE(user_id, notification_type)
);

-- ============================================================
-- Document Subscriptions
-- ============================================================

CREATE TABLE IF NOT EXISTS doc_subscriptions (
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (document_id, user_id)
);

-- ============================================================
-- Webhooks
-- ============================================================

CREATE TABLE IF NOT EXISTS webhooks (
    id TEXT PRIMARY KEY NOT NULL,
    space_id TEXT REFERENCES spaces(id) ON DELETE CASCADE,  -- NULL for admin-level webhooks
    name TEXT NOT NULL,
    url TEXT NOT NULL,
    secret_hash TEXT,  -- HMAC-SHA256 secret
    events TEXT NOT NULL,  -- JSON array of event types
    is_active INTEGER NOT NULL DEFAULT 1,
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_webhooks_space_id ON webhooks(space_id);

CREATE TABLE IF NOT EXISTS webhook_deliveries (
    id TEXT PRIMARY KEY NOT NULL,
    webhook_id TEXT NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    payload TEXT NOT NULL,  -- JSON
    response_status INTEGER,
    response_body TEXT,
    attempt_number INTEGER NOT NULL DEFAULT 1,
    success INTEGER NOT NULL DEFAULT 0,
    delivered_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_webhook_id ON webhook_deliveries(webhook_id);

-- ============================================================
-- Audit Log
-- ============================================================

CREATE TABLE IF NOT EXISTS audit_log (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT REFERENCES users(id) ON DELETE SET NULL,
    ip_address TEXT,
    action TEXT NOT NULL,
    target_type TEXT,  -- user, space, document, folder, webhook, system
    target_id TEXT,
    before_value TEXT,  -- JSON
    after_value TEXT,   -- JSON
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_audit_log_user_id ON audit_log(user_id);
CREATE INDEX IF NOT EXISTS idx_audit_log_action ON audit_log(action);
CREATE INDEX IF NOT EXISTS idx_audit_log_created_at ON audit_log(created_at);
CREATE INDEX IF NOT EXISTS idx_audit_log_target ON audit_log(target_type, target_id);

-- ============================================================
-- Bookmark Cache (URL preview cards)
-- ============================================================

CREATE TABLE IF NOT EXISTS bookmark_cache (
    url_hash TEXT PRIMARY KEY NOT NULL,
    url TEXT NOT NULL,
    title TEXT,
    description TEXT,
    favicon_url TEXT,
    image_url TEXT,
    domain TEXT,
    fetched_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- ============================================================
-- Templates
-- ============================================================

CREATE TABLE IF NOT EXISTS templates (
    id TEXT PRIMARY KEY NOT NULL,
    space_id TEXT NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    content TEXT NOT NULL,  -- Markdown template content
    properties TEXT,  -- JSON: pre-filled property values
    tags TEXT,  -- JSON: array of tag names
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_templates_space_id ON templates(space_id);

-- ============================================================
-- System Setup
-- ============================================================

CREATE TABLE IF NOT EXISTS system_config (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- Track whether initial setup wizard has been completed
INSERT OR IGNORE INTO system_config (key, value) VALUES ('setup_completed', 'false');
