#!/usr/bin/env python3
"""Unit and local structural tests for generated PHP/WASM oracle fixtures."""

from __future__ import annotations

import copy
import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

from scripts.wasm_oracle import generate


REPO_ROOT = Path(__file__).resolve().parents[2]
FIXTURE_ROOT = REPO_ROOT / "tests" / "fixtures" / "wasm" / "php_oracle"


def encode_frame(identifier: str, value_type: str, payload: bytes) -> bytes:
    """Encode synthetic bytes for parser tests without defining PHP expectations."""

    identifier_bytes = identifier.encode("ascii")
    type_bytes = value_type.encode("ascii")
    return (
        b"@"
        + str(len(identifier_bytes)).encode("ascii")
        + b":"
        + identifier_bytes
        + b":"
        + str(len(type_bytes)).encode("ascii")
        + b":"
        + type_bytes
        + b":"
        + str(len(payload)).encode("ascii")
        + b":"
        + payload
        + b"\n"
    )


def expected_frames(fixture: generate.OracleFixture) -> list[generate.FrameExpectation]:
    """Convert matrix case identities/types into strict parser expectations."""

    return [
        generate.FrameExpectation(case.identifier, case.value_type)
        for case in fixture.cases
    ]


def contains_forbidden_contract_key(value: object) -> bool:
    """Detect profile, pin, commit, or hash copies forbidden in the fixture contract."""

    forbidden = {
        "profile",
        "profiles",
        "pins",
        "tag",
        "tag_object",
        "tag_commit",
        "commit",
        "sha",
        "sha256",
        "spec_sha256",
    }
    if isinstance(value, dict):
        if forbidden.intersection(value):
            return True
        return any(contains_forbidden_contract_key(item) for item in value.values())
    if isinstance(value, list):
        return any(contains_forbidden_contract_key(item) for item in value)
    return False


class WasmOracleFixtureTests(unittest.TestCase):
    """Verify fail-closed definitions, deterministic generation, and framing."""

    @classmethod
    def setUpClass(cls) -> None:
        """Load the authored suite once for deterministic structural assertions."""

        cls.suite = generate.load_suite(REPO_ROOT)

    def test_suite_has_four_unique_nonempty_shards(self) -> None:
        """Require four fixture IDs mapped one-to-one onto shards zero through three."""

        self.assertEqual(len(self.suite.fixtures), 4)
        self.assertEqual(
            {fixture.shard for fixture in self.suite.fixtures},
            {0, 1, 2, 3},
        )
        fixture_ids = [fixture.identifier for fixture in self.suite.fixtures]
        self.assertEqual(len(fixture_ids), len(set(fixture_ids)))
        for fixture in self.suite.fixtures:
            self.assertTrue(fixture.cases)
            self.assertTrue(fixture.logical_args)

    def test_case_ids_are_globally_unique_and_expressions_nonempty(self) -> None:
        """Reject ambiguous frame identities and empty authored PHP expressions."""

        case_ids = []
        for fixture in self.suite.fixtures:
            self.assertTrue(fixture.preamble.strip())
            for case in fixture.cases:
                case_ids.append(case.identifier)
                self.assertTrue(case.expression.strip())
        self.assertEqual(len(case_ids), len(set(case_ids)))

    def test_contract_references_sources_without_copying_normative_pins(self) -> None:
        """Keep the generated fixture contract pin-free and loader-oriented."""

        contract = json.loads((FIXTURE_ROOT / "contract.json").read_text(encoding="utf-8"))
        self.assertEqual(
            contract["references"],
            {
                "inventory": "docs/specs/wasm-inventory.json",
                "specification": "docs/specs/wasm-compliance.md",
            },
        )
        self.assertFalse(contains_forbidden_contract_key(contract))
        self.assertEqual(
            contract["execution"]["guest_env"],
            {"LANG": "C.UTF-8", "LC_ALL": "C.UTF-8", "TZ": "UTC"},
        )
        self.assertEqual(contract["execution"]["guest_program"], "oracle.php")
        self.assertEqual(contract["execution"]["guest_preopens"], {})
        self.assertEqual(contract["execution"]["timeout_seconds"], 10)
        self.assertEqual(contract["execution"]["max_output_bytes"], 1_048_576)

    def test_rendering_is_deterministic_and_committed_bytes_are_current(self) -> None:
        """Render twice identically and require every committed output byte to match."""

        first = generate.render_outputs(REPO_ROOT)
        second = generate.render_outputs(REPO_ROOT)
        self.assertEqual(first, second)
        self.assertEqual(generate.check_outputs(first), [])
        self.assertEqual(len(first), 5)

    def test_generated_bool_cases_use_inline_one_or_zero_branches(self) -> None:
        """Ensure boolean payloads are inline branches, never a fragile helper result."""

        for fixture in self.suite.fixtures:
            source = (
                FIXTURE_ROOT / "generated" / f"{fixture.identifier}.php"
            ).read_text(encoding="utf-8")
            self.assertNotIn("function oracle_bool", source)
            bool_count = sum(case.value_type == "bool" for case in fixture.cases)
            self.assertEqual(source.count(" === true) {"), bool_count)
            self.assertEqual(source.count(" === false) {"), bool_count)
            self.assertGreaterEqual(source.count(":4:bool:1:1\\n"), bool_count)
            self.assertGreaterEqual(source.count(":4:bool:1:0\\n"), bool_count)

    def test_generated_scalar_payloads_check_their_runtime_types(self) -> None:
        """Guard scalar framing with PHP strict identity against its exact cast."""

        for fixture in self.suite.fixtures:
            source = (
                FIXTURE_ROOT / "generated" / f"{fixture.identifier}.php"
            ).read_text(encoding="utf-8")
            int_count = sum(case.value_type == "int" for case in fixture.cases)
            string_count = sum(case.value_type == "string" for case in fixture.cases)
            self.assertEqual(source.count(" !== (int) "), int_count)
            self.assertEqual(source.count(" !== (string) "), string_count)

    def test_parser_preserves_binary_payload_boundaries(self) -> None:
        """Parse colons, newlines, NUL, and high bytes strictly by declared length."""

        payload = b"left:right\nnext\x00\xff"
        encoded = encode_frame("binary_case", "string", payload)
        frames = generate.parse_frames(
            encoded,
            [generate.FrameExpectation("binary_case", "string")],
        )
        self.assertEqual(frames, [generate.ParsedFrame("binary_case", "string", payload)])

    def test_parser_rejects_missing_extra_duplicate_reordered_and_trailing_frames(self) -> None:
        """Fail closed for every frame-set and byte-stream ambiguity."""

        first = encode_frame("first_case", "bool", b"1")
        second = encode_frame("second_case", "int", b"2")
        extra = encode_frame("extra_case", "null", b"")
        expected = [
            generate.FrameExpectation("first_case", "bool"),
            generate.FrameExpectation("second_case", "int"),
        ]
        failures = {
            "missing": first,
            "extra": first + second + extra,
            "duplicate": first + first + second,
            "reordered": second + first,
            "trailing": first + second + b"x",
        }
        for label, data in failures.items():
            with self.subTest(label=label):
                with self.assertRaises(generate.FrameProtocolError):
                    generate.parse_frames(data, expected)

    def test_parser_rejects_noncanonical_lengths_and_wrong_types(self) -> None:
        """Reject leading-zero lengths, invalid expectations, and type disagreement."""

        malformed = b"@01:a:4:bool:1:1\n"
        with self.assertRaises(generate.FrameProtocolError):
            generate.parse_frames(malformed)
        with self.assertRaises(generate.FrameProtocolError):
            generate.parse_frames(
                encode_frame("typed_case", "int", b"1"),
                [generate.FrameExpectation("typed_case", "bool")],
            )
        with self.assertRaises(generate.FrameProtocolError):
            generate.parse_frames(
                encode_frame("typed_case", "int", b"1"),
                [
                    generate.FrameExpectation("typed_case", "int"),
                    generate.FrameExpectation("typed_case", "int"),
                ],
            )
        with self.assertRaises(generate.FrameProtocolError):
            generate.parse_frames(
                encode_frame("typed_case", "int", b"1"),
                [generate.FrameExpectation("typed_case", "float")],
            )

    def test_parser_rejects_payloads_incompatible_with_declared_types(self) -> None:
        """Enforce canonical booleans, nulls, and signed 64-bit integer payloads."""

        invalid = (
            ("bool", b"true"),
            ("bool", b"2"),
            ("null", b"null"),
            ("int", b"01"),
            ("int", b"+1"),
            ("int", b"-0"),
            ("int", str(1 << 63).encode("ascii")),
        )
        for value_type, payload in invalid:
            with self.subTest(value_type=value_type, payload=payload):
                with self.assertRaises(generate.FrameProtocolError):
                    generate.parse_frames(
                        encode_frame("typed_case", value_type, payload)
                    )
        self.assertEqual(
            generate.parse_frames(
                encode_frame("typed_case", "int", b"-9223372036854775808")
            )[0].payload,
            b"-9223372036854775808",
        )

    def test_matrix_validation_rejects_unknown_keys_and_control_ids(self) -> None:
        """Require exact case/matrix schemas and control-free stable identifiers."""

        valid_case = {"id": "valid_case", "type": "bool", "expression": "true"}
        unknown = copy.deepcopy(valid_case)
        unknown["expected"] = "1"
        with self.assertRaises(generate.OracleDefinitionError):
            generate.validate_case(unknown, "case")
        control = copy.deepcopy(valid_case)
        control["id"] = "bad\ncase"
        with self.assertRaises(generate.OracleDefinitionError):
            generate.validate_case(control, "case")
        empty = copy.deepcopy(valid_case)
        empty["expression"] = " "
        with self.assertRaises(generate.OracleDefinitionError):
            generate.validate_case(empty, "case")

    def test_path_validation_rejects_absolute_and_parent_traversal(self) -> None:
        """Confine all authored and generated paths to their declared roots."""

        for value in ("/tmp/matrix.json", "../matrix.json", "matrices/../x.json"):
            with self.subTest(value=value):
                with self.assertRaises(generate.OracleDefinitionError):
                    generate.require_relative_path(value, "test.path")

    def test_json_loader_rejects_duplicate_keys_at_every_depth(self) -> None:
        """Reject ambiguous authored JSON even when duplicate keys are nested."""

        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "duplicate.json"
            path.write_text(
                '{"outer":{"value":1,"value":2}}',
                encoding="utf-8",
            )
            with self.assertRaisesRegex(
                generate.OracleDefinitionError,
                "duplicate JSON key 'value'",
            ):
                generate.load_json(path, "duplicate fixture")

    def test_local_php_syntax_and_frames_are_structurally_valid(self) -> None:
        """When PHP exists, lint/run sources and validate framing without oracle claims."""

        php = shutil.which("php")
        if php is None:
            self.skipTest("local PHP CLI is unavailable")
        clean_env = {
            "LANG": "C.UTF-8",
            "LC_ALL": "C.UTF-8",
            "TZ": "UTC",
        }
        for fixture in self.suite.fixtures:
            source = FIXTURE_ROOT / "generated" / f"{fixture.identifier}.php"
            with self.subTest(fixture=fixture.identifier, phase="lint"):
                lint = subprocess.run(
                    [php, "-n", "-l", str(source)],
                    cwd=REPO_ROOT,
                    env=clean_env,
                    capture_output=True,
                    timeout=10,
                    check=False,
                )
                self.assertEqual(lint.returncode, 0, lint.stderr.decode(errors="replace"))
            with self.subTest(fixture=fixture.identifier, phase="run"):
                run = subprocess.run(
                    [php, "-n", str(source), *fixture.logical_args],
                    cwd=REPO_ROOT,
                    env=clean_env,
                    capture_output=True,
                    timeout=10,
                    check=False,
                )
                self.assertEqual(run.returncode, 0, run.stderr.decode(errors="replace"))
                self.assertEqual(run.stderr, b"")
                self.assertLessEqual(len(run.stdout), 1_048_576)
                frames = generate.parse_frames(run.stdout, expected_frames(fixture))
                self.assertEqual(len(frames), len(fixture.cases))


if __name__ == "__main__":
    unittest.main()
