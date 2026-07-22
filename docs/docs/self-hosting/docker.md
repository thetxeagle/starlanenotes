# Docker Setup

## Quick Start

```bash
# Create a deployment directory
mkdir starlane-notes
cd starlane-notes

# Download Compose and the environment template.
# This default file pulls the published image and stores data in ./storage.
curl -fsSLO https://github.com/hbftechnologies/starlanenotes/raw/branch/v1.0.11/docker/docker-compose.yml
curl -fsSLo .env https://github.com/hbftechnologies/starlanenotes/raw/branch/v1.0.11/docker/.env.example

# Generate and write a production JWT secret
JWT_SECRET="$(openssl rand -hex 32)"
sed -i.bak "s/^JWT_SECRET=.*/JWT_SECRET=${JWT_SECRET}/" .env
rm -f .env.bak
# Edit BASE_URL in .env if needed.

# Pull and start
# Only needed when the image is private:
# docker login gitea.2eagles.xyz
mkdir -p storage
sudo chown -R 1000:1000 storage
docker compose pull
docker compose up -d
```

The application will be available at `http://localhost:9320`. The first user to register becomes the admin.

The default Compose file pulls the published Starlane Notes image, maps host port `9320` to the container's internal port `3000`, and stores persistent data in a local `storage/` directory beside the Compose file.

If the Gitea package is private, authenticate Docker before `docker compose pull`. If the repository is private and you download Compose files directly with `curl`, use `curl --netrc` for the raw file downloads. Docker registry auth and `.netrc` are separate.

## Pull and Restart Check

Use this quick check after updating `STARLANE_IMAGE`, pulling a new release tag, or refreshing `latest`:

```bash
docker compose pull
docker compose up -d
docker compose ps
docker compose logs -f starlane-notes
```

In another shell, verify the local health endpoint:

```bash
curl -fsS http://localhost:9320/api/v1/health
```

The health response should be JSON with `"status":"ok"`.

The default local-storage bind mount is `./storage:/data`. Compose resolves `./storage` relative to the directory containing `docker-compose.yml`; if the Compose file is in `/data/starlane-notes`, the host path is `/data/starlane-notes/storage`. Create it explicitly and make it writable by UID `1000`, the non-root `starlane` user inside the image:

```bash
mkdir -p storage
sudo chown -R 1000:1000 storage
```

## Docker Compose

Choose the Compose file that matches how you want to run the app.

| File | Image source | Storage | Use case |
|---|---|---|---|
| `docker-compose.yml` | Pulls `STARLANE_IMAGE` | Local `./storage` folder | Simple self-hosting with visible files |
| `docker-compose.volume.yml` | Pulls `STARLANE_IMAGE` | Docker named volume | Self-hosting with Docker-managed data |
| `docker-compose.dev.yml` | Builds `docker/Dockerfile` | Local `./storage` folder | Development image build with visible files |
| `docker-compose.dev.volume.yml` | Builds `docker/Dockerfile` | Docker named volume | Development image build with Docker-managed data |

For self-hosting, download one of the image-pull files into your deployment directory:

```bash
# Local storage folder
curl -fsSLO https://github.com/hbftechnologies/starlanenotes/raw/branch/v1.0.11/docker/docker-compose.yml

# Or Docker named volume
curl -fsSLo docker-compose.yml https://github.com/hbftechnologies/starlanenotes/raw/branch/v1.0.11/docker/docker-compose.volume.yml
```

For development builds from a repo checkout:

```bash
cd docker

# Local storage folder
docker compose -f docker-compose.dev.yml up -d --build

# Or Docker named volume
docker compose -f docker-compose.dev.volume.yml up -d --build
```

### SQLite (default, single-node, local storage)

`docker-compose.yml`:

```yaml
services:
  starlane-notes:
    image: ${STARLANE_IMAGE:-ghcr.io/hbftechnologies/starlanenotes:latest}
    container_name: starlane-notes
    restart: unless-stopped
    ports:
      - "9320:3000"
    volumes:
      - ./storage:/data
    env_file:
      - .env
```

### SQLite with Docker Named Volume

`docker-compose.volume.yml`:

```yaml
services:
  starlane-notes:
    image: ${STARLANE_IMAGE:-ghcr.io/hbftechnologies/starlanenotes:latest}
    container_name: starlane-notes
    restart: unless-stopped
    ports:
      - "9320:3000"
    volumes:
      - starlane-notes-data:/data
    env_file:
      - .env

volumes:
  starlane-notes-data:
```

### Development Build Variants

The development Compose files mirror the two storage choices, but build the image locally:

```yaml
services:
  starlane-notes:
    build:
      context: ..
      dockerfile: docker/Dockerfile
    container_name: starlane-notes
    restart: unless-stopped
    ports:
      - "9320:3000"
    volumes:
      - ./storage:/data
    env_file:
      - .env
```

### PostgreSQL (multi-node / larger deployments)

For PostgreSQL, disable SQLite in `.env` and point the app at the `db` service with split variables:

```env
SQLITE_ENABLED=false
POSTGRES_HOST=db
POSTGRES_PORT=5432
POSTGRES_USER=starlane
POSTGRES_PASSWORD=password
POSTGRES_DB=starlane_notes
POSTGRES_SSLMODE=disable
```

```yaml
services:
  starlane-notes:
    image: ${STARLANE_IMAGE:-ghcr.io/hbftechnologies/starlanenotes:latest}
    container_name: starlane-notes
    restart: unless-stopped
    ports:
      - "9320:3000"
    volumes:
      - ./storage:/data
    env_file:
      - .env
    depends_on:
      db:
        condition: service_healthy

  db:
    image: postgres:16-alpine
    container_name: starlane-notes-db
    restart: unless-stopped
    volumes:
      - pg-data:/var/lib/postgresql/data
    environment:
      - POSTGRES_USER=${POSTGRES_USER:-starlane}
      - POSTGRES_PASSWORD=${POSTGRES_PASSWORD:-password}
      - POSTGRES_DB=${POSTGRES_DB:-starlane_notes}
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U $${POSTGRES_USER}"]
      interval: 5s
      timeout: 5s
      retries: 5

volumes:
  pg-data:
```

## Volumes

| Mount point | Purpose |
|---|---|
| `storage/` -> `/data` | All persistent data: SQLite database file, document files, search index, attachments |
| `starlane-notes-data` -> `/data` | Docker-managed equivalent used by the named-volume Compose files |

The `/data` directory contains:

- `starlane-notes.db` -- SQLite database (when using SQLite)
- `.starlane-notes-data-dir` -- Startup marker written by the server to prove the mount is writable
- `spaces/` -- Document files organized by space (markdown on disk)
- `uploads/` -- Uploaded files and attachments
- `search_index/` -- Tantivy full-text search index

## Ports

| Port | Protocol | Purpose |
|---|---|---|
| `9320` | HTTP | Host port for the web UI and API |
| `3000` | HTTP | Container-internal API server and web UI |

## Published Image

The default Compose file pulls:

```text
ghcr.io/hbftechnologies/starlanenotes:latest
```

Set `STARLANE_IMAGE` in `.env` to pin a specific release tag. The repository still includes `docker/Dockerfile` for maintainers and CI image publishing.

For private image pulls, run:

```bash
docker login gitea.2eagles.xyz
docker compose pull
```

For full registry details, see [Gitea Container Registry](gitea-container-registry.md).

## Production Checklist

- Set `STARLANE_ENV=production` -- this enforces `JWT_SECRET` and restricts CORS to `BASE_URL`
- Set `JWT_SECRET` to a random string of at least 32 characters (`openssl rand -hex 32`)
- Set `BASE_URL` to your public-facing URL (used for CORS, OAuth callbacks, redirects)
- Optional: set `STARLANE_CORS_ORIGINS` for extra comma-separated production origins. Built-in Tauri native-app origins are already allowed so desktop/mobile apps can connect to the server.
- Put a reverse proxy (NGINX, Caddy, Traefik) in front for TLS termination
- Back up the local `storage/` directory or the `starlane-notes-data` Docker volume regularly

## Reverse Proxy Example (Caddy)

```
notes.example.com {
    reverse_proxy starlane-notes:3000
}
```

## Smoke Test

To build the local development image, run a temporary production-mode container, and verify the health endpoint:

```bash
npm run docker:smoke
```
