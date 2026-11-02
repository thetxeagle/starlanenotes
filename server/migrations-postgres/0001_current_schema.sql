-- PostgreSQL baseline derived from the resulting unchanged SQLite migrations 0001-0048.

-- Independent engine history. TEXT evidence stays exact; signed integers use BIGINT; BLOB uses BYTEA.

-- Foreign keys are installed after all tables/indexes so circular references retain their contract.

CREATE TABLE api_keys (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT ,
    name TEXT NOT NULL,
    key_hash TEXT NOT NULL UNIQUE,
    key_prefix TEXT NOT NULL,
    is_service_account BIGINT NOT NULL DEFAULT 0,
    last_used_at TEXT,
    expires_at TEXT,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    scopes TEXT
);

CREATE TABLE asset_delete_replay_receipts (
    provider TEXT NOT NULL,
    incoming_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    action TEXT NOT NULL,
    payload_fingerprint TEXT NOT NULL,
    space_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    outcome TEXT NOT NULL,
    receipt_json TEXT NOT NULL,
    PRIMARY KEY(provider,incoming_id)
);

CREATE TABLE asset_replay_receipts (
    provider TEXT NOT NULL,
    incoming_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    action TEXT NOT NULL,
    payload_fingerprint TEXT NOT NULL,
    space_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    storage_path TEXT NOT NULL,
    filename TEXT NOT NULL,
    original_name TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    byte_length BIGINT NOT NULL,
    binary_sha256 TEXT NOT NULL,
    source_kind TEXT NOT NULL,
    source_id TEXT,
    source_revision TEXT,
    body_sha256 TEXT,
    receipt_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY(provider,incoming_id)
);

CREATE TABLE attachments (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL ,
    space_id TEXT NOT NULL ,
    filename TEXT NOT NULL,
    original_name TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    size_bytes BIGINT NOT NULL,
    storage_path TEXT NOT NULL,
    created_by TEXT NOT NULL ,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"'))
);

CREATE TABLE audience_mutation_fence (
    id BIGINT PRIMARY KEY NOT NULL CHECK (id = 1)
);

CREATE TABLE audit_log (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT ,
    ip_address TEXT,
    action TEXT NOT NULL,
    target_type TEXT,
    target_id TEXT,
    before_value TEXT,
    after_value TEXT,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    actor_kind TEXT,
    actor_id TEXT
);

CREATE TABLE bookmark_cache (
    url_hash TEXT PRIMARY KEY NOT NULL,
    url TEXT NOT NULL,
    title TEXT,
    description TEXT,
    favicon_url TEXT,
    image_url TEXT,
    domain TEXT,
    fetched_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"'))
);

CREATE TABLE canonical_revision_receipts (
    op_id TEXT PRIMARY KEY,
    space_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    reason TEXT NOT NULL,
    provider TEXT NOT NULL,
    incoming_id TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    action TEXT NOT NULL,
    payload_fingerprint TEXT NOT NULL,
    source_path TEXT NOT NULL,
    source_revision TEXT NOT NULL,
    published_stamp TEXT NOT NULL,
    source_sha256 TEXT,
    content_path TEXT NOT NULL,
    revision TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    byte_length BIGINT NOT NULL CHECK(byte_length>=0),
    manifest_json TEXT NOT NULL,
    verified_at TEXT,
    UNIQUE(provider,incoming_id)
);

CREATE TABLE comments (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL ,
    parent_id TEXT ,
    user_id TEXT NOT NULL ,
    content TEXT NOT NULL,
    selection_start BIGINT,
    selection_end BIGINT,
    selection_text TEXT,
    resolved BIGINT NOT NULL DEFAULT 0,
    resolved_by TEXT ,
    resolved_at TEXT,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    updated_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"'))
);

CREATE TABLE doc_subscriptions (
    document_id TEXT NOT NULL ,
    user_id TEXT NOT NULL ,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    PRIMARY KEY (document_id, user_id)
);

CREATE TABLE doc_tags (
    document_id TEXT NOT NULL ,
    tag_id TEXT NOT NULL ,
    PRIMARY KEY (document_id, tag_id)
);

CREATE TABLE doc_versions (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL ,
    version_number BIGINT NOT NULL,
    change_set BYTEA,
    lines_added BIGINT NOT NULL DEFAULT 0,
    lines_removed BIGINT NOT NULL DEFAULT 0,
    created_by TEXT NOT NULL ,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"'))
);

CREATE TABLE document_access_allowlist (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    user_id TEXT ,
    group_id TEXT ,
    created_by TEXT NOT NULL ,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS')),
    CHECK ((user_id IS NOT NULL AND group_id IS NULL) OR (user_id IS NULL AND group_id IS NOT NULL))
);

CREATE TABLE document_access_policies (
    document_id TEXT PRIMARY KEY NOT NULL ,
    space_id TEXT NOT NULL ,
    mode TEXT NOT NULL CHECK(mode IN ('inherited','restricted')),
    revision TEXT NOT NULL,
    created_by TEXT NOT NULL ,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS')),
    updated_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS')),
    UNIQUE(document_id,space_id)
);

CREATE TABLE document_create_replay_receipts (
    provider TEXT NOT NULL,
    incoming_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    action TEXT NOT NULL,
    payload_fingerprint TEXT NOT NULL,
    space_id TEXT NOT NULL,
    storage_op_id TEXT NOT NULL,
    receipt_json TEXT NOT NULL,
    PRIMARY KEY(provider,incoming_id)
);

CREATE TABLE document_delete_replay_receipts (
    provider TEXT NOT NULL,
    incoming_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    action TEXT NOT NULL,
    payload_fingerprint TEXT NOT NULL,
    space_id TEXT NOT NULL,
    source_fingerprint TEXT NOT NULL,
    created_at TEXT NOT NULL,
    receipt_json TEXT NOT NULL,
    PRIMARY KEY(provider,incoming_id)
);

CREATE TABLE document_draft_access (
    draft_id TEXT NOT NULL ,
    user_id TEXT NOT NULL ,
    role TEXT NOT NULL CHECK (role IN ('viewer', 'editor')),
    granted_by TEXT NOT NULL ,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS')),
    updated_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS')),
    PRIMARY KEY (draft_id, user_id)
);

CREATE TABLE document_drafts (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL ,
    space_id TEXT NOT NULL ,
    owner_id TEXT NOT NULL ,
    title TEXT NOT NULL,
    icon TEXT,
    cover_image TEXT,
    content TEXT NOT NULL DEFAULT '',
    base_updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS')),
    updated_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS')),
    previous_folder_id TEXT,
    previous_parent_id TEXT
);

CREATE TABLE document_identifier_receipts (
    provider TEXT NOT NULL,
    incoming_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    payload_fingerprint TEXT NOT NULL,
    receipt_json TEXT NOT NULL,
    PRIMARY KEY(provider,incoming_id)
);

CREATE TABLE document_maintenance_receipts (
    provider TEXT NOT NULL,
    incoming_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    payload_fingerprint TEXT NOT NULL,
    receipt_json TEXT NOT NULL,
    PRIMARY KEY(provider,incoming_id)
);

CREATE TABLE document_metadata_replay_receipts (
    provider TEXT NOT NULL,
    incoming_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    action TEXT NOT NULL,
    payload_fingerprint TEXT NOT NULL,
    receipt_json TEXT NOT NULL,
    PRIMARY KEY(provider,incoming_id)
);

CREATE TABLE document_viewer_grants (
    id TEXT PRIMARY KEY NOT NULL CHECK(length(trim(id)) > 0),
    document_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    user_id TEXT NOT NULL ,
    role TEXT NOT NULL CHECK(role = 'viewer'),
    scope TEXT NOT NULL CHECK(scope = 'page'),
    revision TEXT NOT NULL CHECK(length(trim(revision)) > 0),
    created_by TEXT NOT NULL ,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(document_id,user_id)
);

CREATE TABLE document_workflow_events (
    id TEXT PRIMARY KEY NOT NULL,
    document_workflow_id TEXT NOT NULL ,
    document_id TEXT NOT NULL ,
    from_stage_id TEXT,
    to_stage_id TEXT NOT NULL,
    action TEXT NOT NULL,
    note TEXT,
    created_by TEXT NOT NULL ,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"'))
);

CREATE TABLE document_workflows (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL ,
    workflow_template_id TEXT NOT NULL ,
    current_stage_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    created_by TEXT NOT NULL ,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    updated_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    UNIQUE(document_id)
);

CREATE TABLE documents (
    id TEXT PRIMARY KEY NOT NULL,
    space_id TEXT NOT NULL ,
    folder_id TEXT ,
    title TEXT NOT NULL,
    icon TEXT,
    cover_image TEXT,
    content_path TEXT NOT NULL,
    locked BIGINT NOT NULL DEFAULT 0,
    locked_by TEXT ,
    public BIGINT NOT NULL DEFAULT 0,
    public_slug TEXT UNIQUE,
    created_by TEXT NOT NULL ,
    last_updated_by TEXT NOT NULL ,
    automerge_state BYTEA,
    sort_order BIGINT NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    updated_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    parent_id TEXT ,
    published_at TEXT,
    archived_at TEXT,
    deleted_at TEXT,
    deleted_by TEXT,
    human_identifier TEXT,
    human_identifier_key TEXT CHECK (
 (human_identifier IS NULL AND human_identifier_key IS NULL) OR
 (human_identifier IS NOT NULL AND human_identifier_key IS NOT NULL AND human_identifier_key=UPPER(human_identifier))
),
    maintenance_user_id TEXT ,
    maintenance_group_id TEXT
 CHECK (maintenance_user_id IS NULL OR maintenance_group_id IS NULL)
);

CREATE TABLE draft_birth_receipts (
    actor_id TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    request_fingerprint TEXT NOT NULL,
    draft_id TEXT NOT NULL,
    result_digest TEXT NOT NULL,
    result_json TEXT NOT NULL,
    PRIMARY KEY (actor_id, operation_id)
);

CREATE TABLE draft_save_replay_receipts (
    provider TEXT NOT NULL,
    incoming_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    action TEXT NOT NULL,
    payload_fingerprint TEXT NOT NULL,
    space_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    draft_id TEXT NOT NULL,
    source_digest TEXT NOT NULL,
    result_digest TEXT NOT NULL,
    created_at TEXT NOT NULL,
    receipt_json TEXT NOT NULL,
    PRIMARY KEY(provider,incoming_id)
);

CREATE TABLE excalidraw_files (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL ,
    space_id TEXT NOT NULL ,
    filename TEXT NOT NULL,
    storage_path TEXT NOT NULL,
    created_by TEXT NOT NULL ,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    updated_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"'))
);

CREATE TABLE folders (
    id TEXT PRIMARY KEY NOT NULL,
    space_id TEXT NOT NULL ,
    parent_id TEXT ,
    name TEXT NOT NULL,
    icon TEXT,
    sort_order BIGINT NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    updated_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"'))
);

CREATE TABLE global_roles (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    permissions TEXT NOT NULL DEFAULT '[]',
    built_in BIGINT NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    updated_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"'))
);

CREATE TABLE group_global_roles (
    group_id TEXT NOT NULL ,
    role_id TEXT NOT NULL ,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    PRIMARY KEY (group_id, role_id)
);

CREATE TABLE group_members (
    id TEXT PRIMARY KEY NOT NULL,
    group_id TEXT NOT NULL ,
    user_id TEXT NOT NULL ,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    UNIQUE(group_id, user_id)
);

CREATE TABLE groups (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    description TEXT,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    updated_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    external_id TEXT,
    scim_managed BIGINT NOT NULL DEFAULT 0,
    managed_space_id TEXT
);

CREATE TABLE move_asset_path_aliases (
    old_path TEXT PRIMARY KEY NOT NULL,
    asset_id TEXT NOT NULL,
    asset_kind TEXT NOT NULL CHECK(asset_kind IN ('attachment','drawing')),
    op_id TEXT NOT NULL ,
    created_at TEXT NOT NULL
);

CREATE TABLE move_commit_receipts (
    op_id TEXT PRIMARY KEY NOT NULL,
    preview_id TEXT NOT NULL UNIQUE,
    actor_id TEXT NOT NULL,
    source_space_id TEXT NOT NULL,
    destination_space_id TEXT NOT NULL,
    request_fingerprint TEXT NOT NULL,
    source_fingerprint TEXT NOT NULL,
    manifest_json TEXT NOT NULL,
    expected_state_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    verified_at TEXT
);

CREATE TABLE move_metadata_escrow (
    op_id TEXT NOT NULL ,
    document_id TEXT NOT NULL,
    family TEXT NOT NULL,
    source_id TEXT NOT NULL,
    disposition TEXT NOT NULL CHECK(disposition IN ('preserve','remove','map','create')),
    original_json TEXT NOT NULL,
    expected_json TEXT NOT NULL,
    PRIMARY KEY(op_id,document_id,family,source_id)
);

CREATE TABLE move_previews (
    id TEXT PRIMARY KEY NOT NULL,
    actor_id TEXT NOT NULL,
    source_space_id TEXT NOT NULL,
    root_kind TEXT NOT NULL,
    root_id TEXT NOT NULL,
    request_json TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    manifest_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    cancelled_at TEXT
);

CREATE TABLE native_auth_handoffs (
    id TEXT PRIMARY KEY NOT NULL,
    code_challenge TEXT NOT NULL,
    user_id TEXT ,
    code_hash TEXT,
    expires_at TEXT NOT NULL,
    consumed_at TEXT
);

CREATE TABLE notification_prefs (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL ,
    notification_type TEXT NOT NULL,
    channel TEXT NOT NULL DEFAULT 'both',
    UNIQUE(user_id, notification_type)
);

CREATE TABLE notifications (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL ,
    type TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT,
    source_user_id TEXT ,
    target_type TEXT,
    target_id TEXT,
    read BIGINT NOT NULL DEFAULT 0,
    read_at TEXT,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"'))
);

CREATE TABLE oauth_accounts (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL ,
    provider TEXT NOT NULL,
    provider_user_id TEXT NOT NULL,
    provider_email TEXT,
    access_token TEXT,
    refresh_token TEXT,
    token_expires_at TEXT,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    updated_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    UNIQUE(provider, provider_user_id)
);

CREATE TABLE passkey_challenges (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT ,
    email TEXT,
    kind TEXT NOT NULL,
    state_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    expires_at TEXT NOT NULL
);

CREATE TABLE passkeys (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL ,
    credential_id TEXT NOT NULL UNIQUE,
    public_key TEXT NOT NULL,
    name TEXT NOT NULL,
    sign_count BIGINT NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    last_used_at TEXT
);

CREATE TABLE password_reset_tokens (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL ,
    token_hash TEXT NOT NULL UNIQUE,
    expires_at TEXT NOT NULL,
    used_at TEXT,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"'))
);

CREATE TABLE properties_schema (
    id TEXT PRIMARY KEY NOT NULL,
    space_id TEXT NOT NULL ,
    name TEXT NOT NULL,
    property_type TEXT NOT NULL,
    options TEXT,
    sort_order BIGINT NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    UNIQUE(space_id, name)
);

CREATE TABLE property_values (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL ,
    property_id TEXT NOT NULL ,
    value TEXT,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    updated_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    UNIQUE(document_id, property_id)
);

CREATE TABLE publication_receipts (
    op_id TEXT PRIMARY KEY,
    space_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    draft_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    source_path TEXT NOT NULL,
    source_revision TEXT NOT NULL,
    content_path TEXT NOT NULL,
    revision TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    byte_length BIGINT NOT NULL CHECK (byte_length >= 0),
    payload_fingerprint TEXT NOT NULL,
    manifest_json TEXT NOT NULL,
    verified_at TEXT
);

CREATE TABLE reactions (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL ,
    user_id TEXT NOT NULL ,
    emoji TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    UNIQUE(document_id, user_id, emoji)
);

CREATE TABLE sessions (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL ,
    refresh_token_hash TEXT NOT NULL,
    device_name TEXT,
    ip_address TEXT,
    user_agent TEXT,
    last_seen_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    session_kind TEXT NOT NULL DEFAULT 'browser',
    client_platform TEXT NOT NULL DEFAULT 'web'
);

CREATE TABLE space_group_permissions (
    id TEXT PRIMARY KEY NOT NULL,
    space_id TEXT NOT NULL ,
    group_id TEXT NOT NULL ,
    role TEXT NOT NULL DEFAULT 'viewer',
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    updated_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    UNIQUE(space_id, group_id)
);

CREATE TABLE space_members (
    id TEXT PRIMARY KEY NOT NULL,
    space_id TEXT NOT NULL ,
    user_id TEXT NOT NULL ,
    role TEXT NOT NULL DEFAULT 'editor',
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    UNIQUE(space_id, user_id)
);

CREATE TABLE space_roles (
    id TEXT PRIMARY KEY,
    space_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    permissions TEXT NOT NULL,
    created_by TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS')),
    updated_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS')),
    UNIQUE (space_id, name)
);

CREATE TABLE space_user_blocks (
    space_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    created_by TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS')),
    PRIMARY KEY (space_id, user_id)
);

CREATE TABLE spaces (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    description TEXT,
    icon TEXT,
    accent_color TEXT,
    visibility TEXT NOT NULL DEFAULT 'private',
    created_by TEXT NOT NULL ,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    updated_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    trash_retention_days BIGINT NOT NULL DEFAULT 30
);

CREATE TABLE starred_documents (
    user_id TEXT NOT NULL ,
    document_id TEXT NOT NULL ,
    starred_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    PRIMARY KEY (user_id, document_id)
);

CREATE TABLE storage_journal_receipts (
    op_id TEXT PRIMARY KEY,
    space_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    draft_id TEXT NOT NULL,
    content_path TEXT NOT NULL,
    initial_revision TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    byte_length BIGINT NOT NULL CHECK (byte_length >= 0),
    state TEXT NOT NULL CHECK (state IN ('pending', 'complete')),
    completed_at TEXT
);

CREATE TABLE storage_restore_current (
    singleton BIGINT PRIMARY KEY CHECK (singleton = 1),
    generation_id TEXT NOT NULL
);

CREATE TABLE storage_restore_generations (
    generation_id TEXT PRIMARY KEY,
    backup_id TEXT NOT NULL,
    manifest_sha256 TEXT NOT NULL,
    database_sha256 TEXT NOT NULL,
    ledger_sha256 TEXT NOT NULL,
    root_device TEXT NOT NULL,
    root_inode TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('pending', 'committed', 'needs_review')),
    created_at TEXT NOT NULL
);

CREATE TABLE sync_replay_checkpoints (
    id TEXT PRIMARY KEY NOT NULL,
    manifest_id TEXT NOT NULL ,
    provider TEXT NOT NULL,
    attempted_by TEXT NOT NULL ,
    space_id TEXT,
    source_server_url TEXT,
    downloaded_by_user_id TEXT,
    checkpoint_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE sync_replay_manifests (
    id TEXT PRIMARY KEY NOT NULL,
    provider TEXT NOT NULL,
    status TEXT NOT NULL,
    attempted_by TEXT NOT NULL ,
    operation_count BIGINT NOT NULL,
    attempted_operation_count BIGINT NOT NULL,
    applied_operation_count BIGINT NOT NULL,
    skipped_operation_count BIGINT NOT NULL,
    failed_operation_count BIGINT NOT NULL,
    unsupported_operation_count BIGINT NOT NULL,
    recovery_checkpoint_count BIGINT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE sync_replay_operations (
    operation_id TEXT PRIMARY KEY NOT NULL,
    provider TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    action TEXT NOT NULL,
    status TEXT NOT NULL,
    attempted_by TEXT NOT NULL ,
    attempted_at TEXT NOT NULL,
    completed_at TEXT,
    error TEXT,
    payload TEXT
);

CREATE TABLE system_config (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"'))
);

CREATE TABLE tags (
    id TEXT PRIMARY KEY NOT NULL,
    space_id TEXT NOT NULL ,
    name TEXT NOT NULL,
    color TEXT NOT NULL DEFAULT '#6b7280',
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    UNIQUE(space_id, name)
);

CREATE TABLE template_drafts (
    id TEXT PRIMARY KEY NOT NULL,
    template_id TEXT NOT NULL ,
    space_id TEXT NOT NULL ,
    owner_id TEXT NOT NULL ,
    snapshot TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS')),
    updated_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS')),
    last_autosaved_at TEXT,
    lock_owner_id TEXT,
    lock_expires_at TEXT
);

CREATE TABLE template_replay_receipts (
    provider TEXT NOT NULL,
    incoming_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    action TEXT NOT NULL,
    payload_fingerprint TEXT NOT NULL,
    space_id TEXT NOT NULL,
    outcome TEXT NOT NULL,
    receipt_json TEXT NOT NULL,
    PRIMARY KEY(provider,incoming_id)
);

CREATE TABLE template_revisions (
    id TEXT PRIMARY KEY NOT NULL,
    template_id TEXT NOT NULL ,
    space_id TEXT NOT NULL ,
    revision_number BIGINT NOT NULL,
    snapshot TEXT NOT NULL,
    created_by TEXT NOT NULL ,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS'))
);

CREATE TABLE templates (
    id TEXT PRIMARY KEY NOT NULL,
    space_id TEXT NOT NULL ,
    name TEXT NOT NULL,
    description TEXT,
    content TEXT NOT NULL,
    properties TEXT,
    tags TEXT,
    created_by TEXT NOT NULL ,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    updated_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"'))
);

CREATE TABLE users (
    id TEXT PRIMARY KEY NOT NULL,
    email TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    password_hash TEXT,
    avatar_url TEXT,
    role TEXT NOT NULL DEFAULT 'member',
    is_active BIGINT NOT NULL DEFAULT 1,
    totp_secret TEXT,
    totp_enabled BIGINT NOT NULL DEFAULT 0,
    email_otp_enabled BIGINT NOT NULL DEFAULT 0,
    recovery_codes TEXT,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    updated_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    external_id TEXT,
    username TEXT,
    account_kind TEXT NOT NULL DEFAULT 'member'
 CHECK(account_kind IN ('member','guest'))
);

CREATE TABLE webhook_deliveries (
    id TEXT PRIMARY KEY NOT NULL,
    webhook_id TEXT NOT NULL ,
    event_type TEXT NOT NULL,
    payload TEXT NOT NULL,
    response_status BIGINT,
    response_body TEXT,
    attempt_number BIGINT NOT NULL DEFAULT 1,
    success BIGINT NOT NULL DEFAULT 0,
    delivered_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"'))
);

CREATE TABLE webhooks (
    id TEXT PRIMARY KEY NOT NULL,
    space_id TEXT ,
    name TEXT NOT NULL,
    url TEXT NOT NULL,
    secret_hash TEXT,
    events TEXT NOT NULL,
    is_active BIGINT NOT NULL DEFAULT 1,
    created_by TEXT NOT NULL ,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    updated_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"'))
);

CREATE TABLE workflow_templates (
    id TEXT PRIMARY KEY NOT NULL,
    space_id TEXT NOT NULL ,
    name TEXT NOT NULL,
    description TEXT,
    stages TEXT NOT NULL,
    approvers TEXT NOT NULL,
    created_by TEXT NOT NULL ,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    updated_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"'))
);

CREATE INDEX canonical_revision_receipts_document ON canonical_revision_receipts(document_id);

CREATE UNIQUE INDEX document_access_allowlist_group ON document_access_allowlist(document_id,group_id);

CREATE INDEX document_access_allowlist_page ON document_access_allowlist(document_id,space_id);

CREATE UNIQUE INDEX document_access_allowlist_user ON document_access_allowlist(document_id,user_id);

CREATE INDEX document_viewer_grants_recipient ON document_viewer_grants(user_id,document_id);

CREATE UNIQUE INDEX documents_direct_grant_binding ON documents(id,space_id);

CREATE INDEX documents_maintenance_group ON documents(maintenance_group_id);

CREATE INDEX documents_maintenance_user ON documents(maintenance_user_id);

CREATE UNIQUE INDEX documents_space_identifier ON documents(space_id,human_identifier_key);

CREATE INDEX idx_api_keys_key_hash ON api_keys(key_hash);

CREATE INDEX idx_api_keys_user_id ON api_keys(user_id);

CREATE INDEX idx_attachments_document_id ON attachments(document_id);

CREATE INDEX idx_attachments_space_id ON attachments(space_id);

CREATE INDEX idx_audit_log_action ON audit_log(action);

CREATE INDEX idx_audit_log_created_at ON audit_log(created_at);

CREATE INDEX idx_audit_log_target ON audit_log(target_type, target_id);

CREATE INDEX idx_audit_log_user_id ON audit_log(user_id);

CREATE INDEX idx_comments_document_id ON comments(document_id);

CREATE INDEX idx_comments_parent_id ON comments(parent_id);

CREATE INDEX idx_doc_versions_document_id ON doc_versions(document_id);

CREATE INDEX idx_document_draft_access_user
    ON document_draft_access(user_id);

CREATE INDEX idx_document_drafts_document_id
    ON document_drafts(document_id);

CREATE INDEX idx_document_drafts_retention
    ON document_drafts(updated_at);

CREATE INDEX idx_document_drafts_space_owner
    ON document_drafts(space_id, owner_id);

CREATE INDEX idx_document_drafts_updated_at
    ON document_drafts(updated_at);

CREATE INDEX idx_document_workflow_events_document_id ON document_workflow_events(document_id);

CREATE INDEX idx_document_workflow_events_workflow_id ON document_workflow_events(document_workflow_id);

CREATE INDEX idx_document_workflows_document_id ON document_workflows(document_id);

CREATE INDEX idx_document_workflows_template_id ON document_workflows(workflow_template_id);

CREATE INDEX idx_documents_archived_at ON documents(archived_at);

CREATE INDEX idx_documents_created_by ON documents(created_by);

CREATE INDEX idx_documents_deleted_at ON documents(deleted_at);

CREATE INDEX idx_documents_folder_id ON documents(folder_id);

CREATE INDEX idx_documents_parent_id ON documents(parent_id);

CREATE INDEX idx_documents_public_slug ON documents(public_slug);

CREATE INDEX idx_documents_space_id ON documents(space_id);

CREATE INDEX idx_excalidraw_files_document_id ON excalidraw_files(document_id);

CREATE INDEX idx_folders_parent_id ON folders(parent_id);

CREATE INDEX idx_folders_space_id ON folders(space_id);

CREATE INDEX idx_group_global_roles_group_id ON group_global_roles(group_id);

CREATE INDEX idx_group_global_roles_role_id ON group_global_roles(role_id);

CREATE INDEX idx_group_members_group_id ON group_members(group_id);

CREATE INDEX idx_group_members_user_id ON group_members(user_id);

CREATE UNIQUE INDEX idx_groups_external_id
    ON groups(external_id)
    WHERE external_id IS NOT NULL;

CREATE UNIQUE INDEX idx_groups_managed_space_id
    ON groups(managed_space_id)
    WHERE managed_space_id IS NOT NULL;

CREATE INDEX idx_move_previews_actor ON move_previews(actor_id, expires_at);

CREATE INDEX idx_native_auth_handoffs_expires_at ON native_auth_handoffs(expires_at);

CREATE INDEX idx_notifications_read ON notifications(user_id, read);

CREATE INDEX idx_notifications_user_id ON notifications(user_id);

CREATE INDEX idx_oauth_accounts_user_id ON oauth_accounts(user_id);

CREATE INDEX idx_passkey_challenges_email ON passkey_challenges(email);

CREATE INDEX idx_passkey_challenges_expires_at ON passkey_challenges(expires_at);

CREATE INDEX idx_passkey_challenges_user_id ON passkey_challenges(user_id);

CREATE INDEX idx_passkeys_user_id ON passkeys(user_id);

CREATE INDEX idx_password_reset_tokens_expires_at ON password_reset_tokens(expires_at);

CREATE INDEX idx_password_reset_tokens_hash ON password_reset_tokens(token_hash);

CREATE INDEX idx_password_reset_tokens_user_id ON password_reset_tokens(user_id);

CREATE INDEX idx_properties_schema_space_id ON properties_schema(space_id);

CREATE INDEX idx_property_values_document_id ON property_values(document_id);

CREATE INDEX idx_reactions_document_id ON reactions(document_id);

CREATE INDEX idx_sessions_expires_at ON sessions(expires_at);

CREATE INDEX idx_sessions_expiry_kind ON sessions(expires_at, session_kind);

CREATE INDEX idx_sessions_user_id ON sessions(user_id);

CREATE INDEX idx_sessions_user_platform
    ON sessions(user_id, client_platform);

CREATE INDEX idx_space_group_permissions_group_id ON space_group_permissions(group_id);

CREATE INDEX idx_space_group_permissions_space_id ON space_group_permissions(space_id);

CREATE INDEX idx_space_members_space_id ON space_members(space_id);

CREATE INDEX idx_space_members_user_id ON space_members(user_id);

CREATE INDEX idx_space_roles_space_id ON space_roles(space_id);

CREATE INDEX idx_space_user_blocks_user_id ON space_user_blocks(user_id);

CREATE INDEX idx_starred_documents_document_id ON starred_documents(document_id);

CREATE INDEX idx_starred_documents_user_id ON starred_documents(user_id);

CREATE INDEX idx_sync_replay_checkpoints_manifest_id
    ON sync_replay_checkpoints(manifest_id);

CREATE INDEX idx_sync_replay_checkpoints_space
    ON sync_replay_checkpoints(space_id, provider);

CREATE INDEX idx_sync_replay_manifests_attempted_by
    ON sync_replay_manifests(attempted_by);

CREATE INDEX idx_sync_replay_manifests_created_at
    ON sync_replay_manifests(created_at);

CREATE INDEX idx_sync_replay_operations_attempted_by
    ON sync_replay_operations(attempted_by);

CREATE INDEX idx_sync_replay_operations_status
    ON sync_replay_operations(status);

CREATE INDEX idx_tags_space_id ON tags(space_id);

CREATE INDEX idx_template_drafts_space_id ON template_drafts(space_id);

CREATE UNIQUE INDEX idx_template_drafts_template_id ON template_drafts(template_id);

CREATE INDEX idx_template_drafts_updated_at ON template_drafts(updated_at);

CREATE UNIQUE INDEX idx_template_revisions_number
    ON template_revisions(template_id, revision_number);

CREATE INDEX idx_template_revisions_template_id
    ON template_revisions(template_id, created_at DESC);

CREATE INDEX idx_templates_space_id ON templates(space_id);

CREATE UNIQUE INDEX idx_users_external_id
    ON users(external_id)
    WHERE external_id IS NOT NULL;

CREATE UNIQUE INDEX idx_users_username_unique ON users(username);

CREATE INDEX idx_webhook_deliveries_webhook_id ON webhook_deliveries(webhook_id);

CREATE INDEX idx_webhooks_space_id ON webhooks(space_id);

CREATE INDEX idx_workflow_templates_space_id ON workflow_templates(space_id);

CREATE INDEX publication_receipts_document ON publication_receipts(document_id);

CREATE INDEX storage_journal_receipts_document ON storage_journal_receipts(document_id);

ALTER TABLE api_keys ADD CONSTRAINT api_keys_fk_0 FOREIGN KEY (user_id) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE attachments ADD CONSTRAINT attachments_fk_0 FOREIGN KEY (created_by) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE NO ACTION;

ALTER TABLE attachments ADD CONSTRAINT attachments_fk_1 FOREIGN KEY (space_id) REFERENCES spaces (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE attachments ADD CONSTRAINT attachments_fk_2 FOREIGN KEY (document_id) REFERENCES documents (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE audit_log ADD CONSTRAINT audit_log_fk_0 FOREIGN KEY (user_id) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE SET NULL;

ALTER TABLE comments ADD CONSTRAINT comments_fk_0 FOREIGN KEY (resolved_by) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE NO ACTION;

ALTER TABLE comments ADD CONSTRAINT comments_fk_1 FOREIGN KEY (user_id) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE NO ACTION;

ALTER TABLE comments ADD CONSTRAINT comments_fk_2 FOREIGN KEY (parent_id) REFERENCES comments (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE comments ADD CONSTRAINT comments_fk_3 FOREIGN KEY (document_id) REFERENCES documents (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE doc_subscriptions ADD CONSTRAINT doc_subscriptions_fk_0 FOREIGN KEY (user_id) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE doc_subscriptions ADD CONSTRAINT doc_subscriptions_fk_1 FOREIGN KEY (document_id) REFERENCES documents (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE doc_tags ADD CONSTRAINT doc_tags_fk_0 FOREIGN KEY (tag_id) REFERENCES tags (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE doc_tags ADD CONSTRAINT doc_tags_fk_1 FOREIGN KEY (document_id) REFERENCES documents (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE doc_versions ADD CONSTRAINT doc_versions_fk_0 FOREIGN KEY (created_by) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE NO ACTION;

ALTER TABLE doc_versions ADD CONSTRAINT doc_versions_fk_1 FOREIGN KEY (document_id) REFERENCES documents (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE document_access_allowlist ADD CONSTRAINT document_access_allowlist_fk_0 FOREIGN KEY (document_id,space_id) REFERENCES document_access_policies (document_id,space_id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE document_access_allowlist ADD CONSTRAINT document_access_allowlist_fk_1 FOREIGN KEY (created_by) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE NO ACTION;

ALTER TABLE document_access_allowlist ADD CONSTRAINT document_access_allowlist_fk_2 FOREIGN KEY (group_id) REFERENCES groups (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE document_access_allowlist ADD CONSTRAINT document_access_allowlist_fk_3 FOREIGN KEY (user_id) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE document_access_policies ADD CONSTRAINT document_access_policies_fk_0 FOREIGN KEY (created_by) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE NO ACTION;

ALTER TABLE document_access_policies ADD CONSTRAINT document_access_policies_fk_1 FOREIGN KEY (space_id) REFERENCES spaces (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE document_access_policies ADD CONSTRAINT document_access_policies_fk_2 FOREIGN KEY (document_id) REFERENCES documents (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE document_draft_access ADD CONSTRAINT document_draft_access_fk_0 FOREIGN KEY (granted_by) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE document_draft_access ADD CONSTRAINT document_draft_access_fk_1 FOREIGN KEY (user_id) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE document_draft_access ADD CONSTRAINT document_draft_access_fk_2 FOREIGN KEY (draft_id) REFERENCES document_drafts (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE document_drafts ADD CONSTRAINT document_drafts_fk_0 FOREIGN KEY (owner_id) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE document_drafts ADD CONSTRAINT document_drafts_fk_1 FOREIGN KEY (space_id) REFERENCES spaces (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE document_drafts ADD CONSTRAINT document_drafts_fk_2 FOREIGN KEY (document_id) REFERENCES documents (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE document_viewer_grants ADD CONSTRAINT document_viewer_grants_fk_0 FOREIGN KEY (space_id) REFERENCES spaces (id) ON UPDATE NO ACTION ON DELETE RESTRICT;

ALTER TABLE document_viewer_grants ADD CONSTRAINT document_viewer_grants_fk_1 FOREIGN KEY (document_id,space_id) REFERENCES documents (id,space_id) ON UPDATE NO ACTION ON DELETE RESTRICT;

ALTER TABLE document_viewer_grants ADD CONSTRAINT document_viewer_grants_fk_2 FOREIGN KEY (created_by) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE RESTRICT;

ALTER TABLE document_viewer_grants ADD CONSTRAINT document_viewer_grants_fk_3 FOREIGN KEY (user_id) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE RESTRICT;

ALTER TABLE document_workflow_events ADD CONSTRAINT document_workflow_events_fk_0 FOREIGN KEY (created_by) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE NO ACTION;

ALTER TABLE document_workflow_events ADD CONSTRAINT document_workflow_events_fk_1 FOREIGN KEY (document_id) REFERENCES documents (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE document_workflow_events ADD CONSTRAINT document_workflow_events_fk_2 FOREIGN KEY (document_workflow_id) REFERENCES document_workflows (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE document_workflows ADD CONSTRAINT document_workflows_fk_0 FOREIGN KEY (created_by) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE NO ACTION;

ALTER TABLE document_workflows ADD CONSTRAINT document_workflows_fk_1 FOREIGN KEY (workflow_template_id) REFERENCES workflow_templates (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE document_workflows ADD CONSTRAINT document_workflows_fk_2 FOREIGN KEY (document_id) REFERENCES documents (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE documents ADD CONSTRAINT documents_fk_0 FOREIGN KEY (maintenance_group_id) REFERENCES groups (id) ON UPDATE NO ACTION ON DELETE SET NULL;

ALTER TABLE documents ADD CONSTRAINT documents_fk_1 FOREIGN KEY (maintenance_user_id) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE SET NULL;

ALTER TABLE documents ADD CONSTRAINT documents_fk_2 FOREIGN KEY (parent_id) REFERENCES documents (id) ON UPDATE NO ACTION ON DELETE SET NULL;

ALTER TABLE documents ADD CONSTRAINT documents_fk_3 FOREIGN KEY (last_updated_by) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE NO ACTION;

ALTER TABLE documents ADD CONSTRAINT documents_fk_4 FOREIGN KEY (created_by) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE NO ACTION;

ALTER TABLE documents ADD CONSTRAINT documents_fk_5 FOREIGN KEY (locked_by) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE NO ACTION;

ALTER TABLE documents ADD CONSTRAINT documents_fk_6 FOREIGN KEY (folder_id) REFERENCES folders (id) ON UPDATE NO ACTION ON DELETE SET NULL;

ALTER TABLE documents ADD CONSTRAINT documents_fk_7 FOREIGN KEY (space_id) REFERENCES spaces (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE excalidraw_files ADD CONSTRAINT excalidraw_files_fk_0 FOREIGN KEY (created_by) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE NO ACTION;

ALTER TABLE excalidraw_files ADD CONSTRAINT excalidraw_files_fk_1 FOREIGN KEY (space_id) REFERENCES spaces (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE excalidraw_files ADD CONSTRAINT excalidraw_files_fk_2 FOREIGN KEY (document_id) REFERENCES documents (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE folders ADD CONSTRAINT folders_fk_0 FOREIGN KEY (parent_id) REFERENCES folders (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE folders ADD CONSTRAINT folders_fk_1 FOREIGN KEY (space_id) REFERENCES spaces (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE group_global_roles ADD CONSTRAINT group_global_roles_fk_0 FOREIGN KEY (role_id) REFERENCES global_roles (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE group_global_roles ADD CONSTRAINT group_global_roles_fk_1 FOREIGN KEY (group_id) REFERENCES groups (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE group_members ADD CONSTRAINT group_members_fk_0 FOREIGN KEY (user_id) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE group_members ADD CONSTRAINT group_members_fk_1 FOREIGN KEY (group_id) REFERENCES groups (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE groups ADD CONSTRAINT groups_fk_0 FOREIGN KEY (managed_space_id) REFERENCES spaces (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE move_asset_path_aliases ADD CONSTRAINT move_asset_path_aliases_fk_0 FOREIGN KEY (op_id) REFERENCES move_commit_receipts (op_id) ON UPDATE NO ACTION ON DELETE NO ACTION;

ALTER TABLE move_metadata_escrow ADD CONSTRAINT move_metadata_escrow_fk_0 FOREIGN KEY (op_id) REFERENCES move_commit_receipts (op_id) ON UPDATE NO ACTION ON DELETE NO ACTION;

ALTER TABLE native_auth_handoffs ADD CONSTRAINT native_auth_handoffs_fk_0 FOREIGN KEY (user_id) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE notification_prefs ADD CONSTRAINT notification_prefs_fk_0 FOREIGN KEY (user_id) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE notifications ADD CONSTRAINT notifications_fk_0 FOREIGN KEY (source_user_id) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE NO ACTION;

ALTER TABLE notifications ADD CONSTRAINT notifications_fk_1 FOREIGN KEY (user_id) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE oauth_accounts ADD CONSTRAINT oauth_accounts_fk_0 FOREIGN KEY (user_id) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE passkey_challenges ADD CONSTRAINT passkey_challenges_fk_0 FOREIGN KEY (user_id) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE passkeys ADD CONSTRAINT passkeys_fk_0 FOREIGN KEY (user_id) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE password_reset_tokens ADD CONSTRAINT password_reset_tokens_fk_0 FOREIGN KEY (user_id) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE properties_schema ADD CONSTRAINT properties_schema_fk_0 FOREIGN KEY (space_id) REFERENCES spaces (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE property_values ADD CONSTRAINT property_values_fk_0 FOREIGN KEY (property_id) REFERENCES properties_schema (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE property_values ADD CONSTRAINT property_values_fk_1 FOREIGN KEY (document_id) REFERENCES documents (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE reactions ADD CONSTRAINT reactions_fk_0 FOREIGN KEY (user_id) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE reactions ADD CONSTRAINT reactions_fk_1 FOREIGN KEY (document_id) REFERENCES documents (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE sessions ADD CONSTRAINT sessions_fk_0 FOREIGN KEY (user_id) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE space_group_permissions ADD CONSTRAINT space_group_permissions_fk_0 FOREIGN KEY (group_id) REFERENCES groups (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE space_group_permissions ADD CONSTRAINT space_group_permissions_fk_1 FOREIGN KEY (space_id) REFERENCES spaces (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE space_members ADD CONSTRAINT space_members_fk_0 FOREIGN KEY (user_id) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE space_members ADD CONSTRAINT space_members_fk_1 FOREIGN KEY (space_id) REFERENCES spaces (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE space_roles ADD CONSTRAINT space_roles_fk_0 FOREIGN KEY (created_by) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE RESTRICT;

ALTER TABLE space_roles ADD CONSTRAINT space_roles_fk_1 FOREIGN KEY (space_id) REFERENCES spaces (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE space_user_blocks ADD CONSTRAINT space_user_blocks_fk_0 FOREIGN KEY (created_by) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE RESTRICT;

ALTER TABLE space_user_blocks ADD CONSTRAINT space_user_blocks_fk_1 FOREIGN KEY (user_id) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE space_user_blocks ADD CONSTRAINT space_user_blocks_fk_2 FOREIGN KEY (space_id) REFERENCES spaces (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE spaces ADD CONSTRAINT spaces_fk_0 FOREIGN KEY (created_by) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE NO ACTION;

ALTER TABLE starred_documents ADD CONSTRAINT starred_documents_fk_0 FOREIGN KEY (document_id) REFERENCES documents (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE starred_documents ADD CONSTRAINT starred_documents_fk_1 FOREIGN KEY (user_id) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE storage_restore_current ADD CONSTRAINT storage_restore_current_fk_0 FOREIGN KEY (generation_id) REFERENCES storage_restore_generations (generation_id) ON UPDATE NO ACTION ON DELETE NO ACTION;

ALTER TABLE sync_replay_checkpoints ADD CONSTRAINT sync_replay_checkpoints_fk_0 FOREIGN KEY (attempted_by) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE NO ACTION;

ALTER TABLE sync_replay_checkpoints ADD CONSTRAINT sync_replay_checkpoints_fk_1 FOREIGN KEY (manifest_id) REFERENCES sync_replay_manifests (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE sync_replay_manifests ADD CONSTRAINT sync_replay_manifests_fk_0 FOREIGN KEY (attempted_by) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE NO ACTION;

ALTER TABLE sync_replay_operations ADD CONSTRAINT sync_replay_operations_fk_0 FOREIGN KEY (attempted_by) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE NO ACTION;

ALTER TABLE tags ADD CONSTRAINT tags_fk_0 FOREIGN KEY (space_id) REFERENCES spaces (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE template_drafts ADD CONSTRAINT template_drafts_fk_0 FOREIGN KEY (owner_id) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE template_drafts ADD CONSTRAINT template_drafts_fk_1 FOREIGN KEY (space_id) REFERENCES spaces (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE template_drafts ADD CONSTRAINT template_drafts_fk_2 FOREIGN KEY (template_id) REFERENCES templates (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE template_revisions ADD CONSTRAINT template_revisions_fk_0 FOREIGN KEY (created_by) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE NO ACTION;

ALTER TABLE template_revisions ADD CONSTRAINT template_revisions_fk_1 FOREIGN KEY (space_id) REFERENCES spaces (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE template_revisions ADD CONSTRAINT template_revisions_fk_2 FOREIGN KEY (template_id) REFERENCES templates (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE templates ADD CONSTRAINT templates_fk_0 FOREIGN KEY (created_by) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE NO ACTION;

ALTER TABLE templates ADD CONSTRAINT templates_fk_1 FOREIGN KEY (space_id) REFERENCES spaces (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE webhook_deliveries ADD CONSTRAINT webhook_deliveries_fk_0 FOREIGN KEY (webhook_id) REFERENCES webhooks (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE webhooks ADD CONSTRAINT webhooks_fk_0 FOREIGN KEY (created_by) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE NO ACTION;

ALTER TABLE webhooks ADD CONSTRAINT webhooks_fk_1 FOREIGN KEY (space_id) REFERENCES spaces (id) ON UPDATE NO ACTION ON DELETE CASCADE;

ALTER TABLE workflow_templates ADD CONSTRAINT workflow_templates_fk_0 FOREIGN KEY (created_by) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE NO ACTION;

ALTER TABLE workflow_templates ADD CONSTRAINT workflow_templates_fk_1 FOREIGN KEY (space_id) REFERENCES spaces (id) ON UPDATE NO ACTION ON DELETE CASCADE;
