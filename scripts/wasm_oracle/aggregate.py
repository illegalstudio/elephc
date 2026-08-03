"""Exact Cartesian-product validation and aggregation of oracle captures."""

from __future__ import annotations

import base64
import binascii
from dataclasses import dataclass
from itertools import product
from pathlib import Path
from typing import Any, Iterable

from .capture import CaptureError, CaptureRecord
from .comparator import ComparisonError, ComparisonResult, compare_records
from .contract import (
    ContractError,
    EXECUTION_CELLS,
    OracleContract,
    RunKey,
    load_json_file,
    sha256_file,
)
from .generate import (
    EXPECTED_GUEST_ENV,
    EXPECTED_MAX_OUTPUT_BYTES,
    EXPECTED_GUEST_PREOPENS,
    EXPECTED_GUEST_PROGRAM,
    EXPECTED_TIMEOUT_SECONDS,
    FrameExpectation,
    FrameProtocolError,
    OracleDefinitionError,
    check_outputs,
    fixture_root,
    load_suite,
    parse_frames,
    render_outputs,
)


AGGREGATE_SCHEMA = "elephc.wasm-oracle.aggregate.v3"


class AggregateError(ValueError):
    """Raised when records do not form one complete, comparable exact matrix."""


def _format_key(key: RunKey) -> str:
    return f"{key.fixture_id}/{key.profile}/{key.runtime}/{key.host}"


@dataclass(frozen=True)
class AggregateResult:
    """A complete, matching oracle matrix anchored to one contract revision."""

    contract: OracleContract
    fixture_ids: tuple[str, ...]
    execution_cells: tuple[tuple[str, str], ...]
    elephc_source_commit: str
    records: tuple[CaptureRecord, ...]
    comparisons: tuple[ComparisonResult, ...]
    schema: str = AGGREGATE_SCHEMA

    def to_dict(self) -> dict[str, Any]:
        """Serialize the complete evidence matrix without timestamps."""

        return {
            "schema": self.schema,
            "contract": self.contract.to_dict(),
            "matrix": {
                "fixture_ids": list(self.fixture_ids),
                "profiles": list(self.contract.profiles),
                "execution_cells": [
                    {"runtime": runtime, "host": host}
                    for runtime, host in self.execution_cells
                ],
                "expected_record_count": (
                    len(self.fixture_ids)
                    * len(self.contract.profiles)
                    * len(self.execution_cells)
                ),
                "actual_record_count": len(self.records),
                "elephc_source_commit": self.elephc_source_commit,
            },
            "records": [record.to_dict() for record in self.records],
            "comparisons": [
                comparison.to_dict() for comparison in self.comparisons
            ],
        }


def load_capture_record(
    path: Path, contract: OracleContract
) -> CaptureRecord:
    """Load one strict JSON capture record and validate every contract anchor."""

    try:
        document = load_json_file(path)
        return CaptureRecord.from_dict(document, contract)
    except (ContractError, CaptureError) as error:
        raise AggregateError(f"invalid capture record {path}: {error}") from error


def aggregate_exact(
    contract: OracleContract,
    fixture_ids: Iterable[str],
    records: Iterable[CaptureRecord],
    execution_cells: tuple[tuple[str, str], ...] = EXECUTION_CELLS,
) -> AggregateResult:
    """Require every fixture × profile × runtime/host cell exactly once."""

    fixtures = tuple(fixture_ids)
    if not fixtures:
        raise AggregateError("fixture_ids must not be empty")
    if len(set(fixtures)) != len(fixtures):
        raise AggregateError("fixture_ids must not contain duplicates")
    for fixture_id in fixtures:
        if (
            not isinstance(fixture_id, str)
            or not fixture_id
            or any(ord(char) < 0x20 for char in fixture_id)
        ):
            raise AggregateError(
                "fixture_ids must contain non-empty control-free strings"
            )
    if tuple(execution_cells) != EXECUTION_CELLS:
        raise AggregateError(
            f"execution_cells must be exactly {EXECUTION_CELLS}, "
            f"got {tuple(execution_cells)}"
        )

    captures = tuple(records)
    by_key: dict[RunKey, CaptureRecord] = {}
    duplicates: list[RunKey] = []
    for record in captures:
        if not isinstance(record, CaptureRecord):
            raise AggregateError("records must contain only CaptureRecord values")
        try:
            record.validate(contract)
        except CaptureError as error:
            raise AggregateError(
                f"invalid record {_format_key(record.key)}: {error}"
            ) from error
        if record.key in by_key:
            duplicates.append(record.key)
        else:
            by_key[record.key] = record
    if duplicates:
        formatted = ", ".join(
            _format_key(key) for key in sorted(set(duplicates))
        )
        raise AggregateError(f"duplicate matrix record(s): {formatted}")

    expected = {
        RunKey(fixture_id, profile, runtime, host)
        for fixture_id, profile, (runtime, host) in product(
            fixtures, contract.profiles, execution_cells
        )
    }
    actual = set(by_key)
    missing = sorted(expected - actual)
    extra = sorted(actual - expected)
    if missing or extra:
        details: list[str] = []
        if missing:
            details.append(
                "missing: " + ", ".join(_format_key(key) for key in missing)
            )
        if extra:
            details.append(
                "extra: " + ", ".join(_format_key(key) for key in extra)
            )
        raise AggregateError(
            "matrix is not an exact Cartesian product; " + "; ".join(details)
        )

    timeouts = {record.timeout_seconds for record in captures}
    output_limits = {record.output_limit_bytes for record in captures}
    if len(timeouts) != 1:
        raise AggregateError("all matrix records must use one exact timeout")
    if len(output_limits) != 1:
        raise AggregateError("all matrix records must use one exact output limit")
    environments = {record.host_control_environment for record in captures}
    if len(environments) != 1:
        raise AggregateError("all matrix records must use one exact environment")
    host_platforms = {
        (record.operating_system, record.architecture) for record in captures
    }
    if len(host_platforms) != 1:
        raise AggregateError(
            "all matrix records must use one exact host OS and architecture"
        )

    provenance_groups: dict[tuple[str, str, str], set[Any]] = {}
    for record in captures:
        profile_scope = (
            record.key.profile if record.key.runtime == "php-src" else "*"
        )
        group = (profile_scope, record.key.runtime, record.key.host)
        provenance_groups.setdefault(group, set()).add(record.provenance)
    inconsistent_provenance = sorted(
        group
        for group, provenance in provenance_groups.items()
        if len(provenance) != 1
    )
    if inconsistent_provenance:
        formatted = ", ".join("/".join(group) for group in inconsistent_provenance)
        raise AggregateError(
            "matrix cells used inconsistent runtime provenance: " + formatted
        )

    for fixture_id in fixtures:
        fixture_records = [
            record for record in captures if record.key.fixture_id == fixture_id
        ]
        fixture_hashes = {record.fixture_sha256 for record in fixture_records}
        if len(fixture_hashes) != 1:
            raise AggregateError(
                f"fixture {fixture_id!r} has inconsistent source hashes"
            )
        logical_arguments = {
            record.logical_arguments for record in fixture_records
        }
        if len(logical_arguments) != 1:
            raise AggregateError(
                f"fixture {fixture_id!r} has inconsistent logical arguments"
            )
        guest_programs = {record.guest_program for record in fixture_records}
        if len(guest_programs) != 1:
            raise AggregateError(
                f"fixture {fixture_id!r} has inconsistent guest programs"
            )
        guest_preopens = {
            record.guest_preopens for record in fixture_records
        }
        if len(guest_preopens) != 1:
            raise AggregateError(
                f"fixture {fixture_id!r} has inconsistent guest preopens"
            )
        guest_environments = {
            record.guest_environment for record in fixture_records
        }
        if len(guest_environments) != 1:
            raise AggregateError(
                f"fixture {fixture_id!r} has inconsistent guest environments"
            )

    for record in captures:
        if record.timed_out:
            raise AggregateError(
                f"timed-out record is not evidence: {_format_key(record.key)}"
            )
        if record.output_limit_exceeded:
            raise AggregateError(
                "output-limited record is not evidence: "
                f"{_format_key(record.key)}"
            )
        if record.signal is not None:
            raise AggregateError(
                f"signalled record is not accepted evidence: {_format_key(record.key)}"
            )

    wasm_artifacts = [
        record
        for record in captures
        if record.key.runtime == "wasm"
    ]
    wasm_commits = {
        record.artifact_provenance.elephc_source_commit
        for record in wasm_artifacts
        if record.artifact_provenance is not None
    }
    if len(wasm_commits) != 1:
        raise AggregateError(
            "all WASM records must use one exact Elephc source commit"
        )
    elephc_source_commit = next(iter(wasm_commits))
    compiler_identities = {
        (
            record.artifact_provenance.compiler_executable_sha256,
            record.artifact_provenance.compiler_version,
        )
        for record in wasm_artifacts
        if record.artifact_provenance is not None
    }
    if len(compiler_identities) != 1:
        raise AggregateError(
            "all WASM records must use one exact compiler executable and version"
        )

    comparisons: list[ComparisonResult] = []
    mismatches: list[ComparisonResult] = []
    for fixture_id in fixtures:
        for profile in contract.profiles:
            reference = by_key[
                RunKey(fixture_id, profile, "php-src", "php-src")
            ]
            candidates = [
                by_key[RunKey(fixture_id, profile, "wasm", host)]
                for host in ("node", "wasmer", "wasmtime")
            ]
            artifacts = {
                candidate.artifact_provenance for candidate in candidates
            }
            if None in artifacts or len(artifacts) != 1:
                raise AggregateError(
                    f"{fixture_id}/{profile} hosts did not execute identical "
                    "WAT/WASM/npm artifacts"
                )
            for candidate in candidates:
                try:
                    comparison = compare_records(reference, candidate)
                except ComparisonError as error:
                    raise AggregateError(
                        f"cannot compare {fixture_id}/{profile}/"
                        f"{candidate.key.host}: {error}"
                    ) from error
                comparisons.append(comparison)
                if not comparison.matched:
                    mismatches.append(comparison)

    if mismatches:
        summary = "; ".join(
            f"{result.fixture_id}/{result.profile}/{result.host}: "
            + ", ".join(difference.field for difference in result.differences)
            for result in mismatches
        )
        raise AggregateError(f"oracle comparison mismatch: {summary}")

    ordered_records = tuple(by_key[key] for key in sorted(expected))
    return AggregateResult(
        contract=contract,
        fixture_ids=fixtures,
        execution_cells=tuple(execution_cells),
        elephc_source_commit=elephc_source_commit,
        records=ordered_records,
        comparisons=tuple(comparisons),
    )


def aggregate_generated_suite(
    contract: OracleContract,
    records: Iterable[CaptureRecord],
) -> AggregateResult:
    """Validate committed fixture semantics before aggregating the exact suite."""

    try:
        suite = load_suite(contract.repo_root)
        stale_outputs = check_outputs(render_outputs(contract.repo_root))
    except OracleDefinitionError as error:
        raise AggregateError(f"invalid generated fixture suite: {error}") from error
    if stale_outputs:
        raise AggregateError("; ".join(stale_outputs))

    expected_inventory = contract.inventory_path.relative_to(
        contract.repo_root
    ).as_posix()
    expected_specification = contract.specification_path.relative_to(
        contract.repo_root
    ).as_posix()
    if suite.inventory_path != expected_inventory:
        raise AggregateError(
            "fixture suite inventory reference does not match the oracle contract"
        )
    if suite.specification_path != expected_specification:
        raise AggregateError(
            "fixture suite specification reference does not match the oracle contract"
        )

    fixtures = {fixture.identifier: fixture for fixture in suite.fixtures}
    captures = tuple(records)
    for record in captures:
        fixture = fixtures.get(record.key.fixture_id)
        if fixture is None:
            continue
        source = (
            fixture_root(contract.repo_root)
            / "generated"
            / f"{fixture.identifier}.php"
        )
        try:
            expected_fixture_sha256 = sha256_file(source)
            expected_stdin = base64.b64decode(
                fixture.stdin_base64,
                validate=True,
            )
            observed_stdout = record.normalized_stdout.to_bytes()
        except (ContractError, CaptureError, binascii.Error) as error:
            raise AggregateError(
                f"cannot validate fixture {fixture.identifier!r}: {error}"
            ) from error

        if record.fixture_sha256 != expected_fixture_sha256:
            raise AggregateError(
                f"{record.key.fixture_id}/{record.key.profile}/"
                f"{record.key.host} fixture hash does not match committed source"
            )
        if record.guest_program != EXPECTED_GUEST_PROGRAM:
            raise AggregateError(
                f"{record.key.fixture_id}/{record.key.profile}/"
                f"{record.key.host} guest argv[0] must be exactly 'oracle.php'"
            )
        if record.logical_arguments != fixture.logical_args:
            raise AggregateError(
                f"{record.key.fixture_id}/{record.key.profile}/"
                f"{record.key.host} logical arguments do not match the suite"
            )
        if dict(record.guest_preopens) != EXPECTED_GUEST_PREOPENS:
            raise AggregateError(
                f"{record.key.fixture_id}/{record.key.profile}/"
                f"{record.key.host} guest preopens must be empty for this suite"
            )
        if dict(record.guest_environment) != EXPECTED_GUEST_ENV:
            raise AggregateError(
                f"{record.key.fixture_id}/{record.key.profile}/"
                f"{record.key.host} guest environment does not match the suite"
            )
        if record.stdin.to_bytes() != expected_stdin:
            raise AggregateError(
                f"{record.key.fixture_id}/{record.key.profile}/"
                f"{record.key.host} stdin does not match the suite"
            )
        if record.timeout_seconds != EXPECTED_TIMEOUT_SECONDS:
            raise AggregateError(
                f"{record.key.fixture_id}/{record.key.profile}/"
                f"{record.key.host} timeout does not match the suite"
            )
        if record.output_limit_bytes != EXPECTED_MAX_OUTPUT_BYTES:
            raise AggregateError(
                f"{record.key.fixture_id}/{record.key.profile}/"
                f"{record.key.host} output limit does not match the suite"
            )
        if record.normalization.replacements:
            raise AggregateError(
                f"{record.key.fixture_id}/{record.key.profile}/"
                f"{record.key.host} normalization must be empty for this suite"
            )

        expectations = tuple(
            FrameExpectation(case.identifier, case.value_type)
            for case in fixture.cases
        )
        try:
            parse_frames(observed_stdout, expectations)
        except FrameProtocolError as error:
            raise AggregateError(
                f"{record.key.fixture_id}/{record.key.profile}/"
                f"{record.key.host} has invalid frames: {error}"
            ) from error

    return aggregate_exact(contract, tuple(fixtures), captures)
