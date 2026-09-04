#!/usr/bin/env python3
"""Export Elephc's stream surface and classify every PHP 8.5.6 drift."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

PHP_RELEASE = "8.5.6"
SCHEMA_VERSION = 1
ROOT = Path(__file__).resolve().parents[2]


def parse_args() -> argparse.Namespace:
    """Parse drift-ledger generation or byte-check arguments."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--php-manifest", required=True, type=Path)
    parser.add_argument("--target", required=True)
    parser.add_argument("--build-profile", required=True)
    parser.add_argument("--elephc-json", type=Path)
    parser.add_argument("--output", type=Path)
    action = parser.add_mutually_exclusive_group(required=True)
    action.add_argument("--write", action="store_true")
    action.add_argument("--check", action="store_true")
    return parser.parse_args()


def canonical_bytes(value: Any) -> bytes:
    """Serialize deterministic UTF-8 JSON with sorted keys and a final LF."""
    return (
        json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + "\n"
    ).encode()


def sha256_bytes(content: bytes) -> str:
    """Return the lowercase SHA-256 digest for arbitrary bytes."""
    return hashlib.sha256(content).hexdigest()


def sha256_file(path: Path) -> str:
    """Return the SHA-256 digest for one file."""
    return sha256_bytes(path.read_bytes())


def git_output(arguments: list[str]) -> bytes:
    """Run one read-only Git command against the Elephc worktree."""
    return subprocess.run(
        ["git", "-C", str(ROOT), *arguments],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    ).stdout


def export_elephc(target: str, path: Path | None) -> dict[str, Any]:
    """Load a supplied Rust export or invoke the single-source exporter."""
    if path is not None:
        return json.loads(path.read_bytes())
    result = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "--example",
            "gen_builtins",
            "--",
            "--streams-compliance",
            f"--target={target}",
        ],
        cwd=ROOT,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if result.returncode != 0:
        raise SystemExit(
            "Elephc stream exporter failed:\n"
            + result.stderr.decode("utf-8", errors="replace")
        )
    return json.loads(result.stdout)


def php_default(value: Any) -> Any:
    """Normalize one PHP reflection default for comparison."""
    if not isinstance(value, dict):
        return value
    value_type = value.get("type")
    if value_type == "string":
        return {
            "kind": "string",
            "base64": value.get("base64"),
            "length": value.get("length"),
        }
    if value_type == "array":
        return {"kind": "array", "entries": value.get("entries", [])}
    return {"kind": value_type, "value": value.get("value")}


def elephc_default(value: Any) -> Any:
    """Normalize one registry default into the PHP reflection comparison shape."""
    if value is None:
        return {"kind": "null", "value": None}
    if isinstance(value, bool):
        return {"kind": "bool", "value": value}
    if isinstance(value, int):
        return {"kind": "int", "value": value}
    if isinstance(value, float):
        return {"kind": "float", "value": value}
    if isinstance(value, str):
        if value == "PHP_INT_MAX":
            return {"kind": "constant", "name": value}
        content = value.encode()
        import base64

        return {
            "kind": "string",
            "base64": base64.b64encode(content).decode("ascii"),
            "length": len(content),
        }
    if value == []:
        return {"kind": "array", "entries": []}
    return value


def php_signature(function: dict[str, Any]) -> dict[str, Any]:
    """Project one PHP reflection record into comparable signature fields."""
    return {
        "parameters": [
            {
                "name": parameter["name"],
                "type": parameter["type"],
                "allows_null": parameter["allows_null"],
                "by_reference": parameter["by_reference"],
                "can_be_passed_by_value": parameter["can_be_passed_by_value"],
                "optional": parameter["optional"],
                "variadic": parameter["variadic"],
                "default_state": parameter["default_state"],
                "default_constant": parameter["default_constant"],
                "default": (
                    php_default(parameter["default"])
                    if parameter["default_available"]
                    else None
                ),
            }
            for parameter in function["parameters"]
        ],
        "return_type": function["return_type"],
        "tentative_return_type": function["tentative_return_type"],
        "returns_reference": function["returns_reference"],
    }


def elephc_builtin_signature(builtin: dict[str, Any]) -> dict[str, Any]:
    """Project one Elephc registry record into comparable signature fields."""
    parameters = [
        {
            "name": parameter["name"],
            "type": parameter["type"],
            "allows_null": None,
            "by_reference": parameter["by_ref"],
            "can_be_passed_by_value": not parameter["by_ref"],
            "optional": parameter["optional"],
            "variadic": False,
            "default_state": "available" if parameter["optional"] else "none",
            "default_constant": None,
            "default": (
                elephc_default(parameter["default"])
                if parameter["optional"]
                else None
            ),
        }
        for parameter in builtin["params"]
    ]
    if builtin.get("variadic") is not None:
        parameters.append(
            {
                "name": builtin["variadic"],
                "type": "mixed",
                "allows_null": True,
                "by_reference": False,
                "can_be_passed_by_value": True,
                "optional": True,
                "variadic": True,
                "default_state": "none",
                "default_constant": None,
                "default": None,
            }
        )
    return {
        "parameters": parameters,
        "return_type": builtin["returns"],
        "tentative_return_type": None,
        "returns_reference": builtin["by_ref_return"],
    }


def php_function_shape(function: dict[str, Any]) -> dict[str, Any]:
    """Project one PHP function into alias and signature metadata."""
    return {
        "alias_of": function.get("alias_of"),
        "deprecated": function["deprecated"],
        "signature": php_signature(function),
    }


def elephc_function_shape(builtin: dict[str, Any]) -> dict[str, Any]:
    """Project one Elephc builtin into alias and signature metadata."""
    return {
        "alias_of": None,
        "deprecated": builtin["deprecated"] is not None,
        "signature": elephc_builtin_signature(builtin),
    }


def elephc_expression(value: Any) -> Any:
    """Normalize one checker expression into the PHP reflection value shape."""
    if not isinstance(value, dict):
        return value
    kind = value.get("kind")
    if kind == "string":
        content = str(value.get("value", "")).encode()
        import base64

        return {
            "kind": "string",
            "base64": base64.b64encode(content).decode("ascii"),
            "length": len(content),
        }
    if kind == "null":
        return {"kind": "null", "value": None}
    return value


def php_property_shape(property_entry: dict[str, Any]) -> dict[str, Any]:
    """Project one PHP property into exact caller-visible metadata."""
    return {
        key: property_entry[key]
        for key in (
            "declaring_class",
            "visibility",
            "set_visibility",
            "static",
            "readonly",
            "final",
            "virtual",
            "has_hooks",
            "type",
            "settable_type",
            "default_available",
        )
    } | {
        "default": (
            php_default(property_entry["default"])
            if property_entry["default_available"]
            else None
        )
    }


def elephc_property_shape(property_entry: dict[str, Any]) -> dict[str, Any]:
    """Project one Elephc property into exact caller-visible metadata."""
    return {
        key: property_entry[key]
        for key in (
            "declaring_class",
            "visibility",
            "set_visibility",
            "static",
            "readonly",
            "final",
            "virtual",
            "has_hooks",
            "type",
            "settable_type",
            "default_available",
        )
    } | {
        "default": (
            elephc_expression(property_entry["default"])
            if property_entry["default_available"]
            else None
        )
    }


def elephc_checker_signature(signature: dict[str, Any]) -> dict[str, Any]:
    """Project one checker method signature into PHP reflection fields."""
    return {
        "parameters": [
            {
                "name": parameter["name"],
                "type": parameter["type"],
                "allows_null": None,
                "by_reference": parameter["by_reference"],
                "can_be_passed_by_value": not parameter["by_reference"],
                "optional": parameter["optional"],
                "variadic": parameter["variadic"],
                "default_state": (
                    "available" if parameter["optional"] else "none"
                ),
                "default_constant": None,
                "default": (
                    elephc_expression(parameter["default"])
                    if parameter["optional"]
                    else None
                ),
            }
            for parameter in signature["parameters"]
        ],
        "return_type": signature["return_type"],
        "tentative_return_type": None,
        "returns_reference": signature["returns_reference"],
    }


def php_method_shape(method: dict[str, Any]) -> dict[str, Any]:
    """Project one PHP method into alias, modifiers, and signature metadata."""
    return {
        "alias_of": method.get("alias_of"),
        "declaring_class": method["declaring_class"],
        "visibility": method["visibility"],
        "static": method["static"],
        "final": method["final"],
        "abstract": method["abstract"],
        "deprecated": method["deprecated"],
        "signature": php_signature(method),
    }


def elephc_method_shape(method: dict[str, Any]) -> dict[str, Any]:
    """Project one Elephc method into alias, modifiers, and signature metadata."""
    return {
        "alias_of": method.get("alias_of"),
        "declaring_class": method["declaring_class"],
        "visibility": method["visibility"],
        "static": method["static"],
        "final": method["final"],
        "abstract": method["abstract"],
        "deprecated": method["deprecated"],
        "signature": elephc_checker_signature(method["signature"]),
    }


def class_shape_php(class_entry: dict[str, Any]) -> dict[str, Any]:
    """Project one PHP class to exact caller-visible structural metadata."""
    return {
        "parent": class_entry["parent"],
        "interfaces": sorted(class_entry["interfaces"]),
        "traits": sorted(class_entry["traits"]),
        "abstract": class_entry["abstract"],
        "final": class_entry["final"],
        "readonly": class_entry["readonly"],
        "internal": class_entry["internal"],
        "instantiable": class_entry["instantiable"],
        "interface": class_entry["interface"],
        "trait": class_entry["trait"],
        "enum": class_entry["enum"],
        "anonymous": class_entry["anonymous"],
        "properties": {
            property_entry["name"]: php_property_shape(property_entry)
            for property_entry in class_entry["properties"]
        },
        "methods": {
            method["canonical_name"]: php_method_shape(method)
            for method in class_entry["methods"]
        },
        "constants": {
            constant["name"]: {
                "declaring_class": constant["declaring_class"],
                "visibility": constant["visibility"],
                "final": constant["final"],
                "deprecated": constant["deprecated"],
                "value": php_default(constant["value"]),
            }
            for constant in class_entry["constants"]
        },
    }


def class_shape_elephc(class_entry: dict[str, Any]) -> dict[str, Any]:
    """Project one Elephc class to exact caller-visible structural metadata."""
    return {
        "parent": class_entry["parent"],
        "interfaces": sorted(class_entry["interfaces"]),
        "traits": sorted(class_entry["traits"]),
        "abstract": class_entry["abstract"],
        "final": class_entry["final"],
        "readonly": class_entry["readonly"],
        "internal": class_entry["internal"],
        "instantiable": class_entry["instantiable"],
        "interface": class_entry["interface"],
        "trait": class_entry["trait"],
        "enum": class_entry["enum"],
        "anonymous": class_entry["anonymous"],
        "properties": {
            property_entry["name"]: elephc_property_shape(property_entry)
            for property_entry in class_entry["properties"]
        },
        "methods": {
            method["canonical_name"]: elephc_method_shape(method)
            for method in class_entry["methods"]
        },
        "constants": {
            constant["name"]: {
                "declaring_class": constant["declaring_class"],
                "visibility": constant["visibility"],
                "final": constant["final"],
                "deprecated": constant["deprecated"],
                "value": elephc_expression(constant["value"]),
            }
            for constant in class_entry["constants"]
        },
    }


def add_drift(
    drifts: list[dict[str, Any]],
    category: str,
    symbol: str,
    php: Any,
    elephc: Any,
) -> None:
    """Append one explicitly classified drift record."""
    drifts.append(
        {
            "category": category,
            "symbol": symbol,
            "php": php,
            "elephc": elephc,
            "classification": "known-incompatibility",
            "closing_gate": 2,
        }
    )


def build_ledger(args: argparse.Namespace) -> dict[str, Any]:
    """Compare the frozen PHP surface with Elephc and classify every difference."""
    php_manifest = json.loads(args.php_manifest.read_bytes())
    profile = php_manifest["profile"]
    if profile["php_release"] != PHP_RELEASE:
        raise SystemExit(f"PHP manifest must be {PHP_RELEASE}")
    if profile["target"] != args.target or profile["name"] != args.build_profile:
        raise SystemExit("PHP manifest profile does not match requested target/profile")
    elephc = export_elephc(args.target, args.elephc_json)
    php_surface = php_manifest["surface"]
    drifts: list[dict[str, Any]] = []

    php_functions = {
        function["canonical_name"]: function
        for function in php_surface["functions"]
    }
    elephc_builtins = {
        builtin["name"].lower(): builtin for builtin in elephc["builtins"]
    }
    elephc_scope = {
        name
        for name, builtin in elephc_builtins.items()
        if builtin["area"] == "io"
    }
    for name in sorted(set(php_functions) - set(elephc_builtins)):
        add_drift(
            drifts,
            "missing-function",
            php_functions[name]["name"],
            php_function_shape(php_functions[name]),
            None,
        )
    for name in sorted(set(php_functions) & set(elephc_builtins)):
        php_sig = php_function_shape(php_functions[name])
        elephc_sig = elephc_function_shape(elephc_builtins[name])
        if php_sig != elephc_sig:
            add_drift(
                drifts,
                "function-signature",
                php_functions[name]["name"],
                php_sig,
                elephc_sig,
            )

    php_classes = {
        class_entry["canonical_name"]: class_entry
        for class_entry in php_surface["classes"]
    }
    elephc_classes = {
        class_entry["name"].lower(): class_entry
        for class_entry in elephc["classes"]
    }
    for name in sorted(set(php_classes) - set(elephc_classes)):
        add_drift(
            drifts,
            "missing-class",
            php_classes[name]["name"],
            class_shape_php(php_classes[name]),
            None,
        )
    for name in sorted(set(php_classes) & set(elephc_classes)):
        php_shape = class_shape_php(php_classes[name])
        elephc_shape = class_shape_elephc(elephc_classes[name])
        if php_shape != elephc_shape:
            add_drift(
                drifts,
                "class-surface",
                php_classes[name]["name"],
                php_shape,
                elephc_shape,
            )

    php_constants = php_surface["constants"]
    elephc_constants = {
        constant["name"]: {"type": constant["type"], "value": constant["value"]}
        for constant in elephc["constants"]
    }
    for name in sorted(set(php_constants) - set(elephc_constants)):
        add_drift(drifts, "missing-constant", name, php_constants[name], None)
    for name in sorted(set(elephc_constants) - set(php_constants)):
        add_drift(drifts, "extra-constant", name, None, elephc_constants[name])
    for name in sorted(set(php_constants) & set(elephc_constants)):
        if php_constants[name] != elephc_constants[name]:
            add_drift(
                drifts,
                "constant-value",
                name,
                php_constants[name],
                elephc_constants[name],
            )

    for capability in ("wrappers", "transports", "filters"):
        if php_surface[capability] != elephc[capability]:
            add_drift(
                drifts,
                "configured-capability",
                capability,
                php_surface[capability],
                elephc[capability],
            )

    category_counts: dict[str, int] = {}
    for drift in drifts:
        category = drift["category"]
        category_counts[category] = category_counts.get(category, 0) + 1
    tracked_diff = git_output(["diff", "--binary", "HEAD", "--", "src", "tools/gen_builtins.rs"])
    return {
        "schema_version": SCHEMA_VERSION,
        "kind": "elephc-php-src-stream-drift-ledger",
        "gate": {
            "number": 0,
            "status": "classified",
            "unclassified_drift": 0,
        },
        "profile": {
            "php_release": PHP_RELEASE,
            "php_src_commit": profile["php_src_commit"],
            "target": args.target,
            "build_profile": args.build_profile,
            "php_surface_sha256": sha256_bytes(canonical_bytes(php_surface)),
            "elephc_head": git_output(["rev-parse", "HEAD"]).decode().strip(),
            "elephc_tracked_diff_sha256": sha256_bytes(tracked_diff),
        },
        "summary": {
            "total": len(drifts),
            "categories": dict(sorted(category_counts.items())),
            "php_functions": len(php_functions),
            "elephc_builtins": len(elephc_builtins),
            "elephc_io_out_of_scope": len(elephc_scope - set(php_functions)),
            "php_classes": len(php_classes),
            "elephc_classes": len(elephc_classes),
            "php_constants": len(php_constants),
            "elephc_stream_constants": len(elephc_constants),
        },
        "drifts": drifts,
        "generator": {
            "script": Path(__file__).relative_to(ROOT).as_posix(),
            "script_sha256": sha256_file(Path(__file__)),
            "elephc_exporter": "cargo run --example gen_builtins -- --streams-compliance",
        },
    }


def default_output(target: str, profile: str) -> Path:
    """Return the checked-in drift-ledger path."""
    return (
        ROOT
        / "tests"
        / "php_oracle"
        / "drift"
        / "streams"
        / f"php-{PHP_RELEASE}"
        / target
        / f"{profile}.json"
    )


def atomic_write(path: Path, content: bytes) -> None:
    """Replace one generated drift ledger atomically."""
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(content)
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def main() -> int:
    """Generate or byte-check one target/profile drift ledger."""
    args = parse_args()
    ledger = build_ledger(args)
    content = canonical_bytes(ledger)
    output = args.output or default_output(args.target, args.build_profile)
    if args.check:
        if not output.exists():
            print(f"missing drift ledger: {output}", file=sys.stderr)
            return 1
        if output.read_bytes() != content:
            print(f"drift ledger changed: regenerate {output}", file=sys.stderr)
            return 1
        return 0
    atomic_write(output, content)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
