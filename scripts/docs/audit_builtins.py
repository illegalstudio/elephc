#!/usr/bin/env python3
"""Audit the generated builtin documentation.

Checks:

1. Every builtin in the registry has a user-facing page.
2. Every builtin that has a lowering has an internals page.
3. Every cross-link in a generated page resolves to an actual file.
4. Per-area indexes only contain builtins that belong to that area.
5. No stray top-level files (everything should be inside an area folder).
6. Backend availability and all 47 non-registry contract routes remain coherent.
7. User-facing pages contain no runs of multiple blank lines.
8. No override table in ``registry.py`` declares the same builtin twice.

Exits 0 on success, 1 on any failure.
"""

from __future__ import annotations

import ast
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
OVERRIDE_TABLES = REPO / "scripts" / "docs" / "elephc_builtins" / "registry.py"
# The hand-curated dicts in `registry.py` that refine what the Rust registry declares.
OVERRIDE_TABLE_NAMES = (
    "PARAM_TYPES",
    "RETURN_TYPE_OVERRIDES",
    "DESCRIPTION_OVERRIDES",
    "RUNTIME_HELPER_OVERRIDES",
    "REGISTRY_AREA_OVERRIDES",
)

# Link target patterns we recognise:
#   [text](path.md)         — Markdown link to another local .md file
#   [text](path/)           — dir is OK, ignore
#   [text](https://...)     — external, skip
LINK_RE = re.compile(r"\[([^\]]+)\]\(([^)]+)\)")
EXCESSIVE_BLANK_LINES_RE = re.compile(r"\n(?:[ \t]*\n){2,}")


def slug(name: str) -> str:
    return name.replace("\\", "-").replace("::", "-")


def area_dir(base: Path, name: str, area: str) -> Path:
    if name.startswith("__elephc_"):
        return base / "_internal" / f"{slug(name)}.md"
    return base / area.lower() / f"{slug(name)}.md"


def _check_override_tables_have_no_duplicate_keys(errors: list[str]) -> int:
    """Fail when a hand-curated override table declares one builtin twice.

    Python keeps the last binding and drops the earlier line without a word, so a duplicate is
    not clutter: editing the dead one is a no-op that reads exactly like a fix. `PARAM_TYPES` had
    reached 188 duplicated keys out of 788, seven of them stating a value nobody had applied in a
    long time. Checked mechanically because the two copies of a key sit hundreds of lines apart.
    """
    tree = ast.parse(OVERRIDE_TABLES.read_text(encoding="utf-8"))
    checked = 0
    for node in ast.walk(tree):
        if isinstance(node, ast.Assign):
            targets = node.targets
        elif isinstance(node, ast.AnnAssign):
            targets = [node.target]
        else:
            continue
        name = getattr(targets[0], "id", None)
        if name not in OVERRIDE_TABLE_NAMES or not isinstance(node.value, ast.Dict):
            continue
        checked += 1
        keys = [k.value for k in node.value.keys if isinstance(k, ast.Constant)]
        for key, count in Counter(keys).items():
            if count == 1:
                continue
            at = [
                k.lineno
                for k in node.value.keys
                if isinstance(k, ast.Constant) and k.value == key
            ]
            errors.append(
                f"{name} declares {key!r} {count} times (lines {at}); "
                "only the last one is live — merge them into a single entry"
            )
    return checked


def _check_links(path: Path, errors: list[str]) -> None:
    """Verify that every local Markdown link in ``path`` resolves to a file."""
    text = path.read_text(encoding="utf-8")
    for label, target in LINK_RE.findall(text):
        if target.startswith(("http://", "https://", "#", "mailto:")):
            continue
        # Drop the in-page anchor before checking the filesystem target
        # ([text](page.md#section) links to a heading inside page.md).
        target = target.split("#", 1)[0]
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


def _check_page_whitespace(path: Path, errors: list[str]) -> None:
    """Reject more than one consecutive blank line in a generated user page."""
    text = path.read_text(encoding="utf-8")
    match = EXCESSIVE_BLANK_LINES_RE.search(text)
    if match is None:
        return
    line = text.count("\n", 0, match.start()) + 1
    errors.append(
        f"excessive blank lines in {path.relative_to(REPO)} near line {line}"
    )


def _check_backend_contracts(
    raw: list[dict], errors: list[str], stats: dict[str, int]
) -> None:
    """Verify generated support metadata against the exceptional contract routes."""
    non_registry = [b for b in raw if (b.get("aot") or {}).get("kind") != "registry"]
    route_counts = Counter((b.get("aot") or {}).get("kind") for b in non_registry)
    expected_counts = {
        "language-construct": 5,
        "dedicated-syntax": 1,
        # MEASURED on the merged catalogue against a base of 4 prelude routes / 13 non-registry
        # contracts: neither side's own number survives. This branch adds `dir()`, the 14 `gz*`
        # stream functions, `zlib_get_coding_type` and `similar_text` as elephc-PHP preludes;
        # main adds the four hash_* and the 34 PHP-visible curl_*, the curl half existing only
        # because the canonical documentation configuration is `--features curl` (see
        # elephc_builtins/extract.py). Main also promoted get_object_vars out of the external
        # surface.
        "prelude": 58,
        "none": 3,
    }
    if len(non_registry) != 67:
        errors.append(f"expected 67 non-registry contracts, found {len(non_registry)}")
    if dict(route_counts) != expected_counts:
        errors.append(
            f"non-registry AOT route counts differ: expected {expected_counts}, "
            f"found {dict(route_counts)}"
        )

    by_name = {b["name"]: b for b in raw}
    for name in ("hash_init", "hash_update", "hash_final", "hash_copy"):
        record = by_name.get(name)
        if record is None:
            errors.append(f"missing shared prelude contract for {name}")
            continue
        aot = record.get("aot") or {}
        if not aot.get("supported") or aot.get("kind") != "prelude":
            errors.append(f"{name} must be AOT-supported through the prelude route")
        if record.get("eval_only"):
            errors.append(f"{name} is incorrectly marked eval-only")

    # The PHP-visible curl surface must document BOTH backends honestly: AOT through
    # the injected curl prelude (never "eval-only", which is what a default-feature
    # docs build used to imply by omitting it entirely) and eval through Magician's
    # own registry bindings.
    curl_names = sorted(name for name in by_name if name.startswith("curl_"))
    if len(curl_names) != 34:
        errors.append(f"expected 34 PHP-visible curl contracts, found {len(curl_names)}")
    for name in curl_names:
        record = by_name[name]
        aot = record.get("aot") or {}
        if not aot.get("supported") or aot.get("kind") != "prelude":
            errors.append(f"{name} must be AOT-supported through the curl prelude")
        if record.get("eval_only"):
            errors.append(f"{name} is incorrectly marked eval-only")
        if (record.get("eval") or {}).get("kind") != "registry":
            errors.append(f"{name} must be eval-supported by a Magician registry binding")

    hash_init = by_name.get("hash_init") or {}
    if (hash_init.get("aot") or {}).get("signature_override_reason") != "prelude-signature-subset":
        errors.append("hash_init must document its narrower AOT prelude signature")
    for name in ("exit", "die"):
        params = ((by_name.get(name) or {}).get("sig") or {}).get("params") or []
        if not params or params[0].get("default") != "0":
            errors.append(f"{name} must derive its optional status default from the shared contract")

    for record in raw:
        supported = bool((record.get("aot") or {}).get("supported"))
        if bool(record.get("eval_only")) == supported:
            errors.append(f"{record['name']} has an inconsistent eval_only flag")
    stats["backend_contract_checks"] = len(non_registry)


def main() -> int:
    raw = json.loads(REGISTRY.read_text(encoding="utf-8"))
    builtins = [b for b in raw if b["in_catalog"]]
    user_builtins = [b for b in builtins if not b["is_internal"]]

    errors: list[str] = []
    stats: dict[str, int] = defaultdict(int)

    _check_backend_contracts(raw, errors, stats)

    # 1. Every non-internal builtin has a user page
    for b in user_builtins:
        path = area_dir(USER_DIR, b["name"], b["area"])
        if not path.exists():
            errors.append(f"missing user page for {b['name']}: expected {path}")
        else:
            stats["user_pages"] += 1
            _check_page_whitespace(path, errors)

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

    stats["override_tables"] = _check_override_tables_have_no_duplicate_keys(errors)

    # Summary
    print("=== Audit summary ===")
    print(f"Total builtins in catalog:   {len(builtins)}")
    print(f"User pages found:            {stats['user_pages']}")
    print(f"Internals pages found:       {stats['internals_pages']}")
    print(f"Master index found:          {stats['master_index']}")
    print(f"Area index checks:           {stats['area_index_checks']}")
    print(f"Backend contract checks:     {stats['backend_contract_checks']}")
    print(f"Override tables checked:     {stats['override_tables']}")
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
