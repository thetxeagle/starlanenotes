#!/usr/bin/env bash

set -euo pipefail

export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
export ANDROID_SDK_ROOT="${ANDROID_SDK_ROOT:-$ANDROID_HOME}"
export JAVA_HOME="${JAVA_HOME:-/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"
export PATH="/opt/homebrew/opt/openjdk@17/bin:$ANDROID_HOME/emulator:$ANDROID_HOME/platform-tools:$PATH"

cd "$(git rev-parse --show-toplevel)/apps/desktop/src-tauri"
exec cargo tauri android "$@"
