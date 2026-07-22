-- Optional stable username for personal-space identity. Existing users retain email fallback.
ALTER TABLE users ADD COLUMN username TEXT;
CREATE UNIQUE INDEX IF NOT EXISTS idx_users_username_unique ON users(username);
