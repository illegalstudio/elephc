"""Validate generated builtins docs for compatibility with the Astro Starlight site.

Scans ``docs/php/builtins/`` and ``docs/internals/builtins/`` and checks:

- Frontmatter has ``title`` and ``description`` (non-empty strings).
- ``sidebar`` only contains an optional integer ``order``.
- Relative ``.md`` links point to files that exist in the working tree.

Exits with a non-zero status if any check fails.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path
from typing import Any


FRONTMATTER_RE = re.compile(r"^---\n(.*?)\n---\n", re.DOTALL)
LINK_RE = re.compile(r"\[([^\]]+)\]\(([^)]+)\)")


def _coerce(value: str) -> Any:
    """Convert a simple frontmatter scalar to its Python equivalent."""
    value = value.strip()
    if not value:
        return None
    if (value.startswith('"') and value.endswith('"')) or (
        value.startswith("'") and value.endswith("'")
    ):
        return value[1:-1]
    if re.fullmatch(r"-?\d+", value):
        return int(value)
    if value.lower() == "true":
        return True
    if value.lower() == "false":
        return False
    return value


def _parse_frontmatter(text: str) -> dict[str, Any]:
    """Parse the simple YAML frontmatter produced by the builtins renderer.

    Raises ``ValueError`` for malformed frontmatter lines.
    """
    m = FRONTMATTER_RE.match(text)
    if not m:
        raise ValueError("frontmatter block not found")
    result: dict[str, Any] = {}
    current_key: str | None = None
    for line in m.group(1).splitlines():
        if not line.strip():
            continue
        if not line.startswith(" "):
            match = re.match(r"^(\w+):\s*(.*)$", line)
            if not match:
                raise ValueError(f"invalid top-level frontmatter line: {line!r}")
            key, value = match.group(1), match.group(2).strip()
            if value:
                result[key] = _coerce(value)
                current_key = None
            else:
                result[key] = {}
                current_key = key
        else:
            match = re.match(r"^\s+(\w+):\s*(.*)$", line)
            if not match:
                raise ValueError(f"invalid nested frontmatter line: {line!r}")
            key, value = match.group(1), match.group(2).strip()
            if current_key is None:
                raise ValueError(f"nested key without parent: {line!r}")
            result[current_key][key] = _coerce(value)
    return result


def validate_frontmatter(path: Path, text: str) -> list[str]:
    """Return a list of human-readable errors for invalid frontmatter."""
    errors: list[str] = []
    try:
        fm = _parse_frontmatter(text)
    except ValueError as exc:
        return [f"{path}: {exc}"]
    if not isinstance(fm, dict):
        return [f"{path}: frontmatter is not a mapping"]
    for key in ("title", "description"):
        if key not in fm:
            errors.append(f"{path}: frontmatter missing '{key}'")
        elif not isinstance(fm[key], str) or not fm[key].strip():
            errors.append(f"{path}: frontmatter '{key}' is empty or not a string")
    sidebar = fm.get("sidebar")
    if sidebar is not None:
        if not isinstance(sidebar, dict):
            errors.append(f"{path}: frontmatter 'sidebar' is not a mapping")
        else:
            for key in sidebar:
                if key != "order":
                    errors.append(
                        f"{path}: frontmatter 'sidebar' contains unsupported key '{key}'"
                    )
            if "order" in sidebar and not isinstance(sidebar["order"], int):
                errors.append(f"{path}: frontmatter 'sidebar.order' is not an integer")
    return errors


def validate_links(path: Path, text: str) -> list[str]:
    """Return a list of broken relative ``.md`` links found in the page body."""
    errors: list[str] = []
    base_dir = path.parent
    for _, url in LINK_RE.findall(text):
        if not url or url.startswith(("http://", "https://", "mailto:", "/")):
            continue
        # Drop any anchor/fragment; only the file path is validated here.
        file_url = url.split("#", 1)[0]
        if not file_url.endswith(".md"):
            continue
        target = base_dir / file_url
        if not target.exists():
            errors.append(f"{path}: broken link to '{url}' (resolved to {target})")
    return errors


def validate_file(path: Path) -> list[str]:
    """Validate a single Markdown file."""
    errors: list[str] = []
    if not path.exists():
        errors.append(f"{path}: file does not exist")
        return errors
    text = path.read_text(encoding="utf-8")
    errors.extend(validate_frontmatter(path, text))
    errors.extend(validate_links(path, text))
    return errors


def validate_tree(root: Path) -> list[str]:
    """Validate every Markdown file under ``root``."""
    errors: list[str] = []
    if not root.exists():
        errors.append(f"{root}: directory does not exist")
        return errors
    for md in sorted(root.rglob("*.md")):
        text = md.read_text(encoding="utf-8")
        errors.extend(validate_frontmatter(md, text))
        errors.extend(validate_links(md, text))
    return errors


def main() -> int:
    """Entry point for the validation script."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=Path(__file__).resolve().parents[3],
    )
    args = parser.parse_args()
    repo = args.repo_root.resolve()
    roots = [
        repo / "docs" / "php" / "builtins",
        repo / "docs" / "internals" / "builtins",
    ]
    extra_files = [
        repo / "docs" / "php" / "builtins.md",
    ]
    all_errors: list[str] = []
    page_count = 0
    for root in roots:
        all_errors.extend(validate_tree(root))
        if root.exists():
            page_count += sum(1 for _ in root.rglob("*.md"))
    for extra in extra_files:
        all_errors.extend(validate_file(extra))
        if extra.exists():
            page_count += 1
    if all_errors:
        print("Validation failed:", file=sys.stderr)
        for err in all_errors:
            print(f"  - {err}", file=sys.stderr)
        return 1
    print(f"OK: validated {page_count} generated builtins pages.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
