FROM node:20-alpine AS builder
WORKDIR /app

COPY package.json package-lock.json ./
COPY apps/docs/package.json apps/docs/package.json
RUN npm ci --workspace=apps/docs

COPY apps/docs apps/docs
COPY docs/docs docs/docs
COPY scripts/docs/sync-starlight-content.mjs scripts/docs/sync-starlight-content.mjs
RUN npm run docs:build

FROM nginxinc/nginx-unprivileged:1.27-alpine
COPY docker/docs.nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=builder /app/apps/docs/dist /usr/share/nginx/html
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 CMD wget -qO- http://127.0.0.1:8080/ >/dev/null || exit 1
