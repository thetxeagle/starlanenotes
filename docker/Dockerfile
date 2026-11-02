# Stage 1: Build React web app
FROM node:20.19-slim AS web-builder
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
COPY Cargo.toml Cargo.lock ./
COPY server/ server/
# Cargo resolves the committed lockfile against the real workspace graph even though this image
# builds only the server package.
COPY apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/Cargo.toml
COPY apps/desktop/src-tauri/src/lib.rs apps/desktop/src-tauri/src/lib.rs
# Copy built web assets for embedding
COPY --from=web-builder /app/apps/web/dist server/web-dist
RUN cargo build --locked --release --package starlane-notes-server

# Stage 3: Runtime
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl python3 && rm -rf /var/lib/apt/lists/*
RUN useradd -m -s /bin/bash starlane
WORKDIR /app

COPY --from=server-builder /app/target/release/starlane-notes-server /app/starlane-notes-server
COPY --from=server-builder /app/server/web-dist /app/web-dist

RUN install -d -m 0700 -o starlane -g starlane /data
VOLUME /data
USER starlane

ENV DATA_DIR=/data
ENV WEB_DIST_DIR=/app/web-dist
ENV RUST_LOG=starlane_notes_server=info

EXPOSE 3000

CMD ["/app/starlane-notes-server"]
