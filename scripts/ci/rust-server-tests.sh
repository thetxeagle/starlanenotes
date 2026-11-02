#!/usr/bin/env bash
# Linux CI gate: one private disposable root and serial process-global fixtures.
set -euo pipefail
umask 077

if [[ $# -ne 0 ]]; then
  echo "Run this gate without arguments; it executes every ordinary server test." >&2
  exit 2
fi

command -v python3 >/dev/null
command -v cargo >/dev/null
repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)"
cd -- "$repo_root"
python3 -I scripts/ci/check-portable-sql-placeholders.py
test_root="$(mktemp -d "${TMPDIR:-/tmp}/starlane-rust-tests.XXXXXX")"
test_root="$(cd -- "$test_root" && pwd -P)"
finish() {
  local result=$?
  if [[ $result -eq 0 ]]; then
    rm -rf -- "$test_root"
  else
    printf 'Server test gate failed; private fixture evidence retained at %s\n' "$test_root" >&2
  fi
}
trap finish EXIT
mkdir -- "$test_root/data" "$test_root/tmp"
export DATA_DIR="$test_root/data"
export TMPDIR="$test_root/tmp"

python3 -I - <<'PY'
import os, pathlib, platform, stat
if platform.system() != "Linux":
    raise SystemExit("This CI gate requires Linux storage/worker semantics; use the documented platform checks separately")
for name in ("DATA_DIR", "TMPDIR"):
    selected = pathlib.Path(os.environ[name])
    metadata = selected.lstat()
    if not stat.S_ISDIR(metadata.st_mode) or metadata.st_uid != os.geteuid() or stat.S_IMODE(metadata.st_mode) != 0o700:
        raise SystemExit(f"{name} must be a newly owned private0700 directory")
# Exercise the real anonymous-inode prerequisite before interpreting test failures.
try:
    fd = os.open(os.environ["TMPDIR"], os.O_TMPFILE | os.O_RDWR, 0o600)
    try:
        assert os.write(fd, b"starlane-ci-probe") == 17
        os.fsync(fd)
        assert os.fstat(fd).st_nlink == 0
    finally:
        os.close(fd)
except (AttributeError, OSError) as error:
    raise SystemExit(f"TMPDIR lacks required Linux O_TMPFILE storage support: {error}; provide a supported filesystem")
print("Qualified private DATA_DIR/TMPDIR and actual Linux anonymous-file support")
PY

# Observe the actual selected kernel/mount without turning collisions into skips.
# Diagnostic failures must not prevent the unchanged Rust assertions from running.
observation_status=0
python3 -I scripts/ci/storage-profile-observation.py || observation_status=$?

# Same-executable custody probes hash the actual test ELF within fixed deadlines.
# Exclude DWARF payload; keep normal test optimization/assertion/overflow defaults.
export CARGO_PROFILE_TEST_DEBUG=0

# Includes the library/binary and external Cargo test harnesses. Individual
# opt-in engine/process tests keep their original selection requirements.
cargo_status=0
cargo test --locked --no-fail-fast -p starlane-notes-server -- --test-threads=1 || cargo_status=$?
if [[ $cargo_status -ne 0 ]]; then
  exit "$cargo_status"
fi
exit "$observation_status"
