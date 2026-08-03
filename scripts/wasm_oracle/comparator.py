"""Exact comparison of one pinned php-src capture with one WASM capture."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from .capture import CaptureError, CaptureRecord


class ComparisonError(ValueError):
    """Raised when records cannot form one complete comparable pair."""


@dataclass(frozen=True)
class Difference:
    """One machine-readable mismatch between reference and candidate."""

    field: str
    reference: Any
    candidate: Any

    def to_dict(self) -> dict[str, Any]:
        """Serialize the mismatch."""

        return {
            "field": self.field,
            "reference": self.reference,
            "candidate": self.candidate,
        }


@dataclass(frozen=True)
class ComparisonResult:
    """Comparison result for one fixture/profile pair."""

    fixture_id: str
    profile: str
    host: str
    differences: tuple[Difference, ...]

    @property
    def matched(self) -> bool:
        """Report whether every compared observable matched exactly."""

        return not self.differences

    def to_dict(self) -> dict[str, Any]:
        """Serialize a deterministic comparison result."""

        return {
            "fixture_id": self.fixture_id,
            "profile": self.profile,
            "host": self.host,
            "matched": self.matched,
            "differences": [
                difference.to_dict() for difference in self.differences
            ],
        }


def _payload_summary(record_payload: Any) -> dict[str, Any]:
    return {
        "sha256": record_payload.sha256,
        "length": record_payload.length,
    }


def compare_records(
    reference: CaptureRecord, candidate: CaptureRecord
) -> ComparisonResult:
    """Compare normalized bytes and termination while retaining raw evidence."""

    if reference.key.runtime != "php-src":
        raise ComparisonError("reference record runtime must be 'php-src'")
    if reference.key.host != "php-src":
        raise ComparisonError("reference record host must be 'php-src'")
    if candidate.key.runtime != "wasm":
        raise ComparisonError("candidate record runtime must be 'wasm'")
    if (
        reference.key.fixture_id != candidate.key.fixture_id
        or reference.key.profile != candidate.key.profile
    ):
        raise ComparisonError("reference and candidate keys do not identify one pair")
    if (
        reference.specification_sha256 != candidate.specification_sha256
        or reference.inventory_sha256 != candidate.inventory_sha256
        or reference.pinned_php_src_tag != candidate.pinned_php_src_tag
        or reference.pinned_php_src_tag_object
        != candidate.pinned_php_src_tag_object
        or reference.pinned_php_src_tag_commit
        != candidate.pinned_php_src_tag_commit
    ):
        raise ComparisonError("reference and candidate contract anchors differ")
    if reference.timed_out or candidate.timed_out:
        raise ComparisonError("timed-out records are not comparable evidence")
    if reference.output_limit_exceeded or candidate.output_limit_exceeded:
        raise ComparisonError("output-limited records are not comparable evidence")
    if reference.module_i32_bits is not None:
        raise ComparisonError("php-src reference unexpectedly carries module i32 status")
    if candidate.key.host == "node" and candidate.module_i32_bits is None:
        raise ComparisonError("Node candidate is missing full module i32 status")
    if candidate.key.host != "node" and candidate.module_i32_bits is not None:
        raise ComparisonError(
            f"{candidate.key.host} candidate must not invent module i32 status"
        )

    try:
        reference_stdin = reference.stdin.to_bytes()
        candidate_stdin = candidate.stdin.to_bytes()
        reference_stdout = reference.normalized_stdout.to_bytes()
        candidate_stdout = candidate.normalized_stdout.to_bytes()
        reference_stderr = reference.normalized_stderr.to_bytes()
        candidate_stderr = candidate.normalized_stderr.to_bytes()
    except CaptureError as error:
        raise ComparisonError(str(error)) from error

    differences: list[Difference] = []
    if reference.fixture_sha256 != candidate.fixture_sha256:
        differences.append(
            Difference(
                "fixture_sha256",
                reference.fixture_sha256,
                candidate.fixture_sha256,
            )
        )
    if reference.guest_program != candidate.guest_program:
        differences.append(
            Difference(
                "guest_program",
                reference.guest_program,
                candidate.guest_program,
            )
        )
    if reference.logical_arguments != candidate.logical_arguments:
        differences.append(
            Difference(
                "logical_arguments",
                list(reference.logical_arguments),
                list(candidate.logical_arguments),
            )
        )
    if reference.guest_preopens != candidate.guest_preopens:
        differences.append(
            Difference(
                "guest_preopens",
                dict(reference.guest_preopens),
                dict(candidate.guest_preopens),
            )
        )
    if reference_stdin != candidate_stdin:
        differences.append(
            Difference(
                "stdin",
                _payload_summary(reference.stdin),
                _payload_summary(candidate.stdin),
            )
        )

    reference_environment = dict(reference.host_control_environment)
    candidate_environment = dict(candidate.host_control_environment)
    if reference_environment != candidate_environment:
        differences.append(
            Difference(
                "host_control_environment",
                reference_environment,
                candidate_environment,
            )
        )
    reference_guest_environment = dict(reference.guest_environment)
    candidate_guest_environment = dict(candidate.guest_environment)
    if reference_guest_environment != candidate_guest_environment:
        differences.append(
            Difference(
                "guest_environment",
                reference_guest_environment,
                candidate_guest_environment,
            )
        )

    if reference_stdout != candidate_stdout:
        differences.append(
            Difference(
                "stdout",
                _payload_summary(reference.normalized_stdout),
                _payload_summary(candidate.normalized_stdout),
            )
        )
    if reference_stderr != candidate_stderr:
        differences.append(
            Difference(
                "stderr",
                _payload_summary(reference.normalized_stderr),
                _payload_summary(candidate.normalized_stderr),
            )
        )
    if reference.host_status != candidate.host_status:
        differences.append(
            Difference(
                "host_status",
                reference.host_status,
                candidate.host_status,
            )
        )
    if (
        reference.host_status_representation
        != candidate.host_status_representation
    ):
        differences.append(
            Difference(
                "host_status_representation",
                reference.host_status_representation,
                candidate.host_status_representation,
            )
        )
    if reference.signal != candidate.signal:
        differences.append(
            Difference("signal", reference.signal, candidate.signal)
        )

    if candidate.key.host == "node":
        assert candidate.module_i32_bits is not None
        if reference.host_status is None:
            differences.append(
                Difference(
                    "module_i32_low_byte",
                    "unavailable because php-src was signalled",
                    candidate.module_i32_bits & 0xFF,
                )
            )
        elif (candidate.module_i32_bits & 0xFF) != reference.host_status:
            differences.append(
                Difference(
                    "module_i32_low_byte",
                    reference.host_status,
                    candidate.module_i32_bits & 0xFF,
                )
            )

    return ComparisonResult(
        fixture_id=reference.key.fixture_id,
        profile=reference.key.profile,
        host=candidate.key.host,
        differences=tuple(differences),
    )
