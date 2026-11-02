#!/usr/bin/env python3
"""Reject SQLite-only numbered placeholders in Rust SQLx source."""

from __future__ import annotations

import pathlib
import re
import sys


REPOSITORY_ROOT = pathlib.Path(__file__).resolve().parents[2]
PLACEHOLDER = re.compile(r"\?[0-9]+")
SEARCH_ROOTS = (REPOSITORY_ROOT / "server" / "src", REPOSITORY_ROOT / "server" / "tests")


def main() -> int:
    findings: list[str] = []
    for search_root in SEARCH_ROOTS:
        for path in sorted(search_root.rglob("*.rs")):
            for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
                if PLACEHOLDER.search(line):
                    findings.append(
                        f"{path.relative_to(REPOSITORY_ROOT)}:{line_number}: {line.strip()}"
                    )

    if findings:
        print(
            "SQLite-only numbered SQL placeholders are not portable; use $1, $2, ... instead:",
            file=sys.stderr,
        )
        print("\n".join(findings), file=sys.stderr)
        return 1

    print("Portable SQL placeholder check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
