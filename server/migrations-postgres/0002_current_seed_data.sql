-- Current seed/backfill semantics; independent PostgreSQL lineage.
INSERT INTO system_config (key,value) VALUES ('setup_completed','false') ON CONFLICT (key) DO NOTHING;

UPDATE sessions
SET client_platform = CASE
    WHEN session_kind = 'app' AND (
        LOWER(COALESCE(device_name, '')) LIKE '%mobile%'
        OR LOWER(COALESCE(device_name, '')) LIKE '%iphone%'
        OR LOWER(COALESCE(device_name, '')) LIKE '%ipad%'
        OR LOWER(COALESCE(device_name, '')) LIKE '%android%'
    ) THEN 'mobile_app'
    WHEN session_kind = 'app' THEN 'desktop_app'
    ELSE 'web'
END;

UPDATE sessions
SET device_name = CASE client_platform
    WHEN 'mobile_app' THEN 'Starlane Notes mobile app ' || SUBSTR(id, 1, 8)
    WHEN 'desktop_app' THEN 'Starlane Notes desktop app ' || SUBSTR(id, 1, 8)
    ELSE 'Web browser ' || SUBSTR(id, 1, 8)
END
WHERE device_name IS NULL OR TRIM(device_name) = '';

UPDATE documents
SET published_at = updated_at
WHERE published_at IS NULL;

INSERT INTO global_roles (id, name, description, permissions, built_in)
VALUES
    ('owner', 'Owner', 'Full administration of an assigned space.', '["view","comment","create_content","edit_content","move_content","duplicate_content","manage_folders","manage_templates","archive_content","delete_content","manage_members","manage_space"]', 1),
    ('admin', 'Admin', 'Manage content, folders, templates, archival, and deletion.', '["view","comment","create_content","edit_content","move_content","duplicate_content","manage_folders","manage_templates","archive_content","delete_content","manage_members"]', 1),
    ('editor', 'Editor', 'Create and modify pages, folders, and templates.', '["view","comment","create_content","edit_content","move_content","duplicate_content","manage_folders","manage_templates","archive_content","delete_content"]', 1),
    ('contributor', 'Contributor', 'Create, edit, move, and duplicate pages.', '["view","comment","create_content","edit_content","move_content","duplicate_content"]', 1),
    ('viewer', 'Viewer', 'Read-only access to space content.', '["view"]', 1)
ON CONFLICT(id) DO NOTHING;

INSERT INTO audience_mutation_fence (id) VALUES (1);
