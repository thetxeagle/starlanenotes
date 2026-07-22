#!/usr/bin/env bash

set -euo pipefail

COMMAND="${1:-}"
if [[ -z "${COMMAND}" ]]; then
  echo "Usage: scripts/mobile/ios-tauri.sh <init|dev|build> [tauri-args...]"
  exit 1
fi
shift || true

export DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}"

TAURI_CONFIG_FILE=""
if [[ -n "${APPLE_DEVELOPMENT_TEAM:-}" || -n "${APPLE_BUNDLE_IDENTIFIER:-}" ]]; then
  TAURI_CONFIG_FILE="$(mktemp)"
  IDENTIFIER_CONFIG=""
  if [[ -n "${APPLE_BUNDLE_IDENTIFIER:-}" ]]; then
    IDENTIFIER_CONFIG="\"identifier\": \"${APPLE_BUNDLE_IDENTIFIER}\","
  fi
  cat > "${TAURI_CONFIG_FILE}" <<EOF
{
  ${IDENTIFIER_CONFIG}
  "bundle": {
    "iOS": {
      "developmentTeam": "${APPLE_DEVELOPMENT_TEAM:-}"
    }
  }
}
EOF
fi

cleanup() {
  if [[ -n "${TAURI_CONFIG_FILE}" && -f "${TAURI_CONFIG_FILE}" ]]; then
    rm -f "${TAURI_CONFIG_FILE}"
  fi
}
trap cleanup EXIT

cd apps/desktop/src-tauri
if [[ "${COMMAND}" == "build" ]]; then
  # Tauri/Xcode can fail on repeat simulator builds if archive/app output already exists.
  rm -rf gen/apple/build
fi

if [[ -n "${TAURI_CONFIG_FILE}" ]]; then
  cargo tauri ios "${COMMAND}" --config "${TAURI_CONFIG_FILE}" "$@"
else
  cargo tauri ios "${COMMAND}" "$@"
fi
