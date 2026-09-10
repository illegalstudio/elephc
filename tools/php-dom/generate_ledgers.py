#!/usr/bin/env python3
"""Generate deterministic PHP 8.5.8 DOM/libxml/SimpleXML PHPT ledgers."""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
from pathlib import Path


COMPONENTS = ("dom", "libxml", "simplexml")


def sha256_file(path: Path) -> str:
    """Returns the SHA-256 digest of one file."""
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run_git(source_root: Path, *arguments: str) -> str:
    """Runs a read-only Git query in the php-src checkout."""
    result = subprocess.run(
        ["git", *arguments],
        cwd=source_root,
        check=True,
        text=True,
        capture_output=True,
    )
    return result.stdout.strip()


def sorted_phpt_digest(paths: list[Path], source_root: Path) -> str:
    """Matches the locked digest of sorted `shasum`-style PHPT records."""
    lines = [
        f"{sha256_file(path)}  {path.relative_to(source_root).as_posix()}\n"
        for path in paths
    ]
    return hashlib.sha256("".join(lines).encode()).hexdigest()


def load_lock(repo_root: Path) -> dict:
    """Loads the checked-in PHP/native source lock."""
    lock_path = repo_root / "tools/php-dom/source-lock.json"
    return json.loads(lock_path.read_text())


def generate_component(
    source_root: Path,
    output_root: Path,
    component: str,
    component_lock: dict,
) -> None:
    """Generates one component's deterministic PHPT ledger."""
    test_root = source_root / component_lock["root"]
    phpt_paths = sorted(test_root.rglob("*.phpt"))
    all_paths = sorted(path for path in test_root.rglob("*") if path.is_file())

    if len(phpt_paths) != component_lock["phpt_count"]:
        raise SystemExit(
            f"{component}: expected {component_lock['phpt_count']} PHPTs, "
            f"found {len(phpt_paths)}"
        )
    if len(all_paths) != component_lock["all_file_count"]:
        raise SystemExit(
            f"{component}: expected {component_lock['all_file_count']} total files, "
            f"found {len(all_paths)}"
        )

    digest = sorted_phpt_digest(phpt_paths, source_root)
    if digest != component_lock["sorted_phpt_digest"]:
        raise SystemExit(
            f"{component}: sorted PHPT digest mismatch: {digest}"
        )

    entries = []
    for path in phpt_paths:
        entries.append(
            {
                "path": path.relative_to(source_root).as_posix(),
                "sha256": sha256_file(path),
                "status": "pending",
                "fixture": None,
                "reason": None,
                "observations": [],
            }
        )

    document = {
        "schema": 1,
        "component": component,
        "source_root": component_lock["root"],
        "phpt_count": len(phpt_paths),
        "all_file_count": len(all_paths),
        "sorted_phpt_digest": digest,
        "closed": False,
        "entries": entries,
    }
    output_path = output_root / f"{component}-php-8.5.8.json"
    output_path.write_text(
        json.dumps(document, indent=2, ensure_ascii=False) + "\n"
    )


def main() -> int:
    """Validates php-src provenance and writes all three ledgers."""
    if len(sys.argv) != 3:
        print(
            "usage: generate_ledgers.py PHP_SRC_ROOT OUTPUT_DIR",
            file=sys.stderr,
        )
        return 2

    source_root = Path(sys.argv[1]).resolve()
    output_root = Path(sys.argv[2]).resolve()
    repo_root = Path(__file__).resolve().parents[2]
    lock = load_lock(repo_root)

    commit = run_git(source_root, "rev-parse", "HEAD")
    if commit != lock["php"]["commit"]:
        raise SystemExit(
            f"expected php-src {lock['php']['commit']}, got {commit}"
        )

    for relative_path, expected_tree in lock["php"]["trees"].items():
        actual_tree = run_git(source_root, "rev-parse", f"HEAD^{{tree}}:{relative_path}")
        if actual_tree != expected_tree:
            raise SystemExit(
                f"{relative_path}: expected tree {expected_tree}, got {actual_tree}"
            )

    for relative_path, metadata in lock["php"]["stubs"].items():
        path = source_root / relative_path
        actual_digest = sha256_file(path)
        if actual_digest != metadata["sha256"]:
            raise SystemExit(
                f"{relative_path}: expected SHA-256 {metadata['sha256']}, "
                f"got {actual_digest}"
            )

    output_root.mkdir(parents=True, exist_ok=True)
    for component in COMPONENTS:
        generate_component(
            source_root,
            output_root,
            component,
            lock["ledgers"][component],
        )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
