"""Fail-closed loading and validation of the pinned WASM/PHP oracle contract."""

from __future__ import annotations

import hashlib
import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Mapping


INVENTORY_RELATIVE_PATH = Path("docs/specs/wasm-inventory.json")
SPECIFICATION_RELATIVE_PATH = Path("docs/specs/wasm-compliance.md")
SUPPORTED_PROFILES = ("8.2", "8.3", "8.4", "8.5")
SUPPORTED_RUNTIMES = ("php-src", "wasm")
EXECUTION_CELLS = (
    ("php-src", "php-src"),
    ("wasm", "node"),
    ("wasm", "wasmer"),
    ("wasm", "wasmtime"),
)
REQUIRED_TOOLCHAIN_PINS = (
    "rust",
    "wat",
    "wasmparser",
    "wasmer",
    "wasmtime",
    "wasm_tools",
    "node",
    "typescript",
    "npm",
)

_SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
_COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")


class ContractError(ValueError):
    """Raised when an oracle contract or one of its prerequisites is invalid."""


def sha256_bytes(data: bytes) -> str:
    """Return the lowercase SHA-256 digest for exact bytes."""

    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    """Hash an existing regular file without text or newline normalization."""

    if not path.is_file():
        raise ContractError(f"required regular file is missing: {path}")
    try:
        return sha256_bytes(path.read_bytes())
    except OSError as error:
        raise ContractError(f"cannot read required file {path}: {error}") from error


def _reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ContractError(f"duplicate JSON key: {key!r}")
        result[key] = value
    return result


def load_json_file(path: Path) -> Any:
    """Load JSON while rejecting duplicate keys and unreadable/non-file inputs."""

    if not path.is_file():
        raise ContractError(f"required JSON file is missing: {path}")
    try:
        raw = path.read_bytes()
    except OSError as error:
        raise ContractError(f"cannot read required JSON file {path}: {error}") from error
    try:
        return json.loads(raw, object_pairs_hook=_reject_duplicate_keys)
    except ContractError:
        raise
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ContractError(f"invalid JSON in {path}: {error}") from error


def _mapping(value: Any, label: str) -> Mapping[str, Any]:
    if not isinstance(value, dict):
        raise ContractError(f"{label} must be a JSON object")
    return value


def _list(value: Any, label: str) -> list[Any]:
    if not isinstance(value, list):
        raise ContractError(f"{label} must be a JSON array")
    return value


def _string(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value:
        raise ContractError(f"{label} must be a non-empty string")
    return value


def _sha256(value: Any, label: str) -> str:
    digest = _string(value, label)
    if not _SHA256_RE.fullmatch(digest):
        raise ContractError(f"{label} must be a lowercase 64-character SHA-256")
    return digest


def _commit(value: Any, label: str) -> str:
    commit = _string(value, label)
    if not _COMMIT_RE.fullmatch(commit):
        raise ContractError(f"{label} must be a lowercase 40-character Git commit")
    return commit


def _string_pairs(
    value: Mapping[str, str] | tuple[tuple[str, str], ...],
    label: str,
    *,
    allow_empty: bool,
) -> tuple[tuple[str, str], ...]:
    items = tuple(value.items()) if isinstance(value, Mapping) else tuple(value)
    if not allow_empty and not items:
        raise ContractError(f"{label} must not be empty")
    seen: set[str] = set()
    normalized: list[tuple[str, str]] = []
    for key, item_value in items:
        if not isinstance(key, str) or not key or "\x00" in key:
            raise ContractError(f"{label} contains an invalid key")
        if key in seen:
            raise ContractError(f"{label} contains duplicate key {key!r}")
        if not isinstance(item_value, str) or not item_value or "\x00" in item_value:
            raise ContractError(f"{label}.{key} must be a non-empty string")
        seen.add(key)
        normalized.append((key, item_value))
    return tuple(sorted(normalized))


@dataclass(frozen=True)
class PhpSrcPin:
    """One immutable php-src tag object plus its peeled source commit."""

    profile: str
    tag: str
    tag_object: str
    tag_commit: str

    def to_dict(self) -> dict[str, str]:
        """Serialize the pin in deterministic field order."""

        return {
            "profile": self.profile,
            "tag": self.tag,
            "tag_object": self.tag_object,
            "tag_commit": self.tag_commit,
        }


@dataclass(frozen=True, order=True)
class RunKey:
    """Stable identity of one fixture/profile/runtime matrix cell."""

    fixture_id: str
    profile: str
    runtime: str
    host: str

    def __post_init__(self) -> None:
        if (
            not isinstance(self.fixture_id, str)
            or not self.fixture_id
            or any(ord(char) < 0x20 for char in self.fixture_id)
        ):
            raise ContractError("fixture_id must be a non-empty control-free string")
        if self.profile not in SUPPORTED_PROFILES:
            raise ContractError(f"unsupported PHP profile: {self.profile!r}")
        if self.runtime not in SUPPORTED_RUNTIMES:
            raise ContractError(f"unsupported oracle runtime: {self.runtime!r}")
        if (self.runtime, self.host) not in EXECUTION_CELLS:
            raise ContractError(
                f"unsupported oracle runtime/host cell: "
                f"{self.runtime!r}/{self.host!r}"
            )

    def to_dict(self) -> dict[str, str]:
        """Serialize the key in deterministic field order."""

        return {
            "fixture_id": self.fixture_id,
            "profile": self.profile,
            "runtime": self.runtime,
            "host": self.host,
        }

    @classmethod
    def from_dict(cls, value: Any) -> "RunKey":
        """Parse a key and reject missing or unknown fields."""

        data = _mapping(value, "key")
        expected = {"fixture_id", "profile", "runtime", "host"}
        if set(data) != expected:
            raise ContractError(
                f"key fields must be exactly {sorted(expected)}, got {sorted(data)}"
            )
        return cls(
            fixture_id=_string(data["fixture_id"], "key.fixture_id"),
            profile=_string(data["profile"], "key.profile"),
            runtime=_string(data["runtime"], "key.runtime"),
            host=_string(data["host"], "key.host"),
        )


@dataclass(frozen=True)
class RuntimeProvenance:
    """Immutable runtime/build prerequisites attached to every capture."""

    executable_sha256: str
    version: str
    source_commit: str | None
    build_configuration: tuple[tuple[str, str], ...]
    ini_mode: str
    ini_sha256: str | None
    extensions: tuple[str, ...]

    @classmethod
    def create(
        cls,
        *,
        executable_sha256: str,
        version: str,
        source_commit: str | None,
        build_configuration: Mapping[str, str] | tuple[tuple[str, str], ...],
        ini_mode: str,
        ini_sha256: str | None,
        extensions: tuple[str, ...] | list[str],
    ) -> "RuntimeProvenance":
        """Construct provenance while canonicalizing maps and extension order."""

        normalized_extensions = tuple(extensions)
        if any(
            not isinstance(extension, str)
            or not extension
            or "\x00" in extension
            for extension in normalized_extensions
        ):
            raise ContractError("extensions must contain only non-empty strings")
        if len(set(normalized_extensions)) != len(normalized_extensions):
            raise ContractError("extensions must not contain duplicates")
        return cls(
            executable_sha256=_sha256(
                executable_sha256, "provenance.executable_sha256"
            ),
            version=_string(version, "provenance.version"),
            source_commit=(
                None
                if source_commit is None
                else _commit(source_commit, "provenance.source_commit")
            ),
            build_configuration=_string_pairs(
                build_configuration,
                "provenance.build_configuration",
                allow_empty=False,
            ),
            ini_mode=_string(ini_mode, "provenance.ini_mode"),
            ini_sha256=(
                None
                if ini_sha256 is None
                else _sha256(ini_sha256, "provenance.ini_sha256")
            ),
            extensions=normalized_extensions,
        )

    def validate_for(self, key: RunKey, contract: "OracleContract") -> None:
        """Check runtime-specific provenance against the pinned contract."""

        _sha256(self.executable_sha256, "provenance.executable_sha256")
        _string(self.version, "provenance.version")
        if self.source_commit is not None:
            _commit(self.source_commit, "provenance.source_commit")
        _string_pairs(
            self.build_configuration,
            "provenance.build_configuration",
            allow_empty=False,
        )
        configuration = dict(self.build_configuration)
        if len(set(self.extensions)) != len(self.extensions):
            raise ContractError("extensions must not contain duplicates")
        if any(not extension or "\x00" in extension for extension in self.extensions):
            raise ContractError("extensions must contain only non-empty strings")

        if key.runtime == "php-src":
            if key.host != "php-src":
                raise ContractError("php-src runtime must use php-src host")
            pin = contract.php_src_pin(key.profile)
            if self.source_commit != pin.tag_commit:
                raise ContractError(
                    f"{key.profile} php-src commit {self.source_commit} does not match "
                    f"pinned peeled tag commit {pin.tag_commit}"
                )
            expected_version = pin.tag.removeprefix("php-")
            if self.version != expected_version:
                raise ContractError(
                    f"{key.profile} php-src version {self.version!r} does not "
                    f"match pinned runtime version {expected_version!r}"
                )
            for field in ("configure_command", "build_flags"):
                if field not in configuration:
                    raise ContractError(
                        f"php-src build_configuration is missing {field}"
                    )
            if self.ini_mode not in {"php-n", "explicit"}:
                raise ContractError(
                    "php-src provenance ini_mode must be 'php-n' or 'explicit'"
                )
            if self.ini_mode == "php-n" and self.ini_sha256 is not None:
                raise ContractError("php-n provenance must not carry ini_sha256")
            if self.ini_mode == "explicit" and self.ini_sha256 is None:
                raise ContractError("explicit PHP INI provenance requires ini_sha256")
        else:
            if self.source_commit is not None:
                raise ContractError(
                    "WASM host provenance must not reuse the Elephc source commit"
                )
            pinned_host_version = dict(contract.toolchain)[key.host]
            if self.version != pinned_host_version:
                raise ContractError(
                    f"{key.host} version {self.version!r} does not match "
                    f"pinned {pinned_host_version!r}"
                )
            if self.ini_mode != "not-applicable":
                raise ContractError(
                    "WASM provenance ini_mode must be 'not-applicable'"
                )
            if self.ini_sha256 is not None or self.extensions:
                raise ContractError(
                    "WASM provenance must not carry PHP INI or extension state"
                )

    def to_dict(self) -> dict[str, Any]:
        """Serialize provenance without dropping an explicitly empty extension set."""

        return {
            "executable_sha256": self.executable_sha256,
            "version": self.version,
            "source_commit": self.source_commit,
            "build_configuration": dict(self.build_configuration),
            "ini_mode": self.ini_mode,
            "ini_sha256": self.ini_sha256,
            "extensions": list(self.extensions),
        }

    @classmethod
    def from_dict(cls, value: Any) -> "RuntimeProvenance":
        """Parse provenance and reject missing or unknown fields."""

        data = _mapping(value, "provenance")
        expected = {
            "executable_sha256",
            "version",
            "source_commit",
            "build_configuration",
            "ini_mode",
            "ini_sha256",
            "extensions",
        }
        if set(data) != expected:
            raise ContractError(
                "provenance fields must be exactly "
                f"{sorted(expected)}, got {sorted(data)}"
            )
        ini_sha256 = data["ini_sha256"]
        if ini_sha256 is not None and not isinstance(ini_sha256, str):
            raise ContractError("provenance.ini_sha256 must be a string or null")
        source_commit = data["source_commit"]
        if source_commit is not None and not isinstance(source_commit, str):
            raise ContractError("provenance.source_commit must be a string or null")
        return cls.create(
            executable_sha256=_string(
                data["executable_sha256"], "provenance.executable_sha256"
            ),
            version=_string(data["version"], "provenance.version"),
            source_commit=source_commit,
            build_configuration=_mapping(
                data["build_configuration"], "provenance.build_configuration"
            ),
            ini_mode=_string(data["ini_mode"], "provenance.ini_mode"),
            ini_sha256=ini_sha256,
            extensions=tuple(_list(data["extensions"], "provenance.extensions")),
        )


@dataclass(frozen=True)
class CompilerArtifactProvenance:
    """Exact Elephc compiler and validated package/artifact hashes."""

    elephc_source_commit: str
    compiler_executable_sha256: str
    compiler_version: str
    wat_sha256: str
    wasm_sha256: str
    validated_artifact_sha256: str
    index_mjs_sha256: str
    package_json_sha256: str

    @classmethod
    def create(
        cls,
        *,
        elephc_source_commit: str,
        compiler_executable_sha256: str,
        compiler_version: str,
        wat_sha256: str,
        wasm_sha256: str,
        validated_artifact_sha256: str,
        index_mjs_sha256: str,
        package_json_sha256: str,
    ) -> "CompilerArtifactProvenance":
        """Construct a fully populated artifact provenance record."""

        result = cls(
            elephc_source_commit=_commit(
                elephc_source_commit, "artifact.elephc_source_commit"
            ),
            compiler_executable_sha256=_sha256(
                compiler_executable_sha256,
                "artifact.compiler_executable_sha256",
            ),
            compiler_version=_string(
                compiler_version, "artifact.compiler_version"
            ),
            wat_sha256=_sha256(wat_sha256, "artifact.wat_sha256"),
            wasm_sha256=_sha256(wasm_sha256, "artifact.wasm_sha256"),
            validated_artifact_sha256=_sha256(
                validated_artifact_sha256,
                "artifact.validated_artifact_sha256",
            ),
            index_mjs_sha256=_sha256(
                index_mjs_sha256, "artifact.index_mjs_sha256"
            ),
            package_json_sha256=_sha256(
                package_json_sha256, "artifact.package_json_sha256"
            ),
        )
        if result.validated_artifact_sha256 != result.wasm_sha256:
            raise ContractError(
                "validated artifact hash must equal the executed WASM hash"
            )
        return result

    def to_dict(self) -> dict[str, str]:
        """Serialize compiler and artifact provenance."""

        return {
            "elephc_source_commit": self.elephc_source_commit,
            "compiler_executable_sha256": self.compiler_executable_sha256,
            "compiler_version": self.compiler_version,
            "wat_sha256": self.wat_sha256,
            "wasm_sha256": self.wasm_sha256,
            "validated_artifact_sha256": self.validated_artifact_sha256,
            "index_mjs_sha256": self.index_mjs_sha256,
            "package_json_sha256": self.package_json_sha256,
        }

    @classmethod
    def from_dict(cls, value: Any) -> "CompilerArtifactProvenance":
        """Parse compiler/artifact provenance without optional hash fields."""

        data = _mapping(value, "artifact")
        expected = {
            "elephc_source_commit",
            "compiler_executable_sha256",
            "compiler_version",
            "wat_sha256",
            "wasm_sha256",
            "validated_artifact_sha256",
            "index_mjs_sha256",
            "package_json_sha256",
        }
        if set(data) != expected:
            raise ContractError(
                f"artifact fields must be exactly {sorted(expected)}, "
                f"got {sorted(data)}"
            )
        return cls.create(
            elephc_source_commit=data["elephc_source_commit"],
            compiler_executable_sha256=data["compiler_executable_sha256"],
            compiler_version=data["compiler_version"],
            wat_sha256=data["wat_sha256"],
            wasm_sha256=data["wasm_sha256"],
            validated_artifact_sha256=data["validated_artifact_sha256"],
            index_mjs_sha256=data["index_mjs_sha256"],
            package_json_sha256=data["package_json_sha256"],
        )


@dataclass(frozen=True)
class OracleContract:
    """Verified pins loaded from the generated inventory and anchored spec."""

    repo_root: Path
    inventory_path: Path
    specification_path: Path
    inventory_schema: str
    inventory_generator_version: str
    inventory_sha256: str
    specification_sha256: str
    wasm_core_tag: str
    wasm_core_commit: str
    wasi_preview1_commit: str
    php_src_pins: tuple[PhpSrcPin, ...]
    toolchain: tuple[tuple[str, str], ...]

    @classmethod
    def load(
        cls,
        repo_root: Path,
        inventory_path: Path | None = None,
        specification_path: Path | None = None,
    ) -> "OracleContract":
        """Load pins and fail if the generated inventory does not anchor the spec."""

        root = Path(repo_root).resolve()
        if not root.is_dir():
            raise ContractError(f"repository root is missing: {root}")
        inventory = (
            Path(inventory_path).resolve()
            if inventory_path is not None
            else root / INVENTORY_RELATIVE_PATH
        )
        specification = (
            Path(specification_path).resolve()
            if specification_path is not None
            else root / SPECIFICATION_RELATIVE_PATH
        )

        document = _mapping(load_json_file(inventory), "inventory")
        metadata = _mapping(document.get("metadata"), "inventory.metadata")
        inventory_schema = _string(
            metadata.get("schema"), "inventory.metadata.schema"
        )
        if inventory_schema != "elephc.wasm-inventory.v4":
            raise ContractError(
                "unsupported inventory schema "
                f"{inventory_schema!r}; expected 'elephc.wasm-inventory.v4'"
            )
        generator_version = _string(
            metadata.get("generator_version"),
            "inventory.metadata.generator_version",
        )
        pins = _mapping(metadata.get("pins"), "inventory.metadata.pins")

        expected_spec_sha = _sha256(
            pins.get("wasm_compliance_sha256"),
            "inventory.metadata.pins.wasm_compliance_sha256",
        )
        actual_spec_sha = sha256_file(specification)
        if actual_spec_sha != expected_spec_sha:
            raise ContractError(
                "WASM specification hash mismatch: "
                f"inventory pins {expected_spec_sha}, file is {actual_spec_sha}"
            )

        wasm_core = _mapping(
            pins.get("wasm_core_3_0"), "inventory.metadata.pins.wasm_core_3_0"
        )
        wasm_core_tag = _string(
            wasm_core.get("tag"), "inventory.metadata.pins.wasm_core_3_0.tag"
        )
        wasm_core_commit = _commit(
            wasm_core.get("commit"),
            "inventory.metadata.pins.wasm_core_3_0.commit",
        )
        wasi_commit = _commit(
            pins.get("wasi_preview1_commit"),
            "inventory.metadata.pins.wasi_preview1_commit",
        )

        raw_php_pins = _list(
            pins.get("php_src"), "inventory.metadata.pins.php_src"
        )
        php_pins: list[PhpSrcPin] = []
        for index, raw_pin in enumerate(raw_php_pins):
            pin = _mapping(
                raw_pin, f"inventory.metadata.pins.php_src[{index}]"
            )
            if set(pin) != {"profile", "tag", "tag_object", "tag_commit"}:
                raise ContractError(
                    "php_src"
                    f"[{index}] fields must be exactly "
                    "profile/tag/tag_object/tag_commit"
                )
            profile = _string(pin["profile"], f"php_src[{index}].profile")
            tag = _string(pin["tag"], f"php_src[{index}].tag")
            tag_object = _commit(
                pin["tag_object"], f"php_src[{index}].tag_object"
            )
            tag_commit = _commit(
                pin["tag_commit"], f"php_src[{index}].tag_commit"
            )
            if not tag.startswith(f"php-{profile}."):
                raise ContractError(
                    f"php_src[{index}] tag {tag!r} does not match profile {profile}"
                )
            if tag_object == tag_commit:
                raise ContractError(
                    f"php_src[{index}] annotated tag object and peeled commit "
                    "must be recorded separately"
                )
            php_pins.append(
                PhpSrcPin(
                    profile=profile,
                    tag=tag,
                    tag_object=tag_object,
                    tag_commit=tag_commit,
                )
            )

        profiles = tuple(pin.profile for pin in php_pins)
        if profiles != SUPPORTED_PROFILES:
            raise ContractError(
                f"php-src profiles must be exactly {SUPPORTED_PROFILES}, got {profiles}"
            )

        raw_toolchain = _mapping(
            pins.get("toolchain"), "inventory.metadata.pins.toolchain"
        )
        for name in REQUIRED_TOOLCHAIN_PINS:
            _string(
                raw_toolchain.get(name),
                f"inventory.metadata.pins.toolchain.{name}",
            )
        toolchain = _string_pairs(
            raw_toolchain,
            "inventory.metadata.pins.toolchain",
            allow_empty=False,
        )

        return cls(
            repo_root=root,
            inventory_path=inventory,
            specification_path=specification,
            inventory_schema=inventory_schema,
            inventory_generator_version=generator_version,
            inventory_sha256=sha256_file(inventory),
            specification_sha256=actual_spec_sha,
            wasm_core_tag=wasm_core_tag,
            wasm_core_commit=wasm_core_commit,
            wasi_preview1_commit=wasi_commit,
            php_src_pins=tuple(php_pins),
            toolchain=toolchain,
        )

    @property
    def profiles(self) -> tuple[str, ...]:
        """Return the exact ordered profile dimension of the oracle matrix."""

        return tuple(pin.profile for pin in self.php_src_pins)

    def php_src_pin(self, profile: str) -> PhpSrcPin:
        """Resolve one supported profile or fail rather than selecting a fallback."""

        for pin in self.php_src_pins:
            if pin.profile == profile:
                return pin
        raise ContractError(f"no pinned php-src revision for profile {profile!r}")

    def to_dict(self) -> dict[str, Any]:
        """Serialize only verified, immutable contract data."""

        return {
            "inventory": {
                "path": str(self.inventory_path),
                "schema": self.inventory_schema,
                "generator_version": self.inventory_generator_version,
                "sha256": self.inventory_sha256,
            },
            "specification": {
                "path": str(self.specification_path),
                "sha256": self.specification_sha256,
            },
            "wasm_core_3_0": {
                "tag": self.wasm_core_tag,
                "commit": self.wasm_core_commit,
            },
            "wasi_preview1_commit": self.wasi_preview1_commit,
            "php_src": [pin.to_dict() for pin in self.php_src_pins],
            "toolchain": dict(self.toolchain),
        }
