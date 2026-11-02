#!/usr/bin/env python3
"""Private, stopped-write SQLite backup sets; never operates on running services.

The caller must stop every application/background/sync writer before create.
Search indexes and logs are excluded. Configuration and secret recovery remain an
external prerequisite. This tool verifies storage bytes, not application journeys.
"""

import argparse
from contextlib import closing, contextmanager
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import sqlite3
import stat
import sys
import tempfile
import uuid

FORMAT = "starlane-notes-stopped-write-backup"
FORMAT_VERSION = 3
JOURNAL_NAMESPACES = (".storage-journal", ".publication-journal", ".canonical-rewrite-journal", ".move-relocation-journal")
RECEIPT_FAMILIES = ("storage_journal_receipts", "publication_receipts", "canonical_revision_receipts", "move_commit_receipts", "asset_replay_receipts")
UPLOAD_NAMESPACES = (".attachment-upload-journal",)
UPLOAD_RECEIPT_FAMILIES = ("attachment_upload_receipts",)
ADMISSION_NAMESPACE = ".attachment-upload-admission"
CAPACITY_RECEIPT_FAMILIES = ("attachment_capacity_reservations", "attachment_retained_byte_ledger", "attachment_erasure_outbox", "attachment_erasure_dispositions", "draft_publication_receipts", "document_publication_snapshots", "snapshot_recovery_receipts", "attachment_audience_origins", "attachment_audience_bindings")
RESTORATION_NAMESPACES = (".restoration-custody",)
RESTORATION_RECEIPT_FAMILIES = ("storage_restore_generations", "storage_restore_current")
DATABASE = "starlane-notes.db"
EXCLUDED = ["logging", "logs", "search_index"]
CONFIG_PREREQUISITE = (
    "Recover the original deployment configuration and secrets separately, securely; "
    "this set does not contain external configuration or secret files. Protect the "
    "database backup itself: it contains private data and authentication material."
)
# Exact compiled producer profile; no runtime/caller-selected migration directory.


class BackupError(ValueError):
    """A failed validation leaves the source and pre-existing destinations intact."""


def require(condition, message):
    if not condition:
        raise BackupError(message)


def safe_relative(value):
    require(isinstance(value, str) and value and "\\" not in value and "\0" not in value,
            "Invalid relative inventory path")
    path = PurePosixPath(value)
    require(bool(path.parts) and not path.is_absolute() and value == path.as_posix()
            and all(part not in (".", "..") for part in path.parts), "Unsafe inventory path")
    return value


def private_path(path):
    """Reject symlinks in existing path components before resolving a path."""
    path = Path(os.path.abspath(path))
    for component in reversed((path, *path.parents)):
        if component.is_symlink():
            raise BackupError("Symbolic links are unsupported")
    return path


def separate(source, destination):
    source, destination = private_path(source), private_path(destination)
    require(source != destination and source not in destination.parents
            and destination not in source.parents, "Source and destination overlap")
    require(not destination.exists(), "Destination already exists")
    require(destination.parent.is_dir(), "Destination parent directory must already exist")
    return source, destination


def allowed_file(relative, sidecars=False):
    return (relative == DATABASE or relative == ".starlane-notes-data-dir"
            or relative.startswith(("spaces/", "uploads/", *(namespace + "/" for namespace in (*JOURNAL_NAMESPACES, *RESTORATION_NAMESPACES, *UPLOAD_NAMESPACES, ADMISSION_NAMESPACE))))
            or (sidecars and relative in (DATABASE + "-wal", DATABASE + "-shm", DATABASE + "-journal")))


def make_private_parents(path):
    # Path.mkdir(parents=True) ignores mode for intermediate ancestors.
    for directory in reversed((path, *path.parents)):
        if not directory.exists():
            directory.mkdir(mode=0o700)


def digest(path, destination=None):
    private_path(path)
    descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
    with os.fdopen(descriptor, "rb") as source:
        before = os.fstat(source.fileno())
        require(stat.S_ISREG(before.st_mode), "Only regular files are supported")
        checksum, size = hashlib.sha256(), 0
        target = None
        try:
            if destination is not None:
                make_private_parents(destination.parent)
                target = os.fdopen(os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), "wb")
            while chunk := source.read(1024 * 1024):
                checksum.update(chunk)
                size += len(chunk)
                if target:
                    target.write(chunk)
            if target:
                target.flush()
                os.fsync(target.fileno())
            after = os.fstat(source.fileno())
            current = os.stat(path, follow_symlinks=False)
            require((before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns, before.st_ctime_ns)
                    == (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns, after.st_ctime_ns)
                    == (current.st_dev, current.st_ino, current.st_size, current.st_mtime_ns, current.st_ctime_ns),
                    "Source changed during capture")
            return {"size_bytes": size, "sha256": checksum.hexdigest()}
        finally:
            if target:
                target.close()


def inventory(root, source=False):
    require(root.is_dir(), "Data directory is missing")
    result = {}
    def raise_walk_error(error):
        raise error
    for current, directories, files in os.walk(root, followlinks=False, onerror=raise_walk_error):
        current = Path(current)
        for name in directories + files:
            entry = current / name
            mode = entry.lstat().st_mode
            require(not stat.S_ISLNK(mode), "Symbolic links are unsupported")
            require(stat.S_ISDIR(mode) or stat.S_ISREG(mode), "Special files are unsupported")
        if current == root:
            require(all(name in ({"spaces", "uploads", *JOURNAL_NAMESPACES, *RESTORATION_NAMESPACES, *UPLOAD_NAMESPACES, ADMISSION_NAMESPACE} | (set(EXCLUDED) if source else set())) for name in directories),
                    "Unrecognized data directory; configure its recovery separately")
            if source:
                directories[:] = [name for name in directories if name not in EXCLUDED]
        for name in files:
            relative = safe_relative((current / name).relative_to(root).as_posix())
            require(allowed_file(relative, sidecars=source), "Unrecognized data file; configure its recovery separately")
            result[relative] = digest(current / name)
    require(DATABASE in result, "SQLite database is missing")
    return result


def source_custody(root):
    """Observe owned source identities; copies cannot recheck original custody.

    Return separate transient fingerprints to detect metadata-only changes during
    the stopped-write window. This is input evidence for application restoration,
    not authorization to install or replay any journal operation.
    """
    files, directories, fingerprints = {}, {}, {}
    root_stat = root.lstat()
    require(stat.S_ISDIR(root_stat.st_mode), "Source custody root is not a directory")
    identity = lambda md: {"device": md.st_dev, "inode": md.st_ino}
    def walk_error(error):
        raise error
    for current, names, leaves in os.walk(root, followlinks=False, onerror=walk_error):
        current = Path(current)
        if current == root:
            names[:] = [name for name in names if name not in EXCLUDED]
        for name in names + leaves:
            path = current / name
            relative = safe_relative(path.relative_to(root).as_posix())
            md = path.lstat()
            require(not stat.S_ISLNK(md.st_mode), "Symbolic links are unsupported")
            if stat.S_ISDIR(md.st_mode):
                directories[relative] = identity(md)
            elif stat.S_ISREG(md.st_mode):
                # SQLite is normalized, and sidecar/shared-memory identity is not
                # portable file ownership. Observe only independently owned bytes.
                if relative in (DATABASE, DATABASE + "-wal", DATABASE + "-shm", DATABASE + "-journal"):
                    continue
                require(allowed_file(relative), "Unrecognized data file; configure its recovery separately")
                files[relative] = {**identity(md), "links": md.st_nlink}
            else:
                raise BackupError("Special files are unsupported")
            fingerprints[relative] = (md.st_dev, md.st_ino, md.st_nlink, md.st_size, md.st_mtime_ns, md.st_ctime_ns)
    return {"version": 1, "root": identity(root_stat), "files": files, "directories": directories}, fingerprints


def validate_source_custody(custody, files, data):
    require(isinstance(custody, dict) and set(custody) == {"version", "root", "files", "directories"}
            and type(custody["version"]) is int and custody["version"] == 1,
            "Malformed source custody evidence")
    def check_identity(value, file=False):
        fields = {"device", "inode", "links"} if file else {"device", "inode"}
        require(isinstance(value, dict) and set(value) == fields
                and all(type(v) is int and 0 <= v < 2**64 for v in value.values())
                and value["inode"] > 0 and (not file or value["links"] > 0),
                "Invalid source custody identity")
    check_identity(custody["root"])
    require(isinstance(custody["files"], dict) and isinstance(custody["directories"], dict),
            "Malformed source custody paths")
    require(set(custody["files"]) == set(files) - {DATABASE}, "Source custody file inventory differs")
    groups = {}
    for relative, value in custody["files"].items():
        safe_relative(relative)
        check_identity(value, file=True)
        key = (value["device"], value["inode"])
        group = groups.setdefault(key, [])
        group.append(relative)
    for group in groups.values():
        first = group[0]
        require(all(files[path] == files[first] and custody["files"][path]["links"] == custody["files"][first]["links"]
                    for path in group) and custody["files"][first]["links"] >= len(group),
                "Inconsistent source hardlink group")
    actual_directories = set()
    for current, names, _ in os.walk(data):
        for name in names:
            actual_directories.add((Path(current) / name).relative_to(data).as_posix())
    require(set(custody["directories"]) == actual_directories, "Source custody directory inventory differs")
    require((custody["root"]["device"], custody["root"]["inode"]) not in groups,
            "Conflicting source root identity")
    directory_keys = set()
    for relative, value in custody["directories"].items():
        safe_relative(relative)
        check_identity(value)
        key = (value["device"], value["inode"])
        require(key not in directory_keys and key != (custody["root"]["device"], custody["root"]["inode"])
                and key not in groups, "Conflicting source directory identity")
        directory_keys.add(key)


def expected_history():
    return [{'version': 1, 'checksum_sha384': '8a7734ef572feaede0e243aef0eeb50ef15a723be0637ae5f922b12960b8e00974cc4f54634f0eaf67c0bad052136667'}, {'version': 2, 'checksum_sha384': '7cacfb98379265b39af6a0c3d84053708a79f803d2adc1e7371a5daf2b16fa240ae13e0cde4200dfec4ac73579e2abb8'}, {'version': 3, 'checksum_sha384': '40fcd067d2d113cd3c8fe8d1dcca680f8020cf60fd9e1441ab74d3c118329c60a1f1a2e9fae7d19bf13a05f5eeb51787'}, {'version': 4, 'checksum_sha384': '98ece2e2e06b576f4f924ce43c91db14486845025929824cb5be5bccfdae91a22edda80423dd3f59b6475084142445c1'}, {'version': 5, 'checksum_sha384': 'b2b906d74ad23728541ea4e755a08415f59fd03bfc596ce16464535c356d264977878385ca1f4cc4a84c7389a16665da'}, {'version': 6, 'checksum_sha384': '8b3d311afbef0c6c897752bd735aeb873770dac0d1e1073d29ce284369400e0caca8d14a325cd2fd4336a0a6dd49430b'}, {'version': 7, 'checksum_sha384': '787ac927194842f5fcf9692b06454657553d4e6952cd7cf1535bdc542fc94e248590f1923d991ecef2eb758764c63892'}, {'version': 8, 'checksum_sha384': '62aa11dfcbdb08e0b2bb6522f925a2628755ef799e8b9802df7d696aec9670fc6311f8598e005bfced449c3602001ea6'}, {'version': 9, 'checksum_sha384': 'f2ae7a5e93f0e4318d7ef464f78e0067d7867fff6dd5fd9f8e1b10b3dc02e777da25fe952b62447bc50b406bfaa128de'}, {'version': 10, 'checksum_sha384': '9afadf2a96855dd08d628ca759b9aabc23e9b1f55ca69e74addc53f05bf4e5c2c8640eb68047c1169c92ac5f1797b4c2'}, {'version': 11, 'checksum_sha384': '53fe578c7223bf2cdff447b0f4d428839ec6e3bd59b2bbbf55b61332f6a82ba497e16a1f2b5d0e81069e1fb045001dde'}, {'version': 12, 'checksum_sha384': '65c1c47a88cadb030bf8f3b7afd1a8423dad45b6610ea8aeeff69f9d0955d28c8a3d3ccbfaf410e84c9b3deb4fc88c98'}, {'version': 13, 'checksum_sha384': 'b25c5d02422013065176adb097a843ee763262842b1625d14777e93739658a1b1ed16c0be99d95d1dad90440e8ba9e12'}, {'version': 14, 'checksum_sha384': 'b47a3da3b50487b86ae54291c44ddd9afff172fbe2424b8643a295c2f3156e0105b478ccfe4a91994d468ae61da89e04'}, {'version': 15, 'checksum_sha384': 'f92c34a85540972ac9952ac7d48b12e0d000626bef6ab9a193184399d7b4a2b78fde9d7b12486710ba7bc5d0ec41c099'}, {'version': 16, 'checksum_sha384': '8f23f0ebe2fce7bf357956e27ce6306d56971e54c1b1e7ef2adbbf9819d77a219f85d5f5a85f4903c972bc16fddc4d40'}, {'version': 17, 'checksum_sha384': '8354237cafae9ed59832081396f91c2fc457fa1dabdd397bb880393d187096d66f0c5f76e12bdca5a4cbfda5c746b86c'}, {'version': 18, 'checksum_sha384': 'a8dfb016f11b7614500033d8f20142cbaa594fd320fda11f652bda62a1ff7f49cec842f7462b70990cd9a147cc5de053'}, {'version': 19, 'checksum_sha384': 'a3530d17d055e41f994180a1ecf100e7a67e9be93f3c2bff4ae89bd651f28a9be5a1060aa2e8e49da692e2b402b7680d'}, {'version': 20, 'checksum_sha384': 'f4ff8ad929a40ee7c90f5ad78b44773e6d9ba85a1cf62097f7bba43e376ea004f6d8a215b086101bf5fb9d37c9993450'}, {'version': 21, 'checksum_sha384': 'f5f9ec716f8f8a427903a057a8d342388e8603be2d8bcfce45bff07f713096ce01abc65307e31be3f1149de5779abc96'}, {'version': 22, 'checksum_sha384': '10ec212bc3aa506c583d2015a79b9156a77364556a676ca7dee7db23dc6da38ea3fc9136638efc6267e283b2b1bf5d39'}, {'version': 23, 'checksum_sha384': '425870eb7fb996767210f986dd75fdbaf7cb9664d57247aabd51467d310908cc30f57f77e762e54483455ef37ded164e'}, {'version': 24, 'checksum_sha384': '71bd72cf4612493c5c4e8c738d6cb194562a6e3285f7b7b71812a768466757a7c852b90a8655f0120d820212c33e04c4'}, {'version': 25, 'checksum_sha384': '64dfd2bd02ed05cbc022c0d3789f4fc166d96443d60277dec1cb5225a4873300e51337eae528c364a3454c93aa5c253f'}, {'version': 26, 'checksum_sha384': '93a9f1c668c29ff48e7787aa7f0c6aeb7b0f028b12ce09a9b017ecb00d0b156023f8c8b613668315359339f0a39bd53c'}, {'version': 27, 'checksum_sha384': 'fa073451747237ab114d0faf26f123717202360973ccb816d7d4d82d05c69bbaf9c62a7c650de83eae23e90dd1886329'}, {'version': 28, 'checksum_sha384': '887c6c8cbc69f04c4718ca5f546ca6cb2b4c2088e0c5b8823ecc72627ceef501b94aad70c1b2bc7e9010105e7b6522c6'}, {'version': 29, 'checksum_sha384': '5efebcc53b98dfa8c1b54ae0eb6f9edae1bdefaf71609ac7bcd57cc66c20a87f983f34d8b5fd46643e9b97e1145e9092'}, {'version': 30, 'checksum_sha384': 'b4fa3f8ad6afc17367367bcf9befe5ae4117e1d19156f29132aff1e857e4f3435d357a1b091464ceefd5eaa4d4a5139d'}, {'version': 31, 'checksum_sha384': 'e8be92c808de0ca9f4083294d38ba4d5e87d74ed10e738476d534c838ef562d6e05d5972b36578243f0c28d343ef131a'}, {'version': 32, 'checksum_sha384': 'a09d70fb4d14a6040e67cd04d4a1b1ebedd21e9579abc52663ca5629aab87ba680c2e56b59b783501ea1d2a636d588ec'}, {'version': 33, 'checksum_sha384': '552db9a1660ecb55fc6ae2a3fccac05309e29a5aa75a03431228a048bab2ec55ced76a99304909762de9f04c0029cadd'}, {'version': 34, 'checksum_sha384': 'a05c8268c9badb932e6c2f58a1862efb8e345eb46c2d41f4eca0aaad6c274957bee49feddb759f09204131d4b7edb541'}, {'version': 35, 'checksum_sha384': 'c8209989e14b4b3f3a990b90028ecf0e61d68201ed25edb4e059b87c3b5d1049cf9a3c5a2d803363d316ea318cb4209b'}, {'version': 36, 'checksum_sha384': 'c8d1c90e28a703f54c0513a6a63fdb278cfb3d9ff9a1c408415da5f08a1c8c76fda39ee1d4fc6710ea7c54716a19bfa8'}, {'version': 37, 'checksum_sha384': 'bdb15949416158baf44ff2c1c84fd7996c18181c3760132e737cb11c1a90f6be48b71a19a1492b3cc69eef180bb16ff9'}, {'version': 38, 'checksum_sha384': '19e5a0648575dc2cf0b426ee919865755699dea79c737f3c745e765eb324ef98a5f772a9077a2c5f49d0044c4fec49a6'}, {'version': 39, 'checksum_sha384': '2f15fcf03dcb2d6bb0fd6aae16e0b50447eba528391fbf5f8a810aa500ca764d6cb8b8a3fbfa6bc714ba329a3f4ae816'}, {'version': 40, 'checksum_sha384': '2098836f0ea24883575517e579ba7f551552c3f9372120ccff1b30474a67ffb36cc94da0c897ab287bd03bb93d2357a5'}, {'version': 41, 'checksum_sha384': '076fe2aff68bfd38b00ce6ff64fedd5e0ca5857a54c3efad50f7d5f7fe5cfee1c8d01659b917f7fa9353576738d7015c'}, {'version': 42, 'checksum_sha384': '3083034a925aedf3a5a809b0aa367de64a7e4511dc21073320c473765a7af1d3dec10a65852da9120cfe9ba80916f24d'}, {'version': 43, 'checksum_sha384': '4c8826bd96cce05b8bb61c67451c8e69aa11205daa7351b2cc4743cc7482e2043a1bc69f7671b87c74a487a6a4cda70f'}, {'version': 44, 'checksum_sha384': '15b7402b04d9fb15d853378500c93e2540289bb60ed90086f0909655e49ecc281214a2c6145c73648935210db0887147'}, {'version': 45, 'checksum_sha384': 'dd3ea6de251a0057a58e199690a5dda2787570ebb61dfe1e9b7111684e8a781c2a642fa0b86c026747f79595e0a85636'}, {'version': 46, 'checksum_sha384': '38de33a006d69fe84126ec4c6e18194a8dfb8e2c21c9298727e1aa82a2238d3c6ec65d151bdde6c80406206773954eaa'}, {'version': 47, 'checksum_sha384': 'b1b6300073d9d5bb7721166affcbe99759389ed72ed83f146f28e7ecbc968899bd07eee633cc483e9311247c74ba21ca'}, {'version': 48, 'checksum_sha384': '0971d2b1f2f38f91d5c6d28c4fc7edd635ae373e059e401494cd19479e9cced5e1c593c97da3f1fb43f9c9ac10ef0583'}, {'version': 49, 'checksum_sha384': 'a5990ba450d88cf400bbcbf58c81936289251ebe880ba815130e7c3d7b733b606a8b5d399a11af032c463732d1030470'}, {'version': 50, 'checksum_sha384': 'b70ab0f479245af0b7c8c24c1c3117368e7bd3b8cfbf5edc764423337558301804fe94779f19f7b7ccd18450b28b4dd8'}, {'version': 51, 'checksum_sha384': 'eaa8953c5c3ebc9fd52d0aa5b874e7a636357e257df8bc6263508f959c26628959b09afcf0b1a95aaba389d9a8390873'}, {'version': 52, 'checksum_sha384': '3c9ac1a707a4fd903da6729b0d4a0bc36e36723b16e021ce0266a3cf8eef4ef5b6e2369cba3d3f19bad93942702a373f'}, {'version': 53, 'checksum_sha384': 'a116a487496f74c713e33d791e40aba17830b983cf37e752fd8b54383b3a898d230fa8c41de122e5c8a346907f87cc2a'}, {'version': 54, 'checksum_sha384': '022a01f97a0475b5fb1de79e7a9828785b28c275150a720f51b7f3a9ace6834622a26e257b14e1400f6dd9ca5f75c6a7'}, {'version': 55, 'checksum_sha384': '4f75a90f5e85902a93ffa7d2e064499c07cad0f3e7bbde81e5186c393de2a8f155429985ad9cd77f4b5e24c872a622e1'}]


_expected_history_through_55 = expected_history


def expected_history():
    history = _expected_history_through_55()
    history.extend([
        {'version': 56, 'checksum_sha384': '8895d8adc735161bfe5b733c1fef29e859dc512232d780b4aa9df51af1ef6284f34b7291798812b9e5813d24b4901349'},
        {'version': 57, 'checksum_sha384': '67c90edce7cc82c7c480b1d8d667460a7f06c7ae5e8176084eaf39f181157e6ee61b683671db3d14ae4203b2e989471a'},
    ])
    return history


def pending_document_stage(connection, root, files, document_id, relative):
    """Preserve a committed first-save stage; this never authorizes restoration.

    Only the pre-install-proof phase is admitted here. Later install phases need
    original physical ownership validation and remain a separate recovery gate.
    """
    table = connection.execute("SELECT name FROM sqlite_master WHERE type='table' AND name='storage_journal_receipts'").fetchone()
    require(table is not None, "Missing canonical content has no recovery receipt")
    rows = connection.execute(
        "SELECT op_id,space_id,document_id,draft_id,content_path,initial_revision,sha256,byte_length,state,completed_at "
        "FROM storage_journal_receipts WHERE document_id=? AND content_path=? AND state='pending'",
        (document_id, relative)).fetchall()
    require(len(rows) == 1, "Missing canonical content has no unique pending recovery receipt")
    op, space, document, draft, path, revision, sha, length, state, completed = rows[0]
    require(all(isinstance(value, str) and value for value in (op, space, document, draft, path, revision, sha))
            and type(length) is int and 0 <= length <= 6_000_000
            and state == 'pending' and completed is None
            and re.fullmatch(r'[0-9a-f]{64}', sha) is not None,
            "Invalid pending recovery receipt")
    try:
        valid_op = str(uuid.UUID(op)) == op
    except ValueError:
        valid_op = False
    require(valid_op, "Invalid pending recovery operation identity")
    current = connection.execute(
        "SELECT d.space_id,d.content_path,d.updated_at,s.slug FROM documents d JOIN spaces s ON s.id=d.space_id "
        "WHERE d.id=? AND d.deleted_at IS NULL AND d.archived_at IS NULL", (document,)).fetchone()
    require(current is not None and current[:3] == (space, path, revision)
            and isinstance(current[3], str) and current[3] and '/' not in current[3] and '\\' not in current[3]
            and path.startswith(f'spaces/{current[3]}/docs/'),
            "Pending canonical ownership/revision differs")
    require(connection.execute(
        "SELECT id FROM document_drafts WHERE id=? AND document_id=? AND space_id=?", (draft, document, space)).fetchone()
            == (draft,), "Pending recovery draft differs")
    canonical = private_path(root / path)
    require(not os.path.lexists(canonical), "Pending canonical target is not absent")
    for parent in canonical.parents:
        if parent == root:
            break
        if os.path.lexists(parent):
            require(parent.is_dir(), "Pending canonical ancestor is not a directory")
    prefix = f'.storage-journal/{op}'
    manifest_path, payload_path = f'{prefix}/manifest.json', f'{prefix}/payload'
    require(manifest_path in files and payload_path in files
            and files[manifest_path]['size_bytes'] <= 65536,
            "Pending recovery stage is missing or oversized")
    require(not any(os.path.lexists(private_path(root / name))
                    for name in (f'{prefix}/install-proof.json', f'{prefix}/install-anchor')),
            "Pending recovery install proof requires application ownership inspection")
    expected = dict(op_id=op, space_id=space, document_id=document, draft_id=draft,
                    path=path, revision=revision, sha256=sha, length=length, version=1)
    try:
        with private_path(root / manifest_path).open("rb") as envelope:
            raw = envelope.read(65537)
        require(len(raw) <= 65536, "Pending recovery manifest is oversized")
        manifest = json.loads(raw, object_pairs_hook=unique_object)
    except (ValueError, UnicodeError) as error:
        raise BackupError('Invalid pending recovery manifest') from error
    require(isinstance(manifest, dict) and manifest == expected
            and type(manifest.get('version')) is int and type(manifest.get('length')) is int,
            "Pending recovery manifest differs from receipt")
    require(digest(private_path(root / manifest_path)) == files[manifest_path]
            and digest(private_path(root / payload_path)) == {'size_bytes': length, 'sha256': sha}
            and files[payload_path] == {'size_bytes': length, 'sha256': sha},
            "Pending recovery payload differs from receipt")


def database_metadata(path, files):
    # Only standalone snapshots/staged copies are read here, never the source DB.
    with closing(sqlite3.connect(path.as_uri() + "?mode=ro&immutable=1", uri=True)) as connection:
        require(connection.execute("PRAGMA integrity_check").fetchall() == [("ok",)], "SQLite integrity check failed")
        require(not connection.execute("PRAGMA foreign_key_check").fetchall(), "SQLite foreign-key check failed")
        rows = connection.execute("SELECT version, checksum, success FROM _sqlx_migrations ORDER BY version").fetchall()
        require(rows and all(type(version) is int and isinstance(checksum, bytes)
                            and len(checksum) == 48 and success == 1 for version, checksum, success in rows),
                "Incomplete or invalid SQLx migration records")
        applied = [{"version": version, "checksum_sha384": checksum.hex()} for version, checksum, _ in rows]
        expected = expected_history()
        require(applied == expected[:len(applied)], "Applied migration history is incompatible with this checkout")
        for document_id, relative in connection.execute("SELECT id,content_path FROM documents"):
            safe_relative(relative)
            require(allowed_file(relative) and relative != DATABASE,
                    "Database references missing or unsafe owned content")
            if relative not in files:
                pending_document_stage(connection, path.parent, files, document_id, relative)
        for table in ("attachments", "excalidraw_files"):
            for (relative,) in connection.execute(f"SELECT storage_path FROM {table}"):
                safe_relative(relative)
                require(relative in files and allowed_file(relative) and relative != DATABASE,
                        "Database references missing or unsafe owned content")
        for relative, size in connection.execute("SELECT storage_path, size_bytes FROM attachments"):
            require(type(size) is int and size >= 0 and files[relative]["size_bytes"] == size,
                    "Attachment byte size differs from database metadata")
        return {"engine": "sqlite", "applied_version": applied[-1]["version"],
                "user_version": connection.execute("PRAGMA user_version").fetchone()[0], "migrations": applied}


def journal_namespaces(root):
    return sorted(namespace for namespace in (*JOURNAL_NAMESPACES, *RESTORATION_NAMESPACES, *UPLOAD_NAMESPACES, ADMISSION_NAMESPACE) if (root / namespace).exists())


def journal_directories(root):
    """Capture empty operation reservations as evidence, not just payload files."""
    result = []
    def raise_walk_error(error):
        raise error
    for namespace in journal_namespaces(root):
        base = private_path(root / namespace)
        require(base.is_dir(), "Journal namespace must be a directory")
        for current, directories, _ in os.walk(base, followlinks=False, onerror=raise_walk_error):
            current = Path(current)
            result.append(safe_relative(current.relative_to(root).as_posix()))
            for name in directories:
                mode = (current / name).lstat().st_mode
                require(stat.S_ISDIR(mode) and not stat.S_ISLNK(mode), "Unsafe journal directory")
    return sorted(result)


def recovery_boundary(root):
    """Derive policy from captured bytes and SQL, never a manifest assertion.

    These identities are host-bound, including completed/historical receipts.
    This tool preserves their evidence without rebinding inodes or granting recovery.
    """
    namespaces = journal_namespaces(root)
    receipts = []
    with closing(sqlite3.connect((root / DATABASE).as_uri() + "?mode=ro&immutable=1", uri=True)) as connection:
        tables = {row[0] for row in connection.execute("SELECT name FROM sqlite_master WHERE type='table'")}
        for table in (*RECEIPT_FAMILIES, *RESTORATION_RECEIPT_FAMILIES, *UPLOAD_RECEIPT_FAMILIES, *CAPACITY_RECEIPT_FAMILIES):
            if table in tables:
                count = connection.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
                if count:
                    receipts.append({"table": table, "rows": count})
    return {"scope": "storage-bytes-only", "ordinary_restore": "blocked-identity-bound-evidence" if namespaces or receipts else "allowed",
            "journal_namespaces": namespaces, "journal_directories": journal_directories(root), "receipt_families": receipts}


def stamp():
    return datetime.now(timezone.utc).isoformat()


def application_version(value):
    require(isinstance(value, str) and re.fullmatch(r"v?\d+\.\d+\.\d+(?:[-+][A-Za-z0-9.-]+)?", value),
            "Application version must be an explicit release version")
    return value


def sync_directory(path):
    descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def write_manifest(path, manifest):
    with os.fdopen(os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), "w") as target:
        json.dump(manifest, target, sort_keys=True, indent=2)
        target.write("\n")
        target.flush()
        os.fsync(target.fileno())


def publish(stage, destination):
    """Reserve a new destination exclusively; finalized manifest becomes visible last."""
    for current, _, _ in os.walk(stage, topdown=False):
        sync_directory(Path(current))
    destination.mkdir(mode=0o700)
    owned_identity = destination.stat().st_ino
    try:
        marker = destination / ".starlane-backup-incomplete"
        with os.fdopen(os.open(marker, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), "wb") as pending:
            pending.write(b"Incomplete stopped-write operation; preserve this directory for inspection.\n")
            pending.flush()
            os.fsync(pending.fileno())
        sync_directory(destination)
        sync_directory(destination.parent)
        children = sorted(stage.iterdir(), key=lambda path: path.name == "manifest.json")
        for child in children:
            os.rename(child, destination / child.name)
        for current, _, _ in os.walk(destination, topdown=False):
            sync_directory(Path(current))
        marker.unlink()
        sync_directory(destination)
        sync_directory(destination.parent)
    except BaseException:
        # The Rust owner retains failed private evidence; never erase unknown progress.
        raise


def operation_records_location(directory, source, bundle):
    """Operational history stays outside the recoverable application payload."""
    directory = private_path(directory)
    for protected in (source, bundle):
        require(directory != protected and directory not in protected.parents
                and protected not in directory.parents, "Operation records overlap source or backup")
    require(directory.parent.is_dir(), "Operation record parent directory must already exist")
    if directory.exists():
        md = directory.lstat()
        require(stat.S_ISDIR(md.st_mode) and stat.S_IMODE(md.st_mode) == 0o700
                and md.st_uid == os.geteuid(), "Operation records require an owned private directory")
    return directory


def record_completed_backup(directory, source, bundle, manifest, custody, fingerprint, expected_directory=None):
    """Retain completion evidence; this does not certify an application restore."""
    directory = operation_records_location(directory, source, bundle)
    require(custody == manifest["source_custody"], "Operation source differs from captured source custody")
    require(verify(bundle) == manifest, "Published backup changed before operation recording")
    require(source_custody(source) == (custody, fingerprint),
            "Source custody changed before operation recording")
    database = (source / DATABASE).lstat()
    require(stat.S_ISREG(database.st_mode), "Source database identity is unavailable")
    record = {
        "format": "starlane-storage-operation-record", "format_version": 1,
        "operation": "sqlite_backup", "operation_id": manifest["backup_id"],
        "status": "completed", "scope": "stopped_write_storage_bytes",
        "completed_at": stamp(), "application_version": manifest["application_version"],
        "database_engine": "sqlite",
        "source": {"path": str(source), "root": custody["root"],
                   "database": {"device": database.st_dev, "inode": database.st_ino}},
        "evidence": {"backup_id": manifest["backup_id"],
                     "manifest": digest(bundle / "manifest.json")},
    }
    try:
        directory.mkdir(mode=0o700)
    except FileExistsError:
        pass
    operation_records_location(directory, source, bundle)
    descriptor = os.open(directory, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        held = os.fstat(descriptor)
        require(stat.S_ISDIR(held.st_mode) and stat.S_IMODE(held.st_mode) == 0o700
                and held.st_uid == os.geteuid(), "Operation record directory changed")
        require(expected_directory is None or (held.st_dev, held.st_ino) == expected_directory,
                "Operation record directory changed during backup capture")
        pending = f".pending-{manifest['backup_id']}.json"
        final = f"backup-{manifest['backup_id']}.json"
        fd = os.open(pending, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW,
                     0o600, dir_fd=descriptor)
        with os.fdopen(fd, "wb") as output:
            output.write((json.dumps(record, sort_keys=True, indent=2) + "\n").encode())
            output.flush()
            os.fsync(output.fileno())
        # link is exclusive: existing records are never replaced. Until pending
        # is retired, readers must not treat the two-link file as completed.
        os.link(pending, final, src_dir_fd=descriptor, dst_dir_fd=descriptor,
                follow_symlinks=False)
        os.fsync(descriptor)
        current = directory.lstat()
        require((current.st_dev, current.st_ino) == (held.st_dev, held.st_ino)
                and source_custody(source) == (custody, fingerprint),
                "Operation record or source root changed; preserve operation evidence")
        os.unlink(pending, dir_fd=descriptor)
        os.fsync(descriptor)
        sync_directory(directory.parent)
    finally:
        os.close(descriptor)
    return record


@contextmanager
def admission_capture_leases(source):
    """Kernel exclusion only. This does not certify restored ownership or disposal."""
    import fcntl
    source = private_path(source)
    namespace = source / ADMISSION_NAMESPACE
    held = []
    expected = {}
    try:
        if namespace.exists():
            require(namespace.is_dir(), "Admission namespace is not a directory")
            for path in sorted(namespace.iterdir()):
                require(path.is_dir() and str(uuid.UUID(path.name)) == path.name,
                        "Malformed admission capsule directory")
                fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
                held.append(fd)
                md = os.fstat(fd)
                require(md.st_uid == os.geteuid() and stat.S_IMODE(md.st_mode) == 0o700,
                        "Admission capsule must be private and owned")
                try:
                    fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                except BlockingIOError as error:
                    raise BackupError("Admission writer active; stop writers before backup") from error
                expected[path.name] = (md.st_dev, md.st_ino)
        yield
        actual = {}
        if namespace.exists():
            for path in namespace.iterdir():
                md = path.lstat()
                require(stat.S_ISDIR(md.st_mode), "Admission capsule changed during capture")
                actual[path.name] = (md.st_dev, md.st_ino)
        require(actual == expected, "Admission capsule inventory changed during capture")
    finally:
        for fd in held:
            os.close(fd)


@contextmanager
def preserved_capture_stage(parent):
    # Internal Rust-owned producer only; retains intermediate evidence on every failure.
    stage = tempfile.mkdtemp(prefix=".starlane-backup-", dir=parent)
    yield stage


def create(source, destination, version, writers_stopped=False, operation_records_directory=None):
    require(os.name == "posix", "Backup creation requires Unix private permissions and directory synchronization")
    require(writers_stopped is True, "Create requires --writers-stopped for all application, background and sync writers")
    with admission_capture_leases(source):
        return _create_with_admission_leases(source, destination, version, writers_stopped,
                                             operation_records_directory)


def _create_with_admission_leases(source, destination, version, writers_stopped=False, operation_records_directory=None):
    require(os.name == "posix", "Backup creation requires Unix private permissions and directory synchronization")
    require(writers_stopped is True, "Create requires --writers-stopped for all application, background and sync writers")
    version = application_version(version)
    source, destination = separate(source, destination)
    records = None if operation_records_directory is None else operation_records_location(
        operation_records_directory, source, destination)
    records_identity = None
    if records is not None and records.exists():
        md = records.lstat()
        records_identity = (md.st_dev, md.st_ino)
    started = stamp()
    custody, custody_fingerprint = source_custody(source)
    before = inventory(source, source=True)
    source_namespaces = journal_namespaces(source)
    source_directories = journal_directories(source)
    journal_size = before.get(DATABASE + "-journal", {}).get("size_bytes", 0)
    if journal_size:
        # SQLite PERSIST keeps a zeroed inactive header. Preserve its exact
        # original bytes; hot/ambiguous journals still require application-owned
        # recovery. The private projection, never the original, is opened below.
        with (source / (DATABASE + "-journal")).open("rb") as journal:
            header = journal.read(28)
        require(len(header) == min(28, journal_size) and not any(header),
                "Hot or ambiguous rollback journal: preserve the source and recover it with the matching application before backup")
    with preserved_capture_stage(destination.parent) as temporary:
        stage = Path(temporary)
        data, capture = stage / "data", stage / "capture"
        data.mkdir(mode=0o700)
        capture.mkdir(mode=0o700)
        for relative in custody["directories"]:
            make_private_parents(data / relative)
        for relative, expected in before.items():
            if relative in (DATABASE, DATABASE + "-wal", DATABASE + "-shm", DATABASE + "-journal"):
                # Copy main DB and WAL/journal together; shared-memory state is rebuildable.
                if relative != DATABASE + "-shm":
                    require(digest(source / relative, capture / relative) == expected, "Source changed during capture")
            else:
                require(digest(source / relative, data / relative) == expected, "Source changed during capture")
        snapshot = data / DATABASE
        descriptor = os.open(snapshot, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        os.close(descriptor)
        with closing(sqlite3.connect((capture / DATABASE).as_uri() + "?mode=ro", uri=True)) as input_db:
            with closing(sqlite3.connect(snapshot)) as output_db:
                input_db.backup(output_db)
        descriptor = os.open(snapshot, os.O_RDONLY)
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)
        shutil.rmtree(capture)
        files = inventory(data)
        schema = database_metadata(snapshot, files)
        require(inventory(source, source=True) == before and journal_namespaces(source) == source_namespaces
                and journal_directories(source) == source_directories
                and source_custody(source) == (custody, custody_fingerprint),
                "Source changed; writers were not stopped")
        manifest = {"format": FORMAT, "format_version": 4 if schema["applied_version"] in (51, 52, 53, 54, 55, 56, 57) else FORMAT_VERSION, "backup_id": str(uuid.uuid4()),
                    "application_version": version, "capture_started_at": started, "capture_finished_at": stamp(),
                    "writers_stopped_asserted": True, "database": schema, "files": files,
                    "excluded": EXCLUDED, "configuration_prerequisite": CONFIG_PREREQUISITE,
                    "recovery_boundary": recovery_boundary(data), "source_custody": custody}
        write_manifest(stage / "manifest.json", manifest)
        verify(stage)
        publish(stage, destination)
    if records is not None:
        record_completed_backup(records, source, destination, manifest, custody, custody_fingerprint, records_identity)
    return manifest


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "Duplicate manifest key")
        result[key] = value
    return result


def verify(bundle, version=None):
    bundle = private_path(bundle)
    require(bundle.is_dir(), "Backup set directory is missing")
    require({path.name for path in bundle.iterdir()} == {"manifest.json", "data"}, "Incomplete or unexpected backup set entries")
    manifest_path = private_path(bundle / "manifest.json")
    require(manifest_path.is_file() and manifest_path.stat().st_size <= 16 * 1024 * 1024, "Invalid manifest file")
    try:
        manifest = json.loads(manifest_path.read_text(), object_pairs_hook=unique_object)
    except (json.JSONDecodeError, UnicodeError) as error:
        raise BackupError("Manifest is not valid UTF-8 JSON") from error
    require(isinstance(manifest, dict), "Malformed manifest fields")
    require(manifest.get("format") == FORMAT and type(manifest.get("format_version")) is int
            and manifest["format_version"] in (1, 2, 3, 4), "Unsupported backup format")
    fields = {"format", "format_version", "backup_id", "application_version", "capture_started_at", "capture_finished_at",
              "writers_stopped_asserted", "database", "files", "excluded", "configuration_prerequisite"}
    if manifest["format_version"] >= 2:
        fields.add("recovery_boundary")
    if manifest["format_version"] >= 3:
        fields.add("source_custody")
    require(set(manifest) == fields, "Malformed manifest fields")
    require(isinstance(manifest["backup_id"], str) and re.fullmatch(r"[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}", manifest["backup_id"]), "Invalid backup identifier")
    application_version(manifest["application_version"])
    if version is not None:
        require(application_version(version) == manifest["application_version"], "Application version mismatch")
    require(all(isinstance(manifest[key], str) for key in ("capture_started_at", "capture_finished_at")), "Invalid capture timestamps")
    try:
        dates = [datetime.fromisoformat(manifest[key]) for key in ("capture_started_at", "capture_finished_at")]
    except ValueError as error:
        raise BackupError("Invalid capture timestamps") from error
    require(all(date.tzinfo is not None and date.utcoffset().total_seconds() == 0 for date in dates)
            and dates[0] <= dates[1], "Invalid capture timestamps")
    require(manifest["writers_stopped_asserted"] is True and manifest["excluded"] == EXCLUDED
            and manifest["configuration_prerequisite"] == CONFIG_PREREQUISITE, "Unsupported backup contract")
    files = manifest["files"]
    require(isinstance(files, dict) and files, "Invalid inventory")
    for relative, entry in files.items():
        safe_relative(relative)
        require(allowed_file(relative) and isinstance(entry, dict) and set(entry) == {"size_bytes", "sha256"}, "Malformed inventory entry")
        require(type(entry["size_bytes"]) is int and entry["size_bytes"] >= 0
                and isinstance(entry["sha256"], str) and re.fullmatch(r"[0-9a-f]{64}", entry["sha256"]), "Invalid inventory checksum or size")
    data = private_path(bundle / "data")
    require(inventory(data) == files, "Missing, extra, or changed backup files")
    schema = manifest["database"]
    require(isinstance(schema, dict) and set(schema) == {"engine", "applied_version", "user_version", "migrations"}
            and schema["engine"] == "sqlite" and type(schema["applied_version"]) is int
            and type(schema["user_version"]) is int and isinstance(schema["migrations"], list), "Malformed database metadata")
    require((manifest["format_version"] == 4) == (schema["applied_version"] in (51, 52, 53, 54, 55, 56, 57)),
            "Capacity profile requires distinct format4 inventory")
    if manifest["format_version"] < 4:
        require(not (bundle / "data" / ADMISSION_NAMESPACE).exists(),
                "Admission namespace requires distinct format4 inventory")
        require(not any(path == ADMISSION_NAMESPACE or path.startswith(ADMISSION_NAMESPACE + "/")
                        for path in manifest.get("source_custody", {}).get("directories", {}))
                and not any(path.startswith(ADMISSION_NAMESPACE + "/") for path in manifest["files"]),
                "Admission namespace requires distinct format4 inventory")
    for migration in schema["migrations"]:
        require(isinstance(migration, dict) and set(migration) == {"version", "checksum_sha384"}
                and type(migration["version"]) is int and isinstance(migration["checksum_sha384"], str)
                and re.fullmatch(r"[0-9a-f]{96}", migration["checksum_sha384"]), "Malformed migration metadata")
    require(database_metadata(data / DATABASE, files) == schema, "Database metadata mismatch")
    boundary = recovery_boundary(data)
    if manifest["format_version"] >= 2:
        # Equality alone would admit bool-as-int row counts: compare canonical JSON types too.
        require(json.dumps(boundary, sort_keys=True) == json.dumps(manifest["recovery_boundary"], sort_keys=True),
                "Recovery boundary differs from captured journal or SQL evidence")
    if manifest["format_version"] >= 3:
        validate_source_custody(manifest["source_custody"], files, data)
    return manifest


def restore(bundle, destination, version):
    require(os.name == "posix", "Restore requires Unix private permissions and directory synchronization")
    bundle, destination = separate(bundle, destination)
    manifest = verify(bundle, application_version(version))
    require(recovery_boundary(bundle / "data")["ordinary_restore"] == "allowed",
            "Ordinary restore blocked: identity-bound journal or SQL receipt evidence requires application-owned recovery; source preserved")
    with tempfile.TemporaryDirectory(prefix=".starlane-restore-", dir=destination.parent) as temporary:
        stage = Path(temporary)
        (stage / "data").mkdir(mode=0o700)
        if manifest["format_version"] >= 3:
            for relative in manifest["source_custody"]["directories"]:
                make_private_parents(stage / "data" / relative)
        for relative, expected in manifest["files"].items():
            require(digest(bundle / "data" / relative, stage / "data" / relative) == expected, "Backup changed during restore")
        write_manifest(stage / "manifest.json", manifest)
        verify(stage, version)
        require(verify(bundle, version) == manifest, "Backup changed during restore")
        require(recovery_boundary(bundle / "data")["ordinary_restore"] == "allowed"
                and recovery_boundary(stage / "data")["ordinary_restore"] == "allowed",
                "Ordinary restore blocked: identity-bound evidence changed during copy; source preserved")
        publish(stage / "data", destination)
    return manifest


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    create_parser = commands.add_parser("create")
    create_parser.add_argument("source", type=Path)
    create_parser.add_argument("destination", type=Path)
    create_parser.add_argument("--application-version", required=True)
    create_parser.add_argument("--writers-stopped", action="store_true", required=True)
    create_parser.add_argument("--operation-records-directory", type=Path,
                               help="Optional private external directory for completed backup records; not application restore certification")
    verify_parser = commands.add_parser("verify")
    verify_parser.add_argument("bundle", type=Path)
    restore_parser = commands.add_parser("restore")
    restore_parser.add_argument("bundle", type=Path)
    restore_parser.add_argument("destination", type=Path)
    restore_parser.add_argument("--application-version", required=True)
    arguments = parser.parse_args(argv)
    try:
        if arguments.command == "create":
            manifest = create(arguments.source, arguments.destination, arguments.application_version, arguments.writers_stopped,
                              arguments.operation_records_directory)
        elif arguments.command == "verify":
            manifest = verify(arguments.bundle)
        else:
            manifest = restore(arguments.bundle, arguments.destination, arguments.application_version)
        print(f"{arguments.command}: verified SQLite set {manifest['backup_id']}")
        print(CONFIG_PREREQUISITE)
        if manifest["format_version"] >= 2:
            boundary = manifest["recovery_boundary"]
        else:
            boundary = recovery_boundary(arguments.bundle / "data")
        print("Recovery boundary: storage bytes only; ordinary restore " + boundary["ordinary_restore"])
        return 0
    except BackupError as error:
        print(f"{arguments.command}: failed: {error}", file=sys.stderr)
        return 1
    except sqlite3.Error:
        print(f"{arguments.command}: failed: SQLite snapshot integrity/schema could not be validated", file=sys.stderr)
        return 1
    except OSError as error:
        reason = os.strerror(error.errno) if error.errno is not None else "file operation did not complete"
        print(f"{arguments.command}: failed: {reason}", file=sys.stderr)
        return 1
    except (ValueError, TypeError, KeyError):
        print(f"{arguments.command}: failed: malformed backup metadata", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
