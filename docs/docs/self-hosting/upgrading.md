# Upgrading

## Before You Upgrade

1. **Read the changelog** for the target version. Look for breaking changes, new required environment variables, or migration notes.
2. **Back up your data** (see below).

## Backup

### SQLite

The SQLite database and all documents live in the `/data` volume. Back up the entire volume, or at minimum:

```bash
# Stop the container to ensure a clean copy
docker compose stop starlane-notes

# Copy the data volume contents
docker cp starlane-notes:/data ./backup-$(date +%Y%m%d)

# Restart
docker compose start starlane-notes
```

If you cannot tolerate downtime, copy the database file while running. SQLite supports this, but for full consistency a stopped copy is preferred.

### PostgreSQL

```bash
docker exec starlane-notes-db pg_dump -U starlane starlane_notes > backup-$(date +%Y%m%d).sql
```

Also back up the Starlane Notes data mount. With the local-storage Compose files, that is `storage/`. With the named-volume Compose files, that is the `starlane-notes-data` Docker volume. It contains document files, attachments, and the search index, which are stored on disk regardless of database backend.

## Docker Image Upgrade

```bash
cd starlane-notes

# Pull the configured image tag
docker compose pull

# Restart with the pulled image
docker compose up -d
```

Database migrations run automatically on startup via `sqlx::migrate!`. No manual migration commands are needed.

## Search Index

The tantivy search index is stored in `/data/search_index/`. It is rebuilt automatically if missing, so it does not need to be backed up. If you encounter search issues after an upgrade, delete the directory and restart -- the index will be reconstructed.

## Rollback

If an upgrade fails:

1. Stop the container: `docker compose down`
2. Restore your backup to `storage/` or to the `starlane-notes-data` Docker volume, depending on which Compose file you use
3. Set `STARLANE_IMAGE` in `.env` to the previous working image tag
4. Pull and restart: `docker compose pull && docker compose up -d`

Database migrations are forward-only. If a new migration was applied and you need to roll back, restore the database from your backup.
