ALTER TABLE sessions ADD COLUMN client_platform TEXT NOT NULL DEFAULT 'web';

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

CREATE INDEX IF NOT EXISTS idx_sessions_user_platform
    ON sessions(user_id, client_platform);
