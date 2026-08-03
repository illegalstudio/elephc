"""Fail-closed aggregation tests for the committed WASM oracle fixture suite."""

from __future__ import annotations

import base64
import json
import subprocess
import sys
import tempfile
import unittest
from dataclasses import replace
from pathlib import Path

from scripts.wasm_oracle import (
    AggregateError,
    CaptureRecord,
    CompilerArtifactProvenance,
    Normalization,
    OracleContract,
    RawBytes,
    RunKey,
    RuntimeProvenance,
    aggregate_generated_suite,
    sha256_file,
)
from scripts.wasm_oracle import generate


REPO_ROOT = Path(__file__).resolve().parents[2]
PYTHON = Path(sys.executable).resolve()
ELEPHC_SOURCE_COMMIT = "1" * 40


def encode_frame(identifier: str, value_type: str, payload: bytes) -> bytes:
    """Encode one canonical observation frame for suite aggregation tests."""

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


def fixture_stdout(fixture: generate.OracleFixture) -> bytes:
    """Return structurally valid deterministic frames for one fixture model."""

    payloads = {
        "bool": b"1",
        "int": b"42",
        "null": b"",
        "string": b"value\x00binary",
    }
    return b"".join(
        encode_frame(case.identifier, case.value_type, payloads[case.value_type])
        for case in fixture.cases
    )


class WasmOracleSuiteEvidenceTests(unittest.TestCase):
    """Verify exact suite semantics before the generic matrix aggregation."""

    @classmethod
    def setUpClass(cls) -> None:
        """Load the frozen contract, fixtures, and stable synthetic tool identity."""

        cls.contract = OracleContract.load(REPO_ROOT)
        cls.suite = generate.load_suite(REPO_ROOT)
        cls.python_sha256 = sha256_file(PYTHON)
        cls.environment = {
            "LANG": "C.UTF-8",
            "LC_ALL": "C.UTF-8",
            "TZ": "UTC",
        }
        cls.artifact = CompilerArtifactProvenance.create(
            elephc_source_commit=ELEPHC_SOURCE_COMMIT,
            compiler_executable_sha256="2" * 64,
            compiler_version="elephc-test",
            wat_sha256="3" * 64,
            wasm_sha256="4" * 64,
            validated_artifact_sha256="4" * 64,
            index_mjs_sha256="5" * 64,
            package_json_sha256="6" * 64,
        )

    def runtime_provenance(
        self,
        profile: str,
        runtime: str,
        host: str,
    ) -> RuntimeProvenance:
        """Create contract-valid synthetic provenance for one execution cell."""

        if runtime == "php-src":
            pin = self.contract.php_src_pin(profile)
            return RuntimeProvenance.create(
                executable_sha256=self.python_sha256,
                version=pin.tag.removeprefix("php-"),
                source_commit=pin.tag_commit,
                build_configuration={
                    "configure_command": "./configure --disable-all",
                    "build_flags": "CFLAGS=-O2",
                },
                ini_mode="php-n",
                ini_sha256=None,
                extensions=(),
            )
        return RuntimeProvenance.create(
            executable_sha256=self.python_sha256,
            version=dict(self.contract.toolchain)[host],
            source_commit=None,
            build_configuration={"adapter_mode": "test"},
            ini_mode="not-applicable",
            ini_sha256=None,
            extensions=(),
        )

    def record(
        self,
        fixture: generate.OracleFixture,
        profile: str,
        runtime: str,
        host: str,
    ) -> CaptureRecord:
        """Build one exact suite record without launching external runtimes."""

        pin = self.contract.php_src_pin(profile)
        source = (
            generate.fixture_root(REPO_ROOT)
            / "generated"
            / f"{fixture.identifier}.php"
        )
        stdout = fixture_stdout(fixture)
        normalization = Normalization()
        return CaptureRecord(
            key=RunKey(fixture.identifier, profile, runtime, host),
            specification_sha256=self.contract.specification_sha256,
            inventory_sha256=self.contract.inventory_sha256,
            pinned_php_src_tag=pin.tag,
            pinned_php_src_tag_object=pin.tag_object,
            pinned_php_src_tag_commit=pin.tag_commit,
            fixture_sha256=sha256_file(source),
            guest_program="oracle.php",
            logical_arguments=fixture.logical_args,
            guest_preopens=(),
            argv=("/absolute/oracle-adapter",),
            cwd="/absolute/oracle-work",
            host_control_environment=tuple(sorted(self.environment.items())),
            guest_environment=tuple(
                sorted(generate.EXPECTED_GUEST_ENV.items())
            ),
            process_environment=tuple(
                sorted(
                    (
                        generate.EXPECTED_GUEST_ENV
                        if runtime == "php-src"
                        else self.environment
                    ).items()
                )
            ),
            operating_system="test-os",
            architecture="test-architecture",
            provenance=self.runtime_provenance(profile, runtime, host),
            artifact_provenance=(
                None if runtime == "php-src" else self.artifact
            ),
            stdin=RawBytes.from_bytes(
                base64.b64decode(fixture.stdin_base64, validate=True)
            ),
            stdout=RawBytes.from_bytes(stdout),
            stderr=RawBytes.from_bytes(b""),
            normalization=normalization,
            normalized_stdout=RawBytes.from_bytes(stdout),
            normalized_stderr=RawBytes.from_bytes(b""),
            host_status=0,
            host_status_representation="posix_process_status",
            signal=None,
            module_i32_bits=0 if host == "node" else None,
            timed_out=False,
            output_limit_exceeded=False,
            timeout_seconds=generate.EXPECTED_TIMEOUT_SECONDS,
            output_limit_bytes=generate.EXPECTED_MAX_OUTPUT_BYTES,
            elapsed_seconds=0.01,
        )

    def exact_records(self) -> list[CaptureRecord]:
        """Return all 64 fixture/profile/runtime-host cells exactly once."""

        records = []
        for fixture in self.suite.fixtures:
            for profile in self.contract.profiles:
                records.append(
                    self.record(fixture, profile, "php-src", "php-src")
                )
                for host in ("node", "wasmer", "wasmtime"):
                    records.append(self.record(fixture, profile, "wasm", host))
        return records

    def test_generated_suite_aggregates_all_sixty_four_exact_cells(self) -> None:
        """Accept the exact suite only after source, inputs, frames, and limits match."""

        result = aggregate_generated_suite(self.contract, self.exact_records())
        self.assertEqual(len(result.records), 64)
        self.assertEqual(len(result.comparisons), 48)
        self.assertTrue(all(comparison.matched for comparison in result.comparisons))

    def test_generated_suite_rejects_guest_and_frame_drift(self) -> None:
        """Reject guest argv0 drift and structurally malformed captured frames."""

        records = self.exact_records()
        records[0] = replace(records[0], guest_program="different.php")
        with self.assertRaisesRegex(AggregateError, r"argv\[0\]"):
            aggregate_generated_suite(self.contract, records)

        records = self.exact_records()
        malformed = RawBytes.from_bytes(b"not-a-frame")
        records[0] = replace(
            records[0],
            stdout=malformed,
            normalized_stdout=malformed,
        )
        with self.assertRaisesRegex(AggregateError, "invalid frames"):
            aggregate_generated_suite(self.contract, records)

        records = self.exact_records()
        records[0] = replace(
            records[0],
            normalization=Normalization(((b"never", b"normalized"),)),
        )
        with self.assertRaisesRegex(AggregateError, "normalization must be empty"):
            aggregate_generated_suite(self.contract, records)

    def test_cli_aggregate_suite_is_exclusive_and_fail_closed(self) -> None:
        """Create one suite aggregate and refuse to overwrite accepted evidence."""

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            record_paths = []
            for index, record in enumerate(self.exact_records()):
                path = root / f"record-{index:02d}.json"
                path.write_text(
                    json.dumps(record.to_dict()),
                    encoding="utf-8",
                )
                record_paths.append(path)
            output = root / "aggregate.json"
            command = [
                str(PYTHON),
                str(REPO_ROOT / "scripts/wasm_php_oracle.py"),
                "--repo-root",
                str(REPO_ROOT),
                "aggregate-suite",
                "--output",
                str(output),
            ]
            for path in record_paths:
                command.extend(("--record", str(path)))

            first = subprocess.run(
                command,
                cwd=REPO_ROOT,
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            self.assertEqual(first.returncode, 0, first.stderr.decode())
            document = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(document["matrix"]["actual_record_count"], 64)

            second = subprocess.run(
                command,
                cwd=REPO_ROOT,
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            self.assertEqual(second.returncode, 2)
            self.assertIn(b"refusing to overwrite", second.stderr)


if __name__ == "__main__":
    unittest.main()
