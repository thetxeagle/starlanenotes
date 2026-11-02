#!/usr/bin/env python3
"""Observe rapid inode mutation stamps; never qualify storage or skip a gate."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import sys
import tempfile

ITERATIONS = 1000
A = b"A" * 64
B = b"B" * 64


def stamp(path):
    s = path.stat()
    return {"device": s.st_dev, "inode": s.st_ino, "length": s.st_size,
            "links": s.st_nlink, "mode": s.st_mode,
            "mtime": s.st_mtime_ns // 1_000_000_000,
            "mtime_ns": s.st_mtime_ns % 1_000_000_000,
            "ctime": s.st_ctime_ns // 1_000_000_000,
            "ctime_ns": s.st_ctime_ns % 1_000_000_000}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def selected_mount(path):
    # Resolve the actual opened directory's mount, including stacked bind mounts.
    fd = os.open(path, os.O_PATH | os.O_DIRECTORY | os.O_CLOEXEC)
    try:
        info = Path(f"/proc/self/fdinfo/{fd}").read_text().splitlines()
        mount_id = next(line.split()[1] for line in info if line.startswith("mnt_id:"))
        for line in Path("/proc/self/mountinfo").read_text().splitlines():
            left, right = line.split(" - ", 1)
            fields, tail = left.split(), right.split()
            if fields[0] != mount_id:
                continue
            safe_flags = {"rw", "ro", "nosuid", "suid", "nodev", "dev", "noexec", "exec",
                          "relatime", "noatime", "strictatime", "nodiratime", "lazytime", "sync", "async"}
            def safe(options):
                return sorted({x for x in options.split(",") if x in safe_flags or re.fullmatch(r"(?:size|nr_inodes|mode)=[0-9]+[kKmMgG]?", x)})
            return {"filesystem_type": tail[0], "sanitized_mount_options": safe(fields[5]),
                    "sanitized_superblock_options": safe(tail[2]),
                    "selection": "opened directory mount ID",
                    "omitted": "mount source, mount paths and non-whitelisted options"}
        raise RuntimeError("No opened directory Linux mount found")
    finally:
        os.close(fd)


def observe(root, case):
    result = {"iterations": ITERATIONS, "metadata_collisions": 0, "ctime_collisions": 0,
              "unexpected_identity_or_final_state": 0, "examples": []}
    path, alias = root / "original", root / "alias"
    for i in range(ITERATIONS):
        path.write_bytes(A)
        before = stamp(path)
        before_hash = digest(path)
        if case == "same_length_write":
            path.write_bytes(B)
            expected = hashlib.sha256(B).hexdigest()
        elif case == "write_then_restore":
            path.write_bytes(B)
            path.write_bytes(A)
            expected = before_hash
        else:
            os.link(path, alias)
            alias.unlink()
            expected = before_hash
        after = stamp(path)
        after_hash = digest(path)
        collision = before == after
        result["metadata_collisions"] += collision
        result["ctime_collisions"] += (before["ctime"], before["ctime_ns"]) == (after["ctime"], after["ctime_ns"])
        unexpected = (before["device"], before["inode"]) != (after["device"], after["inode"]) or after_hash != expected or after["links"] != 1
        result["unexpected_identity_or_final_state"] += unexpected
        if (collision or unexpected) and len(result["examples"]) < 3:
            result["examples"].append({"iteration": i, "before": before, "after": after,
                                       "before_sha256": before_hash, "after_sha256": after_hash,
                                       "metadata_equal": collision, "final_state_expected": not unexpected})
        if i == ITERATIONS - 1:
            last_observation = {"iteration": i, "before": before, "after": after,
                                          "before_sha256": before_hash, "after_sha256": after_hash}
    if not result["examples"]:
        result["examples"].append(last_observation)
    path.unlink()
    return result


def main():
    report = {"purpose": "Bounded observation only; not universal qualification, no gate skipped",
              "iterations_per_case": ITERATIONS, "sleep_used": False,
              "scope": "Only newly generated files in one owned0700 disposable directory"}
    root = None
    failed = False
    try:
        u = os.uname()
        report["kernel"] = {"release": u.release, "version": u.version, "machine": u.machine}
        if u.sysname != "Linux":
            raise RuntimeError("Linux mount metadata required")
        # Preserve the selected parent; never chmod, modify or delete it.
        parent = Path(os.environ.get("TMPDIR") or "/tmp").resolve(strict=True)
        root = Path(tempfile.mkdtemp(prefix="starlane-storage-observation-", dir=parent))
        os.chmod(root, 0o700)
        if root.stat().st_uid != os.geteuid() or root.stat().st_mode & 0o777 != 0o700:
            raise RuntimeError("Generated directory ownership/mode mismatch")
        report["mount"] = selected_mount(root)
        report["cases"] = {case: observe(root, case) for case in
                           ("same_length_write", "write_then_restore", "hardlink_then_unlink")}
    except Exception as error:
        failed = True
        report["error"] = {"type": type(error).__name__, "errno": getattr(error, "errno", None)}
    finally:
        if root is not None:
            try:
                shutil.rmtree(root)
                report["self_cleanup"] = {"attempted": True, "generated_directory_absent": not root.exists()}
            except Exception as error:
                failed = True
                report["self_cleanup"] = {"attempted": True, "error_type": type(error).__name__, "errno": getattr(error, "errno", None)}
        else:
            report["self_cleanup"] = {"attempted": False, "no_generated_directory": True}
    print(json.dumps(report, indent=2))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
