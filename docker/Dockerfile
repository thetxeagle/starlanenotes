# Stage 1: Build React web app
FROM node:20-slim AS web-builder
WORKDIR /app
COPY package.json package-lock.json ./
COPY apps/web/package.json apps/web/
COPY packages/shared-types/package.json packages/shared-types/
RUN npm ci --workspace=apps/web --workspace=packages/shared-types
COPY apps/web/ apps/web/
COPY packages/shared-types/ packages/shared-types/
RUN npm run build --workspace=apps/web

# Stage 2: Build Rust server
FROM rust:1.88-slim AS server-builder
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY server/ server/
# Create a minimal workspace Cargo.toml (server only, no desktop app)
RUN echo '[workspace]\nresolver = "2"\nmembers = ["server"]' > Cargo.toml
# Copy built web assets for embedding
COPY --from=web-builder /app/apps/web/dist server/web-dist
RUN cargo build --release --package starlane-notes-server

# Stage 3: Runtime
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/*
RUN useradd -m -s /bin/bash starlane
WORKDIR /app

COPY --from=server-builder /app/target/release/starlane-notes-server /app/starlane-notes-server
COPY --from=server-builder /app/server/web-dist /app/web-dist

RUN mkdir -p /data && chown starlane:starlane /data
VOLUME /data
USER starlane

ENV DATA_DIR=/data
ENV WEB_DIST_DIR=/app/web-dist
ENV RUST_LOG=starlane_notes_server=info

EXPOSE 3000

CMD ["/app/starlane-notes-server"]
