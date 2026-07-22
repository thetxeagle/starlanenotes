#!/usr/bin/env bash

set -euo pipefail

PASS_COUNT=0
WARN_COUNT=0

pass() {
  PASS_COUNT=$((PASS_COUNT + 1))
  printf 'PASS: %s\n' "$1"
}

warn() {
  WARN_COUNT=$((WARN_COUNT + 1))
  printf 'WARN: %s\n' "$1"
}

info() {
  printf 'INFO: %s\n' "$1"
}

section() {
  printf '\n== %s ==\n' "$1"
}

print_cmd_value() {
  local label="$1"
  local value="$2"
  printf '%s: %s\n' "$label" "$value"
}

section "Xcode Toolchain"
if command -v xcode-select >/dev/null 2>&1; then
  XCODE_PATH="$(xcode-select -p 2>/dev/null || true)"
  if [[ -n "${XCODE_PATH}" ]]; then
    pass "xcode-select is configured"
    print_cmd_value "xcode-select -p" "${XCODE_PATH}"
  else
    warn "xcode-select is not configured"
  fi
else
  warn "xcode-select command not found"
fi

if command -v xcodebuild >/dev/null 2>&1; then
  XCODE_VERSION="$(xcodebuild -version 2>/dev/null | tr '\n' '; ' || true)"
  if [[ -n "${XCODE_VERSION}" ]]; then
    pass "xcodebuild is available"
    print_cmd_value "xcodebuild -version" "${XCODE_VERSION}"
  else
    warn "xcodebuild is not available"
  fi
else
  warn "xcodebuild command not found"
fi

section "Simulator Runtime"
if command -v xcrun >/dev/null 2>&1; then
  if xcrun simctl list devices available >/dev/null 2>&1; then
    pass "simctl can list available devices"
  else
    warn "simctl failed (CoreSimulator service, license, developer-dir, or sandbox restriction)"
  fi
else
  warn "xcrun command not found"
fi

section "Signing Configuration"
if [[ -n "${APPLE_DEVELOPMENT_TEAM:-}" ]]; then
  pass "APPLE_DEVELOPMENT_TEAM is set"
  print_cmd_value "APPLE_DEVELOPMENT_TEAM" "${APPLE_DEVELOPMENT_TEAM}"
else
  warn "APPLE_DEVELOPMENT_TEAM is not set"
fi

if [[ -n "${APPLE_BUNDLE_IDENTIFIER:-}" ]]; then
  pass "APPLE_BUNDLE_IDENTIFIER is set"
  print_cmd_value "APPLE_BUNDLE_IDENTIFIER" "${APPLE_BUNDLE_IDENTIFIER}"
else
  info "APPLE_BUNDLE_IDENTIFIER is not set; using the checked-in Tauri identifier"
fi

section "Code Signing Identities"
if command -v security >/dev/null 2>&1; then
  IDENTITIES_OUTPUT="$(security find-identity -v -p codesigning 2>/dev/null || true)"
  IDENTITIES_COUNT="$(printf '%s' "${IDENTITIES_OUTPUT}" | awk '/valid identities found/ { print $1 }')"
  IDENTITIES_COUNT="${IDENTITIES_COUNT:-0}"
  if [[ "${IDENTITIES_COUNT}" =~ ^[0-9]+$ ]] && [[ "${IDENTITIES_COUNT}" -gt 0 ]]; then
    pass "code signing identities detected (${IDENTITIES_COUNT})"
  else
    warn "no code signing identities detected"
  fi
  print_cmd_value "security find-identity -v -p codesigning" "${IDENTITIES_OUTPUT:-<no output>}"
else
  warn "security command not found"
fi

section "Provisioning Profiles"
PROFILE_DIRS=(
  "${HOME}/Library/MobileDevice/Provisioning Profiles"
  "${HOME}/Library/Developer/Xcode/UserData/Provisioning Profiles"
)
PROFILE_COUNT=0
for PROFILES_DIR in "${PROFILE_DIRS[@]}"; do
  if [[ -d "${PROFILES_DIR}" ]]; then
    DIR_COUNT="$(find "${PROFILES_DIR}" -maxdepth 1 -type f -name '*.mobileprovision' | wc -l | tr -d ' ')"
    if [[ "${DIR_COUNT}" =~ ^[0-9]+$ ]]; then
      PROFILE_COUNT=$((PROFILE_COUNT + DIR_COUNT))
    fi
    print_cmd_value "profiles dir" "${PROFILES_DIR} (${DIR_COUNT:-0})"
  fi
done

if [[ "${PROFILE_COUNT}" -gt 0 ]]; then
  pass "provisioning profiles found (${PROFILE_COUNT})"
else
  warn "no provisioning profiles found"
  print_cmd_value "expected dirs" "${PROFILE_DIRS[*]}"
fi

section "Result"
print_cmd_value "passes" "${PASS_COUNT}"
print_cmd_value "warnings" "${WARN_COUNT}"

if [[ "${WARN_COUNT}" -gt 0 ]]; then
  cat <<'EOF'

Next actions:
1. Sign in to Xcode with your Apple ID and confirm team membership is active.
2. Set APPLE_DEVELOPMENT_TEAM for this shell before running device builds.
3. Ensure at least one Apple Development certificate and matching provisioning profile exist.
EOF
  exit 2
fi

echo "iOS signing prerequisites look ready."
