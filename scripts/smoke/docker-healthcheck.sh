#!/usr/bin/env bash
set -euo pipefail

IMAGE="${IMAGE:-starlane-notes:local}"
PORT="${PORT:-31873}"
CONTAINER_NAME="${CONTAINER_NAME:-starlane-notes-smoke}"
JWT_SECRET="${JWT_SECRET:-$(node scripts/generate-jwt-secret.mjs | sed 's/^JWT_SECRET=//')}"

cleanup() {
  docker stop "$CONTAINER_NAME" >/dev/null 2>&1 || true
}
trap cleanup EXIT

docker build -f docker/Dockerfile -t "$IMAGE" .
cleanup

docker run --rm -d \
  --name "$CONTAINER_NAME" \
  -p "127.0.0.1:${PORT}:3000" \
  -e STARLANE_ENV=production \
  -e "JWT_SECRET=${JWT_SECRET}" \
  -e "BASE_URL=http://127.0.0.1:${PORT}" \
  "$IMAGE" >/dev/null

for _ in $(seq 1 60); do
  if curl -fsS "http://127.0.0.1:${PORT}/api/v1/health" >/dev/null; then
    curl -fsS "http://127.0.0.1:${PORT}/api/v1/health"
    echo
    exit 0
  fi
  sleep 0.5
done

docker logs "$CONTAINER_NAME" >&2
exit 1
