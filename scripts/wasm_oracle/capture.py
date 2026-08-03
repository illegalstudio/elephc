"""Bounded, byte-exact subprocess capture for the WASM/PHP differential oracle."""

from __future__ import annotations

import base64
import binascii
import math
import os
import platform
import signal
import subprocess
import threading
import time
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Any, Mapping

from .contract import (
    CompilerArtifactProvenance,
    ContractError,
    OracleContract,
    RunKey,
    RuntimeProvenance,
    sha256_bytes,
    sha256_file,
)


CAPTURE_SCHEMA = "elephc.wasm-oracle.capture.v3"
MODULE_STATUS_FD_ENV = "ELEPHC_ORACLE_MODULE_STATUS_FD"
REQUIRED_HOST_ENVIRONMENT = {
    "LANG": "C.UTF-8",
    "LC_ALL": "C.UTF-8",
    "TZ": "UTC",
}
_MODULE_STATUS_MAX_BYTES = 64


class CaptureError(ValueError):
    """Raised when capture cannot prove all required execution prerequisites."""


def _strict_fields(value: Any, expected: set[str], label: str) -> Mapping[str, Any]:
    if not isinstance(value, dict):
        raise CaptureError(f"{label} must be an object")
    if set(value) != expected:
        raise CaptureError(
            f"{label} fields must be exactly {sorted(expected)}, got {sorted(value)}"
        )
    return value


def _integer(value: Any, label: str, *, minimum: int | None = None) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise CaptureError(f"{label} must be an integer")
    if minimum is not None and value < minimum:
        raise CaptureError(f"{label} must be at least {minimum}")
    return value


def _number(value: Any, label: str, *, positive: bool = False) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise CaptureError(f"{label} must be a finite number")
    result = float(value)
    if not math.isfinite(result) or (positive and result <= 0):
        raise CaptureError(
            f"{label} must be {'positive and ' if positive else ''}finite"
        )
    return result


def _sha256(value: Any, label: str) -> str:
    if (
        not isinstance(value, str)
        or len(value) != 64
        or any(char not in "0123456789abcdef" for char in value)
    ):
        raise CaptureError(f"{label} must be 64 lowercase hex characters")
    return value


@dataclass(frozen=True)
class RawBytes:
    """Byte payload represented redundantly by base64, SHA-256, and length."""

    base64: str
    sha256: str
    length: int

    @classmethod
    def from_bytes(cls, data: bytes) -> "RawBytes":
        """Encode exact bytes without text conversion."""

        exact = bytes(data)
        return cls(
            base64=base64.b64encode(exact).decode("ascii"),
            sha256=sha256_bytes(exact),
            length=len(exact),
        )

    def to_bytes(self) -> bytes:
        """Decode and verify every redundant integrity field."""

        if not isinstance(self.base64, str):
            raise CaptureError("raw byte base64 field must be a string")
        try:
            decoded = base64.b64decode(self.base64, validate=True)
        except (ValueError, binascii.Error) as error:
            raise CaptureError(f"invalid base64 byte payload: {error}") from error
        if (
            not isinstance(self.sha256, str)
            or len(self.sha256) != 64
            or any(char not in "0123456789abcdef" for char in self.sha256)
        ):
            raise CaptureError("raw byte SHA-256 must be 64 lowercase hex characters")
        if isinstance(self.length, bool) or not isinstance(self.length, int):
            raise CaptureError("raw byte length must be an integer")
        if self.length != len(decoded):
            raise CaptureError(
                f"raw byte length mismatch: metadata {self.length}, actual {len(decoded)}"
            )
        actual_sha = sha256_bytes(decoded)
        if self.sha256 != actual_sha:
            raise CaptureError(
                f"raw byte SHA-256 mismatch: metadata {self.sha256}, actual {actual_sha}"
            )
        return decoded

    def to_dict(self) -> dict[str, Any]:
        """Serialize all redundant integrity fields."""

        return {
            "base64": self.base64,
            "sha256": self.sha256,
            "length": self.length,
        }

    @classmethod
    def from_dict(cls, value: Any) -> "RawBytes":
        """Parse and fully validate a serialized byte payload."""

        data = _strict_fields(value, {"base64", "sha256", "length"}, "raw bytes")
        result = cls(
            base64=data["base64"],
            sha256=data["sha256"],
            length=data["length"],
        )
        result.to_bytes()
        return result


@dataclass(frozen=True)
class Normalization:
    """Ordered literal byte replacements; no decoding, trimming, or regexes."""

    replacements: tuple[tuple[bytes, bytes], ...] = ()

    def __post_init__(self) -> None:
        seen: set[bytes] = set()
        needles: list[bytes] = []
        for needle, replacement in self.replacements:
            if not isinstance(needle, bytes) or not needle:
                raise CaptureError("normalization needles must be non-empty bytes")
            if not isinstance(replacement, bytes):
                raise CaptureError("normalization replacements must be bytes")
            if needle in seen:
                raise CaptureError("normalization needles must be unique")
            seen.add(needle)
            needles.append(needle)

        for index, needle in enumerate(needles):
            for other_index, other in enumerate(needles):
                if index != other_index and (needle in other or other in needle):
                    raise CaptureError(
                        "overlapping normalization needles are not deterministic"
                    )
            for replacement_needle, replacement in self.replacements:
                if replacement_needle != needle and needle in replacement:
                    raise CaptureError(
                        "normalization replacement would trigger another rule"
                    )

    def apply(self, data: bytes) -> bytes:
        """Apply only the explicitly declared literal replacements."""

        normalized = bytes(data)
        for needle, replacement in self.replacements:
            normalized = normalized.replace(needle, replacement)
        return normalized

    def to_dict(self) -> dict[str, Any]:
        """Serialize replacement bytes with the same redundant byte envelope."""

        return {
            "kind": "literal-bytes-v1",
            "replacements": [
                {
                    "needle": RawBytes.from_bytes(needle).to_dict(),
                    "replacement": RawBytes.from_bytes(replacement).to_dict(),
                }
                for needle, replacement in self.replacements
            ],
        }

    @classmethod
    def from_dict(cls, value: Any) -> "Normalization":
        """Parse a minimal normalization contract and reject unknown transforms."""

        data = _strict_fields(value, {"kind", "replacements"}, "normalization")
        if data["kind"] != "literal-bytes-v1":
            raise CaptureError(
                "normalization.kind must be exactly 'literal-bytes-v1'"
            )
        raw_replacements = data["replacements"]
        if not isinstance(raw_replacements, list):
            raise CaptureError("normalization.replacements must be an array")
        replacements: list[tuple[bytes, bytes]] = []
        for index, raw_replacement in enumerate(raw_replacements):
            replacement = _strict_fields(
                raw_replacement,
                {"needle", "replacement"},
                f"normalization.replacements[{index}]",
            )
            replacements.append(
                (
                    RawBytes.from_dict(replacement["needle"]).to_bytes(),
                    RawBytes.from_dict(replacement["replacement"]).to_bytes(),
                )
            )
        return cls(tuple(replacements))


@dataclass(frozen=True)
class CaptureRequest:
    """Complete, explicit execution request for one oracle matrix cell."""

    key: RunKey
    fixture_sha256: str
    guest_program: str
    logical_arguments: tuple[str, ...]
    guest_preopens: Mapping[str, str]
    argv: tuple[str, ...]
    cwd: Path
    host_control_environment: Mapping[str, str]
    guest_environment: Mapping[str, str]
    stdin: bytes
    timeout_seconds: float
    output_limit_bytes: int
    provenance: RuntimeProvenance
    artifact_provenance: CompilerArtifactProvenance | None
    normalization: Normalization = Normalization()
    require_module_i32: bool = False


@dataclass(frozen=True)
class CaptureRecord:
    """Self-verifying byte, process, provenance, and contract capture."""

    key: RunKey
    specification_sha256: str
    inventory_sha256: str
    pinned_php_src_tag: str
    pinned_php_src_tag_object: str
    pinned_php_src_tag_commit: str
    fixture_sha256: str
    guest_program: str
    logical_arguments: tuple[str, ...]
    guest_preopens: tuple[tuple[str, str], ...]
    argv: tuple[str, ...]
    cwd: str
    host_control_environment: tuple[tuple[str, str], ...]
    guest_environment: tuple[tuple[str, str], ...]
    process_environment: tuple[tuple[str, str], ...]
    operating_system: str
    architecture: str
    provenance: RuntimeProvenance
    artifact_provenance: CompilerArtifactProvenance | None
    stdin: RawBytes
    stdout: RawBytes
    stderr: RawBytes
    normalization: Normalization
    normalized_stdout: RawBytes
    normalized_stderr: RawBytes
    host_status: int | None
    host_status_representation: str
    signal: int | None
    module_i32_bits: int | None
    timed_out: bool
    output_limit_exceeded: bool
    timeout_seconds: float
    output_limit_bytes: int
    elapsed_seconds: float
    schema: str = CAPTURE_SCHEMA

    @property
    def module_i32_signed(self) -> int | None:
        """Interpret the retained i32 bit pattern as a signed two's-complement value."""

        if self.module_i32_bits is None:
            return None
        if self.module_i32_bits < 0x8000_0000:
            return self.module_i32_bits
        return self.module_i32_bits - 0x1_0000_0000

    def validate(self, contract: OracleContract) -> None:
        """Validate all redundant fields and their pinned contract relationships."""

        if self.schema != CAPTURE_SCHEMA:
            raise CaptureError(
                f"unsupported capture schema {self.schema!r}; expected {CAPTURE_SCHEMA!r}"
            )
        if self.specification_sha256 != contract.specification_sha256:
            raise CaptureError("capture specification hash does not match contract")
        if self.inventory_sha256 != contract.inventory_sha256:
            raise CaptureError("capture inventory hash does not match contract")
        pin = contract.php_src_pin(self.key.profile)
        if self.pinned_php_src_tag != pin.tag:
            raise CaptureError("capture php-src tag does not match contract")
        if self.pinned_php_src_tag_object != pin.tag_object:
            raise CaptureError(
                "capture php-src annotated tag object does not match contract"
            )
        if self.pinned_php_src_tag_commit != pin.tag_commit:
            raise CaptureError(
                "capture php-src peeled tag commit does not match contract"
            )
        _sha256(self.fixture_sha256, "fixture_sha256")
        if any(
            not isinstance(argument, str) or "\x00" in argument
            for argument in self.logical_arguments
        ):
            raise CaptureError("logical_arguments must contain NUL-free strings")
        _validate_guest_program(self.guest_program)
        _validate_guest_preopens(dict(self.guest_preopens))
        if tuple(sorted(self.guest_preopens)) != self.guest_preopens:
            raise CaptureError("capture guest_preopens must be sorted and unique")
        try:
            self.provenance.validate_for(self.key, contract)
        except ContractError as error:
            raise CaptureError(str(error)) from error
        if self.key.runtime == "php-src":
            if self.artifact_provenance is not None:
                raise CaptureError(
                    "php-src captures must not carry compiler artifact provenance"
                )
        else:
            if self.artifact_provenance is None:
                raise CaptureError(
                    "WASM captures require compiler artifact provenance"
                )
            try:
                CompilerArtifactProvenance.create(
                    **self.artifact_provenance.to_dict()
                )
            except ContractError as error:
                raise CaptureError(str(error)) from error

        if not isinstance(self.argv, tuple) or not self.argv or any(
            not isinstance(argument, str) or "\x00" in argument
            for argument in self.argv
        ):
            raise CaptureError("capture argv must contain control-safe strings")
        if not isinstance(self.logical_arguments, tuple):
            raise CaptureError("logical_arguments must be a tuple")
        if not isinstance(self.cwd, str) or not self.cwd:
            raise CaptureError("capture cwd must be a non-empty string")
        if not isinstance(self.operating_system, str) or not self.operating_system:
            raise CaptureError("capture operating_system must be a non-empty string")
        if not isinstance(self.architecture, str) or not self.architecture:
            raise CaptureError("capture architecture must be a non-empty string")
        _validate_environment(dict(self.host_control_environment))
        if (
            tuple(sorted(self.host_control_environment))
            != self.host_control_environment
        ):
            raise CaptureError(
                "capture host_control_environment must be sorted and unique"
            )
        _validate_guest_environment(dict(self.guest_environment))
        if tuple(sorted(self.guest_environment)) != self.guest_environment:
            raise CaptureError(
                "capture guest_environment must be sorted and unique"
            )
        if tuple(sorted(self.process_environment)) != self.process_environment:
            raise CaptureError(
                "capture process_environment must be sorted and unique"
            )
        expected_process_environment = (
            self.guest_environment
            if self.key.runtime == "php-src"
            else self.host_control_environment
        )
        if self.process_environment != expected_process_environment:
            raise CaptureError(
                "capture process_environment does not match the runtime policy"
            )

        stdin = self.stdin.to_bytes()
        del stdin
        stdout = self.stdout.to_bytes()
        stderr = self.stderr.to_bytes()
        if self.normalized_stdout.to_bytes() != self.normalization.apply(stdout):
            raise CaptureError("normalized stdout does not match declared rules")
        if self.normalized_stderr.to_bytes() != self.normalization.apply(stderr):
            raise CaptureError("normalized stderr does not match declared rules")

        if not isinstance(self.timed_out, bool):
            raise CaptureError("timed_out must be boolean")
        if not isinstance(self.output_limit_exceeded, bool):
            raise CaptureError("output_limit_exceeded must be boolean")
        _number(self.timeout_seconds, "timeout_seconds", positive=True)
        _integer(self.output_limit_bytes, "output_limit_bytes", minimum=1)
        _number(self.elapsed_seconds, "elapsed_seconds")
        if self.elapsed_seconds < 0:
            raise CaptureError("elapsed_seconds must not be negative")

        if self.host_status is not None:
            _integer(self.host_status, "host_status", minimum=0)
            if self.host_status > 255:
                raise CaptureError("host_status must fit POSIX process status")
        if self.host_status_representation != "posix_process_status":
            raise CaptureError(
                "host_status_representation must be 'posix_process_status'"
            )
        if self.signal is not None:
            _integer(self.signal, "signal", minimum=1)
        if (self.host_status is None) == (self.signal is None):
            raise CaptureError(
                "exactly one of host_status or signal must be present"
            )

        if self.module_i32_bits is not None:
            _integer(self.module_i32_bits, "module_i32_bits", minimum=0)
            if self.module_i32_bits > 0xFFFF_FFFF:
                raise CaptureError("module_i32_bits must fit an unsigned 32-bit value")

        interrupted = self.timed_out or self.output_limit_exceeded
        if self.key.host == "node":
            if not interrupted and self.module_i32_bits is None:
                raise CaptureError(
                    "completed Node capture is missing full module i32 status"
                )
        elif self.module_i32_bits is not None:
            raise CaptureError(
                f"{self.key.host} capture must not invent a full module i32 status"
            )

    def to_dict(self) -> dict[str, Any]:
        """Serialize a self-contained deterministic capture record."""

        module_i32: dict[str, Any] | None = None
        if self.module_i32_bits is not None:
            module_i32 = {
                "hex": f"{self.module_i32_bits:08x}",
                "unsigned": self.module_i32_bits,
                "signed": self.module_i32_signed,
            }
        return {
            "schema": self.schema,
            "key": self.key.to_dict(),
            "contract": {
                "specification_sha256": self.specification_sha256,
                "inventory_sha256": self.inventory_sha256,
                "pinned_php_src_tag": self.pinned_php_src_tag,
                "pinned_php_src_tag_object": self.pinned_php_src_tag_object,
                "pinned_php_src_tag_commit": self.pinned_php_src_tag_commit,
            },
            "command": {
                "fixture_sha256": self.fixture_sha256,
                "guest_program": self.guest_program,
                "logical_arguments": list(self.logical_arguments),
                "guest_preopens": dict(self.guest_preopens),
                "argv": list(self.argv),
                "cwd": self.cwd,
                "host_control_environment": dict(
                    self.host_control_environment
                ),
                "guest_environment": dict(self.guest_environment),
                "process_environment": dict(self.process_environment),
                "stdin": self.stdin.to_dict(),
            },
            "host": {
                "operating_system": self.operating_system,
                "architecture": self.architecture,
            },
            "provenance": self.provenance.to_dict(),
            "artifact_provenance": (
                None
                if self.artifact_provenance is None
                else self.artifact_provenance.to_dict()
            ),
            "output": {
                "stdout": self.stdout.to_dict(),
                "stderr": self.stderr.to_dict(),
                "normalization": self.normalization.to_dict(),
                "normalized_stdout": self.normalized_stdout.to_dict(),
                "normalized_stderr": self.normalized_stderr.to_dict(),
            },
            "termination": {
                "host_status": self.host_status,
                "host_status_representation": self.host_status_representation,
                "signal": self.signal,
                "module_i32": module_i32,
                "timed_out": self.timed_out,
                "output_limit_exceeded": self.output_limit_exceeded,
                "timeout_seconds": self.timeout_seconds,
                "output_limit_bytes": self.output_limit_bytes,
                "elapsed_seconds": self.elapsed_seconds,
            },
        }

    @classmethod
    def from_dict(
        cls, value: Any, contract: OracleContract | None = None
    ) -> "CaptureRecord":
        """Parse a serialized capture, optionally validating its current contract."""

        data = _strict_fields(
            value,
            {
                "schema",
                "key",
                "contract",
                "command",
                "host",
                "provenance",
                "artifact_provenance",
                "output",
                "termination",
            },
            "capture",
        )
        contract_data = _strict_fields(
            data["contract"],
            {
                "specification_sha256",
                "inventory_sha256",
                "pinned_php_src_tag",
                "pinned_php_src_tag_object",
                "pinned_php_src_tag_commit",
            },
            "capture.contract",
        )
        command = _strict_fields(
            data["command"],
            {
                "fixture_sha256",
                "guest_program",
                "logical_arguments",
                "guest_preopens",
                "argv",
                "cwd",
                "host_control_environment",
                "guest_environment",
                "process_environment",
                "stdin",
            },
            "capture.command",
        )
        host = _strict_fields(
            data["host"],
            {"operating_system", "architecture"},
            "capture.host",
        )
        output = _strict_fields(
            data["output"],
            {
                "stdout",
                "stderr",
                "normalization",
                "normalized_stdout",
                "normalized_stderr",
            },
            "capture.output",
        )
        termination = _strict_fields(
            data["termination"],
            {
                "host_status",
                "host_status_representation",
                "signal",
                "module_i32",
                "timed_out",
                "output_limit_exceeded",
                "timeout_seconds",
                "output_limit_bytes",
                "elapsed_seconds",
            },
            "capture.termination",
        )
        argv = command["argv"]
        logical_arguments = command["logical_arguments"]
        guest_preopens = command["guest_preopens"]
        host_control_environment = command["host_control_environment"]
        guest_environment = command["guest_environment"]
        process_environment = command["process_environment"]
        if not isinstance(argv, list):
            raise CaptureError("capture.command.argv must be an array")
        if not isinstance(logical_arguments, list):
            raise CaptureError(
                "capture.command.logical_arguments must be an array"
            )
        if not isinstance(guest_preopens, dict):
            raise CaptureError("capture.command.guest_preopens must be an object")
        if not isinstance(host_control_environment, dict):
            raise CaptureError(
                "capture.command.host_control_environment must be an object"
            )
        if not isinstance(guest_environment, dict):
            raise CaptureError(
                "capture.command.guest_environment must be an object"
            )
        if not isinstance(process_environment, dict):
            raise CaptureError(
                "capture.command.process_environment must be an object"
            )

        raw_module_i32 = termination["module_i32"]
        module_i32_bits: int | None = None
        if raw_module_i32 is not None:
            module_i32 = _strict_fields(
                raw_module_i32,
                {"hex", "unsigned", "signed"},
                "capture.termination.module_i32",
            )
            unsigned = _integer(
                module_i32["unsigned"], "module_i32.unsigned", minimum=0
            )
            if unsigned > 0xFFFF_FFFF:
                raise CaptureError("module_i32.unsigned does not fit i32 bits")
            if module_i32["hex"] != f"{unsigned:08x}":
                raise CaptureError("module_i32 hex and unsigned values disagree")
            expected_signed = (
                unsigned if unsigned < 0x8000_0000 else unsigned - 0x1_0000_0000
            )
            signed = _integer(module_i32["signed"], "module_i32.signed")
            if signed != expected_signed:
                raise CaptureError("module_i32 signed and unsigned values disagree")
            module_i32_bits = unsigned

        raw_artifact = data["artifact_provenance"]
        artifact = (
            None
            if raw_artifact is None
            else CompilerArtifactProvenance.from_dict(raw_artifact)
        )
        record = cls(
            schema=data["schema"],
            key=RunKey.from_dict(data["key"]),
            specification_sha256=contract_data["specification_sha256"],
            inventory_sha256=contract_data["inventory_sha256"],
            pinned_php_src_tag=contract_data["pinned_php_src_tag"],
            pinned_php_src_tag_object=contract_data[
                "pinned_php_src_tag_object"
            ],
            pinned_php_src_tag_commit=contract_data[
                "pinned_php_src_tag_commit"
            ],
            fixture_sha256=command["fixture_sha256"],
            guest_program=command["guest_program"],
            logical_arguments=tuple(logical_arguments),
            guest_preopens=tuple(sorted(guest_preopens.items())),
            argv=tuple(argv),
            cwd=command["cwd"],
            host_control_environment=tuple(
                sorted(host_control_environment.items())
            ),
            guest_environment=tuple(sorted(guest_environment.items())),
            process_environment=tuple(sorted(process_environment.items())),
            operating_system=host["operating_system"],
            architecture=host["architecture"],
            provenance=RuntimeProvenance.from_dict(data["provenance"]),
            artifact_provenance=artifact,
            stdin=RawBytes.from_dict(command["stdin"]),
            stdout=RawBytes.from_dict(output["stdout"]),
            stderr=RawBytes.from_dict(output["stderr"]),
            normalization=Normalization.from_dict(output["normalization"]),
            normalized_stdout=RawBytes.from_dict(output["normalized_stdout"]),
            normalized_stderr=RawBytes.from_dict(output["normalized_stderr"]),
            host_status=termination["host_status"],
            host_status_representation=termination[
                "host_status_representation"
            ],
            signal=termination["signal"],
            module_i32_bits=module_i32_bits,
            timed_out=termination["timed_out"],
            output_limit_exceeded=termination["output_limit_exceeded"],
            timeout_seconds=termination["timeout_seconds"],
            output_limit_bytes=termination["output_limit_bytes"],
            elapsed_seconds=termination["elapsed_seconds"],
        )
        if contract is not None:
            record.validate(contract)
        return record


def _validate_environment(environment: Mapping[str, str]) -> None:
    if not isinstance(environment, Mapping):
        raise CaptureError("environment must be an explicit string mapping")
    for key, value in environment.items():
        if (
            not isinstance(key, str)
            or not key
            or "\x00" in key
            or "=" in key
        ):
            raise CaptureError("environment contains an invalid key")
        if not isinstance(value, str) or "\x00" in value:
            raise CaptureError(f"environment value for {key!r} is invalid")
    if dict(environment) != REQUIRED_HOST_ENVIRONMENT:
        raise CaptureError(
            "environment must be exactly "
            f"{REQUIRED_HOST_ENVIRONMENT}, got {dict(environment)}"
        )
    if MODULE_STATUS_FD_ENV in environment:
        raise CaptureError(f"{MODULE_STATUS_FD_ENV} is reserved by the oracle")


def _validate_guest_environment(environment: Mapping[str, str]) -> None:
    """Validate the explicit logical WASI/PHP environment without inheriting it."""

    if not isinstance(environment, Mapping):
        raise CaptureError(
            "guest_environment must be an explicit string mapping"
        )
    for key, value in environment.items():
        if (
            not isinstance(key, str)
            or not key
            or "\x00" in key
            or "=" in key
        ):
            raise CaptureError("guest_environment contains an invalid key")
        if not isinstance(value, str) or "\x00" in value:
            raise CaptureError(
                f"guest_environment value for {key!r} is invalid"
            )


def _validate_guest_program(program: str) -> None:
    """Validate the explicit guest-visible argv[0] string."""

    if not isinstance(program, str) or not program or "\x00" in program:
        raise CaptureError("guest_program must be a non-empty NUL-free string")


def _validate_guest_preopens(preopens: Mapping[str, str]) -> None:
    """Validate explicit guest-to-host directory mappings without inheritance."""

    if not isinstance(preopens, Mapping):
        raise CaptureError("guest_preopens must be an explicit string mapping")
    for guest_path, host_path in preopens.items():
        guest = (
            PurePosixPath(guest_path)
            if isinstance(guest_path, str) and "\x00" not in guest_path
            else None
        )
        if (
            guest is None
            or not guest_path.startswith("/")
            or guest_path.startswith("//")
            or guest_path != str(guest)
            or ".." in guest.parts
        ):
            raise CaptureError(
                f"guest_preopens contains invalid guest path {guest_path!r}"
            )
        host = (
            Path(host_path)
            if isinstance(host_path, str) and "\x00" not in host_path
            else None
        )
        if (
            host is None
            or not host.is_absolute()
            or host_path != str(host)
            or ".." in host.parts
        ):
            raise CaptureError(
                f"guest_preopens host path for {guest_path!r} must be absolute"
            )


def _validate_request(
    contract: OracleContract, request: CaptureRequest
) -> tuple[
    Path,
    tuple[tuple[str, str], ...],
    tuple[tuple[str, str], ...],
]:
    if request.key.profile not in contract.profiles:
        raise CaptureError(f"profile {request.key.profile!r} is not pinned")
    expected_module_channel = request.key.host == "node"
    if request.require_module_i32 != expected_module_channel:
        raise CaptureError(
            "require_module_i32 must be true only for the Node adapter host"
        )
    if not isinstance(request.argv, tuple) or not request.argv:
        raise CaptureError("capture argv must not be empty")
    _sha256(request.fixture_sha256, "fixture_sha256")
    if not isinstance(request.logical_arguments, tuple):
        raise CaptureError("logical_arguments must be a tuple")
    for argument in request.logical_arguments:
        if not isinstance(argument, str) or "\x00" in argument:
            raise CaptureError(
                "logical_arguments must contain only NUL-free strings"
            )
    _validate_guest_program(request.guest_program)
    _validate_guest_preopens(request.guest_preopens)
    for host_path in request.guest_preopens.values():
        if not Path(host_path).is_dir():
            raise CaptureError(
                f"guest_preopens host directory does not exist: {host_path}"
            )
    for argument in request.argv:
        if not isinstance(argument, str) or "\x00" in argument:
            raise CaptureError("capture argv must contain only NUL-free strings")

    executable = Path(request.argv[0])
    if not executable.is_absolute():
        raise CaptureError("capture executable must be an absolute path")
    if not executable.is_file() or not os.access(executable, os.X_OK):
        raise CaptureError(f"capture executable is missing or not executable: {executable}")
    actual_executable_sha = sha256_file(executable)
    if actual_executable_sha != request.provenance.executable_sha256:
        raise CaptureError(
            "capture executable hash does not match runtime provenance: "
            f"{actual_executable_sha} != {request.provenance.executable_sha256}"
        )

    cwd = Path(request.cwd)
    if not cwd.is_absolute() or not cwd.is_dir():
        raise CaptureError("capture cwd must be an existing absolute directory")
    _validate_environment(request.host_control_environment)
    host_control_environment = tuple(
        sorted(request.host_control_environment.items())
    )
    _validate_guest_environment(request.guest_environment)
    guest_environment = tuple(sorted(request.guest_environment.items()))
    if not isinstance(request.stdin, bytes):
        raise CaptureError("capture stdin must be bytes")
    _number(request.timeout_seconds, "timeout_seconds", positive=True)
    _integer(request.output_limit_bytes, "output_limit_bytes", minimum=1)
    try:
        request.provenance.validate_for(request.key, contract)
    except ContractError as error:
        raise CaptureError(str(error)) from error
    if request.key.runtime == "php-src":
        if request.artifact_provenance is not None:
            raise CaptureError(
                "php-src capture cannot carry compiler artifact provenance"
            )
    elif request.artifact_provenance is None:
        raise CaptureError("WASM capture requires compiler artifact provenance")
    if not isinstance(request.normalization, Normalization):
        raise CaptureError("normalization must be a Normalization instance")
    return executable, host_control_environment, guest_environment


def _read_stream(
    stream: Any,
    limit: int,
    destination: bytearray,
    combined_length: list[int],
    combined_lock: threading.Lock,
    exceeded: threading.Event,
    failures: list[BaseException],
) -> None:
    """Capture one pipe while enforcing one combined stdout/stderr byte limit."""

    try:
        while True:
            chunk = stream.read(64 * 1024)
            if not chunk:
                return
            with combined_lock:
                remaining = max(0, limit - combined_length[0])
                accepted = min(len(chunk), remaining)
                destination.extend(chunk[:accepted])
                combined_length[0] += accepted
            if accepted < len(chunk):
                exceeded.set()
    except BaseException as error:  # pragma: no cover - defensive thread boundary
        failures.append(error)
    finally:
        try:
            stream.close()
        except OSError:
            pass


def _write_stdin(
    stream: Any, data: bytes, failures: list[BaseException]
) -> None:
    try:
        stream.write(data)
        stream.flush()
    except BrokenPipeError:
        pass
    except BaseException as error:  # pragma: no cover - defensive thread boundary
        failures.append(error)
    finally:
        try:
            stream.close()
        except OSError:
            pass


def _read_module_status(
    fd: int,
    destination: bytearray,
    malformed: threading.Event,
    failures: list[BaseException],
) -> None:
    try:
        while True:
            chunk = os.read(fd, 64)
            if not chunk:
                return
            remaining = max(0, _MODULE_STATUS_MAX_BYTES - len(destination))
            destination.extend(chunk[:remaining])
            if len(chunk) > remaining:
                malformed.set()
    except BaseException as error:  # pragma: no cover - defensive thread boundary
        failures.append(error)
    finally:
        try:
            os.close(fd)
        except OSError:
            pass


def _kill_process_group(process: subprocess.Popen[bytes]) -> None:
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except (ProcessLookupError, PermissionError):
        try:
            process.kill()
        except ProcessLookupError:
            pass


def _parse_module_status(payload: bytes) -> int:
    if (
        len(payload) != 9
        or payload[-1:] != b"\n"
        or any(byte not in b"0123456789abcdef" for byte in payload[:8])
    ):
        raise CaptureError(
            "module status channel must contain exactly 8 lowercase hex digits "
            "followed by newline"
        )
    return int(payload[:8], 16)


def capture_process(
    contract: OracleContract, request: CaptureRequest
) -> CaptureRecord:
    """Execute one cell with bounded raw capture and an exact WASM i32 side channel."""

    _, host_control_environment, guest_environment = _validate_request(
        contract,
        request,
    )
    child_environment = dict(
        guest_environment
        if request.key.runtime == "php-src"
        else host_control_environment
    )
    process_environment = tuple(sorted(child_environment.items()))
    module_read_fd: int | None = None
    module_write_fd: int | None = None
    pass_fds: tuple[int, ...] = ()
    if request.require_module_i32:
        module_read_fd, module_write_fd = os.pipe()
        child_environment[MODULE_STATUS_FD_ENV] = str(module_write_fd)
        pass_fds = (module_write_fd,)

    started = time.monotonic()
    try:
        process = subprocess.Popen(
            request.argv,
            cwd=request.cwd,
            env=child_environment,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            start_new_session=True,
            pass_fds=pass_fds,
        )
    except OSError as error:
        if module_read_fd is not None:
            os.close(module_read_fd)
        if module_write_fd is not None:
            os.close(module_write_fd)
        raise CaptureError(f"failed to start capture command: {error}") from error

    if module_write_fd is not None:
        os.close(module_write_fd)
    assert process.stdin is not None
    assert process.stdout is not None
    assert process.stderr is not None

    stdout = bytearray()
    stderr = bytearray()
    module_status = bytearray()
    output_exceeded = threading.Event()
    combined_output_length = [0]
    combined_output_lock = threading.Lock()
    module_malformed = threading.Event()
    failures: list[BaseException] = []
    threads = [
        threading.Thread(
            target=_write_stdin,
            args=(process.stdin, request.stdin, failures),
            daemon=True,
        ),
        threading.Thread(
            target=_read_stream,
            args=(
                process.stdout,
                request.output_limit_bytes,
                stdout,
                combined_output_length,
                combined_output_lock,
                output_exceeded,
                failures,
            ),
            daemon=True,
        ),
        threading.Thread(
            target=_read_stream,
            args=(
                process.stderr,
                request.output_limit_bytes,
                stderr,
                combined_output_length,
                combined_output_lock,
                output_exceeded,
                failures,
            ),
            daemon=True,
        ),
    ]
    if module_read_fd is not None:
        threads.append(
            threading.Thread(
                target=_read_module_status,
                args=(
                    module_read_fd,
                    module_status,
                    module_malformed,
                    failures,
                ),
                daemon=True,
            )
        )
    for thread in threads:
        thread.start()

    deadline = started + request.timeout_seconds
    timed_out = False
    output_limit_exceeded = False
    malformed_status = False
    while process.poll() is None:
        if output_exceeded.is_set():
            output_limit_exceeded = True
            _kill_process_group(process)
            break
        if module_malformed.is_set():
            malformed_status = True
            _kill_process_group(process)
            break
        if time.monotonic() >= deadline:
            timed_out = True
            _kill_process_group(process)
            break
        time.sleep(0.005)

    try:
        returncode = process.wait(timeout=5)
    except subprocess.TimeoutExpired as error:  # pragma: no cover - SIGKILL invariant
        _kill_process_group(process)
        raise CaptureError("capture process did not terminate after SIGKILL") from error
    for thread in threads:
        thread.join(timeout=5)
        if thread.is_alive():  # pragma: no cover - closed pipe invariant
            raise CaptureError("capture I/O thread did not terminate")
    elapsed = time.monotonic() - started

    if failures:
        raise CaptureError(f"capture I/O failed: {failures[0]}")
    if malformed_status or module_malformed.is_set():
        raise CaptureError("module status channel exceeded its bounded payload")
    output_limit_exceeded = output_limit_exceeded or output_exceeded.is_set()

    host_status = returncode if returncode >= 0 else None
    terminating_signal = -returncode if returncode < 0 else None
    module_i32_bits: int | None = None
    interrupted = timed_out or output_limit_exceeded
    if request.require_module_i32 and not interrupted:
        module_i32_bits = _parse_module_status(bytes(module_status))

    normalization = request.normalization
    pin = contract.php_src_pin(request.key.profile)
    record = CaptureRecord(
        key=request.key,
        specification_sha256=contract.specification_sha256,
        inventory_sha256=contract.inventory_sha256,
        pinned_php_src_tag=pin.tag,
        pinned_php_src_tag_object=pin.tag_object,
        pinned_php_src_tag_commit=pin.tag_commit,
        fixture_sha256=request.fixture_sha256,
        guest_program=request.guest_program,
        logical_arguments=request.logical_arguments,
        guest_preopens=tuple(sorted(request.guest_preopens.items())),
        argv=request.argv,
        cwd=str(request.cwd),
        host_control_environment=host_control_environment,
        guest_environment=guest_environment,
        process_environment=process_environment,
        operating_system=platform.system(),
        architecture=platform.machine(),
        provenance=request.provenance,
        artifact_provenance=request.artifact_provenance,
        stdin=RawBytes.from_bytes(request.stdin),
        stdout=RawBytes.from_bytes(bytes(stdout)),
        stderr=RawBytes.from_bytes(bytes(stderr)),
        normalization=normalization,
        normalized_stdout=RawBytes.from_bytes(normalization.apply(bytes(stdout))),
        normalized_stderr=RawBytes.from_bytes(normalization.apply(bytes(stderr))),
        host_status=host_status,
        host_status_representation="posix_process_status",
        signal=terminating_signal,
        module_i32_bits=module_i32_bits,
        timed_out=timed_out,
        output_limit_exceeded=output_limit_exceeded,
        timeout_seconds=request.timeout_seconds,
        output_limit_bytes=request.output_limit_bytes,
        elapsed_seconds=elapsed,
    )
    record.validate(contract)
    return record
