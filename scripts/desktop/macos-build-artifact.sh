#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(git rev-parse --show-toplevel)"
cd "${ROOT_DIR}"

VERSION="$(node -p "require('./package.json').version")"
SHORT_SHA="$(git rev-parse --short HEAD)"
ARTIFACT_DIR="${ROOT_DIR}/artifacts/desktop/macos"
APP_BUNDLE="${ROOT_DIR}/target/release/bundle/macos/Starlane Notes.app"
ZIP_NAME="StarlaneNotes-macos-${VERSION}-${SHORT_SHA}.zip"
ZIP_PATH="${ARTIFACT_DIR}/${ZIP_NAME}"

rm -rf "${ARTIFACT_DIR}"
mkdir -p "${ARTIFACT_DIR}"

npm run desktop:build:app

if [[ ! -d "${APP_BUNDLE}" ]]; then
  echo "Expected app bundle was not found: ${APP_BUNDLE}" >&2
  find "${ROOT_DIR}/target/release/bundle" -maxdepth 3 -print >&2 || true
  exit 1
fi

/usr/bin/ditto -c -k --sequesterRsrc --keepParent "${APP_BUNDLE}" "${ZIP_PATH}"
/usr/bin/shasum -a 256 "${ZIP_PATH}" > "${ZIP_PATH}.sha256"

cat <<EOF
macos_zip=${ZIP_PATH}
macos_sha256=${ZIP_PATH}.sha256
EOF
