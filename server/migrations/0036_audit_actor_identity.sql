-- Durable actor provenance without inventing historical identities or user rows.
ALTER TABLE audit_log ADD COLUMN actor_kind TEXT;
ALTER TABLE audit_log ADD COLUMN actor_id TEXT;
