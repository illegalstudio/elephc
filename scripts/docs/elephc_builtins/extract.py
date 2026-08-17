"""Extract builtin metadata from Elephc's shared builtin contracts and bindings.

Every PHP builtin surface is declared once in ``elephc-builtin-contract``. The
compiler's ``builtin!`` files and Magician's ``eval_builtin!`` files join their
backend-specific behavior by stable contract ID and are collected through
`inventory`. The authoritative assembled data is therefore read from the registry
itself, via the ``gen_builtins`` example (``cargo run --example gen_builtins --
--include-internal``), NOT by regex-scraping ``catalog.rs`` / ``signatures.rs``
(which the migration emptied). The exporter also attaches, per builtin, the eval
interpreter's (elephc-magician) support block sourced from the ``eval_builtin!``
registry, plus records for builtins only the eval interpreter exposes.

For each builtin we enrich the registry data with:

1. its backend-neutral lowering boundary and typed EIR runtime target,
2. its documentation area from the registry's own ``Area`` metadata plus the
   smaller user-facing category overrides,
3. optional type-precision refinements for non-scalar params/returns that the
   registry represents coarsely as ``Mixed`` (``PARAM_TYPES`` / ``RETURN_TYPE_OVERRIDES``).

Language constructs, dedicated syntax, injected preludes, and eval-only surfaces
come from the same shared contract export as ordinary registry builtins. Their
compiler lowering route remains backend-specific metadata, but their signature is
never reconstructed in Python.

The output is a list of :class:`registry.Builtin` written to a JSON file in
``scripts/docs/builtin_registry.json``.
"""

from __future__ import annotations

import argparse
import ast
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Optional

# Make ``registry`` importable when running this file directly.
sys.path.insert(0, str(Path(__file__).resolve().parent))

from registry import (  # noqa: E402  (sys.path tweak above)
    AREA_BY_NAME,
    Builtin,
    BuiltinSig,
    DESCRIPTION_OVERRIDES,
    INTERNAL_NOTES,
    LoweringInfo,
    PARAM_TYPES,
    Parameter,
    RETURN_TYPE_OVERRIDES,
    REGISTRY_AREA_DEFAULTS,
    REGISTRY_AREA_OVERRIDES,
    RUNTIME_HELPER_OVERRIDES,
    slug,
)


# ---------------------------------------------------------------------------
# Registry source of truth: the `gen_builtins` binary
# ---------------------------------------------------------------------------

def run_gen_builtins(repo: Path) -> list[dict]:
    """Return the registry as a list of dicts by invoking the `gen_builtins` example.

    Includes `internal` builtins (the docs pipeline renders compiler-internals
    pages for the `__elephc_*` helpers) and per-builtin eval-interpreter support
    blocks. Prefers a prebuilt binary under ``target/{release,debug}/examples/``
    when present (fast path for CI, which builds it first); otherwise falls back
    to ``cargo run``.

    THE CANONICAL DOCUMENTATION CONFIGURATION IS ``--features curl`` (the root
    package's relay; see ``Cargo.toml``). The PHP-visible ``curl_*`` contracts live
    in ``elephc-builtin-contract``'s feature-gated ``catalog_curl`` module and
    Magician's matching ``eval_builtin!`` homes behind its own ``curl`` feature, so
    a default-feature exporter simply cannot see that surface — it would silently
    emit a catalog thirty-four functions short. The committed registry and pages are
    generated feature-on, and :func:`_require_canonical_configuration` below refuses
    to continue against a default-feature build rather than let a stale prebuilt
    binary regenerate a different, smaller catalog.
    """
    cmd: list[str]
    source_inputs = [repo / "Cargo.toml", repo / "Cargo.lock", repo / "tools" / "gen_builtins.rs"]
    source_inputs.extend((repo / "crates").rglob("Cargo.toml"))
    source_inputs.extend((repo / "crates").rglob("build.rs"))
    if (repo / "build.rs").exists():
        source_inputs.append(repo / "build.rs")
    source_inputs.extend((repo / "src").rglob("*.rs"))
    source_inputs.extend((repo / "crates" / "elephc-builtin-contract").rglob("*.rs"))
    source_inputs.extend((repo / "crates" / "elephc-magician" / "src").rglob("*.rs"))
    newest_source_mtime = max(path.stat().st_mtime for path in source_inputs if path.exists())
    for profile in ("release", "debug"):
        exe = repo / "target" / profile / "examples" / "gen_builtins"
        if exe.exists() and exe.stat().st_mtime >= newest_source_mtime:
            cmd = [str(exe), "--include-internal"]
            break
    else:
        cmd = [
            "cargo", "run", "--quiet", "--features", "curl", "--example", "gen_builtins", "--",
            "--include-internal",
        ]
    proc = subprocess.run(cmd, cwd=repo, capture_output=True, text=True)
    if proc.returncode != 0:
        sys.exit(
            "gen_builtins failed "
            "(build it with `cargo build --example gen_builtins --features curl`):\n"
            + proc.stderr
        )
    try:
        entries = json.loads(proc.stdout)
    except json.JSONDecodeError as exc:  # pragma: no cover - defensive
        sys.exit(f"gen_builtins produced invalid JSON: {exc}")
    _require_canonical_configuration(entries)
    return entries


# Every PHP-visible surface the canonical documentation configuration must contain,
# as (predicate name, matcher, expected count). Feature-gated catalog slices are the
# only way the exporter can come back short without failing outright, so each one is
# pinned here: a mismatch means the exporter was built in the wrong configuration.
_REQUIRED_SURFACES = (
    ("PHP-visible curl_* (`--features curl`)", lambda entry: entry["name"].startswith("curl_"), 34),
)


def _require_canonical_configuration(entries: list[dict]) -> None:
    """Fail unless the exporter was built in the canonical documentation configuration."""
    for label, matches, expected in _REQUIRED_SURFACES:
        found = sum(1 for entry in entries if matches(entry))
        if found != expected:
            sys.exit(
                f"gen_builtins exported {found} {label} entries, expected {expected}.\n"
                "The committed docs are generated in ONE canonical configuration; rebuild\n"
                "the exporter with `cargo build --example gen_builtins --features curl`\n"
                "(a stale default-feature binary under target/*/examples/ is the usual cause)."
            )


# ---------------------------------------------------------------------------
# Home-file map: name -> its single-source registry declaration
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

_CONTRACT_RE = re.compile(r'contract:\s*"([^"]+)"')


def build_home_file_map(repo: Path) -> dict[str, str]:
    """Map each registry builtin name to its single-source home file.

    Scans every builtin home file under ``src/builtins/`` (skipping the registry
    machinery files) and reads its ``builtin!`` shared-contract key. Backend
    lowering metadata comes from the exported semantic descriptor, never from
    a Rust emitter path.
    """
    out: dict[str, str] = {}
    builtins_root = repo / "src" / "builtins"
    for path in builtins_root.rglob("*.rs"):
        if path.name in _NON_HOME_FILES:
            continue
        text = path.read_text(encoding="utf-8")
        if "builtin!" not in text:
            continue
        contract_match = _CONTRACT_RE.search(text)
        if not contract_match:
            continue
        canonical = contract_match.group(1).lower()
        out[canonical] = str(path.relative_to(repo))
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


def resolve_registry_lowering(repo: Path, read, entry: dict, sig_file: str) -> LoweringInfo:
    """Describe a registry builtin's backend-neutral EIR lowering boundary."""
    semantics = entry.get("semantics") or {}
    strategy = semantics.get("target_strategy", "unknown")
    lowering_kind = semantics.get("lowering") or {}
    boundary_file = repo / "src" / "builtins" / "semantics.rs"
    definition = find_lowering_function_def(read(boundary_file), "lower_registry_call")
    boundary_line = definition[1] if definition else None
    notes = [
        f"Uses the `{strategy}` strategy from the single-source builtin descriptor.",
    ]
    if lowering_kind.get("kind") == "runtime_call":
        target = lowering_kind.get("target", "unknown")
        notes.extend(
            [
                f"Emits the typed EIR target `runtime.{target}` through `BuiltinLoweringContext`.",
                "The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.",
            ]
        )
    else:
        notes.append(
            "Emits backend-neutral EIR primitives or a small EIR graph through `BuiltinLoweringContext`."
        )
    return LoweringInfo(
        sig_file=sig_file,
        codegen_file=str(boundary_file.relative_to(repo)),
        codegen_line=boundary_line,
        codegen_function="lower_registry_call",
        notes=notes,
    )


def resolve_registry_area(canonical: str, registry_area: str) -> tuple[str, str]:
    """Resolve the stable docs category from registry metadata and explicit PHP families."""
    if canonical in REGISTRY_AREA_OVERRIDES:
        return REGISTRY_AREA_OVERRIDES[canonical]
    if canonical in AREA_BY_NAME:
        return AREA_BY_NAME[canonical]
    try:
        return REGISTRY_AREA_DEFAULTS[registry_area]
    except KeyError as exc:
        raise ValueError(
            f"builtin {canonical!r} has undocumented registry area {registry_area!r}"
        ) from exc


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


NON_REGISTRY_LOWERING_FUNCTIONS: dict[str, str] = {
    "empty": "lower_empty",
    "isset": "lower_isset",
    "unset": "lower_unset_builtin",
}


def validate_presentation_overrides(repo: Path, entries: list[dict]) -> None:
    """Reject dead, oversized, or duplicate Python presentation overrides."""
    by_name = {entry["name"].lower(): entry for entry in entries}
    tables = {
        "AREA_BY_NAME": AREA_BY_NAME,
        "REGISTRY_AREA_OVERRIDES": REGISTRY_AREA_OVERRIDES,
        "PARAM_TYPES": PARAM_TYPES,
        "RETURN_TYPE_OVERRIDES": RETURN_TYPE_OVERRIDES,
        "RUNTIME_HELPER_OVERRIDES": RUNTIME_HELPER_OVERRIDES,
        "DESCRIPTION_OVERRIDES": DESCRIPTION_OVERRIDES,
        "INTERNAL_NOTES": INTERNAL_NOTES,
    }
    errors: list[str] = []
    for table_name, table in tables.items():
        missing = sorted(set(table) - set(by_name))
        if missing:
            errors.append(f"{table_name} references unknown contracts: {', '.join(missing)}")

    for name, refinements in PARAM_TYPES.items():
        entry = by_name.get(name)
        if entry is None:
            continue
        signature = entry.get("aot") or entry
        param_count = len(signature.get("params") or [])
        if len(refinements) > param_count:
            errors.append(
                f"PARAM_TYPES[{name!r}] has {len(refinements)} entries for "
                f"{param_count} effective AOT parameters"
            )

    registry_file = repo / "scripts" / "docs" / "elephc_builtins" / "registry.py"
    tree = ast.parse(registry_file.read_text(encoding="utf-8"), filename=str(registry_file))
    checked_tables = set(tables)
    for node in tree.body:
        if not isinstance(node, (ast.Assign, ast.AnnAssign)):
            continue
        target = node.target if isinstance(node, ast.AnnAssign) else node.targets[0]
        if not isinstance(target, ast.Name) or target.id not in checked_tables:
            continue
        if not isinstance(node.value, ast.Dict):
            errors.append(f"{target.id} must remain a literal dictionary for auditing")
            continue
        keys = [
            key.value
            for key in node.value.keys
            if isinstance(key, ast.Constant) and isinstance(key.value, str)
        ]
        duplicates = sorted({key for key in keys if keys.count(key) > 1})
        if duplicates:
            errors.append(f"{target.id} repeats keys: {', '.join(duplicates)}")

    if errors:
        raise ValueError("invalid builtin documentation overrides:\n- " + "\n- ".join(errors))


# ---------------------------------------------------------------------------
# Orchestration
# ---------------------------------------------------------------------------

# Which injected prelude declares a ``prelude`` route, by the contract's own area.
# Each value is (source file under src/, human name used in the page note). The
# contract crate keeps the curl surface in its own feature-gated catalog module, so
# curl entries also carry a different `sig_file` than the always-on surfaces.
PRELUDE_SOURCES: dict[str, tuple[str, str, str]] = {
    "curl": (
        "curl_prelude.rs",
        "curl",
        "crates/elephc-builtin-contract/src/catalog_curl.rs",
    ),
}
DEFAULT_PRELUDE = ("hash_prelude.rs", "hash", "crates/elephc-builtin-contract/src/catalog_surfaces.rs")


def resolve_non_registry_lowering(
    repo: Path,
    read,
    dispatch: Path,
    lowering_dir: Path,
    canonical: str,
    aot_support: dict,
    area: str,
) -> LoweringInfo:
    """Describe a compiler route that intentionally has no ``builtin!`` home."""
    contract_file = "crates/elephc-builtin-contract/src/catalog_surfaces.rs"
    emitter_fn = NON_REGISTRY_LOWERING_FUNCTIONS.get(canonical)
    if emitter_fn:
        lowering = resolve_lowering(repo, read, dispatch, lowering_dir, emitter_fn, None)
        lowering.sig_file = contract_file
        return lowering

    lowering = LoweringInfo(sig_file=contract_file)
    kind = aot_support.get("kind")
    if kind == "prelude":
        source, label, sig_file = PRELUDE_SOURCES.get(area, DEFAULT_PRELUDE)
        lowering.sig_file = sig_file
        prelude = repo / "src" / source
        match = re.search(rf"^function\s+{re.escape(canonical)}\s*\(", read(prelude), re.MULTILINE)
        lowering.codegen_file = str(prelude.relative_to(repo))
        lowering.codegen_line = read(prelude)[: match.start()].count("\n") + 1 if match else 1
        lowering.codegen_function = canonical
        lowering.notes.append(f"Implemented by the compiler-injected {label} prelude.")
    elif kind == "language-construct":
        lowering.notes.append("Lowered through the compiler's dedicated language-construct path.")
    elif kind == "dedicated-syntax":
        lowering.notes.append("Lowered through a dedicated AST/EIR syntax node.")
    return lowering


def build_registry(repo: Path) -> list[Builtin]:
    """Build the complete catalog from shared contracts and backend bindings."""
    src = repo / "src"
    dispatch = src / "codegen" / "lower_inst" / "builtins.rs"
    lowering_dir = src / "codegen" / "lower_inst" / "builtins"

    gen = run_gen_builtins(repo)
    validate_presentation_overrides(repo, gen)
    home_map = build_home_file_map(repo)

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

    # --- shared contracts (ordinary registry entries and non-registry routes) ---
    for entry in gen:
        name = entry["name"]
        canonical = name.lower()
        aot_support = entry.get("aot") or {"supported": not entry.get("eval_only")}
        is_internal = bool(entry.get("internal"))
        in_catalog = not is_internal

        refine = PARAM_TYPES.get(canonical)
        signature = aot_support if aot_support.get("supported") else entry
        params: list[Parameter] = []
        for i, p in enumerate(signature.get("params", [])):
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

        home_rel = home_map.get(canonical)
        if aot_support.get("kind") == "registry" and home_rel is None:
            raise ValueError(f"registry builtin {canonical!r} has no single-source home file")

        return_type = _normalize_type(entry.get("returns", "mixed"))
        # The registry types non-scalar returns as `Mixed`; recover the precise
        # type from the home file's `check` hook when possible.
        if return_type == "mixed" and home_rel:
            precise = parse_home_check_return(read(repo / home_rel), resolve_check_body)
            if precise:
                return_type = precise
        if canonical in RETURN_TYPE_OVERRIDES:
            return_type = RETURN_TYPE_OVERRIDES[canonical]
        if home_rel is not None:
            lowering = resolve_registry_lowering(repo, read, entry, home_rel)
        else:
            lowering = resolve_non_registry_lowering(
                repo, read, dispatch, lowering_dir, canonical, aot_support, entry["area"]
            )
        if canonical in RUNTIME_HELPER_OVERRIDES:
            lowering.runtime_helpers = RUNTIME_HELPER_OVERRIDES[canonical]

        description = DESCRIPTION_OVERRIDES.get(canonical, "")
        if not description:
            description = entry.get("summary", "") or ""
        if not description and lowering.notes:
            description = lowering.notes[0]

        if is_internal and canonical in INTERNAL_NOTES:
            lowering.notes = INTERNAL_NOTES[canonical] + lowering.notes

        area = resolve_registry_area(canonical, entry["area"])

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
                    variadic=signature.get("variadic"),
                    return_type=return_type,
                ),
                lowering=lowering,
                description=description,
                examples=list(entry.get("examples") or []),
                eval_support=entry.get("eval"),
                aot_support=aot_support,
                eval_only=not bool(aot_support.get("supported")),
                is_extension=bool(entry.get("extension")),
                semantics=entry.get("semantics"),
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
        "is_extension": b.is_extension,
        "description": b.description,
        "examples": b.examples,
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
        "semantics": b.semantics,
        "aot": b.aot_support or {"supported": not b.eval_only, "kind": "unknown"},
        "eval": b.eval_support or {"supported": False, "kind": "none"},
        "eval_only": b.eval_only,
    }


if __name__ == "__main__":
    sys.exit(main())
