#!/usr/bin/env python3
"""Audit Memoir's isolation, migrated owners and optional Magician archive."""
from __future__ import annotations

import argparse
from dataclasses import dataclass
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile

MEMOIR = "elephc-memoir"
MAGICIAN = "elephc-magician"
BANNED_TYPES = {"PhpType", "BuiltinId", "RuntimeFnId"}
BANNED_MODULES = {"codegen", "codegen_support", "types", "ir", "ir_lower", "pipeline"}
BANNED_ATTRIBUTES = {"link", "no_mangle", "export_name", "crate_type", "proc_macro"}
MIGRATED_OWNER_FIELDS = {
    "enum_cases": "RuntimeOwnerMap",
    "enum_case_values": "RuntimeOwnerMap",
    "constants": "RuntimeOwnerMap",
    "static_locals": "RuntimeOwnerMap",
    "static_properties": "RuntimeOwnerMap",
    "class_constants": "RuntimeOwnerMap",
    "instance_defaults": "RuntimeOwnerMap",
    "error_handlers": "RuntimeOwnerHistory",
    "exception_handlers": "RuntimeOwnerHistory",
    "pcntl_foreign_callables": "RuntimeOwnerRecordMap",
}
MIGRATED_SOURCE_CONTRACTS = {
    "crates/elephc-magician/src/context/core.rs": {
        "required": ("pending_throw: RuntimeOwnerSlot",),
    },
    "crates/elephc-magician/src/context/pcntl_runtime.rs": {
        "required": ("callback: Option<RuntimeAddressOwner>",),
    },
    "crates/elephc-magician/src/scope.rs": {
        "required": (
            "cell: RuntimeOwnerSlot",
            "RuntimeOwnerSlot::for_ownership(",
        ),
        "forbidden": ("RuntimeCellHandle::from_raw(cell.as_ptr())",),
    },
    "crates/elephc-magician/src/stream_resources/types.rs": {
        "required": ("callback_owners: [RuntimeOwnerSlot;",),
    },
    "src/codegen/lower_inst/callables/selector_tables.rs": {
        "required": ("callables::storage::emit_allocate_descriptor(",),
        "forbidden": (
            'instruction("bl __rt_heap_alloc")',
            'instruction("call __rt_heap_alloc")',
            'instruction("bl __rt_object_handle_acquire")',
            'instruction("call __rt_object_handle_acquire")',
        ),
    },
    "src/codegen_support/runtime/callables/closure_copy.rs": {
        "required": ("emit_allocate_initialized_descriptor_from_result_size(",),
        "forbidden": (
            'instruction("bl __rt_heap_alloc")',
            'instruction("call __rt_heap_alloc")',
            'instruction("bl __rt_object_handle_acquire")',
            'instruction("call __rt_object_handle_acquire")',
        ),
    },
    "src/codegen_support/runtime/exceptions/initialize.rs": {
        "required": ("throwable_storage::emit_initialize(",),
    },
    "src/codegen_support/runtime/exceptions/chain.rs": {
        "required": ("__rt_throwable_append_previous",),
    },
    "src/codegen_support/runtime/exceptions/destructor_throw.rs": {
        "required": ("__rt_throwable_take_boxed",),
        "forbidden": (
            'instruction("bl __rt_mixed_unbox")',
            'instruction("call __rt_mixed_unbox")',
            'instruction("bl __rt_decref_mixed")',
            'instruction("call __rt_decref_mixed")',
        ),
    },
    "src/codegen_support/wrappers/callback/descriptor.rs": {
        "required": ("callables::argument_storage::emit_allocate_mixed_array(",),
    },
    "src/codegen_support/wrappers/fiber.rs": {
        "required": ("callables::argument_storage::emit_allocate_mixed_array(",),
    },
    "src/codegen_support/runtime/warning_dispatch.rs": {
        "required": ("callables::argument_storage::emit_allocate_mixed_array(",),
    },
    "src/codegen_support/runtime/exceptions/throw_current.rs": {
        "required": (
            "callables::argument_storage::emit_allocate_mixed_array(",
            "callables::argument_storage::emit_append_owned_cell_from_registers(",
            "callables::argument_storage::emit_box_owned_container(",
        ),
    },
    "src/codegen_support/callable_invoker_args.rs": {
        "required": (
            "values::array_header::emit_load_value_type_tag(",
            "callables::argument_storage::emit_box_owned_container(",
        ),
    },
    "src/codegen/lower_inst/callables/scope_probe.rs": {
        "required": ("values::array_header::emit_load_value_type_tag(",),
    },
    "src/codegen/lower_inst/builtins/isset.rs": {
        "required": (
            "values::array_header::emit_load_storage_kind(",
            "values::array_header::emit_load_value_type_tag(",
        ),
    },
    "src/codegen_support/runtime/zval/zval_pack_array_packed.rs": {
        "required": ("values::array_header::emit_load_value_type_tag(",),
    },
    "src/codegen_support/abi/symbols.rs": {
        "required": (
            "symbol_owners::emit_replace_result_owner(",
            "symbol_owners::emit_publish_owner(",
            "emit_drain_symbol_owner(",
        ),
    },
    "src/codegen/context.rs": {
        "required": ("symbol_owners::emit_publish_owner(",),
    },
    "src/codegen/lower_inst/local_stores.rs": {
        "required": (
            "symbol_owners::emit_publish_owner(",
            "symbol_owners::emit_publish_address_owner(",
        ),
    },
    "src/codegen/lower_inst/objects/property_stores.rs": {
        "required": ("symbol_owners::emit_publish_address_owner(",),
        "forbidden": ("release_previous_property_value(",),
    },
    "src/codegen/eval_property_helpers/replacement.rs": {
        "required": ("symbol_owners::emit_publish_owner(",),
    },
    "src/codegen/lower_inst/builtins/eval/scope_reload.rs": {
        "required": ("symbol_owners::emit_publish_staged_owner(",),
    },
    "src/codegen/frame.rs": {
        "required": ("abi::emit_drain_static_owner(",),
        "forbidden": ("emit_static_symbol_value_cleanup(",),
    },
    "src/codegen/web.rs": {
        "required": ("abi::emit_drain_static_owner(",),
        "forbidden": ("emit_release_symbol_value(",),
    },
    "src/codegen/web/application/shutdown.rs": {
        "required": ("abi::emit_drain_static_owner(",),
    },
    "src/codegen/eval_constructor_helpers.rs": {
        "required": (
            "throwable_storage::emit_store_message_pair(",
            "throwable_storage::emit_store_code(",
            "throwable_storage::emit_store_previous(",
        ),
    },
    "src/codegen_support/runtime/memoir/resources.rs": {
        "required": ("resource_weak::emit_reuse_registered_cell(",),
        "forbidden": ("CELL_OFFSET", "__rt_incref"),
    },
    "crates/elephc-magician/src/interpreter/scope_cells.rs": {
        "required": ("finish_owner_cleanup(", "owners.release("),
    },
    "crates/elephc-magician/src/interpreter/output_handlers.rs": {
        "required": ("finish_owner_cleanup(", "release_in_order("),
    },
    "src/codegen/eval_static_property_helpers.rs": {
        "required": ("symbol_owners::emit_replace_result_owner(",),
    },
    "src/codegen_support/runtime/strings/bin2hex_len.rs": {
        "required": ("memory::emit_branch_if_size_exceeds_heap(",),
        "forbidden": ("_heap_max",),
    },
    "src/codegen_support/runtime/strings/chunk_split_len.rs": {
        "required": ("memory::emit_branch_if_size_exceeds_heap(",),
        "forbidden": ("_heap_max",),
    },
    "src/codegen_support/runtime/exceptions/protected.rs": {
        "required": (
            "activation_records::emit_save_gc_release_suppression(",
            "activation_records::emit_restore_gc_release_suppression(",
        ),
        "forbidden": ("_gc_release_suppressed",),
    },
    "src/codegen_support/runtime/objects/dump_dynamic_props.rs": {
        "required": ("values::object_storage::emit_load_trailing_word(",),
    },
    "src/codegen_support/runtime/eval_bridge/aarch64_arrays.rs": {
        "required": ("values::object_storage::emit_load_trailing_word(",),
    },
    "src/codegen_support/runtime/eval_bridge/x86_64_arrays.rs": {
        "required": ("values::object_storage::emit_load_trailing_word(",),
    },
}


@dataclass(frozen=True)
class Finding:
    """One actionable boundary violation with a stable category."""
    code: str
    location: str
    message: str


def audit_metadata(metadata: dict, root: Path) -> list[Finding]:
    """Validate all workspace production edges, including aliases and target cfgs."""
    findings: list[Finding] = []
    root = root.resolve()
    if Path(metadata["workspace_root"]).resolve() != root:
        return [Finding("workspace", str(root), "Cargo metadata belongs to another workspace")]
    members = set(metadata["workspace_members"])
    packages = [p for p in metadata["packages"] if p["id"] in members]
    expected = root / "crates/elephc-memoir/Cargo.toml"
    matches = [p for p in packages if p["name"] == MEMOIR and Path(p["manifest_path"]).resolve() == expected]
    if len(matches) != 1:
        return [Finding("package", str(expected), "expected one local Memoir workspace package")]
    memoir = matches[0]
    loc = str(expected.relative_to(root))
    dependencies = memoir["dependencies"]
    if (len(dependencies) != 1 or dependencies[0]["name"] != "libc"
            or not dependencies[0].get("optional")
            or any(dependencies[0].get(key) is not None for key in ("kind", "target", "rename", "path"))):
        findings.append(Finding("dependency", loc, "Memoir may depend only on optional registry libc for the runtime feature"))
    if memoir["features"] != {"default": [], "codegen": [], "runtime": ["dep:libc"]}:
        findings.append(Finding("features", loc, "Memoir must isolate dependency-free codegen from the opt-in libc runtime"))
    if memoir.get("links") is not None:
        findings.append(Finding("native-link", loc, "Memoir must not declare a native links library"))
    library = [t for t in memoir["targets"] if Path(t["src_path"]).resolve() == expected.parent / "src/lib.rs"]
    if (len(library) != 1 or library[0]["crate_types"] != ["rlib"]
            or any(set(t["crate_types"]) - {"rlib", "bin"} for t in memoir["targets"])):
        findings.append(Finding("crate-type", loc, "Memoir must produce only an ordinary rlib, never a native library"))
    if any(set(t["kind"]) & {"custom-build", "bin"} for t in memoir["targets"]):
        findings.append(Finding("native-build", loc, "Memoir must not have a build script or binary target"))
    compiler = [p for p in packages if Path(p["manifest_path"]).resolve() == root / "Cargo.toml"]
    if len(compiler) != 1:
        return findings + [Finding("compiler", "Cargo.toml", "expected one root compiler package")]
    compiler = compiler[0]
    magician_manifest = root / "crates" / MAGICIAN / "Cargo.toml"
    magicians = [p for p in packages if p["name"] == MAGICIAN
                 and Path(p["manifest_path"]).resolve() == magician_manifest]
    if len(magicians) != 1:
        return findings + [Finding("magician-link", str(magician_manifest), "expected one local Magician workspace package")]
    consumers = {compiler["id"]: ("codegen", "compiler-link"), magicians[0]["id"]: ("runtime", "magician-link")}
    by_path = {Path(p["manifest_path"]).resolve().parent: p["id"] for p in packages}
    by_name = {p["name"]: p["id"] for p in packages}
    graph: dict[str, list[str]] = {}
    for package in packages:
        edges = []
        direct_memoir = []
        for dependency in package["dependencies"]:
            if dependency.get("kind") is not None:
                continue  # Build/dev consumers are host tools, not generated-program dependencies.
            path = dependency.get("path")
            target = by_path.get(Path(path).resolve()) if path else by_name.get(dependency["name"])
            # Cargo's name is canonical even when rename changes the Rust dependency identifier.
            if dependency["name"] == MEMOIR:
                target = memoir["id"]
            if target:
                edges.append(target)
            if target == memoir["id"]:
                direct_memoir.append(dependency)
        graph[package["id"]] = edges
        if package["id"] in consumers:
            feature, code = consumers[package["id"]]
            if (len(direct_memoir) != 1 or direct_memoir[0].get("optional")
                    or direct_memoir[0].get("target") is not None
                    or Path(direct_memoir[0].get("path") or ".").resolve() != expected.parent
                    or direct_memoir[0].get("uses_default_features") is not False
                    or direct_memoir[0].get("features") != [feature]):
                manifest = str(Path(package["manifest_path"]).relative_to(root))
                findings.append(Finding(code, manifest, f"expected a direct local Memoir dependency with only {feature} enabled"))

    def reaches_memoir(start: str) -> bool:
        """Handle cycles without hiding indirect production consumers."""
        pending = list(graph.get(start, []))
        visited = set()
        while pending:
            item = pending.pop()
            if item == memoir["id"]:
                return True
            if item not in visited:
                visited.add(item)
                pending.extend(graph.get(item, []))
        return False

    for package in packages:
        if package["id"] != memoir["id"] and package["id"] not in consumers and reaches_memoir(package["id"]):
            findings.append(Finding("runtime-consumer", package["name"], "production dependency reaches Memoir outside the compiler and Magician"))
    return findings


@dataclass(frozen=True)
class Token:
    """An identifier or punctuation token outside Rust comments and literals."""
    text: str
    line: int


def rust_tokens(source: str) -> list[Token]:
    """Skip nested comments, escaped/raw strings, chars and lifetime punctuation."""
    tokens = []
    i = 0
    line = 1
    while i < len(source):
        start = i
        if source[i].isspace():
            line += source[i] == "\n"
            i += 1
            continue
        if source.startswith("//", i):
            end = source.find("\n", i)
            i = len(source) if end < 0 else end
            continue
        if source.startswith("/*", i):
            depth = 1
            i += 2
            while depth and i < len(source):
                if source.startswith("/*", i):
                    depth += 1
                    i += 2
                elif source.startswith("*/", i):
                    depth -= 1
                    i += 2
                else:
                    i += 1
            if depth:
                raise ValueError("unterminated Rust block comment")
            line += source[start:i].count("\n")
            continue
        raw = re.match(r'(?:br|cr|r)(#*)"', source[i:])
        if raw:
            terminator = '"' + raw.group(1)
            end = source.find(terminator, i + raw.end())
            if end < 0:
                raise ValueError("unterminated Rust raw string")
            i = end + len(terminator)
            line += source[start:i].count("\n")
            continue
        prefix = 1 if source.startswith(('b"', 'c"'), i) else 0
        if source[i + prefix:i + prefix + 1] == '"':
            i += prefix + 1
            while i < len(source):
                if source[i] == "\\":
                    i += 2
                elif source[i] == '"':
                    i += 1
                    break
                else:
                    i += 1
            else:
                raise ValueError("unterminated Rust string")
            line += source[start:i].count("\n")
            continue
        char_start = i + 1 if source.startswith("b'", i) else i
        if source[char_start:char_start + 1] == "'":
            char_end = char_start + 2
            if source[char_start + 1:char_start + 2] == "\\":
                char_end = char_start + 3
                if source[char_start + 2:char_start + 4] == "u{":
                    close = source.find("}", char_start + 4)
                    char_end = close + 1 if close >= 0 else len(source)
                elif source[char_start + 2:char_start + 3] == "x":
                    char_end = char_start + 5
            if source[char_end:char_end + 1] == "'":
                i = char_end + 1
                continue
            # A lifetime is punctuation followed by an identifier, not a character literal.
        ident = re.match(r'(?:r#)?([A-Za-z_][A-Za-z0-9_]*)', source[i:])
        if ident:
            tokens.append(Token(ident.group(1), line))
            i += ident.end()
            continue
        if source.startswith("::", i):
            tokens.append(Token("::", line))
            i += 2
        else:
            tokens.append(Token(source[i], line))
            i += 1
    return tokens


def audit_source(location: str, source: str) -> list[Finding]:
    """Reject compiler semantic types and native-export attributes in real Rust code."""
    try:
        tokens = rust_tokens(source)
    except ValueError as error:
        return [Finding("rust-source", location, str(error))]
    findings = []
    for index, token in enumerate(tokens):
        where = f"{location}:{token.line}"
        if token.text in BANNED_TYPES:
            findings.append(Finding("php-eir", where, f"{token.text} belongs to compiler semantics"))
        if (token.text == "crate" and index + 2 < len(tokens)
                and tokens[index + 1].text == "::" and tokens[index + 2].text in BANNED_MODULES):
            findings.append(Finding("compiler-module", where, "compiler modules must not be copied into Memoir"))
        if token.text == "#":
            cursor = index + 1
            if cursor < len(tokens) and tokens[cursor].text == "!":
                cursor += 1
            if cursor < len(tokens) and tokens[cursor].text == "[":
                depth = 1
                cursor += 1
                while cursor < len(tokens) and depth:
                    item = tokens[cursor]
                    depth += (item.text == "[") - (item.text == "]")
                    if item.text in BANNED_ATTRIBUTES:
                        findings.append(Finding("native-export", where, f"attribute {item.text} would expose a separate native ABI"))
                    cursor += 1
    unsafe_policy = {"lib.rs": "deny", "codegen_root.rs": "forbid"}
    policy = unsafe_policy.get(Path(location).name)
    if policy:
        required = ["#", "!", "[", policy, "(", "unsafe_code", ")", "]"]
        depth = 0
        unconditional = False
        for index, token in enumerate(tokens):
            if depth == 0 and [t.text for t in tokens[index:index + len(required)]] == required:
                unconditional = True
            depth += (token.text == "{") - (token.text == "}")
        if not unconditional:
            findings.append(Finding("unsafe-policy", location, f"expected unconditional {policy}(unsafe_code)"))
    return findings


def audit_migrated_owner_fields(location: str, source: str) -> list[Finding]:
    """Keep persistent Magician cell inventories on their Memoir owner protocols."""
    try:
        tokens = rust_tokens(source)
    except ValueError as error:
        return [Finding("rust-source", location, str(error))]
    findings = []
    for field, expected in MIGRATED_OWNER_FIELDS.items():
        declarations = [index for index, token in enumerate(tokens[:-2])
                        if token.text == field and tokens[index + 1].text == ":"]
        if not declarations:
            findings.append(Finding("owner-field", location,
                                    f"migrated owner field {field} is missing"))
            continue
        actual = tokens[declarations[0] + 2]
        if actual.text != expected:
            findings.append(Finding(
                "owner-container", f"{location}:{actual.line}",
                f"{field} must use Memoir {expected}, not {actual.text}",
            ))
    return findings


def audit_migrated_source_contract(
    location: str,
    source: str,
    contract: dict[str, tuple[str, ...]],
) -> list[Finding]:
    """Keep migrated consumers on Memoir APIs instead of restoring physical policy locally."""
    findings = []
    for required in contract.get("required", ()):
        if required not in source:
            findings.append(Finding(
                "memoir-delegation",
                location,
                f"migrated consumer must delegate through {required}",
            ))
    for forbidden in contract.get("forbidden", ()):
        if forbidden in source:
            findings.append(Finding(
                "consumer-policy",
                location,
                f"migrated consumer must not own physical memory policy {forbidden}",
            ))
    return findings


def audit_archive_symbols(location: str, symbols: str) -> list[Finding]:
    """Require the eval registry and reject compiler emitter definitions in Magician."""
    memoir_symbols = [line for line in symbols.splitlines() if "elephc_memoir" in line]
    findings = []
    for symbol in ("allocate", "resolve", "release"):
        # Legacy and v0 Rust mangling both retain the length-prefixed function path.
        path = f"13elephc_memoir12eval_buffers{len(symbol)}{symbol}"
        if not any(path in line for line in memoir_symbols):
            findings.append(Finding("archive-runtime", location, f"Memoir eval buffer {symbol} is missing"))
    if any("codegen_root" in line for line in memoir_symbols):
        findings.append(Finding("archive-codegen", location, "Magician contains Memoir compiler emitter symbols"))
    return findings


def audit_magician_archive(archive: Path) -> list[Finding]:
    """Inspect Memoir's objects without asking nm to parse unrelated archive members."""
    result = subprocess.run(["ar", "t", str(archive)], check=True, capture_output=True, text=True)
    memoir_members = [name for name in result.stdout.splitlines()
                      if name.startswith("elephc_memoir-") and name.endswith(".o")]
    symbols = ""
    if memoir_members:
        with tempfile.TemporaryDirectory(prefix="elephc-memoir-symbols-") as directory:
            objects = []
            for index, name in enumerate(memoir_members):
                result = subprocess.run(["ar", "p", str(archive), name], check=True, capture_output=True)
                obj = Path(directory) / f"{index}.o"
                obj.write_bytes(result.stdout)
                objects.append(str(obj))
            result = subprocess.run(["nm", "-gU", *objects], check=True, capture_output=True, text=True)
            symbols = result.stdout
    return audit_archive_symbols(str(archive), symbols)


def main() -> int:
    """Audit this checkout with Cargo's authoritative interpretation of its manifests."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--magician-archive", type=Path, action="append", default=[],
                        help="inspect a built Magician static archive for Memoir symbols")
    args = parser.parse_args()
    root = args.root.resolve()
    try:
        result = subprocess.run(["cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"],
                                cwd=root, check=True, capture_output=True, text=True)
        metadata = json.loads(result.stdout)
        findings = audit_metadata(metadata, root)
        for path in sorted((root / "crates/elephc-memoir/src").rglob("*.rs")):
            findings.extend(audit_source(path.relative_to(root).as_posix(), path.read_text()))
        owner_context = root / "crates/elephc-magician/src/context/core.rs"
        findings.extend(audit_migrated_owner_fields(
            owner_context.relative_to(root).as_posix(), owner_context.read_text()))
        for location, contract in MIGRATED_SOURCE_CONTRACTS.items():
            path = root / location
            if not path.is_file():
                findings.append(Finding(
                    "memoir-delegation", location, "migrated consumer source is missing"))
                continue
            findings.extend(audit_migrated_source_contract(
                location, path.read_text(), contract))
        for archive in args.magician_archive:
            findings.extend(audit_magician_archive(archive))
    except subprocess.CalledProcessError as error:
        print(f"Memoir boundary audit could not complete: {error}", file=sys.stderr)
        stderr = error.stderr
        if isinstance(stderr, bytes):
            stderr = stderr.decode(errors="replace")
        if stderr:
            print(stderr.rstrip(), file=sys.stderr)
        return 2
    except (OSError, ValueError, KeyError) as error:
        print(f"Memoir boundary audit could not complete: {error}", file=sys.stderr)
        return 2
    for finding in findings:
        print(f"{finding.location}: [{finding.code}] {finding.message}", file=sys.stderr)
    if findings:
        return 1
    print("Memoir boundary: isolated compiler codegen and Magician runtime features; no separate native ABI.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
