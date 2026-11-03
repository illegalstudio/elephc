"""Independently validate pinned php-src build artifacts for oracle execution."""

from __future__ import annotations

import json
import os
import re
import stat
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Any, Mapping

from .contract import (
    ContractError,
    OracleContract,
    RunKey,
    RuntimeProvenance,
    SUPPORTED_PROFILES,
    sha256_bytes,
    sha256_file,
)


ROOT_SCHEMA = "elephc.pinned-php-src-build-set.v2"
PROFILE_SCHEMA = "elephc.pinned-php-src-build.v2"
BUILD_ENVIRONMENT_SCHEMA = "elephc.pinned-php-src-build-environment.v1"
INSTALL_TREE_SCHEMA = "elephc.pinned-php-src-install-tree.v1"
RUNTIME_METADATA_SCHEMA = "elephc.pinned-php-src-runtime-metadata.v1"
PHP_SRC_REPOSITORY = "https://github.com/php/php-src.git"
_COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
_OPTIONAL_BUILD_FLAGS = (
    "CC",
    "CFLAGS",
    "CPPFLAGS",
    "LDFLAGS",
    "PKG_CONFIG_PATH",
)
_RESOLVED_BUILD_TOOLS = tuple(sorted((
    "autoconf",
    "bison",
    "cc",
    "git",
    "make",
    "re2c",
    "tar",
)))
_EXPLICITLY_CLEARED_ENVIRONMENT = tuple(sorted((
    "BASH_ENV",
    "CONFIG_SITE",
    "ENV",
    "GIT_ASKPASS",
    "GIT_CONFIG_COUNT",
    "GIT_SSH",
    "GIT_SSH_COMMAND",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "LD_LIBRARY_PATH",
    "LIBRARY_PATH",
    "MAKEFLAGS",
    "MAKELEVEL",
    "MFLAGS",
    "NO_PROXY",
    "PHP_INI_SCAN_DIR",
    "all_proxy",
    "http_proxy",
    "https_proxy",
    "no_proxy",
)))
_CONFIGURE_ARGS = (
    "--prefix=/install",
    "--disable-all",
    "--enable-cli",
    "--disable-cgi",
    "--disable-phpdbg",
    "--without-pear",
)

_PROFILE_HASH_PATHS = (
    "install/bin/php",
    "install-tree.json",
    "php-metadata.json",
    "dynamic-dependencies.txt",
    "provenance.json",
)
_ROOT_BASE_HASH_PATHS = (
    "build-environment.json",
    "elephc-git-status.txt",
    "provenance.json",
)


@dataclass(frozen=True)
class PhpSrcRuntimeArtifact:
    """One fully verified php-src runtime and its immutable evidence hashes."""

    profile: str
    executable: Path
    provenance: RuntimeProvenance
    root_provenance_sha256: str
    profile_provenance_sha256: str
    build_environment_sha256: str
    install_tree_sha256: str
    runtime_metadata_sha256: str
    dynamic_dependencies_sha256: str

    def to_dict(self) -> dict[str, Any]:
        """Serialize the verified artifact without omitting evidence hashes."""

        return {
            "profile": self.profile,
            "executable": str(self.executable),
            "provenance": self.provenance.to_dict(),
            "evidence": {
                "root_provenance_sha256": self.root_provenance_sha256,
                "profile_provenance_sha256": self.profile_provenance_sha256,
                "build_environment_sha256": self.build_environment_sha256,
                "install_tree_sha256": self.install_tree_sha256,
                "runtime_metadata_sha256": self.runtime_metadata_sha256,
                "dynamic_dependencies_sha256": self.dynamic_dependencies_sha256,
            },
        }


def _mapping(value: Any, label: str) -> Mapping[str, Any]:
    if not isinstance(value, dict):
        raise ContractError(f"{label} must be a JSON object")
    return value


def _list(value: Any, label: str) -> list[Any]:
    if not isinstance(value, list):
        raise ContractError(f"{label} must be a JSON array")
    return value


def _string(value: Any, label: str, *, allow_empty: bool = False) -> str:
    if not isinstance(value, str) or (not allow_empty and not value):
        qualifier = "" if allow_empty else " non-empty"
        raise ContractError(f"{label} must be a{qualifier} string")
    if "\x00" in value:
        raise ContractError(f"{label} must not contain NUL")
    return value


def _exact_fields(
    value: Mapping[str, Any], expected: set[str], label: str
) -> None:
    if set(value) != expected:
        raise ContractError(
            f"{label} fields must be exactly {sorted(expected)}, "
            f"got {sorted(value)}"
        )


def _load_json_snapshot(path: Path, label: str) -> tuple[Any, str]:
    try:
        raw = path.read_bytes()
    except OSError as error:
        raise ContractError(f"cannot read {label}: {path}: {error}") from error

    def reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
        document: dict[str, Any] = {}
        for key, value in pairs:
            if key in document:
                raise ContractError(f"{label} contains duplicate JSON key {key!r}")
            document[key] = value
        return document

    try:
        document = json.loads(raw, object_pairs_hook=reject_duplicate_keys)
    except ContractError:
        raise
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ContractError(f"invalid JSON in {label}: {path}: {error}") from error
    return document, sha256_bytes(raw)


def _canonical_json(value: Any) -> str:
    return json.dumps(
        value,
        ensure_ascii=True,
        sort_keys=True,
        separators=(",", ":"),
    )


def _safe_relative_path(value: Any, label: str) -> PurePosixPath:
    text = _string(value, label)
    if "\\" in text:
        raise ContractError(f"{label} must use POSIX separators")
    relative = PurePosixPath(text)
    if (
        relative.is_absolute()
        or text in {".", ".."}
        or ".." in relative.parts
        or relative.as_posix() != text
    ):
        raise ContractError(f"{label} must be a safe relative path")
    return relative


def _regular_file(root: Path, relative: Any, label: str) -> Path:
    path = _safe_relative_path(relative, label)
    current = root
    for part in path.parts:
        current = current / part
        try:
            details = current.lstat()
        except OSError as error:
            raise ContractError(f"{label} is missing: {current}") from error
        if current != root / Path(*path.parts) and stat.S_ISLNK(details.st_mode):
            raise ContractError(f"{label} traverses a symlink: {current}")
    if not stat.S_ISREG(current.lstat().st_mode):
        raise ContractError(f"{label} must be a regular file: {current}")
    return current


def _read_hash_manifest(
    root: Path,
    manifest: Path,
    expected_paths: tuple[str, ...],
    label: str,
) -> tuple[dict[str, str], str]:
    try:
        raw = manifest.read_bytes()
        text = raw.decode("utf-8")
    except (OSError, UnicodeDecodeError) as error:
        raise ContractError(f"cannot read {label}: {manifest}: {error}") from error
    if not raw.endswith(b"\n") or b"\r" in raw:
        raise ContractError(f"{label} must use canonical LF-terminated lines")
    lines = text.splitlines()
    if not lines:
        raise ContractError(f"{label} must not be empty")
    records: dict[str, str] = {}
    for line_number, line in enumerate(lines, start=1):
        if len(line) < 67 or line[64:66] != "  ":
            raise ContractError(
                f"{label} line {line_number} must use '<sha256>  <path>'"
            )
        digest = line[:64]
        relative_text = line[66:]
        if (
            len(digest) != 64
            or any(character not in "0123456789abcdef" for character in digest)
        ):
            raise ContractError(f"{label} line {line_number} has invalid SHA-256")
        relative = _safe_relative_path(
            relative_text, f"{label} line {line_number} path"
        ).as_posix()
        if relative in records:
            raise ContractError(f"{label} contains duplicate path {relative!r}")
        records[relative] = digest
    if tuple(records) != expected_paths:
        raise ContractError(
            f"{label} paths must be exactly {list(expected_paths)}, "
            f"got {list(records)}"
        )
    for relative, expected in records.items():
        path = _regular_file(root, relative, f"{label} entry {relative}")
        actual = sha256_file(path)
        if actual != expected:
            raise ContractError(
                f"{label} SHA-256 mismatch for {relative}: "
                f"expected {expected}, got {actual}"
            )
    return records, sha256_bytes(raw)


def _install_tree_entry(root: Path, path: Path) -> dict[str, Any]:
    relative = path.relative_to(root).as_posix()
    details = path.lstat()
    mode = f"{stat.S_IMODE(details.st_mode):04o}"
    if stat.S_ISDIR(details.st_mode):
        return {"path": relative, "type": "directory", "mode": mode}
    if stat.S_ISREG(details.st_mode):
        return {
            "path": relative,
            "type": "file",
            "mode": mode,
            "size": details.st_size,
            "sha256": sha256_file(path),
        }
    if stat.S_ISLNK(details.st_mode):
        return {
            "path": relative,
            "type": "symlink",
            "mode": mode,
            "target": os.readlink(path),
        }
    raise ContractError(f"unsupported special file in install tree: {relative}")


def _verify_install_tree(profile_root: Path, relative: Any) -> str:
    manifest = _regular_file(
        profile_root, relative, "profile artifact.install_tree"
    )
    raw_document, manifest_sha = _load_json_snapshot(manifest, "install tree")
    document = _mapping(raw_document, "install tree")
    _exact_fields(document, {"schema", "root", "entries"}, "install tree")
    if document["schema"] != INSTALL_TREE_SCHEMA:
        raise ContractError(
            f"install tree schema must be {INSTALL_TREE_SCHEMA!r}"
        )
    if document["root"] != "install":
        raise ContractError("install tree root must be 'install'")
    install = profile_root / "install"
    try:
        root_details = install.lstat()
    except OSError as error:
        raise ContractError(f"install tree is missing: {install}") from error
    if not stat.S_ISDIR(root_details.st_mode) or stat.S_ISLNK(root_details.st_mode):
        raise ContractError("install tree must be a real directory")
    paths = sorted(
        install.rglob("*"),
        key=lambda item: item.relative_to(install).as_posix(),
    )
    expected_entries = [
        _install_tree_entry(install, install),
        *(_install_tree_entry(install, path) for path in paths),
    ]
    entries = _list(document["entries"], "install tree.entries")
    if entries != expected_entries:
        raise ContractError("install tree manifest does not match installed tree")
    return manifest_sha


def _verify_build_environment(
    root: Path, relative: Any
) -> tuple[str, dict[str, str | None]]:
    path = _regular_file(root, relative, "root build environment")
    raw_document, environment_sha = _load_json_snapshot(
        path, "build environment"
    )
    document = _mapping(raw_document, "build environment")
    _exact_fields(
        document,
        {
            "schema",
            "passed",
            "toolchain_overrides",
            "resolved_tools",
            "observed",
            "platform_injected_allowed",
            "explicitly_cleared",
            "source_date_epoch",
        },
        "build environment",
    )
    if document["schema"] != BUILD_ENVIRONMENT_SCHEMA:
        raise ContractError(
            f"build environment schema must be {BUILD_ENVIRONMENT_SCHEMA!r}"
        )
    if document["source_date_epoch"] != "recorded per profile":
        raise ContractError("build environment source_date_epoch policy is invalid")
    passed = _mapping(document["passed"], "build environment.passed")
    required_passed = {
        "PATH",
        "LC_ALL",
        "TZ",
        "GIT_CONFIG_NOSYSTEM",
        "GIT_CONFIG_GLOBAL",
        "GIT_TERMINAL_PROMPT",
    }
    if set(passed) != required_passed:
        raise ContractError("build environment passed allowlist is incomplete")
    if (
        passed["LC_ALL"] != "C"
        or passed["TZ"] != "UTC"
        or passed["GIT_CONFIG_NOSYSTEM"] != "1"
        or passed["GIT_CONFIG_GLOBAL"] != "/dev/null"
        or passed["GIT_TERMINAL_PROMPT"] != "0"
    ):
        raise ContractError("build environment deterministic settings are invalid")
    overrides = _mapping(
        document["toolchain_overrides"], "build environment.toolchain_overrides"
    )
    if tuple(overrides) != _OPTIONAL_BUILD_FLAGS:
        raise ContractError("build environment toolchain overrides are incomplete")
    expected_build_flags: dict[str, str | None] = {}
    for name, raw_details in overrides.items():
        details = _mapping(
            raw_details, f"build environment.toolchain_overrides.{name}"
        )
        _exact_fields(details, {"set", "value"}, f"toolchain override {name}")
        if not isinstance(details["set"], bool):
            raise ContractError(f"toolchain override {name}.set must be boolean")
        if details["set"]:
            expected_build_flags[name] = _string(
                details["value"],
                f"toolchain override {name}.value",
                allow_empty=True,
            )
        elif details["value"] is not None:
            raise ContractError(
                f"unset toolchain override {name}.value must be null"
            )
        else:
            expected_build_flags[name] = None

    resolved_tools = _mapping(
        document["resolved_tools"], "build environment.resolved_tools"
    )
    if tuple(resolved_tools) != _RESOLVED_BUILD_TOOLS:
        raise ContractError("build environment resolved tool set is invalid")
    for name, value in resolved_tools.items():
        tool_path = Path(
            _string(value, f"build environment.resolved_tools.{name}")
        )
        if not tool_path.is_absolute() or ".." in tool_path.parts:
            raise ContractError(
                f"resolved build tool {name} must be an absolute path"
            )

    observed = _mapping(document["observed"], "build environment.observed")
    expected_observed = dict(passed)
    expected_observed.update(
        {
            name: value
            for name, value in expected_build_flags.items()
            if value is not None
        }
    )
    allowed = _list(
        document["platform_injected_allowed"],
        "build environment.platform_injected_allowed",
    )
    expected_allowed = sorted(set(observed) & {"__CF_USER_TEXT_ENCODING"})
    if allowed != expected_allowed:
        raise ContractError(
            "build environment platform injection evidence is invalid"
        )
    for name in allowed:
        _string(observed[name], f"build environment.observed.{name}")
    if set(observed) != set(expected_observed) | set(allowed):
        raise ContractError(
            "build environment observed values exceed the allowlist"
        )
    if any(
        observed.get(name) != value
        for name, value in expected_observed.items()
    ):
        raise ContractError(
            "build environment observed values disagree with passed values"
        )
    if document["explicitly_cleared"] != list(
        _EXPLICITLY_CLEARED_ENVIRONMENT
    ):
        raise ContractError(
            "build environment cleared-variable set is invalid"
        )
    return environment_sha, expected_build_flags


def _verify_dynamic_dependencies(path: Path) -> str:
    try:
        raw = path.read_bytes()
        content = raw.decode("utf-8")
    except (OSError, UnicodeDecodeError) as error:
        raise ContractError(
            f"cannot read dynamic dependency evidence: {path}: {error}"
        ) from error
    lines = content.splitlines()
    if len(lines) < 5:
        raise ContractError("dynamic dependency evidence is incomplete")
    platform_name = lines[0].removeprefix("platform: ")
    tool = lines[1].removeprefix("tool: ")
    if (
        lines[0] != f"platform: {platform_name}"
        or lines[1] != f"tool: {tool}"
        or lines[2] != "binary: install/bin/php"
        or lines[3] != "output:"
        or not any(line.strip() for line in lines[4:])
    ):
        raise ContractError("dynamic dependency evidence format is invalid")
    expected_tool = {"Darwin": "otool -L", "Linux": "ldd"}.get(platform_name)
    if expected_tool is None or tool != expected_tool:
        raise ContractError("dynamic dependency platform/tool pair is invalid")
    output = "\n".join(lines[4:])
    if (
        re.search(r"=>\s+not found(?:\s|$)", output)
        or "\x00" in content
        or str(path.parent / "install" / "bin" / "php") in output
    ):
        raise ContractError(
            "dynamic dependency evidence contains unresolved state"
        )
    return sha256_bytes(raw)


def _verify_runtime_metadata(
    profile_root: Path,
    relative: Any,
    *,
    expected_version: str,
    source_date_epoch: int,
    configure_args: list[Any],
) -> tuple[str, tuple[str, ...], tuple[str, ...], Mapping[str, Any]]:
    path = _regular_file(
        profile_root, relative, "profile artifact.runtime_metadata"
    )
    raw_document, metadata_sha = _load_json_snapshot(path, "runtime metadata")
    document = _mapping(raw_document, "runtime metadata")
    _exact_fields(
        document,
        {"schema", "environment", "environment_policy", "php", "configure"},
        "runtime metadata",
    )
    if document["schema"] != RUNTIME_METADATA_SCHEMA:
        raise ContractError(
            f"runtime metadata schema must be {RUNTIME_METADATA_SCHEMA!r}"
        )
    environment = _mapping(document["environment"], "runtime metadata.environment")
    required_environment = {
        "LC_ALL": "C",
        "SOURCE_DATE_EPOCH": str(source_date_epoch),
        "TZ": "UTC",
    }
    if any(environment.get(key) != value for key, value in required_environment.items()):
        raise ContractError("runtime metadata deterministic environment is invalid")
    if set(environment) - set(required_environment) - {"__CF_USER_TEXT_ENCODING"}:
        raise ContractError("runtime metadata environment exceeds the allowlist")
    policy = _mapping(
        document["environment_policy"], "runtime metadata.environment_policy"
    )
    _exact_fields(
        policy,
        {"passed", "platform_injected_allowed"},
        "runtime metadata.environment_policy",
    )
    if policy["passed"] != required_environment:
        raise ContractError("runtime metadata passed environment is invalid")
    allowed = _list(
        policy["platform_injected_allowed"],
        "runtime metadata.environment_policy.platform_injected_allowed",
    )
    if allowed != sorted(set(environment) & {"__CF_USER_TEXT_ENCODING"}):
        raise ContractError("runtime metadata platform environment evidence is invalid")

    php = _mapping(document["php"], "runtime metadata.php")
    _exact_fields(
        php,
        {"version", "sapi", "ini", "extensions", "zend_extensions"},
        "runtime metadata.php",
    )
    if php["version"] != expected_version or php["sapi"] != "cli":
        raise ContractError("runtime metadata PHP version or SAPI is invalid")
    ini = _mapping(php["ini"], "runtime metadata.php.ini")
    _exact_fields(
        ini,
        {
            "loaded_file",
            "scanned_files",
            "config_file_path",
            "config_file_scan_dir",
        },
        "runtime metadata.php.ini",
    )
    if ini["loaded_file"] is not None or ini["scanned_files"] is not None:
        raise ContractError("pinned PHP runtime metadata did not use -n")
    for key in ("config_file_path", "config_file_scan_dir"):
        value = ini[key]
        if value is not False:
            _string(
                value,
                f"runtime metadata.php.ini.{key}",
                allow_empty=True,
            )
    extensions = tuple(_list(php["extensions"], "runtime metadata.php.extensions"))
    zend_extensions = tuple(
        _list(php["zend_extensions"], "runtime metadata.php.zend_extensions")
    )
    for label, values in (
        ("extensions", extensions),
        ("zend_extensions", zend_extensions),
    ):
        if any(not isinstance(value, str) or not value for value in values):
            raise ContractError(f"runtime metadata {label} contains invalid names")
        if list(values) != sorted(set(values)):
            raise ContractError(
                f"runtime metadata {label} must be sorted and unique"
            )
    configure = _mapping(document["configure"], "runtime metadata.configure")
    _exact_fields(configure, {"args", "command"}, "runtime metadata.configure")
    if configure["args"] != configure_args:
        raise ContractError("runtime metadata configure args do not match provenance")
    if configure["command"] != ["configure", *configure_args]:
        raise ContractError("runtime metadata configure command is invalid")
    return metadata_sha, extensions, zend_extensions, ini


def _probe_php_runtime(
    executable: Path, source_date_epoch: int
) -> Mapping[str, Any]:
    probe = (
        "$extensions=get_loaded_extensions(false);sort($extensions,SORT_STRING);"
        "$zend=get_loaded_extensions(true);sort($zend,SORT_STRING);"
        "echo json_encode(['version'=>PHP_VERSION,'sapi'=>PHP_SAPI,"
        "'loaded_ini'=>php_ini_loaded_file()?:null,"
        "'scanned_ini'=>php_ini_scanned_files()?:null,"
        "'extensions'=>$extensions,'zend_extensions'=>$zend],"
        "JSON_UNESCAPED_SLASHES);"
    )
    environment = {
        "LC_ALL": "C",
        "SOURCE_DATE_EPOCH": str(source_date_epoch),
        "TZ": "UTC",
    }
    try:
        with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
            completed = subprocess.run(
                [str(executable), "-n", "-r", probe],
                cwd=executable.parent,
                env=environment,
                check=False,
                stdout=stdout,
                stderr=stderr,
                timeout=10,
            )
            stdout_size = stdout.tell()
            stderr_size = stderr.tell()
            if stdout_size > 1024 * 1024 or stderr_size > 1024 * 1024:
                raise ContractError("pinned PHP runtime probe exceeded the output limit")
            stdout.seek(0)
            stderr.seek(0)
            stdout_bytes = stdout.read()
            stderr_bytes = stderr.read()
    except (OSError, subprocess.TimeoutExpired) as error:
        raise ContractError(f"cannot probe pinned PHP runtime: {error}") from error
    if completed.returncode != 0:
        stderr_text = stderr_bytes[:4096].decode("utf-8", errors="replace")
        raise ContractError(
            f"pinned PHP runtime probe exited {completed.returncode}: {stderr_text}"
        )
    if stderr_bytes:
        raise ContractError("pinned PHP runtime probe emitted stderr")
    try:
        value = json.loads(stdout_bytes)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ContractError(f"pinned PHP runtime probe returned invalid JSON: {error}") from error
    return _mapping(value, "pinned PHP runtime probe")


def _validate_runtime_probe(
    observed: Mapping[str, Any],
    *,
    expected_version: str,
    extensions: tuple[str, ...],
    zend_extensions: tuple[str, ...],
) -> None:
    _exact_fields(
        observed,
        {
            "version",
            "sapi",
            "loaded_ini",
            "scanned_ini",
            "extensions",
            "zend_extensions",
        },
        "pinned PHP runtime probe",
    )
    if observed["version"] != expected_version or observed["sapi"] != "cli":
        raise ContractError("pinned PHP runtime probe version or SAPI is invalid")
    if observed["loaded_ini"] is not None or observed["scanned_ini"] is not None:
        raise ContractError("pinned PHP runtime probe did not honor -n")
    if tuple(_list(observed["extensions"], "runtime probe.extensions")) != extensions:
        raise ContractError("pinned PHP runtime extensions changed after publication")
    if (
        tuple(
            _list(
                observed["zend_extensions"],
                "runtime probe.zend_extensions",
            )
        )
        != zend_extensions
    ):
        raise ContractError(
            "pinned PHP runtime Zend extensions changed after publication"
        )


def load_php_src_runtime_artifact(
    build_root: Path,
    profile: str,
    contract: OracleContract,
    elephc_source_commit: str,
) -> PhpSrcRuntimeArtifact:
    """Validate one published build set and return its executable provenance."""

    if profile not in SUPPORTED_PROFILES:
        raise ContractError(f"unsupported PHP profile: {profile!r}")
    if not isinstance(elephc_source_commit, str) or not _COMMIT_RE.fullmatch(
        elephc_source_commit
    ):
        raise ContractError(
            "expected Elephc source commit must be 40 lowercase hexadecimal characters"
        )
    supplied_root = Path(build_root)
    try:
        root_details = supplied_root.lstat()
    except OSError as error:
        raise ContractError(
            f"php-src build root is missing: {supplied_root}"
        ) from error
    if not stat.S_ISDIR(root_details.st_mode) or stat.S_ISLNK(root_details.st_mode):
        raise ContractError("php-src build root must be a real directory")
    root = supplied_root.resolve()

    root_provenance_path = _regular_file(
        root, "provenance.json", "root provenance"
    )
    raw_root_document, root_provenance_sha = _load_json_snapshot(
        root_provenance_path, "root provenance"
    )
    root_document = _mapping(raw_root_document, "root provenance")
    _exact_fields(
        root_document,
        {"schema", "repository", "selection", "inputs", "profiles"},
        "root provenance",
    )
    if root_document["schema"] != ROOT_SCHEMA:
        raise ContractError(f"root provenance schema must be {ROOT_SCHEMA!r}")
    if root_document["repository"] != PHP_SRC_REPOSITORY:
        raise ContractError("root provenance repository is not canonical php-src")
    selection = tuple(_list(root_document["selection"], "root selection"))
    if (
        not selection
        or any(item not in SUPPORTED_PROFILES for item in selection)
        or len(selection) != len(set(selection))
        or tuple(sorted(selection)) != selection
    ):
        raise ContractError("root provenance selection is invalid")
    if profile not in selection:
        raise ContractError(f"root provenance does not contain profile {profile}")

    inputs = _mapping(root_document["inputs"], "root provenance.inputs")
    _exact_fields(
        inputs,
        {
            "inventory",
            "wasm_specification",
            "builder_script",
            "build_environment",
            "elephc",
        },
        "root provenance.inputs",
    )
    inventory = _mapping(inputs["inventory"], "root input inventory")
    specification = _mapping(
        inputs["wasm_specification"], "root input wasm_specification"
    )
    builder = _mapping(inputs["builder_script"], "root input builder_script")
    build_environment_input = _mapping(
        inputs["build_environment"], "root input build_environment"
    )
    elephc = _mapping(inputs["elephc"], "root input elephc")
    _exact_fields(inventory, {"path", "sha256"}, "root input inventory")
    _exact_fields(
        specification,
        {"path", "sha256", "inventory_pin_sha256"},
        "root input wasm_specification",
    )
    _exact_fields(builder, {"path", "sha256"}, "root input builder_script")
    _exact_fields(
        build_environment_input,
        {"path", "sha256"},
        "root input build_environment",
    )
    _exact_fields(
        elephc,
        {"head", "dirty", "status", "status_sha256"},
        "root input elephc",
    )
    if inventory["sha256"] != contract.inventory_sha256:
        raise ContractError("php-src build used a different WASM inventory")
    if (
        specification["sha256"] != contract.specification_sha256
        or specification["inventory_pin_sha256"] != contract.specification_sha256
    ):
        raise ContractError("php-src build used a different WASM specification")
    builder_path = contract.repo_root / "scripts" / "build-pinned-php-src.sh"
    if builder["path"] != "scripts/build-pinned-php-src.sh":
        raise ContractError("root builder script path is invalid")
    if builder["sha256"] != sha256_file(builder_path):
        raise ContractError("php-src build used a different builder script")
    if (
        not isinstance(elephc["dirty"], bool)
        or elephc["dirty"]
        or elephc["head"] != elephc_source_commit
    ):
        raise ContractError("php-src build was not produced from the exact clean Elephc SHA")
    if elephc["status"] != "elephc-git-status.txt":
        raise ContractError("root Elephc status path is not canonical")
    status_path = _regular_file(root, elephc["status"], "root Elephc status")
    try:
        status_bytes = status_path.read_bytes()
    except OSError as error:
        raise ContractError(f"cannot read root Elephc status: {error}") from error
    if sha256_bytes(status_bytes) != elephc["status_sha256"]:
        raise ContractError("root Elephc status SHA-256 mismatch")
    if status_bytes:
        raise ContractError(
            "clean Elephc provenance requires an empty Git status"
        )

    if build_environment_input["path"] != "build-environment.json":
        raise ContractError("root build environment path is not canonical")
    build_environment_sha, expected_build_flags = _verify_build_environment(
        root, build_environment_input["path"]
    )
    if build_environment_sha != build_environment_input["sha256"]:
        raise ContractError("root build environment SHA-256 mismatch")

    profile_entries = _list(root_document["profiles"], "root profiles")
    if len(profile_entries) != len(selection):
        raise ContractError("root profile count does not match selection")
    indexed_profiles: dict[str, Mapping[str, Any]] = {}
    for index, raw_entry in enumerate(profile_entries):
        entry = _mapping(raw_entry, f"root profiles[{index}]")
        entry_profile = _string(entry.get("profile"), f"root profiles[{index}].profile")
        if entry_profile in indexed_profiles:
            raise ContractError(f"duplicate root profile entry {entry_profile!r}")
        indexed_profiles[entry_profile] = entry
    if tuple(indexed_profiles) != selection:
        raise ContractError("root profile order does not match selection")

    root_expected_paths = (
        *_ROOT_BASE_HASH_PATHS,
        *(
            relative
            for selected in selection
            for relative in (
                f"{selected}/install/bin/php",
                f"{selected}/install-tree.json",
                f"{selected}/php-metadata.json",
                f"{selected}/dynamic-dependencies.txt",
                f"{selected}/provenance.json",
                f"{selected}/hashes.sha256",
            )
        ),
    )
    root_hash_manifest_path = _regular_file(
        root, "hashes.sha256", "root hash manifest"
    )
    _, root_hash_manifest_sha = _read_hash_manifest(
        root,
        root_hash_manifest_path,
        root_expected_paths,
        "root hash manifest",
    )

    root_entry = indexed_profiles[profile]
    expected_root_entry_fields = {
        "profile",
        "tag",
        "tag_object",
        "tag_commit",
        "peeled_commit",
        "php_binary",
        "php_version",
        "php_sha256",
        "configure_command",
        "configure_args",
        "build_flags",
        "ini_mode",
        "ini",
        "extensions",
        "zend_extensions",
        "install_tree_sha256",
        "runtime_metadata_sha256",
        "dynamic_dependencies_sha256",
        "provenance",
        "hashes",
    }
    _exact_fields(root_entry, expected_root_entry_fields, f"root profile {profile}")

    profile_root = root / profile
    if (
        root_entry["provenance"] != f"{profile}/provenance.json"
        or root_entry["hashes"] != f"{profile}/hashes.sha256"
    ):
        raise ContractError(f"{profile} root evidence paths are not canonical")
    profile_document_path = _regular_file(
        root, root_entry["provenance"], f"{profile} provenance"
    )
    if profile_document_path.parent != profile_root:
        raise ContractError(f"{profile} provenance path is outside its profile")
    raw_profile_document, profile_provenance_sha = _load_json_snapshot(
        profile_document_path, f"{profile} provenance"
    )
    profile_document = _mapping(
        raw_profile_document, f"{profile} provenance"
    )
    _exact_fields(
        profile_document,
        {
            "schema",
            "profile",
            "repository",
            "inputs",
            "source",
            "build",
            "runtime",
            "artifact",
        },
        f"{profile} provenance",
    )
    if (
        profile_document["schema"] != PROFILE_SCHEMA
        or profile_document["profile"] != profile
        or profile_document["repository"] != PHP_SRC_REPOSITORY
    ):
        raise ContractError(f"{profile} provenance identity is invalid")

    profile_inputs = _mapping(
        profile_document["inputs"], f"{profile} provenance.inputs"
    )
    _exact_fields(
        profile_inputs,
        {
            "inventory_sha256",
            "wasm_specification_sha256",
            "builder_script_sha256",
            "build_environment",
            "build_environment_sha256",
        },
        f"{profile} provenance.inputs",
    )
    if (
        profile_inputs["inventory_sha256"] != contract.inventory_sha256
        or profile_inputs["wasm_specification_sha256"]
        != contract.specification_sha256
        or profile_inputs["builder_script_sha256"] != builder["sha256"]
        or profile_inputs["build_environment_sha256"] != build_environment_sha
        or profile_inputs["build_environment"] != "../build-environment.json"
    ):
        raise ContractError(f"{profile} provenance inputs do not match root inputs")

    pin = contract.php_src_pin(profile)
    source = _mapping(profile_document["source"], f"{profile} provenance.source")
    _exact_fields(
        source,
        {
            "tag",
            "inventory_tag_object",
            "inventory_tag_commit",
            "tag_object",
            "tag_commit",
            "peeled_commit",
            "head",
            "detached",
            "dirty",
            "materialization",
            "source_date_epoch",
        },
        f"{profile} provenance.source",
    )
    if (
        source["tag"] != pin.tag
        or source["inventory_tag_object"] != pin.tag_object
        or source["tag_object"] != pin.tag_object
        or source["inventory_tag_commit"] != pin.tag_commit
        or source["tag_commit"] != pin.tag_commit
        or source["peeled_commit"] != pin.tag_commit
        or source["head"] != pin.tag_commit
        or source["detached"] is not True
        or source["dirty"] is not False
        or source["materialization"] != "git archive of verified HEAD"
        or not isinstance(source["source_date_epoch"], int)
        or isinstance(source["source_date_epoch"], bool)
        or source["source_date_epoch"] < 0
    ):
        raise ContractError(f"{profile} php-src pin or checkout provenance is invalid")

    build = _mapping(profile_document["build"], f"{profile} provenance.build")
    _exact_fields(
        build,
        {
            "configure_args",
            "configure_command",
            "build_flags",
            "environment",
            "ini_mode",
            "tools",
        },
        f"{profile} provenance.build",
    )
    configure_args = _list(
        build["configure_args"], f"{profile} build.configure_args"
    )
    if (
        configure_args != list(_CONFIGURE_ARGS)
        or build["configure_command"] != ["configure", *configure_args]
        or build["environment"] != "../build-environment.json"
        or build["ini_mode"] != "-n"
    ):
        raise ContractError(f"{profile} build configuration is invalid")
    build_flags = _mapping(
        build["build_flags"], f"{profile} build.build_flags"
    )
    if dict(build_flags) != expected_build_flags:
        raise ContractError(
            f"{profile} build flags disagree with the build environment"
        )
    for name, value in build_flags.items():
        if value is not None and not isinstance(value, str):
            raise ContractError(f"{profile} build flag {name} is invalid")
    tools = _mapping(build["tools"], f"{profile} build.tools")
    if set(tools) != {"git", "autoconf", "bison", "re2c", "make", "cc"}:
        raise ContractError(f"{profile} build tool evidence is incomplete")
    for name, value in tools.items():
        _string(value, f"{profile} build.tools.{name}")

    runtime = _mapping(
        profile_document["runtime"], f"{profile} provenance.runtime"
    )
    _exact_fields(
        runtime,
        {"ini_mode", "ini", "extensions", "zend_extensions"},
        f"{profile} provenance.runtime",
    )
    if runtime["ini_mode"] != "-n":
        raise ContractError(f"{profile} runtime did not use -n")

    artifact = _mapping(
        profile_document["artifact"], f"{profile} provenance.artifact"
    )
    _exact_fields(
        artifact,
        {
            "php_binary",
            "php_version",
            "php_sha256",
            "runtime_metadata",
            "runtime_metadata_sha256",
            "install_tree",
            "install_tree_sha256",
            "dynamic_dependencies",
            "dynamic_dependencies_sha256",
        },
        f"{profile} provenance.artifact",
    )
    expected_artifact_paths = {
        "php_binary": "install/bin/php",
        "runtime_metadata": "php-metadata.json",
        "install_tree": "install-tree.json",
        "dynamic_dependencies": "dynamic-dependencies.txt",
    }
    for field, expected_path in expected_artifact_paths.items():
        if artifact[field] != expected_path:
            raise ContractError(
                f"{profile} artifact.{field} path must be {expected_path!r}"
            )
    executable = _regular_file(
        profile_root, artifact["php_binary"], f"{profile} PHP binary"
    )
    if not executable.stat().st_mode & 0o111:
        raise ContractError(f"{profile} PHP binary is not executable")
    executable_sha = sha256_file(executable)
    if executable_sha != artifact["php_sha256"]:
        raise ContractError(f"{profile} PHP binary SHA-256 mismatch")
    expected_version = pin.tag.removeprefix("php-")
    if artifact["php_version"] != expected_version:
        raise ContractError(f"{profile} PHP version does not match pinned tag")

    install_tree_sha = _verify_install_tree(profile_root, artifact["install_tree"])
    runtime_metadata_sha, extensions, zend_extensions, ini = (
        _verify_runtime_metadata(
            profile_root,
            artifact["runtime_metadata"],
            expected_version=expected_version,
            source_date_epoch=source["source_date_epoch"],
            configure_args=configure_args,
        )
    )
    dependencies_path = _regular_file(
        profile_root,
        artifact["dynamic_dependencies"],
        f"{profile} dynamic dependencies",
    )
    dynamic_dependencies_sha = _verify_dynamic_dependencies(dependencies_path)
    if (
        install_tree_sha != artifact["install_tree_sha256"]
        or runtime_metadata_sha != artifact["runtime_metadata_sha256"]
        or dynamic_dependencies_sha != artifact["dynamic_dependencies_sha256"]
    ):
        raise ContractError(f"{profile} artifact evidence SHA-256 mismatch")
    if (
        runtime["ini"] != ini
        or tuple(runtime["extensions"]) != extensions
        or tuple(runtime["zend_extensions"]) != zend_extensions
    ):
        raise ContractError(f"{profile} runtime metadata disagrees with provenance")

    expected_root_values = {
        "profile": profile,
        "tag": pin.tag,
        "tag_object": pin.tag_object,
        "tag_commit": pin.tag_commit,
        "peeled_commit": pin.tag_commit,
        "php_binary": f"{profile}/{artifact['php_binary']}",
        "php_version": expected_version,
        "php_sha256": executable_sha,
        "configure_command": build["configure_command"],
        "configure_args": configure_args,
        "build_flags": dict(build_flags),
        "ini_mode": "-n",
        "ini": dict(ini),
        "extensions": list(extensions),
        "zend_extensions": list(zend_extensions),
        "install_tree_sha256": install_tree_sha,
        "runtime_metadata_sha256": runtime_metadata_sha,
        "dynamic_dependencies_sha256": dynamic_dependencies_sha,
        "provenance": f"{profile}/provenance.json",
        "hashes": f"{profile}/hashes.sha256",
    }
    if dict(root_entry) != expected_root_values:
        raise ContractError(f"root and profile provenance disagree for {profile}")

    profile_hash_manifest_path = _regular_file(
        root, root_entry["hashes"], f"{profile} hash manifest"
    )
    profile_hash_manifest_sha = sha256_file(profile_hash_manifest_path)
    _read_hash_manifest(
        profile_root,
        profile_hash_manifest_path,
        _PROFILE_HASH_PATHS,
        f"{profile} hash manifest",
    )
    observed = _probe_php_runtime(executable, source["source_date_epoch"])
    _validate_runtime_probe(
        observed,
        expected_version=expected_version,
        extensions=extensions,
        zend_extensions=zend_extensions,
    )
    stable_hashes = (
        (root_provenance_path, root_provenance_sha),
        (profile_document_path, profile_provenance_sha),
        (root_hash_manifest_path, root_hash_manifest_sha),
        (profile_hash_manifest_path, profile_hash_manifest_sha),
        (status_path, elephc["status_sha256"]),
        (executable, executable_sha),
        (dependencies_path, dynamic_dependencies_sha),
    )
    for path, expected_sha in stable_hashes:
        if sha256_file(path) != expected_sha:
            raise ContractError(f"php-src build artifact changed while loading: {path}")
    if (
        _verify_build_environment(root, build_environment_input["path"])[0]
        != build_environment_sha
        or _verify_install_tree(profile_root, artifact["install_tree"])
        != install_tree_sha
        or _verify_runtime_metadata(
            profile_root,
            artifact["runtime_metadata"],
            expected_version=expected_version,
            source_date_epoch=source["source_date_epoch"],
            configure_args=configure_args,
        )[0]
        != runtime_metadata_sha
    ):
        raise ContractError("php-src build evidence changed while loading")

    build_configuration = {
        "configure_command": _canonical_json(build["configure_command"]),
        "build_flags": _canonical_json(dict(build_flags)),
        "build_tools": _canonical_json(dict(tools)),
        "build_environment_sha256": build_environment_sha,
        "install_tree_sha256": install_tree_sha,
        "runtime_metadata_sha256": runtime_metadata_sha,
        "dynamic_dependencies_sha256": dynamic_dependencies_sha,
        "root_provenance_sha256": root_provenance_sha,
        "profile_provenance_sha256": profile_provenance_sha,
        "zend_extensions": _canonical_json(list(zend_extensions)),
    }
    provenance = RuntimeProvenance.create(
        executable_sha256=executable_sha,
        version=expected_version,
        source_commit=pin.tag_commit,
        build_configuration=build_configuration,
        ini_mode="php-n",
        ini_sha256=None,
        extensions=extensions,
    )
    provenance.validate_for(
        # Only runtime/profile are relevant to validation; fixture identity is inert.
        RunKey(
            fixture_id="php-src-artifact-validation",
            profile=profile,
            runtime="php-src",
            host="php-src",
        ),
        contract,
    )
    return PhpSrcRuntimeArtifact(
        profile=profile,
        executable=executable,
        provenance=provenance,
        root_provenance_sha256=root_provenance_sha,
        profile_provenance_sha256=profile_provenance_sha,
        build_environment_sha256=build_environment_sha,
        install_tree_sha256=install_tree_sha,
        runtime_metadata_sha256=runtime_metadata_sha,
        dynamic_dependencies_sha256=dynamic_dependencies_sha,
    )
