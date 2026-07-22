-- Optional scope metadata for personal and future service-account API keys.

ALTER TABLE api_keys ADD COLUMN scopes TEXT;
