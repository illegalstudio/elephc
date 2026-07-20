#!/usr/bin/env python3
"""Inventory and audit the PHP builtin to EIR/backend boundary.

The default report is a deterministic summary of the current architecture.
Use ``--json`` for the complete per-builtin inventory and
``--enforce-target-architecture`` for the final legacy-removal gate.

The inventory is derived from the live ``builtin!``/``eval_builtin!`` registries
and source references. It is deliberately not a hand-maintained builtin list.
Once the target semantic fields exist in ``BuiltinSpec``, this audit must read
those exported fields directly and stop inferring them from legacy source shape.
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
    ops = eir_op_names()

    codegen_root = REPO / "src" / "codegen" / "lower_inst"
    codegen_sources = [(path, read(path)) for path in sorted(codegen_root.rglob("*.rs"))]
    ir_return_source = read(REPO / "src" / "ir_lower" / "expr" / "mod.rs")
    ast_effect_source = read(REPO / "src" / "optimize" / "effects" / "builtins.rs")
    ast_pure_body = function_body(ast_effect_source, "is_pure_non_throwing_builtin")
    callable_source = read(REPO / "src" / "codegen_support" / "callable_dispatch.rs")
    runtime_callable_body = function_body(callable_source, "runtime_builtin_name_supported")

    records: list[dict[str, Any]] = []
    for exported_record in sorted(registry_records, key=lambda item: item["name"].lower()):
        name = exported_record["name"]
        canonical = name.lower()
        emitter_name, _, home_relative = home_map.get(canonical, ("", "", ""))
        home_source = read(REPO / home_relative) if home_relative else ""
        block = builtin_macro_block(home_source)
        semantic_descriptor = field_value(block, "semantics")
        declared_strategy = field_value(block, "target_strategy")
        declared_target_support = field_value(block, "target_support")
        if semantic_descriptor and "unary_string_runtime" in semantic_descriptor:
            declared_strategy = "BuiltinTargetStrategy::RuntimeCall"
            declared_target_support = "BuiltinTargetSupport::All"

        docs_lowering = docs_extract.resolve_lowering(
            REPO,
            read,
            REPO / "src" / "codegen" / "lower_inst" / "builtins.rs",
            REPO / "src" / "codegen" / "lower_inst" / "builtins",
            emitter_name,
            home_relative or None,
        )
        emitter_file, emitter_line, emitter_body = find_emitter(
            emitter_name,
            docs_lowering.codegen_file,
            codegen_sources,
        )
        runtime_helpers = sorted(
            set(docs_lowering.runtime_helpers)
            | set(re.findall(r"\b__rt_[A-Za-z0-9_]+", emitter_body))
        )
        requirements = sorted(
            set(rust_string_calls(home_source, "require_builtin_library"))
            | {
                f"macos:{library}"
                for library in rust_string_calls(home_source, "require_macos_builtin_library")
            }
        )
        category, strategy, category_evidence = classify_strategy(
            name, emitter_body, runtime_helpers, requirements, ops
        )
        return_lines = [
            index
            for index, line in enumerate(ir_return_source.splitlines(), start=1)
            if f'"{canonical}"' in line
        ]
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
                    "hook": field_value(block, "check"),
                    "lazy": field_value(block, "lazy_check") == "true",
                    "semantic_descriptor": semantic_descriptor,
                },
                "result_type": {
                    "registry_checker_default": exported_record.get("returns"),
                    "checker_hook_can_override": field_value(block, "check") is not None,
                    "separate_eir_name_override_lines": return_lines,
                },
                "effects": {
                    "ast_pure_non_throwing_allowlist": f'"{canonical}"' in ast_pure_body,
                    "eir_source": "effects_lookup::builtin_effects(name) -> Op::BuiltinCall.default_effects()",
                },
                "ownership": {
                    "returns_fresh_storage": field_value(block, "returns_fresh_storage") == "true",
                    "returns_independent_storage": field_value(block, "returns_independent_storage") == "true",
                    "by_ref_param_indexes": [
                        index for index, param in enumerate(params) if param.get("by_ref")
                    ],
                },
                "lowering": {
                    "legacy_hook": field_value(block, "lower"),
                    "semantic_descriptor": semantic_descriptor,
                    "emitter_function": emitter_name or None,
                    "emitter_file": emitter_file,
                    "emitter_line": emitter_line,
                    "runtime_helpers": runtime_helpers,
                    "target_category": category,
                    "target_strategy": declared_strategy or strategy,
                    "category_evidence": category_evidence,
                },
                "requirements": requirements,
                "lookup": {
                    "case_insensitive": True,
                    "namespace_fallback": True,
                },
                "callable": {
                    "first_class_signature_from_registry": True,
                    "runtime_string_wrapper_allowlisted": f'"{canonical}"' in runtime_callable_body,
                },
                "eval": exported_record.get("eval"),
                "targets": {
                    "required": SUPPORTED_TARGETS,
                    "explicit_emitter_mentions": target_mentions(emitter_body),
                    "declared_support": declared_target_support,
                    "verified": declared_target_support == "BuiltinTargetSupport::All",
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
            "missing_legacy_emitters": sorted(
                record["name"]
                for record in records
                if record["lowering"]["legacy_hook"] is not None
                and not record["lowering"]["emitter_function"]
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
            if forbidden_builtins.search(line):
                errors.append(f"{path.relative_to(REPO)}:{line_number}: backend dependency")

    required_absences = {
        "src/ir/instr.rs": [r"\bBuiltinCall\b"],
        "src/types/signatures.rs": [r"\blegacy_builtin_call_sig\b"],
        "src/ir_lower/effects_lookup.rs": [r"fn builtin_effects\s*\("],
        "src/codegen_support/callable_dispatch.rs": [r"fn runtime_builtin_name_supported\s*\("],
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

    for record in inventory["compiler_resident"]:
        if record["kind"] == "ordinary_builtin_legacy":
            errors.append(f"{record['name']}: ordinary builtin remains outside the registry")
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
