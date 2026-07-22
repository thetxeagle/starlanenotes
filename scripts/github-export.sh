#!/usr/bin/env bash

# Produce a clean, history-free public GitHub checkout from this Gitea workspace.
# The output contains product source, public docs, Docker assets, GitHub Actions,
# and the GitHub-specific README, but never assistant instructions or session data.

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/github-export.sh [--update] <destination> [git-ref]

Creates an independent Git repository on branch main at <destination>. The
destination must be empty or not exist. The default git-ref is HEAD.

Pass --update to refresh an existing, clean public export checkout without
rewriting its history. This replaces its tracked public files with the
sanitized export and creates a normal follow-up commit when content changed.

Example:
  scripts/github-export.sh /tmp/starlanenotes-github v1.3.0
  git -C /tmp/starlanenotes-github remote add origin https://github.com/hbftechnologies/starlanenotes.git
  git -C /tmp/starlanenotes-github push -u origin main

  git clone https://github.com/hbftechnologies/starlanenotes.git /tmp/starlanenotes-github
  scripts/github-export.sh --update /tmp/starlanenotes-github v1.3.0
  git -C /tmp/starlanenotes-github push
EOF
}

update_existing=false
if [[ "${1:-}" == "--update" ]]; then
  update_existing=true
  shift
fi

if [[ $# -lt 1 || $# -gt 2 ]]; then
  usage >&2
  exit 64
fi

destination="$1"
ref="${2:-HEAD}"
repo_root="$(git rev-parse --show-toplevel)"

if [[ "$update_existing" == false ]] && [[ -e "$destination" ]] && [[ -n "$(find "$destination" -mindepth 1 -maxdepth 1 -print -quit)" ]]; then
  echo "Refusing to overwrite non-empty destination: $destination" >&2
  exit 65
fi

if [[ "$update_existing" == true ]]; then
  if [[ ! -d "$destination/.git" ]]; then
    echo "--update requires an existing Git checkout: $destination" >&2
    exit 65
  fi
  if [[ -n "$(git -C "$destination" status --porcelain)" ]]; then
    echo "Refusing to update a dirty export checkout: $destination" >&2
    exit 65
  fi
fi

mkdir -p "$destination"
stage_dir="$(mktemp -d "${TMPDIR:-/tmp}/starlanenotes-github-export.XXXXXX")"
trap 'rm -rf "$stage_dir"' EXIT

git -C "$repo_root" archive --format=tar "$ref" | tar -xf - -C "$stage_dir"

rm -rf \
  "$stage_dir/.claude" \
  "$stage_dir/.codex" \
  "$stage_dir/.gitea" \
  "$stage_dir/changelogs" \
  "$stage_dir/repo-docs" \
  "$stage_dir/sessions"
rm -f \
  "$stage_dir/AGENTS.md" \
  "$stage_dir/CLAUDE.md" \
  "$stage_dir/CODEX.md" \
  "$stage_dir/SCRATCHPAD.md"

if [[ ! -f "$stage_dir/README.github.md" ]]; then
  echo "Missing GitHub README template at README.github.md" >&2
  exit 66
fi
mv "$stage_dir/README.github.md" "$stage_dir/README.md"

# Rewrite deployment defaults and canonical source links for the public mirror.
while IFS= read -r -d '' file; do
  perl -0pi -e 's#gitea\.2eagles\.xyz/hft-applications/starlanenotes-docs#ghcr.io/hbftechnologies/starlanenotes-docs#g' "$file"
  perl -0pi -e 's#gitea\.2eagles\.xyz/hft-applications/starlanenotes#ghcr.io/hbftechnologies/starlanenotes#g' "$file"
  perl -0pi -e 's#https://gitea\.2eagles\.xyz/hft-applications/StarlaneNotes#https://github.com/hbftechnologies/starlanenotes#g' "$file"
done < <(find "$stage_dir" -type f \( -name '*.md' -o -name '*.yml' -o -name '*.yaml' -o -name '*.env' -o -name '*.example' \) -print0)

if [[ "$update_existing" == true ]]; then
  git -C "$destination" rm -r --ignore-unmatch --quiet -- .
  cp -R "$stage_dir"/. "$destination"/
  # The private checkout intentionally ignores data directories such as
  # server/src/storage. The archive is authoritative, so force-add it.
  git -C "$destination" add --all --force
  if git -C "$destination" diff --cached --quiet; then
    echo "Public GitHub export is already current: $destination"
  else
    git -C "$destination" -c user.name="Starlane Notes Export" -c user.email="noreply@starlane-notes.invalid" \
      commit --quiet -m "chore: refresh Starlane Notes source"
    echo "Public GitHub export updated at: $destination"
    echo "Next: git -C '$destination' push"
  fi
else
  cp -R "$stage_dir"/. "$destination"/
  git -C "$destination" init -b main --quiet
  # The private checkout intentionally ignores data directories such as
  # server/src/storage. The archive is authoritative, so force-add it.
  git -C "$destination" add --all --force
  git -C "$destination" -c user.name="Starlane Notes Export" -c user.email="noreply@starlane-notes.invalid" \
    commit --quiet -m "chore: publish Starlane Notes source"

  echo "Public GitHub export created at: $destination"
  echo "Next: git -C '$destination' remote add origin https://github.com/hbftechnologies/starlanenotes.git"
  echo "Then: git -C '$destination' push -u origin main"
fi
