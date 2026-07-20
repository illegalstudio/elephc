#!/usr/bin/env python3
"""Inventory and audit the PHP builtin to EIR/backend boundary.

The default report is a deterministic summary of the current architecture.
Use ``--json`` for the complete per-builtin inventory and
``--enforce-target-architecture`` for the final legacy-removal gate.

The inventory is derived from the live ``builtin!``/``eval_builtin!`` registries,
their exported semantic metadata, and source references. It is deliberately not
a hand-maintained builtin list.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from collections import Counter
from pathlib import Path
from typing import Any, Iterable

REPO = Path(__file__).resolve().parents[1]
DOCS_LIB = REPO / "scripts" / "docs" / "elephc_builtins"
sys.path.insert(0, str(DOCS_LIB))

import extract as docs_extract  # noqa: E402

SUPPORTED_TARGETS = ["macos-aarch64", "linux-aarch64", "linux-x86_64"]
LANGUAGE_CONSTRUCTS = {"buffer_new", "die", "empty", "exit", "isset", "unset"}
SOURCE_SUFFIXES = {".php", ".rs", ".snap"}


def read(path: Path) -> str:
    """Read a repository text file as UTF-8."""
    return path.read_text(encoding="utf-8")


def function_body(source: str, name: str) -> str:
    """Return a brace-balanced Rust function body, including its braces."""
    pattern = re.compile(
        rf"(?:pub(?:\([^)]*\))?\s+)?(?:unsafe\s+)?fn\s+{re.escape(name)}\s*\("
    )
    match = pattern.search(source)
    if match is None:
        return ""
    brace = source.find("{", match.end())
    if brace < 0:
        return ""
    depth = 0
    for index in range(brace, len(source)):
        char = source[index]
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return source[brace : index + 1]
    return ""


def builtin_macro_block(source: str) -> str:
    """Return the first brace-balanced ``builtin!`` declaration in a home file."""
    match = re.search(r"(?m)^builtin!\s*\{", source)
    if match is None:
        return ""
    brace = source.find("{", match.start())
    if brace < 0:
        return ""
    depth = 0
    for index in range(brace, len(source)):
        char = source[index]
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return source[brace : index + 1]
    return ""


def field_value(block: str, field: str) -> str | None:
    """Extract a simple comma-terminated field value from a builtin macro block."""
    match = re.search(rf"(?m)^\s*{re.escape(field)}:\s*([^,\n]+)", block)
    return match.group(1).strip() if match else None


def rust_string_calls(source: str, function: str) -> list[str]:
    """Return sorted string literal arguments passed to a named Rust function."""
    pattern = re.compile(rf"\b{re.escape(function)}\s*\(\s*\"([^\"]+)\"\s*\)")
    return sorted(set(pattern.findall(source)))


def source_files_with_tests() -> list[tuple[str, str]]:
    """Load integration fixtures and Rust modules that contain test code."""
    paths: set[Path] = set()
    for root in (REPO / "tests", REPO / "crates" / "elephc-magician" / "src"):
        if root.exists():
            paths.update(path for path in root.rglob("*") if path.suffix in SOURCE_SUFFIXES)
    for path in (REPO / "src").rglob("*.rs"):
        source = read(path)
        if "#[test]" in source or "#[cfg(test)]" in source:
            paths.add(path)
    return [(str(path.relative_to(REPO)), read(path)) for path in sorted(paths)]


def build_test_index(
    names: Iterable[str], sources: Iterable[tuple[str, str]]
) -> dict[str, list[str]]:
    """Index call-like and quoted builtin mentions with one pass over test sources."""
    canonical_names = {name.lower() for name in names}
    index: dict[str, set[str]] = {name: set() for name in canonical_names}
    call_pattern = re.compile(r"(?i)(?<![A-Za-z0-9_\\])([A-Za-z_\\][A-Za-z0-9_\\]*)\s*\(")
    string_pattern = re.compile(r"[\"']([A-Za-z_\\][A-Za-z0-9_\\]*)[\"']")
    for relative, source in sources:
        mentioned = {
            token.lower().lstrip("\\")
            for token in call_pattern.findall(source) + string_pattern.findall(source)
        }
        for name in mentioned & canonical_names:
            index[name].add(relative)
    return {name: sorted(paths) for name, paths in index.items()}


def eir_op_names() -> set[str]:
    """Return normalized EIR opcode spellings for mechanical strategy classification."""
    source = read(REPO / "src" / "ir" / "instr.rs")
    match = re.search(r"pub enum Op\s*\{(.*?)\n\}", source, re.DOTALL)
    if match is None:
        return set()
    variants = re.findall(r"(?m)^\s*([A-Z][A-Za-z0-9_]*)\s*,", match.group(1))
    return {re.sub(r"[^a-z0-9]", "", variant.lower()) for variant in variants}


def classify_strategy(
    name: str,
    emitter_body: str,
    runtime_helpers: list[str],
    requirements: list[str],
    ops: set[str],
) -> tuple[int, str, str]:
    """Classify the target lowering strategy from the current implementation shape.

    This is a reproducible migration inventory, not target semantic metadata. The
    final registry descriptor replaces this inference and becomes authoritative.
    """
    normalized_name = re.sub(r"[^a-z0-9]", "", name.lower())
    conditional_markers = (
        "result_php_type",
        "value_php_type",
        "codegen_repr",
        "match ",
        "if let PhpType",
        "RuntimeCallableSelector",
    )
    if len(runtime_helpers) > 1 or any(marker in emitter_body for marker in conditional_markers):
        return (4, "conditional", "multiple helpers or type/value-dependent backend branches")
    if normalized_name in ops:
        return (1, "eir_primitive", "builtin spelling matches an existing general EIR primitive")
    if runtime_helpers or requirements:
        return (3, "typed_runtime_call", "current implementation uses runtime or bridge helpers")
    return (2, "eir_graph", "current inline lowering has no directly observed runtime helper")


def find_emitter(
    emitter_name: str,
    emitter_hint: str | None,
    codegen_sources: list[tuple[Path, str]],
) -> tuple[str | None, int | None, str]:
    """Locate one current assembly emitter and return its file, line, and full body."""
    candidates = codegen_sources
    if emitter_hint:
        hint_path = REPO / emitter_hint
        candidates = sorted(
            codegen_sources,
            key=lambda item: (item[0] != hint_path, str(item[0])),
        )
    pattern = re.compile(
        rf"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?(?:unsafe\s+)?fn\s+{re.escape(emitter_name)}\s*\("
    )
    for path, source in candidates:
        match = pattern.search(source)
        if match is None:
            continue
        line = source.count("\n", 0, match.start()) + 1
        return str(path.relative_to(REPO)), line, function_body(source, emitter_name)
    return None, None, ""


def target_mentions(source: str) -> list[str]:
    """Return explicit target/platform enum variants mentioned by an emitter."""
    variants = set(re.findall(r"(?:Target|Platform)::([A-Za-z0-9_]+)", source))
    if "Aarch64" in source:
        variants.add("Aarch64")
    if "X86_64" in source or "x86_64" in source:
        variants.add("X86_64")
    return sorted(variants)


def build_inventory() -> dict[str, Any]:
    """Build the complete deterministic registry and compiler-resident inventory."""
    exported = docs_extract.run_gen_builtins(REPO)
    registry_records = [
        record
        for record in exported
        if "eval_only" not in record and "aot_resident" not in record
    ]
    resident_records = [record for record in exported if record.get("aot_resident")]
    eval_only_records = [record for record in exported if record.get("eval_only")]
    home_map = docs_extract.build_home_lowering_map(REPO)
    all_aot_names = [record["name"] for record in registry_records + resident_records]
    test_index = build_test_index(all_aot_names, source_files_with_tests())

    records: list[dict[str, Any]] = []
    for exported_record in sorted(registry_records, key=lambda item: item["name"].lower()):
        name = exported_record["name"]
        canonical = name.lower()
        _, _, home_relative = home_map.get(canonical, ("", "", ""))
        home_source = read(REPO / home_relative) if home_relative else ""
        block = builtin_macro_block(home_source)
        semantics = exported_record.get("semantics", {})
        strategy = semantics.get("target_strategy")
        category = {
            "eir_primitive": 1,
            "eir_graph": 2,
            "runtime_call": 3,
            "conditional": 4,
        }.get(strategy, 0)
        params = exported_record.get("params", [])
        records.append(
            {
                "name": name,
                "canonical_name": canonical,
                "aliases": [],
                "area": exported_record.get("area"),
                "visibility": {
                    "extension": bool(exported_record.get("extension")),
                    "internal": bool(exported_record.get("internal")),
                },
                "home_file": home_relative or None,
                "signature": {
                    "params": params,
                    "variadic": exported_record.get("variadic"),
                    "min_args_override": exported_record.get("min_args"),
                    "max_args_override": exported_record.get("max_args"),
                    "arity_error": exported_record.get("arity_error"),
                    "by_ref_return": bool(exported_record.get("by_ref_return")),
                },
                "validation": {
                    **semantics.get("validation", {}),
                },
                "result_type": {
                    "registry_checker_default": exported_record.get("returns"),
                    "resolver": semantics.get("result_type"),
                    "separate_eir_name_override_lines": [],
                },
                "effects": semantics.get("effects"),
                "ownership": {
                    **semantics.get("ownership", {}),
                    "by_ref_param_indexes": [
                        index for index, param in enumerate(params) if param.get("by_ref")
                    ],
                },
                "lowering": {
                    "legacy_hook": field_value(block, "lower"),
                    **semantics.get("lowering", {}),
                    "target_category": category,
                    "target_strategy": strategy,
                },
                "requirements": semantics.get("requirements"),
                "lookup": {
                    "case_insensitive": True,
                    "namespace_fallback": True,
                },
                "callable": {
                    "first_class_signature_from_registry": True,
                    **semantics.get("callable", {}),
                },
                "eval": exported_record.get("eval"),
                "targets": {
                    "required": SUPPORTED_TARGETS,
                    "declared_support": semantics.get("target_support", []),
                    "verified": semantics.get("target_support", []) == SUPPORTED_TARGETS,
                },
                "tests": test_index.get(canonical, []),
            }
        )

    compiler_resident = []
    for record in sorted(resident_records, key=lambda item: item["name"].lower()):
        name = record["name"]
        is_construct = name.lower() in LANGUAGE_CONSTRUCTS
        compiler_resident.append(
            {
                "name": name,
                "kind": "language_construct" if is_construct else "ordinary_builtin_legacy",
                "target_category": 5 if is_construct else 4,
                "required_action": "remain compiler-resident" if is_construct else "migrate into registry",
                "eval": record.get("eval"),
                "tests": test_index.get(name.lower(), []),
            }
        )

    duplicate_names = sorted(
        name for name, count in Counter(record["canonical_name"] for record in records).items() if count > 1
    )
    return {
        "schema_version": 1,
        "supported_targets": SUPPORTED_TARGETS,
        "registry_builtins": records,
        "compiler_resident": compiler_resident,
        "eval_only": sorted(record["name"] for record in eval_only_records),
        "invariants": {
            "duplicate_registry_names": duplicate_names,
            "missing_home_files": sorted(record["name"] for record in records if not record["home_file"]),
            "missing_semantic_descriptors": sorted(
                record["name"] for record in records if not record["lowering"].get("kind")
            ),
        },
    }


def target_architecture_errors(inventory: dict[str, Any]) -> list[str]:
    """Return structural errors that must be absent when the migration is complete."""
    errors: list[str] = []
    forbidden_builtins = re.compile(
        r"crate::codegen|FunctionContext|CodegenIrError|crate::ir::Instruction|\bir::Instruction\b"
    )
    for path in sorted((REPO / "src" / "builtins").rglob("*.rs")):
        for line_number, line in enumerate(read(path).splitlines(), start=1):
            if not line.lstrip().startswith("//") and forbidden_builtins.search(line):
                errors.append(f"{path.relative_to(REPO)}:{line_number}: backend dependency")
            if not line.lstrip().startswith("//") and re.search(
                r"require_(?:macos_)?builtin_library\s*\(", line
            ):
                errors.append(
                    f"{path.relative_to(REPO)}:{line_number}: checker-side requirement mutation"
                )

    for path in sorted((REPO / "src").rglob("*.rs")):
        source = read(path)
        if re.search(r"\bOp::BuiltinCall\b|\bBuiltinCall\b", source):
            errors.append(f"{path.relative_to(REPO)}: opaque builtin EIR opcode remains")

    required_absences = {
        "src/ir/instr.rs": [r"\bBuiltinCall\b"],
        "src/types/signatures.rs": [r"\blegacy_builtin_call_sig\b"],
        "src/ir_lower/effects_lookup.rs": [r"fn builtin_effects\s*\("],
        "src/codegen_support/callable_dispatch.rs": [r"fn runtime_builtin_name_supported\s*\("],
        "src/codegen/lower_inst.rs": [r"match\s+php_symbol_key\s*\(\s*name"],
        "src/builtins/spec.rs": [
            r"pub\s+returns_fresh_storage\s*:",
            r"pub\s+returns_independent_storage\s*:",
            r"pub\s+check\s*:",
            r"pub\s+lazy_check\s*:",
            r"\*_builtin_return_type",
        ],
        "src/types/checker/builtins/mod.rs": [r"legacy per-area dispatch"],
        "src/types/checker/builtins/catalog.rs": [r"legacy static list", r"legacy catalog"],
        "src/ir_lower/expr/mod.rs": [
            r"fn\s+builtin_return_type_override\s*\(",
            r'"count"\s*=>\s*lower_count_args',
            r'"date"\s*=>\s*lower_date_args',
            r'"json_decode"\s*=>\s*lower_json_decode_args',
        ],
        "src/ir/runtime_call.rs": [r"enforced_arity_bounds\s*\(\s*target\.as_eir"],
    }
    for relative, patterns in required_absences.items():
        source = read(REPO / relative)
        for pattern in patterns:
            if re.search(pattern, source):
                errors.append(f"{relative}: legacy pattern still present: {pattern}")

    for record in inventory["registry_builtins"]:
        if record["lowering"]["legacy_hook"] is not None:
            errors.append(f"{record['home_file']}: {record['name']} still has a legacy lower hook")
        if not record["targets"]["verified"]:
            errors.append(f"{record['name']}: supported-target implementation is not verified")
        strategy = record["lowering"].get("target_strategy")
        lowering_kind = record["lowering"].get("kind")
        if strategy == "runtime_call" and lowering_kind != "runtime_call":
            errors.append(
                f"{record['name']}: runtime_call strategy does not use typed RuntimeCall lowering"
            )
        if strategy in {"eir_primitive", "eir_graph", "conditional"} and lowering_kind != "eir":
            errors.append(
                f"{record['name']}: {strategy} strategy does not use backend-neutral EIR lowering"
            )

    for record in inventory["compiler_resident"]:
        if record["kind"] == "ordinary_builtin_legacy":
            errors.append(f"{record['name']}: ordinary builtin remains outside the registry")

    target_source = read(REPO / "src" / "ir" / "runtime_fn.rs")
    builtin_target_variants = dict(
        re.findall(r'RuntimeFnId::([A-Za-z0-9_]+)\s*=>\s*"([^"]+)"', target_source)
    )
    builtin_variant_by_name = {name: variant for variant, name in builtin_target_variants.items()}
    builtin_backend_source = "\n".join(
        read(path)
        for path in sorted(
            (REPO / "src" / "codegen" / "lower_inst" / "runtime_functions").glob("group_*.rs")
        )
    )
    unary_source = read(REPO / "src" / "ir" / "runtime_call.rs")
    unary_variants = dict(
        re.findall(r'UnaryStringRuntime::([A-Za-z0-9_]+)\s*=>\s*"([^"]+)"', unary_source)
    )
    unary_variant_by_name = {name: variant for variant, name in unary_variants.items()}
    unary_backend_source = read(REPO / "src" / "codegen" / "lower_inst" / "runtime_calls.rs")
    builtin_semantics_source = "\n".join(
        read(path) for path in sorted((REPO / "src" / "builtins").rglob("*.rs"))
    )
    referenced_runtime_variants = set(
        re.findall(r"RuntimeFnId::([A-Za-z0-9_]+)", builtin_semantics_source)
    )
    for variant in sorted(set(builtin_target_variants) - referenced_runtime_variants):
        errors.append(f"RuntimeFnId::{variant}: runtime function is not referenced by builtin semantics")
    for variant in sorted(referenced_runtime_variants - set(builtin_target_variants)):
        errors.append(f"RuntimeFnId::{variant}: builtin semantics reference an unknown runtime function")
    for variant in sorted(referenced_runtime_variants):
        if f"RuntimeFnId::{variant}" not in builtin_backend_source:
            errors.append(f"RuntimeFnId::{variant}: runtime function has no backend implementation arm")
    for record in inventory["registry_builtins"]:
        lowering = record["lowering"]
        if lowering.get("kind") != "runtime_call":
            continue
        target = lowering.get("target")
        if target in builtin_variant_by_name:
            variant = builtin_variant_by_name[target]
            if f"RuntimeFnId::{variant}" not in builtin_backend_source:
                errors.append(f"{record['name']}: typed target {target} has no backend arm")
        elif target in unary_variant_by_name:
            variant = unary_variant_by_name[target]
            if f"UnaryStringRuntime::{variant}" not in unary_backend_source:
                errors.append(f"{record['name']}: unary runtime target {target} has no backend arm")
        else:
            errors.append(f"{record['name']}: unknown typed runtime target {target}")
    return errors


def print_summary(inventory: dict[str, Any]) -> None:
    """Print the compact deterministic baseline/current-state report."""
    records = inventory["registry_builtins"]
    categories = Counter(record["lowering"]["target_strategy"] for record in records)
    areas = Counter(record["area"] for record in records)
    print("=== Builtin to EIR boundary inventory ===")
    print(f"Registry-backed AOT builtins: {len(records)}")
    print(f"Compiler-resident AOT names:  {len(inventory['compiler_resident'])}")
    print(f"Eval-only names:              {len(inventory['eval_only'])}")
    print(f"Extension builtins:           {sum(r['visibility']['extension'] for r in records)}")
    print(f"Internal builtins:            {sum(r['visibility']['internal'] for r in records)}")
    print(f"By-reference builtins:        {sum(bool(r['ownership']['by_ref_param_indexes']) for r in records)}")
    print(f"Variadic builtins:            {sum(r['signature']['variadic'] is not None for r in records)}")
    print(f"Eval-supported AOT builtins:  {sum(bool(r['eval'].get('supported')) for r in records)}")
    print(f"Backend-dependent homes:      {sum(r['lowering']['legacy_hook'] is not None for r in records)}")
    print("Target strategy inventory:")
    for strategy, count in sorted(categories.items()):
        print(f"  {strategy:20} {count}")
    print("Registry areas:")
    for area, count in sorted(areas.items()):
        print(f"  {area:20} {count}")
    print("Structural inventory errors:")
    for key, values in inventory["invariants"].items():
        print(f"  {key:28} {len(values)}")


def main() -> int:
    """Run the requested inventory or final target-architecture audit mode."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", action="store_true", help="print the complete per-builtin JSON inventory")
    parser.add_argument(
        "--enforce-target-architecture",
        action="store_true",
        help="fail if any legacy backend boundary or unverified target remains",
    )
    args = parser.parse_args()
    inventory = build_inventory()

    structural_errors = [
        f"{key}: {', '.join(values)}"
        for key, values in inventory["invariants"].items()
        if values
    ]
    if args.json:
        print(json.dumps(inventory, indent=2, sort_keys=True))
    else:
        print_summary(inventory)

    errors = structural_errors
    if args.enforce_target_architecture:
        errors = errors + target_architecture_errors(inventory)
    if errors:
        print("", file=sys.stderr)
        print(f"Errors: {len(errors)}", file=sys.stderr)
        for error in errors[:100]:
            print(f"  - {error}", file=sys.stderr)
        if len(errors) > 100:
            print(f"  ... {len(errors) - 100} more", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
