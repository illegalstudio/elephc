"""Extract builtin metadata from the Elephc `builtin!` registry.

Since the single-source builtin registry migration, every PHP builtin is declared
once via `builtin!` in ``src/builtins/<area>/<name>.rs`` and collected through the
`inventory` crate. The authoritative data is therefore read from the registry
itself, via the ``gen_builtins`` binary (``cargo run --bin gen_builtins
--include-internal``), NOT by regex-scraping ``catalog.rs`` / ``signatures.rs``
(which the migration emptied).

For each builtin we enrich the registry data with:

1. its lowering location — the emitter its home-file ``lower`` hook dispatches to,
   plus that emitter's ``__rt_*`` runtime helpers and leading ``///`` doc notes,
2. its documentation area (derived from the lowering file path, as before),
3. optional type-precision refinements for non-scalar params/returns that the
   registry represents coarsely as ``Mixed`` (``PARAM_TYPES`` / ``RETURN_TYPE_OVERRIDES``).

The 8 PHP language constructs that intentionally stay checker-resident
(``isset``/``unset``/``empty``/``exit``/``die``/``buffer_len``/``buffer_free``/
``buffer_new``) are not in the registry; they are added from a small hand-curated
table so their documentation pages are preserved.

The output is a list of :class:`registry.Builtin` written to a JSON file in
``scripts/docs/builtin_registry.json``.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Optional

# Make ``registry`` importable when running this file directly.
sys.path.insert(0, str(Path(__file__).resolve().parent))

from registry import (  # noqa: E402  (sys.path tweak above)
    AREA_BY_FILE,
    AREA_BY_LOWERING_FN,
    AREA_BY_MODULE,
    AREA_BY_NAME,
    Builtin,
    BuiltinSig,
    DESCRIPTION_OVERRIDES,
    INTERNAL_NOTES,
    LoweringInfo,
    PARAM_TYPES,
    Parameter,
    RETURN_TYPE_OVERRIDES,
    slug,
)


# ---------------------------------------------------------------------------
# Registry source of truth: the `gen_builtins` binary
# ---------------------------------------------------------------------------

def run_gen_builtins(repo: Path) -> list[dict]:
    """Return the registry as a list of dicts by invoking the `gen_builtins` binary.

    Includes `internal` builtins (the docs pipeline renders compiler-internals
    pages for the `__elephc_*` helpers). Prefers a prebuilt binary under
    ``target/{release,debug}/`` when present (fast path for CI, which builds it
    first); otherwise falls back to ``cargo run``.
    """
    cmd: list[str]
    for profile in ("release", "debug"):
        exe = repo / "target" / profile / "gen_builtins"
        if exe.exists():
            cmd = [str(exe), "--include-internal"]
            break
    else:
        cmd = ["cargo", "run", "--quiet", "--bin", "gen_builtins", "--", "--include-internal"]
    proc = subprocess.run(cmd, cwd=repo, capture_output=True, text=True)
    if proc.returncode != 0:
        sys.exit(
            "gen_builtins failed (build it with `cargo build --bin gen_builtins`):\n"
            + proc.stderr
        )
    try:
        return json.loads(proc.stdout)
    except json.JSONDecodeError as exc:  # pragma: no cover - defensive
        sys.exit(f"gen_builtins produced invalid JSON: {exc}")


# ---------------------------------------------------------------------------
# Home-file lowering map: name -> the emitter its `lower` hook dispatches to
# ---------------------------------------------------------------------------

# Core registry-machinery files under src/builtins/ that are NOT builtin homes.
_NON_HOME_FILES = {
    "spec.rs",
    "registry.rs",
    "macros.rs",
    "convert.rs",
    "docs.rs",
    "mod.rs",
    "parity_tests.rs",
}

_NAME_RE = re.compile(r'name:\s*"([^"]+)"')
# The `lower` hook dispatches to the real emitter via a fully-qualified path,
# e.g. `crate::codegen_ir::lower_inst::builtins::math::lower_abs(ctx, inst)`
# (the `(ctx` may be on the following line — `\s*` spans newlines).
_EMITTER_RE = re.compile(r"lower_inst::builtins::([A-Za-z0-9_:]+)\s*\(\s*ctx\b")


def build_home_lowering_map(repo: Path) -> dict[str, tuple[str, str, str]]:
    """Map each registry builtin name (lowercased) to ``(emitter_fn, module, home_rel)``.

    Scans every builtin home file under ``src/builtins/`` (skipping the registry
    machinery files), reads its ``builtin!`` name and the emitter path its
    ``lower`` hook dispatches to. ``module`` is the last path segment before the
    emitter function (used for the AREA_BY_MODULE area fallback); ``home_rel`` is
    the home file path relative to the repo root.
    """
    out: dict[str, tuple[str, str, str]] = {}
    builtins_root = repo / "src" / "builtins"
    for path in builtins_root.rglob("*.rs"):
        if path.name in _NON_HOME_FILES:
            continue
        text = path.read_text(encoding="utf-8")
        if "builtin!" not in text:
            continue
        name_match = _NAME_RE.search(text)
        if not name_match:
            continue
        canonical = name_match.group(1).lower()
        emitter_fn = ""
        module = ""
        emit_match = _EMITTER_RE.search(text)
        if emit_match:
            segments = emit_match.group(1).split("::")
            emitter_fn = segments[-1]
            module = segments[-2] if len(segments) >= 2 else ""
        out[canonical] = (emitter_fn, module, str(path.relative_to(repo)))
    return out


# ---------------------------------------------------------------------------
# Emitter resolution: find the emitter fn definition, its doc + runtime helpers
# ---------------------------------------------------------------------------

DOC_COMMENT_RE = re.compile(r"^///\s?(.*)$")


def find_lowering_function_def(src: str, fn_name: str) -> Optional[tuple[str, int]]:
    """Find the (line_text, line_number) of ``fn <fn_name>(`` in ``src``."""
    lines = src.splitlines()
    for i, line in enumerate(lines, start=1):
        if re.match(rf"\s*(pub(?:\([^)]*\))?\s+)?fn\s+{re.escape(fn_name)}\s*\(", line):
            return (line, i)
    return None


def _leading_doc_comment(src: str, line: int) -> str:
    """Return the ``///`` doc-comment block immediately above the function at ``line``."""
    lines = src.splitlines()
    i = line - 2  # 1-based → index above the def
    out: list[str] = []
    while i >= 0 and lines[i].lstrip().startswith("///"):
        m = DOC_COMMENT_RE.match(lines[i].lstrip())
        if m:
            out.append(m.group(1).strip())
        i -= 1
    out.reverse()
    return "\n".join(out)


def collect_runtime_helpers(notes: str, body: str) -> list[str]:
    """Return the sorted set of ``__rt_*`` symbols mentioned in the doc + lowering body."""
    found = set(re.findall(r"\b__rt_[A-Za-z0-9_]+", notes)) | set(
        re.findall(r"\b__rt_[A-Za-z0-9_]+", body)
    )
    return sorted(found)


def parse_area_for_file(rel_path: str) -> tuple[Optional[str], str]:
    """Look up the ``(area, sub_area)`` for a lowering file path under ``builtins/``.

    Returns ``(None, "")`` as a sentinel when the file is the root dispatcher and
    the area should be inferred from the module/function instead.
    """
    key = rel_path.replace("builtins/", "").replace("builtins\\", "")
    if key in AREA_BY_FILE:
        val = AREA_BY_FILE[key]
        return (None, "") if val is None else val
    base = Path(key).name
    if base in AREA_BY_FILE:
        val = AREA_BY_FILE[base]
        return (None, "") if val is None else val
    return ("Misc", "Misc")


def resolve_lowering(
    repo: Path,
    read,
    dispatch: Path,
    lowering_dir: Path,
    emitter_fn: str,
    sig_file: Optional[str],
) -> LoweringInfo:
    """Resolve an emitter function name to its definition, doc notes, and helpers.

    Searches ``builtins.rs`` (root dispatcher) and every per-area submodule for
    ``fn <emitter_fn>(``. Returns a populated :class:`LoweringInfo` (with
    ``codegen_file``/``codegen_line``/``notes``/``runtime_helpers``) when found, or
    a bare one carrying only ``sig_file`` when not.
    """
    lowering = LoweringInfo(sig_file=sig_file)
    if not emitter_fn:
        return lowering
    for candidate in [dispatch, *sorted(lowering_dir.rglob("*.rs"))]:
        src_text = read(candidate)
        defn = find_lowering_function_def(src_text, emitter_fn)
        if defn is None:
            continue
        _, def_line = defn
        doc = _leading_doc_comment(src_text, def_line)
        body = "\n".join(src_text.splitlines()[def_line - 1 : def_line + 30])
        helpers = collect_runtime_helpers(doc, body)
        notes = [line for line in doc.splitlines() if line.strip()]
        return LoweringInfo(
            sig_file=sig_file,
            codegen_file=str(candidate.relative_to(repo)),
            codegen_line=def_line,
            codegen_function=emitter_fn,
            runtime_helpers=helpers,
            notes=notes,
        )
    return lowering


def resolve_area(
    canonical: str, lowering: LoweringInfo, emitter_fn: str, module: str
) -> tuple[str, str]:
    """Resolve a builtin's documentation ``(area, sub_area)``.

    Priority (most specific first): per-name override → the lowering file's path →
    the generic libm/lowering-fn mapping → the dispatch module → ``Misc``.
    """
    area = AREA_BY_NAME.get(canonical, ("Misc", "Misc"))
    if area == ("Misc", "Misc") and lowering.codegen_file:
        cf = lowering.codegen_file
        prefix = "src/codegen_ir/lower_inst/builtins"
        rel_under = cf[len(prefix) + 1 :] if cf.startswith(prefix + "/") else cf
        file_area = parse_area_for_file(rel_under)
        if file_area[0] is not None and (file_area[0] != "Misc" or file_area[1] != "Misc"):
            area = file_area
    if area == ("Misc", "Misc"):
        fn_area = AREA_BY_LOWERING_FN.get(emitter_fn) if emitter_fn else None
        if fn_area is not None:
            area = fn_area
        elif module:
            mod_area = AREA_BY_MODULE.get(module)
            if mod_area is not None:
                area = mod_area
    return area


# ---------------------------------------------------------------------------
# Type + default rendering (registry data → doc vocabulary)
# ---------------------------------------------------------------------------

def _normalize_type(reg_type: str) -> str:
    """Map a registry type string to the doc's simple type vocabulary.

    The registry renders `TypeSpec::ArrayOf`/`AssocOf` as ``array<...>`` and
    unions as ``a|b``; the docs collapse those to ``array`` / ``mixed``. Scalars
    (``int``/``float``/``string``/``bool``/``mixed``/``null``/``void``) pass through.
    """
    reg_type = reg_type.strip()
    if "|" in reg_type:
        return "mixed"
    if reg_type.startswith("array"):
        return "array"
    return reg_type


def _param_refine_type(entry) -> Optional[str]:
    """Extract the display type from a `PARAM_TYPES` entry (``str`` or ``(type, name)``)."""
    if entry is None:
        return None
    if isinstance(entry, str):
        return entry or None
    ty = entry[0]
    return ty or None


# Maps a Rust `PhpType::<Variant>` to the doc's display type.
_PHPTYPE_DISPLAY = {
    "Str": "string",
    "Int": "int",
    "Bool": "bool",
    "Float": "float",
    "Void": "void",
    "Null": "null",
    "Mixed": "mixed",
    "Never": "never",
    "Array": "array",
    "AssocArray": "array",
    "Union": "mixed",
    "Buffer": "buffer",
}


def _extract_fn_body(text: str, fn_name: str) -> str:
    """Return the brace-matched body of ``fn <fn_name>(`` in ``text`` (or '')."""
    for prefix in ("pub(crate) ", "pub(super) ", "pub ", ""):
        start = text.find(f"{prefix}fn {fn_name}(")
        if start >= 0:
            break
    else:
        return ""
    brace = text.find("{", start)
    if brace < 0:
        return ""
    depth = 0
    for i in range(brace, len(text)):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return text[brace : i + 1]
    return ""


def parse_home_check_return(home_text: str, resolve_body) -> Optional[str]:
    """Recover a precise return type from a home file's ``check`` hook, or ``None``.

    The registry types non-scalar returns coarsely as ``Mixed`` (arrays are
    declared ``Mixed`` + a check hook that returns the precise type). We locate the
    hook's body — a local ``fn check`` or, when ``check:`` points to a distinctively
    named shared fn (e.g. ``support::check_declared_names``), that fn resolved via
    ``resolve_body`` — then scan its ``Ok(PhpType::<Variant>)`` returns. When they
    agree on a single non-``mixed`` display type (or an array type dominates), we
    return it; otherwise ``None``.
    """
    m = re.search(r"\bcheck:\s*([A-Za-z0-9_:]+)", home_text)
    if not m:
        return None
    target = m.group(1)
    fn_name = target.split("::")[-1]
    if "::" in target and fn_name != "check":
        body = resolve_body(fn_name) or _extract_fn_body(home_text, fn_name)
    else:
        body = _extract_fn_body(home_text, "check")
    if not body:
        return None
    variants = re.findall(r"Ok\(\s*PhpType::([A-Za-z0-9_]+)", body)
    displays = {_PHPTYPE_DISPLAY.get(v, "mixed") for v in variants}
    # Array-passthrough pattern: the hook validates the argument is an array and
    # returns it unchanged (`Ok(ty)`), so the literal PhpType is never written.
    if re.search(r"Ok\(\s*[a-z_]\w*\s*\)", body) and "PhpType::Array" in body:
        displays.add("array")
    non_mixed = displays - {"mixed"}
    if len(non_mixed) == 1:
        return next(iter(non_mixed))
    if "array" in non_mixed:
        return "array"
    return None


def _render_default(value, optional: bool) -> Optional[str]:
    """Render a registry default value as a PHP-literal display string.

    Required params (``optional`` false) have no default (``None``). Optional
    params render their default: ``null``, ``true``/``false``, integers/floats
    verbatim, strings single-quoted, the ``PHP_INT_MAX``/``PHP_INT_MIN`` sentinels
    as constants, and the empty-array sentinel as ``[]``.
    """
    if not optional:
        return None
    if value is None:
        return "null"
    # bool must precede int: bool is a subclass of int in Python.
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return str(value)
    if isinstance(value, list):
        return "[]"
    if isinstance(value, str):
        if value in ("PHP_INT_MAX", "PHP_INT_MIN"):
            return value
        return repr(value)
    return str(value)


# ---------------------------------------------------------------------------
# PHP language constructs (checker-resident, NOT in the registry)
# ---------------------------------------------------------------------------

# These stay in the type checker (they operate on l-values / are lazy constructs)
# and are absent from the `builtin!` registry. We add them by hand so their doc
# pages are preserved. Each: params [(name, type, by_ref, default, optional)],
# variadic, return_type, (area, sub_area), description, emitter_fn (or None).
LANGUAGE_CONSTRUCTS: dict[str, dict] = {
    "isset": {
        "params": [("var", "mixed", False, None, False)],
        "variadic": "vars",
        "return_type": "bool",
        "area": ("Misc", "Variable"),
        "description": "Determines whether a variable is set and is not null.",
        "emitter_fn": "lower_isset",
    },
    "unset": {
        "params": [("var", "mixed", False, None, False)],
        "variadic": "vars",
        "return_type": "void",
        "area": ("Misc", "Variable"),
        "description": "Unsets the given variables.",
        "emitter_fn": "lower_unset_builtin",
    },
    "empty": {
        "params": [("value", "mixed", False, None, False)],
        "variadic": None,
        "return_type": "bool",
        "area": ("Misc", "Variable"),
        "description": "Determines whether a variable is considered empty.",
        "emitter_fn": "lower_empty",
    },
    "exit": {
        "params": [("status", "int", False, None, True)],
        "variadic": None,
        "return_type": "void",
        "area": ("Process", "Process"),
        "description": "",
        "emitter_fn": None,
    },
    "die": {
        "params": [("status", "int", False, None, True)],
        "variadic": None,
        "return_type": "void",
        "area": ("Process", "Process"),
        "description": "",
        "emitter_fn": None,
    },
    "buffer_len": {
        "params": [("buffer", "buffer", False, None, False)],
        "variadic": None,
        "return_type": "int",
        "area": ("Buffer", "Buffer"),
        "description": "Lowers `buffer_len()` through the direct buffer opcode helper.",
        "emitter_fn": "lower_buffer_len",
    },
    "buffer_free": {
        "params": [("buffer", "buffer", False, None, False)],
        "variadic": None,
        "return_type": "mixed",
        "area": ("Buffer", "Buffer"),
        "description": "Lowers `buffer_free()` through the direct buffer opcode helper.",
        "emitter_fn": "lower_buffer_free",
    },
    "buffer_new": {
        "params": [("length", "int", False, None, False)],
        "variadic": None,
        "return_type": "mixed",
        "area": ("Misc", "Misc"),
        "description": "",
        "emitter_fn": None,
    },
}


# ---------------------------------------------------------------------------
# Orchestration
# ---------------------------------------------------------------------------

def build_registry(repo: Path) -> list[Builtin]:
    """Build the full list of builtins from the registry + language constructs."""
    src = repo / "src"
    dispatch = src / "codegen_ir" / "lower_inst" / "builtins.rs"
    lowering_dir = src / "codegen_ir" / "lower_inst" / "builtins"

    gen = run_gen_builtins(repo)
    home_map = build_home_lowering_map(repo)

    file_cache: dict[Path, str] = {}

    def read(p: Path) -> str:
        if p not in file_cache:
            file_cache[p] = p.read_text(encoding="utf-8")
        return file_cache[p]

    builtins_root = src / "builtins"

    def resolve_check_body(fn_name: str) -> str:
        """Return the body of a shared check fn ``fn <fn_name>(`` defined under src/builtins/."""
        for path in sorted(builtins_root.rglob("*.rs")):
            text = read(path)
            if f"fn {fn_name}(" in text:
                body = _extract_fn_body(text, fn_name)
                if body:
                    return body
        return ""

    builtins: list[Builtin] = []

    # --- registry builtins (PHP-visible + internal helpers) ---
    for entry in gen:
        name = entry["name"]
        canonical = name.lower()
        is_internal = bool(entry.get("internal"))
        in_catalog = not is_internal

        refine = PARAM_TYPES.get(canonical)
        params: list[Parameter] = []
        for i, p in enumerate(entry.get("params", [])):
            php_type = _normalize_type(p["type"])
            if php_type == "mixed" and refine and i < len(refine):
                better = _param_refine_type(refine[i])
                if better:
                    php_type = better
            params.append(
                Parameter(
                    name=p["name"],
                    php_type=php_type,
                    by_ref=bool(p.get("by_ref")),
                    default=_render_default(p.get("default"), bool(p.get("optional"))),
                    optional=bool(p.get("optional")),
                )
            )

        emitter_fn, module, home_rel = home_map.get(canonical, ("", "", None))

        return_type = _normalize_type(entry.get("returns", "mixed"))
        # The registry types non-scalar returns as `Mixed`; recover the precise
        # type from the home file's `check` hook when possible.
        if return_type == "mixed" and home_rel:
            precise = parse_home_check_return(read(repo / home_rel), resolve_check_body)
            if precise:
                return_type = precise
        if canonical in RETURN_TYPE_OVERRIDES:
            return_type = RETURN_TYPE_OVERRIDES[canonical]
        lowering = resolve_lowering(
            repo, read, dispatch, lowering_dir, emitter_fn, home_rel
        )

        description = DESCRIPTION_OVERRIDES.get(canonical, "")
        if not description:
            description = entry.get("summary", "") or ""
        if not description and lowering.notes:
            description = lowering.notes[0]

        if is_internal and canonical in INTERNAL_NOTES:
            lowering.notes = INTERNAL_NOTES[canonical]

        area = resolve_area(canonical, lowering, emitter_fn, module)

        builtins.append(
            Builtin(
                name=name,
                canonical_name=canonical,
                in_catalog=in_catalog,
                is_internal=is_internal,
                area=area[0],
                sub_area=area[1],
                sig=BuiltinSig(
                    params=params,
                    variadic=entry.get("variadic"),
                    return_type=return_type,
                ),
                lowering=lowering,
                description=description,
            )
        )

    # --- language constructs (checker-resident, hand-curated) ---
    for canonical, spec in LANGUAGE_CONSTRUCTS.items():
        params = [
            Parameter(
                name=pname,
                php_type=ptype,
                by_ref=by_ref,
                default=default,
                optional=optional,
            )
            for (pname, ptype, by_ref, default, optional) in spec["params"]
        ]
        emitter_fn = spec.get("emitter_fn") or ""
        lowering = resolve_lowering(repo, read, dispatch, lowering_dir, emitter_fn, None)
        description = DESCRIPTION_OVERRIDES.get(canonical, spec.get("description", ""))
        builtins.append(
            Builtin(
                name=canonical,
                canonical_name=canonical,
                in_catalog=True,
                is_internal=False,
                area=spec["area"][0],
                sub_area=spec["area"][1],
                sig=BuiltinSig(
                    params=params,
                    variadic=spec.get("variadic"),
                    return_type=spec["return_type"],
                ),
                lowering=lowering,
                description=description,
            )
        )

    # Deterministic order for reproducible JSON.
    builtins.sort(key=lambda b: b.canonical_name)
    return builtins


def main_with(repo_root: Path, out: Path) -> int:
    """Build the registry from ``repo_root`` and write the JSON registry to ``out``."""
    builtins = build_registry(repo_root)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(
        json.dumps([_builtin_to_dict(b) for b in builtins], indent=2, sort_keys=True),
        encoding="utf-8",
    )
    print(f"Wrote {len(builtins)} builtins to {out}", file=sys.stderr)
    return 0


def main() -> int:
    """CLI entry point: parse the registry and write ``builtin_registry.json``."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[3])
    parser.add_argument("--out", type=Path, default=None)
    args = parser.parse_args()
    repo = args.repo_root.resolve()
    out = (args.out or repo / "scripts" / "docs" / "builtin_registry.json").resolve()
    return main_with(repo, out)


def _builtin_to_dict(b: Builtin) -> dict:
    """Serialize a :class:`Builtin` to the JSON schema consumed by the renderer."""
    return {
        "name": b.name,
        "canonical_name": b.canonical_name,
        "slug": slug(b.name),
        "area": b.area,
        "sub_area": b.sub_area,
        "in_catalog": b.in_catalog,
        "is_internal": b.is_internal,
        "description": b.description,
        "sig": {
            "params": [
                {
                    "name": p.name,
                    "type": p.php_type,
                    "by_ref": p.by_ref,
                    "default": p.default,
                    "optional": p.optional,
                }
                for p in b.sig.params
            ],
            "variadic": b.sig.variadic,
            "return_type": b.sig.return_type,
        },
        "lowering": {
            "sig_file": b.lowering.sig_file,
            "sig_line": b.lowering.sig_line,
            "sig_arm": b.lowering.sig_arm,
            "checker_file": b.lowering.checker_file,
            "checker_line": b.lowering.checker_line,
            "codegen_file": b.lowering.codegen_file,
            "codegen_line": b.lowering.codegen_line,
            "codegen_function": b.lowering.codegen_function,
            "runtime_helpers": b.lowering.runtime_helpers,
            "notes": b.lowering.notes,
        },
    }


if __name__ == "__main__":
    sys.exit(main())
