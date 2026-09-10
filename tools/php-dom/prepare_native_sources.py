#!/usr/bin/env python3
"""Create deterministic offline native-source archives from the pinned releases."""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import shutil
import tarfile
import tempfile
from pathlib import Path, PurePosixPath


PHP_ARCHIVE_SHA256 = "58910198d19e873048fe87cdfe16bc790025417ede3d1651bfa1c4b533d573f2"
LIBXML_ARCHIVE_SHA256 = "78262a6e7ac170d6528ebfe2efccdf220191a5af6a6cd61ea4a9a9a5042c7a07"
PHP_PREFIXES = (
    PurePosixPath("php-8.5.8/ext/dom"),
    PurePosixPath("php-8.5.8/ext/lexbor"),
    PurePosixPath("php-8.5.8/ext/libxml"),
    PurePosixPath("php-8.5.8/ext/simplexml"),
)


def sha256_file(path: Path) -> str:
    """Returns the SHA-256 digest of one file."""
    return hashlib.sha256(path.read_bytes()).hexdigest()


def selected_php_member(name: str) -> bool:
    """Returns whether one official PHP tar member belongs in the native adapter archive."""
    path = PurePosixPath(name)
    if path == PurePosixPath("php-8.5.8/LICENSE"):
        return True
    if "tests" in path.parts:
        return False
    return any(path == prefix or prefix in path.parents for prefix in PHP_PREFIXES)


def normalized_info(source: tarfile.TarInfo) -> tarfile.TarInfo:
    """Copies one tar header with deterministic ownership, timestamp, and mode."""
    info = tarfile.TarInfo(source.name)
    info.size = source.size
    info.type = source.type
    info.linkname = source.linkname
    info.mode = 0o755 if source.mode & 0o111 else 0o644
    info.mtime = 0
    info.uid = 0
    info.gid = 0
    info.uname = ""
    info.gname = ""
    return info


def build_php_native_archive(source: Path, destination: Path) -> list[dict[str, object]]:
    """Writes the deterministic PHP adapter/Lexbor subset and returns its file manifest."""
    files: list[dict[str, object]] = []
    with tarfile.open(source, "r:xz") as archive:
        selected = [
            member
            for member in archive.getmembers()
            if selected_php_member(member.name)
        ]
        selected.sort(key=lambda member: member.name)
        with tarfile.open(destination, "w:xz", preset=9) as output:
            for member in selected:
                info = normalized_info(member)
                if member.isfile():
                    extracted = archive.extractfile(member)
                    if extracted is None:
                        raise SystemExit(f"cannot extract {member.name}")
                    payload = extracted.read()
                    output.addfile(info, io.BytesIO(payload))
                    files.append(
                        {
                            "path": member.name,
                            "size": len(payload),
                            "sha256": hashlib.sha256(payload).hexdigest(),
                        }
                    )
                else:
                    output.addfile(info)
    return files


def write_manifest(
    destination: Path,
    php_native_archive: Path,
    php_files: list[dict[str, object]],
    libxml_archive: Path,
) -> None:
    """Writes provenance and content digests for both vendored native archives."""
    manifest = {
        "schema": 1,
        "php": {
            "version": "8.5.8",
            "official_archive_sha256": PHP_ARCHIVE_SHA256,
            "native_archive": php_native_archive.name,
            "native_archive_sha256": sha256_file(php_native_archive),
            "file_count": len(php_files),
            "files": php_files,
            "lexbor_version": "2.7.0",
        },
        "libxml": {
            "version": "2.15.3",
            "archive": libxml_archive.name,
            "archive_sha256": sha256_file(libxml_archive),
            "official_archive_sha256": LIBXML_ARCHIVE_SHA256,
        },
    }
    destination.write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    )


def prepare(
    php_archive: Path,
    libxml_archive: Path,
    destination: Path,
) -> None:
    """Verifies official inputs and writes both vendored archives plus their manifest."""
    if sha256_file(php_archive) != PHP_ARCHIVE_SHA256:
        raise SystemExit("official PHP archive SHA-256 mismatch")
    if sha256_file(libxml_archive) != LIBXML_ARCHIVE_SHA256:
        raise SystemExit("official libxml2 archive SHA-256 mismatch")

    destination.mkdir(parents=True, exist_ok=True)
    vendored_libxml = destination / "libxml2-2.15.3.tar.xz"
    shutil.copyfile(libxml_archive, vendored_libxml)
    php_native = destination / "php-8.5.8-dom-native.tar.xz"
    php_files = build_php_native_archive(php_archive, php_native)
    write_manifest(destination / "sources.json", php_native, php_files, vendored_libxml)


def check(
    php_archive: Path,
    libxml_archive: Path,
    destination: Path,
) -> None:
    """Rebuilds native archives in a temporary directory and compares exact bytes."""
    with tempfile.TemporaryDirectory(prefix="elephc-dom-native-check-") as temporary:
        candidate = Path(temporary)
        prepare(php_archive, libxml_archive, candidate)
        for name in (
            "libxml2-2.15.3.tar.xz",
            "php-8.5.8-dom-native.tar.xz",
            "sources.json",
        ):
            expected = destination / name
            actual = candidate / name
            if not expected.exists() or expected.read_bytes() != actual.read_bytes():
                raise SystemExit(f"vendored native source is stale: {expected}")


def main() -> None:
    """Parses CLI arguments and creates or checks deterministic vendored archives."""
    parser = argparse.ArgumentParser()
    parser.add_argument("--php-archive", type=Path, required=True)
    parser.add_argument("--libxml-archive", type=Path, required=True)
    parser.add_argument(
        "--destination",
        type=Path,
        default=Path("crates/elephc-dom/vendor"),
    )
    parser.add_argument("--check", action="store_true")
    arguments = parser.parse_args()
    if arguments.check:
        check(
            arguments.php_archive,
            arguments.libxml_archive,
            arguments.destination,
        )
    else:
        prepare(
            arguments.php_archive,
            arguments.libxml_archive,
            arguments.destination,
        )


if __name__ == "__main__":
    main()
