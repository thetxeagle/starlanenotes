# Reverse Proxy Setup

Starlane Notes listens on port `3000` inside the container. The default Compose file exposes it on host port `9320` and requires a reverse proxy for HTTPS in production. WebSocket support is required for real-time CRDT sync.

## Caddy (recommended)

```
notes.example.com {
    reverse_proxy localhost:9320
}
```

Caddy provisions TLS automatically via Let's Encrypt. WebSocket proxying is included by default.

## nginx

```nginx
server {
    listen 80;
    server_name notes.example.com;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl;
    server_name notes.example.com;

    ssl_certificate     /etc/letsencrypt/live/notes.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/notes.example.com/privkey.pem;

    location / {
        proxy_pass         http://localhost:9320;
        proxy_set_header   Host $host;
        proxy_set_header   X-Real-IP $remote_addr;
        proxy_set_header   X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header   X-Forwarded-Proto $scheme;

        # WebSocket support (required for CRDT sync)
        proxy_http_version 1.1;
        proxy_set_header   Upgrade $http_upgrade;
        proxy_set_header   Connection "upgrade";
        proxy_read_timeout 86400;
    }
}
```

## Traefik (Docker label-based)

Add labels to the `starlane-notes` service in `docker-compose.yml`:

```yaml
labels:
  - "traefik.enable=true"
  - "traefik.http.routers.starlane.rule=Host(`notes.example.com`)"
  - "traefik.http.routers.starlane.entrypoints=websecure"
  - "traefik.http.routers.starlane.tls.certresolver=letsencrypt"
  - "traefik.http.services.starlane.loadbalancer.server.port=3000"
```

## Required environment variables

When running behind a proxy, set these in `docker/.env`:

```env
BASE_URL=https://notes.example.com
STARLANE_ENV=production
JWT_SECRET=<output of: openssl rand -hex 32>
# Optional extra comma-separated browser/app origins.
# Native Tauri app origins are already allowed by the server.
STARLANE_CORS_ORIGINS=
```

## Port binding

For Caddy/nginx on the same host, bind Docker to localhost only so the app is not directly reachable:

```yaml
ports:
  - "127.0.0.1:9320:3000"
```

Traefik running on the same Docker network can keep using the container-internal service port `3000`.
