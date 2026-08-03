from __future__ import annotations

import copy
import json
import subprocess
import sys
import tempfile
import unittest
from dataclasses import replace
from pathlib import Path

from scripts.wasm_oracle import (
    AggregateError,
    CaptureError,
    CaptureRecord,
    CaptureRequest,
    CompilerArtifactProvenance,
    ContractError,
    MODULE_STATUS_FD_ENV,
    Normalization,
    OracleContract,
    RawBytes,
    RunKey,
    RuntimeProvenance,
    aggregate_exact,
    capture_process,
    compare_records,
    sha256_bytes,
    sha256_file,
)


REPO_ROOT = Path(__file__).resolve().parents[2]
PYTHON = Path(sys.executable).resolve()
FIXTURE_BYTES = b"<?php echo 'oracle';"
FIXTURE_SHA256 = sha256_bytes(FIXTURE_BYTES)
WASM_SOURCE_COMMIT = "1" * 40


class WasmPhpOracleTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.contract = OracleContract.load(REPO_ROOT)
        cls.python_sha256 = sha256_file(PYTHON)
        cls.environment = {
            "LANG": "C.UTF-8",
            "LC_ALL": "C.UTF-8",
            "TZ": "UTC",
        }

    def php_provenance(self, profile: str) -> RuntimeProvenance:
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

    def wasm_provenance(self, host: str) -> RuntimeProvenance:
        return RuntimeProvenance.create(
            executable_sha256=self.python_sha256,
            version=dict(self.contract.toolchain)[host],
            source_commit=None,
            build_configuration={
                "adapter_mode": (
                    "full-i32-side-channel"
                    if host == "node"
                    else "posix-process-status"
                ),
            },
            ini_mode="not-applicable",
            ini_sha256=None,
            extensions=(),
        )

    def artifact_provenance(
        self,
        *,
        elephc_source_commit: str = WASM_SOURCE_COMMIT,
        wasm_sha256: str = "2" * 64,
    ) -> CompilerArtifactProvenance:
        return CompilerArtifactProvenance.create(
            elephc_source_commit=elephc_source_commit,
            compiler_executable_sha256="4" * 64,
            compiler_version="elephc-test",
            wat_sha256="3" * 64,
            wasm_sha256=wasm_sha256,
            validated_artifact_sha256=wasm_sha256,
            index_mjs_sha256="5" * 64,
            package_json_sha256="6" * 64,
        )

    def capture_request(
        self,
        *,
        runtime: str,
        host: str | None = None,
        code: str,
        profile: str = "8.2",
        timeout_seconds: float = 2.0,
        output_limit_bytes: int = 1024,
        normalization: Normalization = Normalization(),
        environment: dict[str, str] | None = None,
        guest_environment: dict[str, str] | None = None,
        fixture_sha256: str = FIXTURE_SHA256,
    ) -> CaptureRequest:
        selected_host = host or ("php-src" if runtime == "php-src" else "node")
        provenance = (
            self.php_provenance(profile)
            if runtime == "php-src"
            else self.wasm_provenance(selected_host)
        )
        return CaptureRequest(
            key=RunKey("strict-equality", profile, runtime, selected_host),
            fixture_sha256=fixture_sha256,
            guest_program="oracle.php",
            logical_arguments=("--case", "binary"),
            guest_preopens={},
            argv=(str(PYTHON), "-c", code),
            cwd=REPO_ROOT,
            host_control_environment=dict(
                self.environment if environment is None else environment
            ),
            guest_environment=dict(
                self.environment
                if guest_environment is None
                else guest_environment
            ),
            stdin=b"\x00stdin\xff",
            timeout_seconds=timeout_seconds,
            output_limit_bytes=output_limit_bytes,
            provenance=provenance,
            artifact_provenance=(
                None if runtime == "php-src" else self.artifact_provenance()
            ),
            normalization=normalization,
            require_module_i32=selected_host == "node",
        )

    def direct_record(
        self,
        *,
        fixture_id: str,
        profile: str,
        runtime: str,
        host: str | None = None,
        stdout: bytes = b"same\x00\xff",
        stderr: bytes = b"",
        host_status: int = 0,
        module_i32_bits: int | None = None,
        fixture_sha256: str = FIXTURE_SHA256,
    ) -> CaptureRecord:
        pin = self.contract.php_src_pin(profile)
        selected_host = host or ("php-src" if runtime == "php-src" else "node")
        provenance = (
            self.php_provenance(profile)
            if runtime == "php-src"
            else self.wasm_provenance(selected_host)
        )
        normalization = Normalization()
        return CaptureRecord(
            key=RunKey(fixture_id, profile, runtime, selected_host),
            specification_sha256=self.contract.specification_sha256,
            inventory_sha256=self.contract.inventory_sha256,
            pinned_php_src_tag=pin.tag,
            pinned_php_src_tag_object=pin.tag_object,
            pinned_php_src_tag_commit=pin.tag_commit,
            fixture_sha256=fixture_sha256,
            guest_program="oracle.php",
            logical_arguments=("--case", "binary"),
            guest_preopens=(),
            argv=("/absolute/oracle-adapter",),
            cwd="/absolute/oracle-work",
            host_control_environment=tuple(sorted(self.environment.items())),
            guest_environment=tuple(sorted(self.environment.items())),
            process_environment=tuple(sorted(self.environment.items())),
            operating_system="test-os",
            architecture="test-architecture",
            provenance=provenance,
            artifact_provenance=(
                None if runtime == "php-src" else self.artifact_provenance()
            ),
            stdin=RawBytes.from_bytes(b"\x00stdin\xff"),
            stdout=RawBytes.from_bytes(stdout),
            stderr=RawBytes.from_bytes(stderr),
            normalization=normalization,
            normalized_stdout=RawBytes.from_bytes(stdout),
            normalized_stderr=RawBytes.from_bytes(stderr),
            host_status=host_status,
            host_status_representation="posix_process_status",
            signal=None,
            module_i32_bits=(
                module_i32_bits if selected_host == "node" else None
            ),
            timed_out=False,
            output_limit_exceeded=False,
            timeout_seconds=2.0,
            output_limit_bytes=1024,
            elapsed_seconds=0.01,
        )

    def exact_records(self, fixture_id: str = "strict-equality") -> list[CaptureRecord]:
        records: list[CaptureRecord] = []
        for profile in self.contract.profiles:
            records.append(
                self.direct_record(
                    fixture_id=fixture_id,
                    profile=profile,
                    runtime="php-src",
                )
            )
            for host in ("node", "wasmer", "wasmtime"):
                records.append(
                    self.direct_record(
                        fixture_id=fixture_id,
                        profile=profile,
                        runtime="wasm",
                        host=host,
                        module_i32_bits=(0 if host == "node" else None),
                    )
                )
        return records

    def test_contract_loads_v4_pins_and_verifies_specification_hash(self) -> None:
        self.assertEqual(self.contract.inventory_schema, "elephc.wasm-inventory.v4")
        self.assertEqual(
            self.contract.specification_sha256,
            "5865de45b4c9b4f9e3d6d11f4bb5b9fa088eafaec5a350e0142c1a66e3fad061",
        )
        self.assertEqual(self.contract.profiles, ("8.2", "8.3", "8.4", "8.5"))
        pin = self.contract.php_src_pin("8.2")
        self.assertEqual(
            pin.tag_object, "fa98f62b39a612ae88b7be5d5ea9ff9b794b454b"
        )
        self.assertEqual(
            pin.tag_commit, "651db3ebfa622cae0c4e6b39766812efbd274ced"
        )

    def test_contract_rejects_specification_hash_drift(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            inventory = root / "inventory.json"
            specification = root / "spec.md"
            inventory.write_bytes(
                (REPO_ROOT / "docs/specs/wasm-inventory.json").read_bytes()
            )
            specification.write_bytes(
                (REPO_ROOT / "docs/specs/wasm-compliance.md").read_bytes()
                + b"\ndrift\n"
            )
            with self.assertRaisesRegex(ContractError, "hash mismatch"):
                OracleContract.load(
                    root,
                    inventory_path=inventory,
                    specification_path=specification,
                )

    def test_contract_rejects_ambiguous_unpeeled_php_commit(self) -> None:
        source = json.loads(
            (REPO_ROOT / "docs/specs/wasm-inventory.json").read_text()
        )
        metadata = copy.deepcopy(source["metadata"])
        first = metadata["pins"]["php_src"][0]
        first["commit"] = first.pop("tag_object")
        first.pop("tag_commit")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            inventory = root / "inventory.json"
            specification = root / "spec.md"
            inventory.write_text(json.dumps({"metadata": metadata}))
            specification.write_bytes(
                (REPO_ROOT / "docs/specs/wasm-compliance.md").read_bytes()
            )
            with self.assertRaisesRegex(ContractError, "tag_object/tag_commit"):
                OracleContract.load(
                    root,
                    inventory_path=inventory,
                    specification_path=specification,
                )

    def test_contract_rejects_duplicate_json_keys(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            inventory = root / "inventory.json"
            specification = root / "spec.md"
            inventory.write_text('{"metadata": {}, "metadata": {}}')
            specification.write_text("spec")
            with self.assertRaisesRegex(ContractError, "duplicate JSON key"):
                OracleContract.load(
                    root,
                    inventory_path=inventory,
                    specification_path=specification,
                )

    def test_run_key_rejects_missing_or_mismatched_host_dimension(self) -> None:
        with self.assertRaisesRegex(ContractError, "runtime/host cell"):
            RunKey("strict-equality", "8.2", "wasm", "")
        with self.assertRaisesRegex(ContractError, "runtime/host cell"):
            RunKey("strict-equality", "8.2", "php-src", "wasmer")

    def test_capture_preserves_raw_binary_stdout_stderr_and_status(self) -> None:
        record = capture_process(
            self.contract,
            self.capture_request(
                runtime="php-src",
                code=(
                    "import sys;"
                    "sys.stdout.buffer.write(b'\\x00\\xffout');"
                    "sys.stderr.buffer.write(b'err\\x00\\xfe');"
                    "raise SystemExit(7)"
                ),
            ),
        )
        self.assertEqual(record.stdout.to_bytes(), b"\x00\xffout")
        self.assertEqual(record.stderr.to_bytes(), b"err\x00\xfe")
        self.assertEqual(record.stdout.length, 5)
        self.assertEqual(record.stdout.sha256, sha256_bytes(b"\x00\xffout"))
        self.assertEqual(record.host_status, 7)
        self.assertIsNone(record.signal)
        self.assertIsNone(record.module_i32_bits)
        self.assertEqual(record.stdin.to_bytes(), b"\x00stdin\xff")

    def test_capture_retains_exact_module_i32_bits(self) -> None:
        code = (
            "import os;"
            f"fd=int(os.environ['{MODULE_STATUS_FD_ENV}']);"
            "os.write(fd,b'ffffffff\\n');"
            "os.close(fd)"
        )
        record = capture_process(
            self.contract,
            self.capture_request(runtime="wasm", code=code),
        )
        self.assertEqual(record.module_i32_bits, 0xFFFF_FFFF)
        self.assertEqual(record.module_i32_signed, -1)

    def test_cli_hosts_record_posix_status_without_inventing_full_i32(self) -> None:
        for host in ("wasmer", "wasmtime"):
            record = capture_process(
                self.contract,
                self.capture_request(runtime="wasm", host=host, code="pass"),
            )
            self.assertEqual(record.host_status, 0)
            self.assertEqual(
                record.host_status_representation,
                "posix_process_status",
            )
            self.assertIsNone(record.module_i32_bits)

    def test_capture_rejects_missing_or_malformed_module_i32(self) -> None:
        with self.assertRaisesRegex(CaptureError, "exactly 8 lowercase"):
            capture_process(
                self.contract,
                self.capture_request(runtime="wasm", code="pass"),
            )
        malformed = (
            "import os;"
            f"fd=int(os.environ['{MODULE_STATUS_FD_ENV}']);"
            "os.write(fd,b'FFFFFFFF\\n');"
            "os.close(fd)"
        )
        with self.assertRaisesRegex(CaptureError, "exactly 8 lowercase"):
            capture_process(
                self.contract,
                self.capture_request(runtime="wasm", code=malformed),
            )

    def test_capture_records_timeout_and_output_limit(self) -> None:
        timeout = capture_process(
            self.contract,
            self.capture_request(
                runtime="php-src",
                code="import time; time.sleep(1)",
                timeout_seconds=0.05,
            ),
        )
        self.assertTrue(timeout.timed_out)
        self.assertIsNotNone(timeout.signal)

        limited = capture_process(
            self.contract,
            self.capture_request(
                runtime="php-src",
                code="import sys; sys.stdout.buffer.write(b'x'*10000)",
                output_limit_bytes=32,
            ),
        )
        self.assertTrue(limited.output_limit_exceeded)
        self.assertEqual(limited.stdout.length, 32)

        combined = capture_process(
            self.contract,
            self.capture_request(
                runtime="php-src",
                code=(
                    "import os;"
                    "os.write(1,b'o'*24);"
                    "os.write(2,b'e'*24)"
                ),
                output_limit_bytes=32,
            ),
        )
        self.assertTrue(combined.output_limit_exceeded)
        self.assertLessEqual(
            combined.stdout.length + combined.stderr.length,
            32,
        )

    def test_capture_applies_only_declared_literal_normalization(self) -> None:
        normalization = Normalization(((b"/tmp/source.php", b"<SOURCE>"),))
        record = capture_process(
            self.contract,
            self.capture_request(
                runtime="php-src",
                code="import sys;sys.stdout.buffer.write(b'/tmp/source.php\\r\\n ')",
                normalization=normalization,
            ),
        )
        self.assertEqual(record.stdout.to_bytes(), b"/tmp/source.php\r\n ")
        self.assertEqual(record.normalized_stdout.to_bytes(), b"<SOURCE>\r\n ")

    def test_php_capture_executes_with_the_logical_guest_environment(self) -> None:
        """php-src sees the guest mapping, not the adapter's host environment."""

        record = capture_process(
            self.contract,
            self.capture_request(
                runtime="php-src",
                code=(
                    "import os,sys;"
                    "sys.stdout.write('|'.join(sorted(os.environ)))"
                ),
                guest_environment={"ORACLE_CASE": "explicit"},
            ),
        )

        observed = set(record.stdout.to_bytes().decode("ascii").split("|"))
        self.assertIn("ORACLE_CASE", observed)
        self.assertTrue({"LANG", "LC_ALL", "TZ"}.isdisjoint(observed))
        self.assertEqual(
            record.guest_environment,
            (("ORACLE_CASE", "explicit"),),
        )
        self.assertEqual(
            dict(record.host_control_environment),
            self.environment,
        )
        self.assertEqual(
            dict(record.process_environment),
            {"ORACLE_CASE": "explicit"},
        )

    def test_capture_rejects_missing_environment_and_wrong_fixture_hash(self) -> None:
        environment = dict(self.environment)
        environment.pop("TZ")
        with self.assertRaisesRegex(CaptureError, "environment must be exactly"):
            capture_process(
                self.contract,
                self.capture_request(
                    runtime="php-src",
                    code="pass",
                    environment=environment,
                ),
            )
        environment = dict(self.environment)
        environment["PATH"] = "/not-guest-state"
        with self.assertRaisesRegex(CaptureError, "environment must be exactly"):
            capture_process(
                self.contract,
                self.capture_request(
                    runtime="php-src",
                    code="pass",
                    environment=environment,
                ),
            )
        environment = dict(self.environment)
        environment["LANG"] = "C"
        with self.assertRaisesRegex(CaptureError, "environment must be exactly"):
            capture_process(
                self.contract,
                self.capture_request(
                    runtime="php-src",
                    code="pass",
                    environment=environment,
                ),
            )
        with self.assertRaisesRegex(CaptureError, "fixture_sha256"):
            capture_process(
                self.contract,
                self.capture_request(
                    runtime="php-src",
                    code="pass",
                    fixture_sha256="not-a-hash",
                ),
            )

    def test_capture_rejects_implicit_guest_programs_and_preopens(self) -> None:
        """Require explicit canonical guest argv0 and absolute preopen mappings."""

        request = self.capture_request(runtime="php-src", code="pass")
        with self.assertRaisesRegex(CaptureError, "guest_program"):
            capture_process(
                self.contract,
                replace(request, guest_program=""),
            )
        with self.assertRaisesRegex(CaptureError, "guest path"):
            capture_process(
                self.contract,
                replace(request, guest_preopens={"work": "/absolute/work"}),
            )
        with self.assertRaisesRegex(CaptureError, "must be absolute"):
            capture_process(
                self.contract,
                replace(request, guest_preopens={"/work": "relative/work"}),
            )

    def test_capture_record_roundtrip_rejects_unknown_or_corrupt_fields(self) -> None:
        record = self.direct_record(
            fixture_id="strict-equality",
            profile="8.2",
            runtime="wasm",
            module_i32_bits=0,
        )
        encoded = record.to_dict()
        decoded = CaptureRecord.from_dict(encoded, self.contract)
        self.assertEqual(decoded, record)

        unknown = copy.deepcopy(encoded)
        unknown["unknown"] = True
        with self.assertRaisesRegex(CaptureError, "fields must be exactly"):
            CaptureRecord.from_dict(unknown, self.contract)

        corrupt = copy.deepcopy(encoded)
        corrupt["output"]["stdout"]["sha256"] = "0" * 64
        with self.assertRaisesRegex(CaptureError, "SHA-256 mismatch"):
            CaptureRecord.from_dict(corrupt, self.contract)

    def test_compare_records_detects_bytes_arguments_and_module_status(self) -> None:
        reference = self.direct_record(
            fixture_id="strict-equality",
            profile="8.2",
            runtime="php-src",
        )
        candidate = self.direct_record(
            fixture_id="strict-equality",
            profile="8.2",
            runtime="wasm",
            stdout=b"different",
            module_i32_bits=1,
        )
        candidate = replace(
            candidate,
            guest_program="different.php",
            logical_arguments=("--other",),
            guest_preopens=(("/work", "/absolute/work"),),
            guest_environment=(("ORACLE_CASE", "different"),),
        )
        result = compare_records(reference, candidate)
        self.assertFalse(result.matched)
        self.assertEqual(
            {difference.field for difference in result.differences},
            {
                "guest_program",
                "logical_arguments",
                "guest_preopens",
                "guest_environment",
                "stdout",
                "module_i32_low_byte",
            },
        )

    def test_comparator_keeps_host_status_distinct_from_full_node_i32(self) -> None:
        reference = self.direct_record(
            fixture_id="negative-exit",
            profile="8.2",
            runtime="php-src",
            host_status=255,
        )
        node = self.direct_record(
            fixture_id="negative-exit",
            profile="8.2",
            runtime="wasm",
            host="node",
            host_status=255,
            module_i32_bits=0xFFFF_FFFF,
        )
        wasmer = self.direct_record(
            fixture_id="negative-exit",
            profile="8.2",
            runtime="wasm",
            host="wasmer",
            host_status=255,
        )
        self.assertTrue(compare_records(reference, node).matched)
        self.assertTrue(compare_records(reference, wasmer).matched)

    def test_aggregate_accepts_only_the_exact_matching_cartesian_product(self) -> None:
        records = self.exact_records()
        result = aggregate_exact(
            self.contract,
            ("strict-equality",),
            records,
        )
        self.assertEqual(len(result.records), 16)
        self.assertEqual(len(result.comparisons), 12)
        self.assertEqual(result.elephc_source_commit, WASM_SOURCE_COMMIT)
        self.assertTrue(all(item.matched for item in result.comparisons))

    def test_aggregate_rejects_missing_extra_and_duplicate_records(self) -> None:
        records = self.exact_records()
        with self.assertRaisesRegex(AggregateError, "missing:"):
            aggregate_exact(
                self.contract,
                ("strict-equality",),
                records[:-1],
            )
        with self.assertRaisesRegex(AggregateError, "duplicate"):
            aggregate_exact(
                self.contract,
                ("strict-equality",),
                records + [records[0]],
            )
        extra = self.direct_record(
            fixture_id="unexpected",
            profile="8.2",
            runtime="wasm",
            host="node",
            module_i32_bits=0,
        )
        with self.assertRaisesRegex(AggregateError, "extra:"):
            aggregate_exact(
                self.contract,
                ("strict-equality",),
                records + [extra],
            )

    def test_aggregate_rejects_mismatch_timeout_and_mixed_elephc_commits(self) -> None:
        records = self.exact_records()
        mismatch = self.direct_record(
            fixture_id="strict-equality",
            profile="8.2",
            runtime="wasm",
            stdout=b"mismatch",
            module_i32_bits=0,
        )
        records[1] = mismatch
        with self.assertRaisesRegex(AggregateError, "comparison mismatch"):
            aggregate_exact(
                self.contract,
                ("strict-equality",),
                records,
            )

        records = self.exact_records()
        records[0] = replace(records[0], timed_out=True)
        with self.assertRaisesRegex(AggregateError, "timed-out"):
            aggregate_exact(
                self.contract,
                ("strict-equality",),
                records,
            )

        records = self.exact_records()
        different_artifact = replace(
            records[3].artifact_provenance,
            elephc_source_commit="7" * 40,
        )
        records[3] = replace(
            records[3],
            artifact_provenance=different_artifact,
        )
        with self.assertRaisesRegex(AggregateError, "one exact Elephc"):
            aggregate_exact(
                self.contract,
                ("strict-equality",),
                records,
            )

    def test_aggregate_rejects_missing_duplicate_host_and_artifact_mismatch(self) -> None:
        records = self.exact_records()
        missing_wasmtime = [
            record
            for record in records
            if not (
                record.key.profile == "8.2"
                and record.key.host == "wasmtime"
            )
        ]
        with self.assertRaisesRegex(AggregateError, "missing:"):
            aggregate_exact(
                self.contract,
                ("strict-equality",),
                missing_wasmtime,
            )

        node = next(
            record
            for record in records
            if record.key.profile == "8.2" and record.key.host == "node"
        )
        with self.assertRaisesRegex(AggregateError, "duplicate"):
            aggregate_exact(
                self.contract,
                ("strict-equality",),
                records + [node],
            )

        wasmer_index = next(
            index
            for index, record in enumerate(records)
            if record.key.profile == "8.2" and record.key.host == "wasmer"
        )
        artifact = records[wasmer_index].artifact_provenance
        assert artifact is not None
        records[wasmer_index] = replace(
            records[wasmer_index],
            artifact_provenance=replace(
                artifact,
                index_mjs_sha256="8" * 64,
            ),
        )
        with self.assertRaisesRegex(AggregateError, "identical WAT/WASM/npm"):
            aggregate_exact(
                self.contract,
                ("strict-equality",),
                records,
            )

    def test_aggregate_rejects_compiler_and_runtime_provenance_drift(self) -> None:
        """Every shard must use the same compiler and pinned host executables."""

        fixtures = ("strict-equality-a", "strict-equality-b")
        records = self.exact_records(fixtures[0]) + self.exact_records(fixtures[1])
        for index, record in enumerate(records):
            if record.key.fixture_id != fixtures[1] or record.key.runtime != "wasm":
                continue
            artifact = record.artifact_provenance
            assert artifact is not None
            records[index] = replace(
                record,
                artifact_provenance=replace(
                    artifact,
                    compiler_executable_sha256="9" * 64,
                ),
            )
        with self.assertRaisesRegex(AggregateError, "one exact compiler"):
            aggregate_exact(self.contract, fixtures, records)

        records = self.exact_records(fixtures[0]) + self.exact_records(fixtures[1])
        node_index = next(
            index
            for index, record in enumerate(records)
            if record.key.fixture_id == fixtures[1]
            and record.key.host == "node"
        )
        records[node_index] = replace(
            records[node_index],
            provenance=replace(
                records[node_index].provenance,
                executable_sha256="a" * 64,
            ),
        )
        with self.assertRaisesRegex(AggregateError, "runtime provenance"):
            aggregate_exact(self.contract, fixtures, records)

    def test_cli_validates_and_prints_the_contract(self) -> None:
        output = subprocess.run(
            [
                str(PYTHON),
                str(REPO_ROOT / "scripts/wasm_php_oracle.py"),
                "--repo-root",
                str(REPO_ROOT),
                "contract",
            ],
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        self.assertEqual(output.returncode, 0, output.stderr.decode())
        document = json.loads(output.stdout)
        self.assertEqual(
            document["specification"]["sha256"],
            self.contract.specification_sha256,
        )

    def test_cli_aggregates_all_four_execution_cells_without_overwrite(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            record_paths: list[Path] = []
            for index, record in enumerate(self.exact_records()):
                path = root / f"record-{index}.json"
                path.write_text(json.dumps(record.to_dict()))
                record_paths.append(path)
            output_path = root / "aggregate.json"
            command = [
                str(PYTHON),
                str(REPO_ROOT / "scripts/wasm_php_oracle.py"),
                "--repo-root",
                str(REPO_ROOT),
                "aggregate",
                "--fixture",
                "strict-equality",
            ]
            for path in record_paths:
                command.extend(("--record", str(path)))
            command.extend(("--output", str(output_path)))
            first = subprocess.run(
                command,
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            self.assertEqual(first.returncode, 0, first.stderr.decode())
            aggregate = json.loads(output_path.read_text())
            self.assertEqual(aggregate["matrix"]["actual_record_count"], 16)
            self.assertEqual(len(aggregate["comparisons"]), 12)

            second = subprocess.run(
                command,
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            self.assertEqual(second.returncode, 2)
            self.assertIn(b"refusing to overwrite", second.stderr)


if __name__ == "__main__":
    unittest.main()
