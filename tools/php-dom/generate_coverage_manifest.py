#!/usr/bin/env python3
"""Generate a deterministic PHP DOM coverage-manifest skeleton from source files.

The generator deliberately records source facts only: the reviewed coverage
owners, target reports, and closure statuses are added later by the campaign.
Keeping the inventory mechanical makes it possible for the strict checker to
detect both omissions and a changed upstream PHPT byte stream.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import tempfile
from pathlib import Path
from typing import Any


def sha256_file(path: Path) -> str:
    """Return the SHA-256 digest of one source artifact."""
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_json(path: Path) -> dict[str, Any]:
    """Load a JSON object, rejecting non-object input with a useful error."""
    document = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(document, dict):
        raise ValueError("input must be a JSON object")
    return document


class UnsafePathError(ValueError):
    """Describe a path which is not confined to the caller's repository root."""


def repo_path(repo_root: Path, value: Path | str, subject: str) -> Path:
    """Resolve a relative path and reject absolute, traversal, and escaping links."""
    raw = Path(value)
    if not raw.is_absolute() and ".." in raw.parts:
        raise UnsafePathError(f"UNSAFE_PATH:{subject}")
    resolved = raw.resolve() if raw.is_absolute() else (repo_root / raw).resolve()
    try:
        resolved.relative_to(repo_root)
    except ValueError as error:
        raise UnsafePathError(f"UNSAFE_PATH:{subject}") from error
    return resolved


def component_paths(source_root: Path, components: dict[str, Any]) -> list[tuple[str, Path]]:
    """Return sorted component PHPT paths beneath the declared php-src root."""
    paths: list[tuple[str, Path]] = []
    for component in sorted(components):
        test_root = source_root / "ext" / component / "tests"
        if not test_root.is_dir():
            raise ValueError(f"missing PHPT root: {test_root}")
        paths.extend((component, path) for path in sorted(test_root.rglob("*.phpt")))
    return paths


def generate(repo_root: Path, source: dict[str, Any]) -> dict[str, Any]:
    """Build schema-1 inventory rows from the reviewed source declaration."""
    if source.get("schema") != 1:
        raise ValueError("input schema must be 1")
    relative_source_root = source.get("php_src_root")
    components = source.get("components")
    if not isinstance(relative_source_root, str) or not isinstance(components, dict):
        raise ValueError("input requires php_src_root and components")

    source_root = repo_path(repo_root, relative_source_root, "php_src_root")
    phpt_paths = component_paths(source_root, components)
    actual_components = {
        component: sum(1 for found_component, _ in phpt_paths if found_component == component)
        for component in sorted(components)
    }
    for component, expected_count in components.items():
        if not isinstance(expected_count, int) or expected_count < 0:
            raise ValueError(f"components.{component} must be a non-negative integer")
        if actual_components[component] != expected_count:
            raise ValueError(
                f"{component}: expected {expected_count} PHPTs, found {actual_components[component]}"
            )

    requirements = source.get("requirements", [])
    routes = source.get("routes", [])
    if not isinstance(requirements, list) or not isinstance(routes, list):
        raise ValueError("requirements and routes must be arrays")
    phpts = [
        {
            "path": path.relative_to(repo_root).as_posix(),
            "sha256": sha256_file(path),
            "status": "pending",
            "component": component,
        }
        for component, path in phpt_paths
    ]
    return {
        "schema": 1,
        "source": {"php_src_root": relative_source_root},
        "inventory": {
            "requirements": len(requirements),
            "routes": len(routes),
            "phpts": len(phpts),
            "components": actual_components,
        },
        "requirements": requirements,
        "routes": routes,
        "phpts": phpts,
    }


def write_json_atomically(path: Path, document: dict[str, Any]) -> None:
    """Durably replace the output with JSON without exposing a truncated manifest."""
    encoded = (json.dumps(document, indent=2, sort_keys=True) + "\n").encode("utf-8")
    descriptor, temporary_name = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    temporary = Path(temporary_name)
    try:
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(encoded)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
    finally:
        if temporary.exists():
            temporary.unlink()


def parse_args() -> argparse.Namespace:
    """Parse the repository-scoped generator command-line interface."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", required=True, type=Path)
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    return parser.parse_args()


def main() -> int:
    """Generate the manifest and return a nonzero status for invalid inputs."""
    arguments = parse_args()
    try:
        repo_root = arguments.repo_root.resolve()
        input_path = repo_path(repo_root, arguments.input, "input")
        output_path = repo_path(repo_root, arguments.output, "output")
        source = load_json(input_path)
        manifest = generate(repo_root, source)
        output_path.parent.mkdir(parents=True, exist_ok=True)
        write_json_atomically(output_path, manifest)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"INPUT_ERROR:{error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
