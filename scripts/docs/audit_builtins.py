#!/usr/bin/env python3
"""Audit the generated builtin documentation.

Checks:

1. Every builtin in the registry has a user-facing page.
2. Every builtin that has a lowering has an internals page.
3. Every cross-link in a generated page resolves to an actual file.
4. Per-area indexes only contain builtins that belong to that area.
5. No stray top-level files (everything should be inside an area folder).

Exits 0 on success, 1 on any failure.
"""

from __future__ import annotations

import json
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path
from typing import Iterable

REPO = Path(__file__).resolve().parents[2]
REGISTRY = REPO / "scripts" / "docs" / "builtin_registry.json"
USER_DIR = REPO / "docs" / "php" / "builtins"
MASTER_INDEX = REPO / "docs" / "php" / "builtins.md"
INTERNALS_DIR = REPO / "docs" / "internals" / "builtins"

# Link target patterns we recognise:
#   [text](path.md)         — Markdown link to another local .md file
#   [text](path/)           — dir is OK, ignore
#   [text](https://...)     — external, skip
LINK_RE = re.compile(r"\[([^\]]+)\]\(([^)]+)\)")


def slug(name: str) -> str:
    return name.replace("\\", "-").replace("::", "-")


def area_dir(base: Path, name: str, area: str) -> Path:
    if name.startswith("__elephc_"):
        return base / "_internal" / f"{slug(name)}.md"
    return base / area.lower() / f"{slug(name)}.md"


def _check_links(path: Path, errors: list[str]) -> None:
    """Verify that every local Markdown link in ``path`` resolves to a file."""
    text = path.read_text(encoding="utf-8")
    for label, target in LINK_RE.findall(text):
        if target.startswith(("http://", "https://", "#", "mailto:")):
            continue
        if target.startswith("/"):
            # Absolute path from site root — verify under the repo's docs/.
            abs_target = (REPO / "docs" / target.lstrip("/")).resolve()
        else:
            abs_target = (path.parent / target).resolve()
        if not abs_target.exists() and not str(abs_target).endswith("/"):
            errors.append(
                f"broken link in {path.relative_to(REPO)}: "
                f"[{label}]({target}) → {abs_target}"
            )


def main() -> int:
    raw = json.loads(REGISTRY.read_text(encoding="utf-8"))
    builtins = [b for b in raw if b["in_catalog"]]
    user_builtins = [b for b in builtins if not b["is_internal"]]

    errors: list[str] = []
    stats: dict[str, int] = defaultdict(int)

    # 1. Every non-internal builtin has a user page
    for b in user_builtins:
        path = area_dir(USER_DIR, b["name"], b["area"])
        if not path.exists():
            errors.append(f"missing user page for {b['name']}: expected {path}")
        else:
            stats["user_pages"] += 1

    # 1b. The master index page exists at docs/php/builtins.md
    if not MASTER_INDEX.exists():
        errors.append(f"missing master index: {MASTER_INDEX}")
    else:
        stats["master_index"] += 1

    # 2. Every catalog builtin with a lowering has an internals page
    for b in builtins:
        if b["lowering"].get("codegen_file"):
            path = area_dir(INTERNALS_DIR, b["name"], b["area"])
            if not path.exists():
                errors.append(f"missing internals page for {b['name']}: expected {path}")
            else:
                stats["internals_pages"] += 1

    # 3. Every cross-link in a generated page resolves.
    #    - per-builtin pages (user + internals)
    #    - top-level indexes (master index + area indexes)
    checked_paths: set[Path] = set()
    for b in builtins:
        for base in (USER_DIR, INTERNALS_DIR):
            path = area_dir(base, b["name"], b["area"])
            if path.exists() and path not in checked_paths:
                _check_links(path, errors)
                checked_paths.add(path)
    if MASTER_INDEX.exists() and MASTER_INDEX not in checked_paths:
        _check_links(MASTER_INDEX, errors)
        checked_paths.add(MASTER_INDEX)
    for idx_path in USER_DIR.glob("*.md"):
        if idx_path not in checked_paths:
            _check_links(idx_path, errors)
            checked_paths.add(idx_path)

    # 4. Per-area indexes only contain builtins that belong to that area.
    for b in user_builtins:
        idx_path = USER_DIR / f"{b['area'].lower()}.md"
        if not idx_path.exists():
            errors.append(f"missing area index for {b['area']}")
            continue
        slug_str = slug(b["name"])
        text = idx_path.read_text(encoding="utf-8")
        if f"{slug_str}.md" not in text:
            errors.append(
                f"area index {idx_path.relative_to(REPO)} is missing {b['name']} ({slug_str})"
            )
        stats["area_index_checks"] += 1

    # 5. No stray top-level .md files (only <area>.md allowed)
    expected_top = {f"{a.lower()}.md" for a in {b["area"] for b in user_builtins}}
    for path in USER_DIR.iterdir():
        if path.is_file() and path.suffix == ".md" and path.name not in expected_top:
            errors.append(f"stray top-level file: {path.relative_to(REPO)}")

    # Summary
    print("=== Audit summary ===")
    print(f"Total builtins in catalog:   {len(builtins)}")
    print(f"User pages found:            {stats['user_pages']}")
    print(f"Internals pages found:       {stats['internals_pages']}")
    print(f"Master index found:          {stats['master_index']}")
    print(f"Area index checks:           {stats['area_index_checks']}")
    print(f"Errors:                      {len(errors)}")
    if errors:
        print()
        for e in errors[:50]:
            print(f"  - {e}")
        if len(errors) > 50:
            print(f"  ... ({len(errors) - 50} more)")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
