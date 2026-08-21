#!/usr/bin/env python3
"""Generate the stable DOM bridge operation manifest and Rust lookup table."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any


ABI_VERSION = 1
FIRST_PUBLIC_OPCODE = 0x1000
MAGIC_SIMPLEXML_OPERATIONS = (
    "cast",
    "compare",
    "count",
    "get_iterator",
    "has_dimension",
    "has_property",
    "read_dimension",
    "read_property",
    "unset_dimension",
    "unset_property",
    "write_dimension",
    "write_property",
)
INTERNAL_OPERATIONS = (
    "bridge.object.clone",
    "bridge.wrapper.release",
    "bridge.wrapper.retain",
)


def canonical_json(value: Any) -> bytes:
    """Serialize one value into the byte-stable canonical JSON used for digests."""
    return json.dumps(
        value,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")


def operation_records(surface: dict[str, Any]) -> list[dict[str, Any]]:
    """Build one deterministic native-operation record per callable/property/object-handler action."""
    operations: list[dict[str, Any]] = []
    seen_classes: set[str] = set()

    for extension in surface["extensions"]:
        extension_name = extension["name"]
        for class_spec in extension["classes"]:
            canonical_name = class_spec["canonical_name"]
            canonical_key = canonical_name.lower()
            if canonical_key in seen_classes:
                continue
            seen_classes.add(canonical_key)

            for method in class_spec["methods"]:
                operations.append(
                    {
                        "key": f"method:{canonical_key}::{method['name'].lower()}",
                        "kind": "method",
                        "extension": extension_name,
                        "class": canonical_name,
                        "member": method["name"],
                        "static": method["static"],
                        "required_parameters": method["required_parameters"],
                        "parameter_count": len(method["parameters"]),
                    }
                )
            for property_spec in class_spec["properties"]:
                base = {
                    "extension": extension_name,
                    "class": canonical_name,
                    "member": property_spec["name"],
                    "static": property_spec["static"],
                    "required_parameters": 0,
                    "parameter_count": 0,
                }
                operations.append(
                    {
                        **base,
                        "key": f"property-get:{canonical_key}::${property_spec['name']}",
                        "kind": "property-get",
                    }
                )
                if property_spec["writable"]:
                    operations.append(
                        {
                            **base,
                            "key": f"property-set:{canonical_key}::${property_spec['name']}",
                            "kind": "property-set",
                            "required_parameters": 1,
                            "parameter_count": 1,
                        }
                    )

        for function in extension["functions"]:
            operations.append(
                {
                    "key": f"function:{function['exported_name'].lower()}",
                    "kind": "function",
                    "extension": extension_name,
                    "class": None,
                    "member": function["exported_name"],
                    "static": True,
                    "required_parameters": function["required_parameters"],
                    "parameter_count": len(function["parameters"]),
                }
            )

    for member in MAGIC_SIMPLEXML_OPERATIONS:
        operations.append(
            {
                "key": f"object-handler:simplexml::{member}",
                "kind": "object-handler",
                "extension": "SimpleXML",
                "class": "SimpleXMLElement",
                "member": member,
                "static": False,
                "required_parameters": 0,
                "parameter_count": 0,
            }
        )
    for key in INTERNAL_OPERATIONS:
        operations.append(
            {
                "key": f"internal:{key}",
                "kind": "internal",
                "extension": "dom",
                "class": None,
                "member": key,
                "static": True,
                "required_parameters": 0,
                "parameter_count": 0,
            }
        )

    operations.sort(key=lambda operation: operation["key"])
    for offset, operation in enumerate(operations):
        operation["opcode"] = FIRST_PUBLIC_OPCODE + offset
    return operations


def build_manifest(surface_path: Path) -> dict[str, Any]:
    """Build the complete versioned opcode manifest from the locked Reflection surface."""
    surface_bytes = surface_path.read_bytes()
    surface = json.loads(surface_bytes)
    manifest: dict[str, Any] = {
        "schema": 1,
        "abi_version": ABI_VERSION,
        "php_version": surface["php_version"],
        "libxml_version": surface["libxml_version"],
        "surface_sha256": hashlib.sha256(surface_bytes).hexdigest(),
        "first_public_opcode": FIRST_PUBLIC_OPCODE,
        "operations": operation_records(surface),
    }
    manifest["manifest_sha256"] = hashlib.sha256(canonical_json(manifest)).hexdigest()
    return manifest


def rust_source(manifest: dict[str, Any]) -> str:
    """Render the bridge's generated Rust opcode lookup table."""
    lines = [
        "//! Purpose:",
        "//! Defines stable numeric DOM bridge opcodes generated from the locked PHP surface.",
        "//! Keeps compiler and native dispatch on the same manifest digest and ABI version.",
        "//!",
        "//! Called from:",
        "//! - `crate::exports` for native operation dispatch.",
        "//!",
        "//! Key details:",
        "//! - This file is generated by `tools/php-dom/generate_opcodes.py`; do not edit it manually.",
        "",
        "/// SHA-256 of the canonical opcode manifest without this Rust rendering.",
        f'pub const MANIFEST_SHA256: &str = "{manifest["manifest_sha256"]}";',
        "",
        "/// Stable `(opcode, operation-key)` entries in numeric order.",
        "pub const OPERATIONS: &[(u32, &str)] = &[",
    ]
    for operation in manifest["operations"]:
        lines.append(f'    ({operation["opcode"]}, {json.dumps(operation["key"])}),')
    lines.extend(
        [
            "];",
            "",
            "/// Returns the stable operation key for one public opcode.",
            "pub fn operation_key(opcode: u32) -> Option<&'static str> {",
            "    let offset = opcode.checked_sub(OPERATIONS.first()?.0)? as usize;",
            "    OPERATIONS",
            "        .get(offset)",
            "        .filter(|(candidate, _)| *candidate == opcode)",
            "        .map(|(_, key)| *key)",
            "}",
            "",
        ]
    )
    return "\n".join(lines)


def write_or_check(path: Path, content: bytes, check: bool) -> None:
    """Write generated bytes or fail when the checked-in artifact differs."""
    if check:
        if not path.exists() or path.read_bytes() != content:
            raise SystemExit(f"generated artifact is stale: {path}")
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(content)


def main() -> None:
    """Parse CLI arguments and generate or verify both opcode artifacts."""
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--surface",
        type=Path,
        default=Path("tests/php_dom/surface/php-8.5.8.json"),
    )
    parser.add_argument(
        "--manifest",
        type=Path,
        default=Path("tests/php_dom/surface/opcodes-php-8.5.8.json"),
    )
    parser.add_argument(
        "--rust",
        type=Path,
        default=Path("crates/elephc-dom/src/generated/opcodes.rs"),
    )
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()

    manifest = build_manifest(args.surface)
    manifest_bytes = (
        json.dumps(manifest, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    ).encode("utf-8")
    rust_bytes = rust_source(manifest).encode("utf-8")
    write_or_check(args.manifest, manifest_bytes, args.check)
    write_or_check(args.rust, rust_bytes, args.check)


if __name__ == "__main__":
    main()
