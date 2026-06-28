"""Render the JSON registry into per-builtin Markdown pages.

We generate two trees:

- ``docs/php/builtins/<slug>.md`` — user-facing reference (what does this
  function do, what does it return, examples).
- ``docs/internals/builtins/<slug>.md`` — compiler-internals reference
  (which file lowers it, which runtime helpers it calls, what the type
  checker says about its arity).

The renderer is *additive* by default: if a hand-written page already
exists, it is left untouched. Use ``--force`` to overwrite.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))

from registry import AREAS, slug  # noqa: E402


USER_TEMPLATE = """---
title: "{name}()"
description: "{short_description}"
sidebar:
  order: {order}
---

## {name}()

```php
{signature}
```

{description}

{parameters_section}

{return_section}

{examples_section}

{notes_section}

{see_also_section}

{internals_link}
"""

INTERNALS_TEMPLATE = """---
title: "{name}() — internals"
description: "{short_description}"
sidebar:
  order: {order}
---

## `{name}()` — internals

## Where it lives

- **Signature**: [`{sig_file}`]({sig_url})
- **Lowering**: [`{codegen_file}`:{codegen_line}]({codegen_url}){checker_clause}
- **Function symbol**: `{codegen_function}()`
{codegen_notes}

## Runtime helpers

{runtime_helpers_section}

## Signature summary

```php
{signature}
```

## What the type checker enforces

{checker_notes}

## Cross-references

{user_link}
{see_also_section}
"""


def _short_description(b: dict) -> str:
    """Compose a one-liner description when none was authored."""
    if b.get("description"):
        return b["description"]
    area = b["area"]
    name = b["name"]
    if name.startswith("__elephc_"):
        return f"Internal compiler helper: {name}."
    return f"{name}() — {area.lower()} builtin supported by Elephc."


def _internals_short_description(b: dict) -> str:
    """Compose a one-liner for the internals page frontmatter."""
    return (
        f"Compiler internals for {b['name']}(): lowering path, type checks, "
        "and runtime helpers."
    )


def _signature_line(b: dict) -> str:
    parts: list[str] = []
    for p in b["sig"]["params"]:
        prefix = ""
        if p["by_ref"]:
            prefix += "&"
        if not p.get("optional"):
            prefix += ""
        # PHP-style render: `string $name`
        if p.get("default") is not None:
            # already rendered PHP literal
            parts.append(f"{p['type']} ${p['name']} = {p['default']}")
        else:
            parts.append(f"{p['type']} ${p['name']}")
    params = ", ".join(parts)
    if b["sig"]["variadic"]:
        sep = ", " if parts else ""
        params = f"{params}{sep}...${b['sig']['variadic']}"
    return f"function {b['name']}({params}): {b['sig']['return_type']}"


def _parameters_section(b: dict) -> str:
    if not b["sig"]["params"] and not b["sig"].get("variadic"):
        return "**Parameters**: none."
    lines = ["**Parameters**:"]
    for p in b["sig"]["params"]:
        line = f"- `${p['name']}` (`{p['type']}`)"
        if p["by_ref"]:
            line += ", passed by reference"
        if p.get("default") is not None:
            line += f", default `{p['default']}`"
        if p.get("optional"):
            line += ", optional"
        lines.append(line)
    v = b["sig"].get("variadic")
    if v:
        lines.append(f"- `...${v}` — variadic: collects excess arguments into `${v}`.")
    return "\n".join(lines)


def _return_section(b: dict) -> str:
    return f"**Returns**: `{b['sig']['return_type']}`"


def _examples_section(b: dict) -> str:
    if not b.get("examples"):
        return (
            "_No examples yet — check `examples/` and `showcases/` for usage patterns._\n"
        )
    blocks = ["**Examples**:"]
    for ex in b["examples"]:
        blocks.append(ex)
    return "\n\n".join(blocks)


def _notes_section(b: dict) -> str:
    if not b.get("notes"):
        return ""
    lines = ["**Notes**:"]
    for n in b["notes"]:
        lines.append(f"- {n}")
    return "\n".join(lines)


def _see_also_section(b: dict, prefix: str = "See also") -> str:
    if not b.get("see_also"):
        return ""
    return f"**{prefix}**: " + ", ".join(f"`{n}`" for n in b["see_also"])


def _runtime_helpers_section(b: dict) -> str:
    helpers = b["lowering"].get("runtime_helpers", [])
    if not helpers:
        return "_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._"
    lines = ["The following runtime helpers are referenced:"]
    for h in helpers:
        lines.append(f"- `{h}`")
    return "\n".join(lines)


def _github_url(repo_root: Path, file_path: str) -> str:
    """Build a GitHub permalink for a file (assuming `main` branch — adjust as needed)."""
    rel = file_path
    return f"https://github.com/illegalstudio/elephc/blob/main/{rel}"


def _github_url_with_line(repo_root: Path, file_path: str, line: int) -> str:
    rel = file_path
    if line is None:
        return _github_url(repo_root, file_path)
    return f"https://github.com/illegalstudio/elephc/blob/main/{rel}#L{line}"


def _internals_link(b: dict) -> str:
    """Cross-link to the internals page for this builtin, if it has been lowered.

    The link is built relative to the current user-page path
    (docs/php/builtins/<area>/<name>.md) → the internals page lives at
    docs/internals/builtins/<area>/<name>.md.
    """
    if not b["lowering"].get("codegen_file"):
        return ""
    name = b["name"]
    # From docs/php/builtins/<area>/<name>.md → docs/internals/builtins/<area>/<name>.md
    # requires three .. to climb out of php/builtins/<area>/, then descend.
    area = b["area"]
    target = f"../../../internals/builtins/{area.lower()}/{slug(name)}.md"
    if name.startswith("__elephc_"):
        target = f"../../../internals/builtins/_internal/{slug(name)}.md"
    return (
        f"\n## Internals\n\n"
        f"For how `{name}` is implemented in the compiler, see "
        f"[the internals page]({target}).\n"
    )


def render_user(b: dict, order: int, repo_root: Path) -> str:
    _ = repo_root  # reserved for future cross-repo links
    area_lower = b['area'].lower()
    article = "an" if area_lower[0] in "aeiou" else "a"
    return USER_TEMPLATE.format(
        name=b["name"],
        short_description=_short_description(b).replace('"', '\\"'),
        area=b["area"],
        order=order,
        signature=_signature_line(b),
        description=b.get("description")
        or f"`{b['name']}()` is {article} {area_lower} builtin supported by Elephc. "
           "Behavior matches the PHP manual unless noted below.",
        parameters_section=_parameters_section(b),
        return_section=_return_section(b),
        examples_section=_examples_section(b),
        notes_section=_notes_section(b),
        see_also_section=_see_also_section(b),
        internals_link=_internals_link(b),
    )


def render_internals(b: dict, order: int, repo_root: Path) -> str:
    sig_file = b["lowering"].get("sig_file") or "src/types/signatures.rs"
    codegen_file = b["lowering"].get("codegen_file")
    codegen_line = b["lowering"].get("codegen_line")
    codegen_function = b["lowering"].get("codegen_function") or "(none — type-checker only)"
    notes = b["lowering"].get("notes") or []
    helpers = b["lowering"].get("runtime_helpers", [])

    sig_url = _github_url(repo_root, sig_file)
    codegen_url = (
        _github_url_with_line(repo_root, codegen_file, codegen_line)
        if codegen_file and codegen_line
        else ""
    )

    checker_clause = ""
    if codegen_file:
        checker_clause = f" (`{codegen_function}`)"

    codegen_notes = ""
    if notes:
        codegen_notes = "\n\n### Lowering notes\n\n" + "\n".join(f"- {n}" for n in notes)

    see_also = b.get("see_also") or []
    see_also_section = ""
    if see_also:
        see_also_section = "\n" + "\n".join(f"- `{n}()`" for n in see_also)

    if b.get("is_internal"):
        user_link = "- _No user-facing reference — this is a compiler internal helper._"
    elif b["name"].startswith("__elephc_"):
        user_link = (
            f"- [User reference for `{b['name']}()`](../../../php/builtins/_internal/{slug(b['name'])}.md)"
        )
    else:
        user_link = (
            f"- [User reference for `{b['name']}()`](../../../php/builtins/{b['area'].lower()}/{slug(b['name'])}.md)"
        )
    return INTERNALS_TEMPLATE.format(
        name=b["name"],
        short_description=_internals_short_description(b).replace('"', '\\"'),
        order=order,
        slug=slug(b["name"]),
        user_link=user_link,
        sig_file=sig_file,
        sig_url=sig_url,
        codegen_file=codegen_file or "(not lowered)",
        codegen_line=codegen_line or 0,
        codegen_url=codegen_url,
        checker_clause=checker_clause,
        codegen_function=codegen_function,
        codegen_notes=codegen_notes,
        runtime_helpers_section=_runtime_helpers_section(b),
        signature=_signature_line(b),
        checker_notes=_checker_notes(b),
        see_also_section=see_also_section,
    )


def _checker_notes(b: dict) -> str:
    """Best-effort notes about what the type checker enforces. Pulled from
    check_builtin() arms in src/types/checker/builtins/*.rs — for now we
    just embed the arity information we know."""
    params = b["sig"]["params"]
    required = sum(1 for p in params if not p.get("optional"))
    total = len(params)
    lines = []
    if total == 0:
        lines.append("- **Arity**: takes no arguments.")
    elif required == total:
        lines.append(f"- **Arity**: takes exactly {total} argument{'s' if total != 1 else ''}.")
    else:
        lines.append(
            f"- **Arity**: takes {required}–{total} arguments "
            f"({total - required} optional)."
        )
    by_ref = [p["name"] for p in params if p.get("by_ref")]
    if by_ref:
        lines.append(f"- **By-reference parameters**: {', '.join('`$' + n + '`' for n in by_ref)}.")
    if b["sig"]["variadic"]:
        lines.append(f"- **Variadic**: collects excess arguments into `${b['sig']['variadic']}`.")
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# Area index page
# ---------------------------------------------------------------------------


def _index_table_rows(builtins: list[dict], link_prefix: str = ".") -> list[str]:
    """Build the Markdown table rows used by area and master indexes.

    ``link_prefix`` is the path from the containing index page to the
    ``builtins/`` directory. For an area index at ``docs/php/builtins/<area>.md``
    it is ``"."``; for the master index at ``docs/php/builtins.md`` it is
    ``"./builtins"``.
    """
    rows: list[str] = []
    for b in builtins:
        full = _signature_line(b)
        import re as _re
        m = _re.match(r"^function\s+\S+\((.*)\)\s*:\s*(.*)$", full)
        if m:
            params = m.group(1).strip()
            rtype = m.group(2).strip()
            sig = f"({params}): {rtype}"
        else:
            sig = full
        sig = _re.sub(r"\s+", " ", sig).strip()
        area_folder = b["area"].lower()
        if b["name"].startswith("__elephc_"):
            link = f"{link_prefix}/_internal/{slug(b['name'])}.md"
        else:
            link = f"{link_prefix}/{area_folder}/{slug(b['name'])}.md"
        rows.append(
            f"| [`{b['name']}()`]({link}) | `{sig}` | `{b['sig']['return_type']}` |"
        )
    return rows


def render_area_index(area: str, builtins: list[dict], order: int = 0) -> str:
    """Render a per-area index page at docs/php/builtins/<area>.md."""
    relevant = [b for b in builtins if b["area"] == area and not b["is_internal"]]
    relevant.sort(key=lambda b: b["name"])
    lines = [
        "---",
        f'title: "{area} builtins"',
        f'description: "Builtins in the {area} category."',
        "sidebar:",
        f"  order: {order}",
        "---",
        "",
        f"## {area} builtins",
        "",
        "| Function | Signature | Returns |",
        "|---|---|---|",
    ]
    lines.extend(_index_table_rows(relevant))
    return "\n".join(lines) + "\n"


def render_master_index(builtins: list[dict]) -> str:
    """Render the master builtins index at docs/php/builtins.md."""
    relevant = [b for b in builtins if not b["is_internal"]]
    relevant.sort(key=lambda b: (b["area"], b["name"]))
    lines = [
        "---",
        'title: "Builtins"',
        'description: "Index of all PHP builtins supported by Elephc."',
        "sidebar:",
        "  order: 0",
        "---",
        "",
        "## Builtins",
        "",
        "| Function | Signature | Returns |",
        "|---|---|---|",
    ]
    lines.extend(_index_table_rows(relevant, link_prefix="./builtins"))
    return "\n".join(lines) + "\n"


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------


def main_with(
    repo_root: Path,
    registry: Path,
    force: bool = False,
    user_dir: Path | None = None,
    internals_dir: Path | None = None,
) -> int:
    repo = repo_root.resolve()
    registry_path = registry.resolve()
    ud = (user_dir or (repo / "docs" / "php" / "builtins")).resolve()
    id_ = (internals_dir or (repo / "docs" / "internals" / "builtins")).resolve()
    return _do_render(repo, registry_path, ud, id_, force)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=Path(__file__).resolve().parents[3],
    )
    parser.add_argument(
        "--registry",
        type=Path,
        default=None,
    )
    parser.add_argument(
        "--out-user",
        type=Path,
        default=None,
        help="User-facing docs directory (default: <repo>/docs/php/builtins)",
    )
    parser.add_argument(
        "--out-internals",
        type=Path,
        default=None,
        help="Internals docs directory (default: <repo>/docs/internals/builtins)",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="Overwrite existing hand-written pages",
    )
    args = parser.parse_args()

    repo = args.repo_root.resolve()
    registry_path = (
        args.registry or repo / "scripts" / "docs" / "builtin_registry.json"
    ).resolve()
    user_dir = (args.out_user or repo / "docs" / "php" / "builtins").resolve()
    internals_dir = (args.out_internals or repo / "docs" / "internals" / "builtins").resolve()
    return _do_render(repo, registry_path, user_dir, internals_dir, args.force)


def _clean_output_tree(root: Path) -> None:
    """Remove generated .md files and area subfolders so a --force render starts fresh.

    Keeps any hand-written top-level .md files (e.g. a manually curated README) and
    ignores non-.md entries. This prevents stale case variants of area folders on
    case-insensitive filesystems.
    """
    if not root.exists():
        return
    for entry in list(root.iterdir()):
        if entry.is_file() and entry.suffix == ".md":
            entry.unlink()
        elif entry.is_dir():
            # Remove the whole area subfolder; it will be recreated on demand.
            import shutil
            shutil.rmtree(entry, ignore_errors=True)


def _do_render(repo: Path, registry_path: Path, user_dir: Path, internals_dir: Path, force: bool) -> int:
    raw = json.loads(registry_path.read_text(encoding="utf-8"))
    # Split the catalog: user-facing pages skip compiler-internal helpers.
    catalog_builtins = [b for b in raw if b["in_catalog"]]
    internal_only = [b for b in raw if b["is_internal"] and not b["in_catalog"]]
    user_facing = [b for b in catalog_builtins if not b["is_internal"]]
    user_facing.sort(key=lambda b: (b["area"], b["name"]))
    catalog_builtins.sort(key=lambda b: (b["area"], b["name"]))
    internal_only.sort(key=lambda b: b["name"])

    if force:
        _clean_output_tree(user_dir)
        _clean_output_tree(internals_dir)

    user_dir.mkdir(parents=True, exist_ok=True)
    internals_dir.mkdir(parents=True, exist_ok=True)

    def area_dir(base: Path, b: dict) -> Path:
        """Resolve the per-area subfolder for a builtin (or _internal for compiler helpers)."""
        if b["name"].startswith("__elephc_"):
            return base / "_internal"
        return base / b["area"].lower()

    written = 0
    skipped = 0

    # Internals pages are emitted for every catalog builtin, including helpers.
    for idx, b in enumerate(catalog_builtins, start=1):
        i_dir = area_dir(internals_dir, b)
        i_dir.mkdir(parents=True, exist_ok=True)
        internals_path = i_dir / f"{slug(b['name'])}.md"
        if internals_path.exists() and not force:
            skipped += 1
        else:
            internals_path.write_text(render_internals(b, idx, repo), encoding="utf-8")
            written += 1

    # Compiler-only builtin entries are tracked outside the PHP-visible catalog.
    # Render them after the catalog so they cannot perturb existing sidebar order.
    internal_start = len(catalog_builtins) + 1
    for idx, b in enumerate(internal_only, start=internal_start):
        i_dir = area_dir(internals_dir, b)
        i_dir.mkdir(parents=True, exist_ok=True)
        internals_path = i_dir / f"{slug(b['name'])}.md"
        if internals_path.exists() and not force:
            skipped += 1
        else:
            content = render_internals(b, idx, repo).rstrip() + "\n"
            internals_path.write_text(content, encoding="utf-8")
            written += 1

    # User-facing pages are emitted only for non-internal builtins.
    for idx, b in enumerate(user_facing, start=1):
        u_dir = area_dir(user_dir, b)
        u_dir.mkdir(parents=True, exist_ok=True)
        user_path = u_dir / f"{slug(b['name'])}.md"
        if user_path.exists() and not force:
            skipped += 1
        else:
            user_path.write_text(render_user(b, idx, repo), encoding="utf-8")
            written += 1

    # Master index: docs/php/builtins.md
    index_path = user_dir.parent / "builtins.md"
    if not index_path.exists() or force:
        index_path.write_text(
            render_master_index(user_facing),
            encoding="utf-8",
        )

    # The Astro site filters out README.md pages, so drop any legacy index.
    legacy_readme = user_dir / "README.md"
    if legacy_readme.exists():
        legacy_readme.unlink()

    # Per-area index pages: docs/php/builtins/<area>.md
    # (one top-level file per area, linking to <area>/<name>.md)
    for area_index, area in enumerate(AREAS, start=1):
        relevant = [b for b in user_facing if b["area"] == area]
        if not relevant:
            continue
        area_path = user_dir / f"{area.lower()}.md"
        if area_path.exists() and not force:
            continue
        area_path.write_text(
            render_area_index(area, user_facing, order=100 + area_index),
            encoding="utf-8",
        )

    print(
        f"Rendered: {written} pages ({skipped} kept hand-written). Master index at {index_path}",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
