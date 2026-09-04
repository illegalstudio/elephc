#!/usr/bin/env python3
"""Write the generated "Functions" section into each hand-written module page.

For every docs/php page named in module_pages.MODULE_PAGES, replace (or append) the block
between the `elephc:generated:symbols` markers with a table of every function the shared
catalog attributes to that page's PHP module(s), linked to the generated per-function
reference pages, followed by the module's classes and global constants.

The prose around the block stays hand-written; the block is owned by this script and the
`builtins-docs-sync` CI job regenerates it, so `doc-update` and the builtin-docs audits
know exactly what each side owns.

Usage (from the repo root, after extract_builtins.py):
    python3 scripts/docs/gen_module_sections.py
"""
from __future__ import annotations

import json
import sys
from collections import defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parent / "elephc_builtins"))

from module_pages import BEGIN_MARKER, END_MARKER, MODULE_PAGES, SECTION_ANCHOR  # noqa: E402
from render import _index_table_rows  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parents[2]
CONSTANT_LIST_LIMIT = 60


def _module_title(module: str) -> str:
    return {
        "gd": "GD",
        "pcre": "PCRE",
        "pdo": "PDO",
        "spl": "SPL",
        "zend opcache": "Zend OPcache",
        "cairo": "Cairo (PECL)",
        "imagick": "Imagick (PECL)",
        "gmagick": "Gmagick (PECL)",
        "pdo_ibm": "pdo_ibm (PECL)",
    }.get(module, module)


def _constants_line(constants: list[dict]) -> str:
    names = sorted(c["name"] for c in constants)
    if len(names) <= CONSTANT_LIST_LIMIT:
        return ", ".join(f"`{n}`" for n in names)
    prefixes = defaultdict(int)
    for name in names:
        prefixes[name.split("_")[0] + "_*"] += 1
    families = ", ".join(f"`{p}` ({n})" for p, n in sorted(prefixes.items(), key=lambda i: -i[1]))
    return f"{len(names)} constants: {families}"


def render_block(page: str, modules: list[str], registry: list[dict], symbols: dict) -> str:
    lines = [
        BEGIN_MARKER,
        "",
        f"## Functions {{#{SECTION_ANCHOR}}}",
        "",
        "Generated from the shared symbol catalog by `scripts/docs/gen_module_sections.py`; "
        "do not edit this section by hand. Each function links to its reference page.",
    ]
    for module in modules:
        functions = [
            b for b in registry
            if b["module"] == module and not b["is_internal"] and not b["is_extension"]
        ]
        classes = [
            c for c in symbols["classes"]
            if c["module"] == module and not c["internal"] and not c["extension"]
        ]
        constants = [
            c for c in symbols["constants"]
            if c["module"] == module and not c["internal"] and not c["extension"]
        ]
        if not (functions or classes or constants):
            continue
        if len(modules) > 1:
            lines += ["", f"### {_module_title(module)}"]
        if functions:
            functions.sort(key=lambda b: b["name"])
            lines += [
                "",
                "| Function | Signature | Returns | AOT | eval() |",
                "|---|---|---|:-:|:-:|",
            ]
            lines.extend(_index_table_rows(functions, link_prefix="./builtins"))
        if classes:
            names = ", ".join(f"`{c['name']}`" for c in sorted(classes, key=lambda c: c["name"].lower()))
            lines += ["", f"Classes: {names}."]
        if constants:
            lines += ["", f"Constants: {_constants_line(constants)}."]
    lines += ["", END_MARKER]
    return "\n".join(lines)


def update_page(path: Path, block: str) -> bool:
    text = path.read_text(encoding="utf-8")
    if BEGIN_MARKER in text and END_MARKER in text:
        start = text.index(BEGIN_MARKER)
        end = text.index(END_MARKER) + len(END_MARKER)
        new_text = text[:start] + block + text[end:]
    else:
        new_text = text.rstrip("\n") + "\n\n" + block + "\n"
    if new_text != text:
        path.write_text(new_text, encoding="utf-8")
        return True
    return False


def run(repo_root: Path = REPO_ROOT) -> int:
    scripts_docs = repo_root / "scripts" / "docs"
    registry = json.loads((scripts_docs / "builtin_registry.json").read_text(encoding="utf-8"))
    symbols = json.loads((scripts_docs / "symbol_registry.json").read_text(encoding="utf-8"))
    pages: dict[str, list[str]] = defaultdict(list)
    for module, page in MODULE_PAGES.items():
        pages[page].append(module)
    changed = 0
    for page, modules in sorted(pages.items()):
        path = repo_root / "docs" / "php" / page
        if not path.exists():
            print(f"error: {path} does not exist (module_pages.MODULE_PAGES)", file=sys.stderr)
            return 1
        block = render_block(page, modules, registry, symbols)
        if update_page(path, block):
            changed += 1
    print(f"updated {changed} module page(s) under docs/php")
    return 0


if __name__ == "__main__":
    raise SystemExit(run())
