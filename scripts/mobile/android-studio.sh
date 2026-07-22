#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
ANDROID_PROJECT="${REPO_ROOT}/apps/desktop/src-tauri/gen/android"

export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
export ANDROID_SDK_ROOT="${ANDROID_SDK_ROOT:-$ANDROID_HOME}"
export JAVA_HOME="${JAVA_HOME:-/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home}"
export PATH="/opt/homebrew/opt/openjdk@17/bin:$ANDROID_HOME/emulator:$ANDROID_HOME/platform-tools:$PATH"

if [[ ! -f "${ANDROID_PROJECT}/settings.gradle" ]]; then
  cat >&2 <<MSG
Android Studio project not found at:
  ${ANDROID_PROJECT}

Generate it first with:
  npm run mobile:android:init
MSG
  exit 1
fi

if command -v studio >/dev/null 2>&1; then
  exec studio "${ANDROID_PROJECT}"
fi

if [[ "$(uname -s)" == "Darwin" ]] && open -Ra "Android Studio"; then
  exec open -a "Android Studio" "${ANDROID_PROJECT}"
fi

cat >&2 <<MSG
Android Studio was not found on PATH and could not be opened via macOS LaunchServices.

Open this folder manually in Android Studio:
  ${ANDROID_PROJECT}
MSG
exit 1
