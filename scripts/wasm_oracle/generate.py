#!/usr/bin/env python3
"""Generate and validate deterministic PHP/WASM differential oracle fixtures.

The JSON suite and matrices are the only authored sources of truth. This module
renders the public fixture contract and PHP sources, and also exposes the strict
binary-frame parser that the future capture/comparison layer can reuse.
"""

from __future__ import annotations

import argparse
import base64
import binascii
import json
import os
import re
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Any, Iterable, Mapping, Sequence


SUITE_SCHEMA = "elephc.wasm-php-oracle-suite.v2"
MATRIX_SCHEMA = "elephc.wasm-php-oracle-matrix.v1"
CONTRACT_SCHEMA = "elephc.wasm-php-oracle-contract.v2"
FRAME_FORMAT = "@<idlen>:<id>:<typelen>:<type>:<payloadlen>:<payload>\\n"
FRAME_ENCODING = "lengths are unsigned decimal byte counts; id/type are ASCII"
EXPECTED_GUEST_ENV = {
    "LANG": "C.UTF-8",
    "LC_ALL": "C.UTF-8",
    "TZ": "UTC",
}
EXPECTED_GUEST_PROGRAM = "oracle.php"
EXPECTED_GUEST_PREOPENS: dict[str, str] = {}
EXPECTED_TIMEOUT_SECONDS = 10
EXPECTED_MAX_OUTPUT_BYTES = 1_048_576
EXPECTED_SHARDS = frozenset(range(4))
ALLOWED_FRAME_TYPES = frozenset({"bool", "int", "null", "string"})
ID_PATTERN = re.compile(r"[a-z][a-z0-9_]{0,63}\Z")
DECIMAL_PATTERN = re.compile(rb"(?:0|[1-9][0-9]*)\Z")
INTEGER_PAYLOAD_PATTERN = re.compile(rb"(?:0|-?[1-9][0-9]*)\Z")


class OracleDefinitionError(ValueError):
    """Report a malformed authored suite, matrix, path, or generated contract."""


class FrameProtocolError(ValueError):
    """Report malformed, incomplete, duplicated, or unexpected captured frames."""


@dataclass(frozen=True)
class FrameExpectation:
    """Describe one expected frame identity and its declared PHP value type."""

    identifier: str
    value_type: str


@dataclass(frozen=True)
class ParsedFrame:
    """Hold one strictly decoded frame while preserving its arbitrary payload bytes."""

    identifier: str
    value_type: str
    payload: bytes


@dataclass(frozen=True)
class OracleCase:
    """Describe one generated PHP expression and its frame observation type."""

    identifier: str
    value_type: str
    expression: str


@dataclass(frozen=True)
class OracleFixture:
    """Describe one fixture, its stable shard, logical inputs, and ordered cases."""

    identifier: str
    shard: int
    description: str
    logical_args: tuple[str, ...]
    stdin_base64: str
    preamble: str
    matrix_path: str
    cases: tuple[OracleCase, ...]


@dataclass(frozen=True)
class OracleSuite:
    """Hold the validated suite-level contract inputs and four fixture matrices."""

    inventory_path: str
    specification_path: str
    fixtures: tuple[OracleFixture, ...]


def repository_root() -> Path:
    """Return the repository root containing this generator."""

    return Path(__file__).resolve().parents[2]


def fixture_root(repo_root: Path | None = None) -> Path:
    """Return the confined directory containing oracle sources and outputs."""

    root = repository_root() if repo_root is None else repo_root.resolve()
    return root / "tests" / "fixtures" / "wasm" / "php_oracle"


def require_exact_keys(
    document: Mapping[str, Any], expected: Iterable[str], location: str
) -> None:
    """Reject missing or unknown object keys at the named JSON location."""

    expected_set = set(expected)
    actual_set = set(document)
    missing = sorted(expected_set - actual_set)
    extra = sorted(actual_set - expected_set)
    if missing or extra:
        details = []
        if missing:
            details.append(f"missing={missing}")
        if extra:
            details.append(f"unknown={extra}")
        raise OracleDefinitionError(f"{location} has invalid keys: {', '.join(details)}")


def require_identifier(value: Any, location: str) -> str:
    """Return a control-free stable identifier or reject it."""

    if not isinstance(value, str) or ID_PATTERN.fullmatch(value) is None:
        raise OracleDefinitionError(
            f"{location} must match {ID_PATTERN.pattern!r}, got {value!r}"
        )
    return value


def require_nonempty_text(value: Any, location: str, *, multiline: bool = False) -> str:
    """Return nonempty source/description text after rejecting unsafe controls."""

    if not isinstance(value, str) or not value.strip():
        raise OracleDefinitionError(f"{location} must be a nonempty string")
    forbidden = {"\x00", "\r"}
    if not multiline:
        forbidden.add("\n")
    if any(character in value for character in forbidden):
        raise OracleDefinitionError(f"{location} contains a forbidden control character")
    return value


def require_logical_args(value: Any, location: str) -> tuple[str, ...]:
    """Validate an explicit logical WASI argument vector excluding argv[0]."""

    if not isinstance(value, list):
        raise OracleDefinitionError(f"{location} must be an explicit JSON array")
    arguments = []
    for index, argument in enumerate(value):
        if not isinstance(argument, str):
            raise OracleDefinitionError(f"{location}[{index}] must be a string")
        if "\x00" in argument:
            raise OracleDefinitionError(f"{location}[{index}] contains an embedded NUL")
        arguments.append(argument)
    return tuple(arguments)


def require_canonical_base64(value: Any, location: str) -> str:
    """Validate canonical base64 while allowing an explicitly empty stdin."""

    if not isinstance(value, str):
        raise OracleDefinitionError(f"{location} must be a base64 string")
    try:
        decoded = base64.b64decode(value, validate=True)
    except (binascii.Error, ValueError) as error:
        raise OracleDefinitionError(f"{location} is not valid base64") from error
    canonical = base64.b64encode(decoded).decode("ascii")
    if canonical != value:
        raise OracleDefinitionError(f"{location} is not canonical base64")
    return value


def require_relative_path(value: Any, location: str) -> str:
    """Return a normalized repository-relative POSIX path without traversal."""

    if not isinstance(value, str) or not value:
        raise OracleDefinitionError(f"{location} must be a nonempty relative path")
    candidate = PurePosixPath(value)
    if candidate.is_absolute() or any(part in {"", ".", ".."} for part in candidate.parts):
        raise OracleDefinitionError(f"{location} is not a confined relative path: {value!r}")
    normalized = candidate.as_posix()
    if normalized != value:
        raise OracleDefinitionError(f"{location} is not normalized: {value!r}")
    return normalized


def resolve_confined(base: Path, relative: str, location: str) -> Path:
    """Resolve a relative path and reject any escape from the requested base."""

    resolved_base = base.resolve()
    resolved = (resolved_base / relative).resolve()
    if not resolved.is_relative_to(resolved_base):
        raise OracleDefinitionError(f"{location} escapes {resolved_base}: {relative!r}")
    return resolved


def load_json(path: Path, location: str) -> Mapping[str, Any]:
    """Load one UTF-8 JSON object and reject duplicate keys at every depth."""

    def reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
        """Build one JSON object while rejecting ambiguous duplicate member names."""

        document: dict[str, Any] = {}
        for key, value in pairs:
            if key in document:
                raise OracleDefinitionError(
                    f"{location} contains duplicate JSON key {key!r}"
                )
            document[key] = value
        return document

    try:
        document = json.loads(
            path.read_text(encoding="utf-8"),
            object_pairs_hook=reject_duplicate_keys,
        )
    except OSError as error:
        raise OracleDefinitionError(f"cannot read {location}: {path}: {error}") from error
    except (UnicodeError, json.JSONDecodeError) as error:
        raise OracleDefinitionError(f"invalid UTF-8 JSON in {location}: {error}") from error
    if not isinstance(document, dict):
        raise OracleDefinitionError(f"{location} must contain a JSON object")
    return document


def validate_case(document: Any, location: str) -> OracleCase:
    """Validate one exact-key matrix case and return its immutable model."""

    if not isinstance(document, dict):
        raise OracleDefinitionError(f"{location} must be an object")
    require_exact_keys(document, {"id", "type", "expression"}, location)
    identifier = require_identifier(document["id"], f"{location}.id")
    value_type = document["type"]
    if value_type not in ALLOWED_FRAME_TYPES:
        raise OracleDefinitionError(
            f"{location}.type must be one of {sorted(ALLOWED_FRAME_TYPES)}"
        )
    expression = require_nonempty_text(document["expression"], f"{location}.expression")
    return OracleCase(identifier, value_type, expression)


def validate_matrix(document: Mapping[str, Any], matrix_path: str) -> OracleFixture:
    """Validate one fixture matrix, including exact keys and ordered unique cases."""

    location = f"matrix {matrix_path}"
    require_exact_keys(
        document,
        {
            "$schema",
            "id",
            "shard",
            "description",
            "logical_args",
            "stdin_base64",
            "preamble",
            "cases",
        },
        location,
    )
    if document["$schema"] != MATRIX_SCHEMA:
        raise OracleDefinitionError(f"{location} has unsupported schema")
    identifier = require_identifier(document["id"], f"{location}.id")
    shard = document["shard"]
    if not isinstance(shard, int) or isinstance(shard, bool) or shard not in EXPECTED_SHARDS:
        raise OracleDefinitionError(f"{location}.shard must be an integer from 0 through 3")
    description = require_nonempty_text(document["description"], f"{location}.description")
    logical_args = require_logical_args(document["logical_args"], f"{location}.logical_args")
    stdin_base64 = require_canonical_base64(
        document["stdin_base64"], f"{location}.stdin_base64"
    )
    preamble = require_nonempty_text(
        document["preamble"], f"{location}.preamble", multiline=True
    )
    raw_cases = document["cases"]
    if not isinstance(raw_cases, list) or not raw_cases:
        raise OracleDefinitionError(f"{location}.cases must be a nonempty array")
    cases = tuple(
        validate_case(case, f"{location}.cases[{index}]")
        for index, case in enumerate(raw_cases)
    )
    case_ids = [case.identifier for case in cases]
    if len(case_ids) != len(set(case_ids)):
        raise OracleDefinitionError(f"{location} contains duplicate case IDs")
    return OracleFixture(
        identifier,
        shard,
        description,
        logical_args,
        stdin_base64,
        preamble,
        matrix_path,
        cases,
    )


def load_suite(repo_root: Path | None = None) -> OracleSuite:
    """Load and fail-closed validate the suite plus all four matrix files."""

    root = repository_root() if repo_root is None else repo_root.resolve()
    fixtures_root = fixture_root(root)
    suite_path = fixtures_root / "suite.json"
    document = load_json(suite_path, "oracle suite")
    require_exact_keys(
        document,
        {"$schema", "references", "protocol", "execution", "shard_count", "matrices"},
        "oracle suite",
    )
    if document["$schema"] != SUITE_SCHEMA:
        raise OracleDefinitionError("oracle suite has unsupported schema")

    references = document["references"]
    if not isinstance(references, dict):
        raise OracleDefinitionError("oracle suite references must be an object")
    require_exact_keys(references, {"inventory", "specification"}, "oracle suite references")
    inventory = require_relative_path(references["inventory"], "references.inventory")
    specification = require_relative_path(
        references["specification"], "references.specification"
    )
    for relative, label in (
        (inventory, "inventory"),
        (specification, "specification"),
    ):
        path = resolve_confined(root, relative, f"references.{label}")
        if not path.is_file():
            raise OracleDefinitionError(f"referenced {label} does not exist: {relative}")

    protocol = document["protocol"]
    if not isinstance(protocol, dict):
        raise OracleDefinitionError("oracle suite protocol must be an object")
    require_exact_keys(protocol, {"format", "encoding"}, "oracle suite protocol")
    if protocol != {"format": FRAME_FORMAT, "encoding": FRAME_ENCODING}:
        raise OracleDefinitionError("oracle suite protocol does not match the frozen v1 framing")

    execution = document["execution"]
    if not isinstance(execution, dict):
        raise OracleDefinitionError("oracle suite execution must be an object")
    require_exact_keys(
        execution,
        {
            "guest_program",
            "guest_env",
            "guest_preopens",
            "timeout_seconds",
            "max_output_bytes",
        },
        "oracle suite execution",
    )
    if execution["guest_program"] != EXPECTED_GUEST_PROGRAM:
        raise OracleDefinitionError(
            "oracle suite guest_program must be exactly oracle.php"
        )
    if execution["guest_env"] != EXPECTED_GUEST_ENV:
        raise OracleDefinitionError("oracle suite guest_env must be exactly LANG/LC_ALL/TZ")
    if execution["guest_preopens"] != EXPECTED_GUEST_PREOPENS:
        raise OracleDefinitionError("oracle suite guest_preopens must be empty")
    if execution["timeout_seconds"] != EXPECTED_TIMEOUT_SECONDS:
        raise OracleDefinitionError("oracle suite timeout_seconds must be exactly 10")
    if execution["max_output_bytes"] != EXPECTED_MAX_OUTPUT_BYTES:
        raise OracleDefinitionError("oracle suite max_output_bytes must be exactly 1048576")
    if document["shard_count"] != len(EXPECTED_SHARDS):
        raise OracleDefinitionError("oracle suite shard_count must be exactly 4")

    matrix_values = document["matrices"]
    if not isinstance(matrix_values, list) or len(matrix_values) != 4:
        raise OracleDefinitionError("oracle suite must list exactly four matrices")
    matrix_paths = [
        require_relative_path(value, f"oracle suite matrices[{index}]")
        for index, value in enumerate(matrix_values)
    ]
    if len(matrix_paths) != len(set(matrix_paths)):
        raise OracleDefinitionError("oracle suite contains duplicate matrix paths")

    fixtures = []
    for matrix_path in matrix_paths:
        path = resolve_confined(fixtures_root, matrix_path, f"matrix {matrix_path}")
        if not path.is_file():
            raise OracleDefinitionError(f"matrix does not exist: {matrix_path}")
        fixtures.append(validate_matrix(load_json(path, f"matrix {matrix_path}"), matrix_path))

    fixture_ids = [fixture.identifier for fixture in fixtures]
    if len(fixture_ids) != len(set(fixture_ids)):
        raise OracleDefinitionError("oracle suite contains duplicate fixture IDs")
    shards = {fixture.shard for fixture in fixtures}
    if shards != EXPECTED_SHARDS:
        raise OracleDefinitionError(
            f"oracle suite must populate shards 0..3 exactly once, got {sorted(shards)}"
        )
    all_case_ids = [
        case.identifier for fixture in fixtures for case in fixture.cases
    ]
    if len(all_case_ids) != len(set(all_case_ids)):
        raise OracleDefinitionError("oracle suite contains duplicate frame IDs")
    return OracleSuite(inventory, specification, tuple(fixtures))


def render_php(fixture: OracleFixture) -> bytes:
    """Render one deterministic PHP source with inline, single-evaluation frames."""

    lines = [
        "<?php",
        "",
        "// Generated by scripts/wasm_oracle/generate.py; do not edit.",
        f"// Fixture: {fixture.identifier}; shard: {fixture.shard}.",
        "",
        fixture.preamble.rstrip("\n"),
        "",
    ]
    for index, case in enumerate(fixture.cases):
        value_name = f"$__oracle_value_{index:03d}"
        payload_name = f"$__oracle_payload_{index:03d}"
        prefix = (
            f"@{len(case.identifier.encode('ascii'))}:{case.identifier}:"
            f"{len(case.value_type.encode('ascii'))}:{case.value_type}:"
        )
        lines.append(f"{value_name} = {case.expression};")
        if case.value_type == "bool":
            lines.extend(
                [
                    f"if ({value_name} === true) {{",
                    f'    echo "{prefix}1:1\\n";',
                    f"}} elseif ({value_name} === false) {{",
                    f'    echo "{prefix}1:0\\n";',
                    "} else {",
                    "    exit(97);",
                    "}",
                ]
            )
        elif case.value_type == "null":
            lines.extend(
                [
                    f"if ({value_name} !== null) {{",
                    "    exit(97);",
                    "}",
                    f'echo "{prefix}0:\\n";',
                ]
            )
        else:
            conversion = f"(string) {value_name}" if case.value_type == "int" else value_name
            assertion_cast = "(int)" if case.value_type == "int" else "(string)"
            lines.extend(
                [
                    f"if ({value_name} !== {assertion_cast} {value_name}) {{",
                    "    exit(97);",
                    "}",
                    f"{payload_name} = {conversion};",
                    (
                        f'echo "{prefix}", strlen({payload_name}), ":", '
                        f'{payload_name}, "\\n";'
                    ),
                ]
            )
        lines.append("")
    source = "\n".join(lines).rstrip() + "\n"
    if not source.strip():
        raise OracleDefinitionError(f"generated source is empty: {fixture.identifier}")
    return source.encode("utf-8")


def render_contract(suite: OracleSuite) -> bytes:
    """Render the pin-free fixture contract consumed by a future oracle loader."""

    fixtures = []
    for fixture in suite.fixtures:
        fixtures.append(
            {
                "id": fixture.identifier,
                "shard": fixture.shard,
                "matrix": fixture.matrix_path,
                "source": f"generated/{fixture.identifier}.php",
                "logical_args": list(fixture.logical_args),
                "stdin_base64": fixture.stdin_base64,
                "frames": [
                    {"id": case.identifier, "type": case.value_type}
                    for case in fixture.cases
                ],
            }
        )
    document = {
        "$schema": CONTRACT_SCHEMA,
        "references": {
            "inventory": suite.inventory_path,
            "specification": suite.specification_path,
        },
        "protocol": {
            "format": FRAME_FORMAT,
            "encoding": FRAME_ENCODING,
        },
        "execution": {
            "guest_program": EXPECTED_GUEST_PROGRAM,
            "guest_env": EXPECTED_GUEST_ENV,
            "guest_preopens": EXPECTED_GUEST_PREOPENS,
            "timeout_seconds": EXPECTED_TIMEOUT_SECONDS,
            "max_output_bytes": EXPECTED_MAX_OUTPUT_BYTES,
        },
        "shard_count": len(EXPECTED_SHARDS),
        "fixtures": fixtures,
    }
    return (json.dumps(document, indent=2, ensure_ascii=False) + "\n").encode("utf-8")


def render_outputs(repo_root: Path | None = None) -> dict[Path, bytes]:
    """Return every generated path and its deterministic byte content."""

    root = repository_root() if repo_root is None else repo_root.resolve()
    suite = load_suite(root)
    output_root = fixture_root(root)
    outputs = {output_root / "contract.json": render_contract(suite)}
    for fixture in suite.fixtures:
        outputs[output_root / "generated" / f"{fixture.identifier}.php"] = render_php(
            fixture
        )
    return outputs


def check_outputs(outputs: Mapping[Path, bytes]) -> list[str]:
    """Return human-readable byte mismatches without modifying generated files."""

    errors = []
    for path, expected in sorted(outputs.items(), key=lambda item: str(item[0])):
        try:
            actual = path.read_bytes()
        except OSError:
            errors.append(f"missing generated file: {path}")
            continue
        if actual != expected:
            errors.append(f"generated file is stale: {path}")
    return errors


def atomic_write(path: Path, content: bytes) -> None:
    """Atomically replace one generated file with fixed non-executable permissions."""

    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary_name = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    temporary = Path(temporary_name)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(content)
            stream.flush()
            os.fsync(stream.fileno())
        os.chmod(temporary, 0o644)
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def write_outputs(outputs: Mapping[Path, bytes]) -> None:
    """Write every deterministic generated output through atomic replacement."""

    for path, content in sorted(outputs.items(), key=lambda item: str(item[0])):
        atomic_write(path, content)


def parse_decimal(data: bytes, offset: int, label: str) -> tuple[int, int]:
    """Parse one canonical unsigned decimal field terminated by a colon."""

    colon = data.find(b":", offset)
    if colon < 0:
        raise FrameProtocolError(f"missing colon after {label}")
    digits = data[offset:colon]
    if DECIMAL_PATTERN.fullmatch(digits) is None:
        raise FrameProtocolError(f"invalid canonical decimal {label}: {digits!r}")
    return int(digits), colon + 1


def parse_frames(
    data: bytes, expected: Sequence[FrameExpectation] | None = None
) -> list[ParsedFrame]:
    """Strictly parse frames and enforce exact identities, types, count, and order."""

    frames = []
    seen = set()
    offset = 0
    while offset < len(data):
        if data[offset : offset + 1] != b"@":
            raise FrameProtocolError(f"unexpected trailing or non-frame byte at offset {offset}")
        offset += 1
        identifier_length, offset = parse_decimal(data, offset, "id length")
        if identifier_length == 0 or offset + identifier_length > len(data):
            raise FrameProtocolError("invalid or truncated frame identifier")
        identifier_bytes = data[offset : offset + identifier_length]
        offset += identifier_length
        if data[offset : offset + 1] != b":":
            raise FrameProtocolError("missing colon after frame identifier")
        offset += 1

        type_length, offset = parse_decimal(data, offset, "type length")
        if type_length == 0 or offset + type_length > len(data):
            raise FrameProtocolError("invalid or truncated frame type")
        type_bytes = data[offset : offset + type_length]
        offset += type_length
        if data[offset : offset + 1] != b":":
            raise FrameProtocolError("missing colon after frame type")
        offset += 1

        payload_length, offset = parse_decimal(data, offset, "payload length")
        if offset + payload_length > len(data):
            raise FrameProtocolError("truncated frame payload")
        payload = data[offset : offset + payload_length]
        offset += payload_length
        if data[offset : offset + 1] != b"\n":
            raise FrameProtocolError("missing newline after frame payload")
        offset += 1
        try:
            identifier = identifier_bytes.decode("ascii")
            value_type = type_bytes.decode("ascii")
        except UnicodeDecodeError as error:
            raise FrameProtocolError("frame id/type must be ASCII") from error
        require_identifier_for_frame(identifier)
        if value_type not in ALLOWED_FRAME_TYPES:
            raise FrameProtocolError(f"unsupported frame type: {value_type!r}")
        if identifier in seen:
            raise FrameProtocolError(f"duplicate frame ID: {identifier}")
        validate_frame_payload(value_type, payload)
        seen.add(identifier)
        frames.append(ParsedFrame(identifier, value_type, payload))

    if expected is not None:
        expected_ids = [item.identifier for item in expected]
        expected_id_set = set(expected_ids)
        if len(expected_ids) != len(expected_id_set):
            raise FrameProtocolError("expected frame list contains duplicate IDs")
        for item in expected:
            require_identifier_for_frame(item.identifier)
            if item.value_type not in ALLOWED_FRAME_TYPES:
                raise FrameProtocolError(
                    f"unsupported expected frame type: {item.value_type!r}"
                )
        actual_ids = [item.identifier for item in frames]
        missing = [identifier for identifier in expected_ids if identifier not in seen]
        extra = [identifier for identifier in actual_ids if identifier not in expected_id_set]
        if missing or extra:
            raise FrameProtocolError(f"frame set mismatch: missing={missing}, extra={extra}")
        if actual_ids != expected_ids:
            raise FrameProtocolError(
                f"frame order mismatch: expected={expected_ids}, actual={actual_ids}"
            )
        for frame, expectation in zip(frames, expected, strict=True):
            if frame.value_type != expectation.value_type:
                raise FrameProtocolError(
                    f"frame {frame.identifier} type mismatch: "
                    f"expected={expectation.value_type}, actual={frame.value_type}"
                )
    return frames


def validate_frame_payload(value_type: str, payload: bytes) -> None:
    """Reject payload bytes that cannot canonically represent the declared type."""

    if value_type == "bool" and payload not in {b"0", b"1"}:
        raise FrameProtocolError("boolean frame payload must be exactly 0 or 1")
    if value_type == "null" and payload:
        raise FrameProtocolError("null frame payload must be empty")
    if value_type == "int":
        if INTEGER_PAYLOAD_PATTERN.fullmatch(payload) is None:
            raise FrameProtocolError(
                f"integer frame payload is not canonical decimal: {payload!r}"
            )
        integer = int(payload)
        if integer < -(1 << 63) or integer > (1 << 63) - 1:
            raise FrameProtocolError("integer frame payload exceeds signed 64-bit range")


def require_identifier_for_frame(identifier: str) -> None:
    """Convert identifier validation failures into frame protocol failures."""

    if ID_PATTERN.fullmatch(identifier) is None:
        raise FrameProtocolError(f"invalid frame ID: {identifier!r}")


def parse_arguments(arguments: Sequence[str] | None = None) -> argparse.Namespace:
    """Parse the generator CLI, whose default mode updates committed outputs."""

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="fail unless committed generated files are byte-identical",
    )
    return parser.parse_args(arguments)


def main(arguments: Sequence[str] | None = None) -> int:
    """Generate outputs or fail closed when --check detects missing/stale bytes."""

    options = parse_arguments(arguments)
    try:
        outputs = render_outputs()
        if options.check:
            errors = check_outputs(outputs)
            if errors:
                for error in errors:
                    print(f"wasm oracle: {error}", file=sys.stderr)
                return 1
            print(f"wasm oracle: {len(outputs)} generated files are current")
            return 0
        write_outputs(outputs)
        print(f"wasm oracle: wrote {len(outputs)} generated files")
        return 0
    except (OracleDefinitionError, FrameProtocolError) as error:
        print(f"wasm oracle: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
