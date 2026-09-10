#!/usr/bin/env python3
"""Run pinned PHP DOM-family PHPTs against PHP 8.5.8 and Elephc."""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Sequence


COMPONENTS = ("dom", "libxml", "simplexml")
SOURCE_SECTIONS = ("FILE", "FILEEOF", "FILE_EXTERNAL")
EXPECTATION_SECTIONS = ("EXPECT", "EXPECTF", "EXPECTREGEX")
EXTERNAL_SECTIONS = {
    "FILE_EXTERNAL": "FILE",
    "EXPECT_EXTERNAL": "EXPECT",
    "EXPECTF_EXTERNAL": "EXPECTF",
    "EXPECTREGEX_EXTERNAL": "EXPECTREGEX",
}
SUPPORTED_SECTIONS = {
    "TEST",
    "DESCRIPTION",
    "CREDITS",
    "FILE",
    "FILEEOF",
    "FILE_EXTERNAL",
    "EXPECT",
    "EXPECTF",
    "EXPECTREGEX",
    "EXPECT_EXTERNAL",
    "EXPECTF_EXTERNAL",
    "EXPECTREGEX_EXTERNAL",
    "SKIPIF",
    "CLEAN",
    "INI",
    "ENV",
    "ARGS",
    "EXTENSIONS",
    "XFAIL",
}
UPSTREAM_SECTIONS = SUPPORTED_SECTIONS | {
    "EXPECTHEADERS",
    "POST",
    "POST_RAW",
    "GZIP_POST",
    "DEFLATE_POST",
    "PUT",
    "GET",
    "COOKIE",
    "FILE_EXTERNAL",
    "REDIRECTTEST",
    "CAPTURE_STDIO",
    "STDIN",
    "CGI",
    "PHPDBG",
    "XLEAK",
    "CONFLICTS",
    "WHITESPACE_SENSITIVE",
    "FLAKY",
}
SECTION_RE = re.compile(br"^--([_A-Z]+)--")
ENV_PLACEHOLDER_RE = re.compile(r"\{ENV:([^}\s]+)\}")
PASSING_STATUSES = {"passed", "oracle_skip"}

# Dependencies outside a component's own test directory must be explicit. This
# keeps the sandboxes small while preserving the relative paths used by PHPTs.
KNOWN_EXTERNAL_FIXTURES: dict[str, tuple[str, ...]] = {
    "ext/simplexml/tests/gh17153.phpt": (
        "ext/xsl/tests/53965/collection.xml",
        "ext/xsl/tests/53965/collection.xsl",
    ),
}
ELEPHC_NATIVE_PROJECT_FIXTURE = Path("examples/hello-preg")


class HarnessError(Exception):
    """Indicates invalid input or a harness condition that cannot be inferred."""


@dataclass(frozen=True)
class PhptCase:
    """One parsed PHPT and its validated byte-preserving sections."""

    path: Path
    sections: dict[str, bytes]

    @property
    def title(self) -> str:
        """Return the human-readable TEST section."""
        return self.sections["TEST"].decode("utf-8", "replace").strip()

    @property
    def source(self) -> bytes:
        """Return the executable FILE payload after FILEEOF normalization."""
        return self.sections["FILE"]

    @property
    def expectation_mode(self) -> str:
        """Return the case's single expectation section name."""
        return next(name for name in EXPECTATION_SECTIONS if name in self.sections)


@dataclass(frozen=True)
class Execution:
    """Raw merged-output result of one compiler or program invocation."""

    command: tuple[str, ...]
    returncode: int | None
    output: bytes
    timed_out: bool
    duration_seconds: float


@dataclass(frozen=True)
class Sandbox:
    """Paths belonging to one isolated oracle or Elephc PHPT copy."""

    root: Path
    phpt_path: Path
    source_path: Path
    skipif_path: Path | None
    clean_path: Path | None


def sha256_bytes(payload: bytes) -> str:
    """Return the hexadecimal SHA-256 digest of bytes."""
    return hashlib.sha256(payload).hexdigest()


def sha256_file(path: Path) -> str:
    """Return the hexadecimal SHA-256 digest of a file."""
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def normalize_output(output: bytes) -> bytes:
    """Mirror PHP 8.5.8 run-tests.php CRLF normalization and trim set."""
    return output.replace(b"\r\n", b"\n").strip(b" \t\n\r\x00\x0b")


def parse_phpt(path: Path) -> PhptCase:
    """Parse and validate the focused CLI subset of a PHPT without decoding it."""
    payload = path.read_bytes()
    lines = payload.splitlines(keepends=True)
    if not lines or not lines[0].startswith(b"--TEST--"):
        raise HarnessError("tests must start with --TEST--")

    sections: dict[str, bytearray] = {"TEST": bytearray()}
    current = "TEST"
    file_done = False
    for line in lines[1:]:
        marker = SECTION_RE.match(line)
        if marker is not None:
            name = marker.group(1).decode("ascii")
            if name not in UPSTREAM_SECTIONS:
                raise HarnessError(f'unknown section "{name}"')
            if name in sections and sections[name]:
                raise HarnessError(f"duplicated {name} section")
            sections.setdefault(name, bytearray())
            current = name
            file_done = False
            continue

        if not file_done:
            sections[current].extend(line)
        if current in SOURCE_SECTIONS and re.match(br"^===DONE===\s*$", line):
            # PHP includes this sentinel in FILE (typically after
            # __halt_compiler()) and ignores everything following it.
            file_done = True

    unsupported = sorted(set(sections) - SUPPORTED_SECTIONS)
    if unsupported:
        raise HarnessError(
            "unsupported PHPT section(s): " + ", ".join(unsupported)
        )

    source_count = sum(name in sections for name in SOURCE_SECTIONS)
    if source_count != 1:
        raise HarnessError(
            "expected exactly one --FILE--, --FILEEOF--, or --FILE_EXTERNAL-- section"
        )
    if "FILEEOF" in sections:
        sections["FILE"] = bytearray(bytes(sections.pop("FILEEOF")).rstrip(b"\r\n"))

    expectation_count = sum(
        name in sections
        for name in (*EXPECTATION_SECTIONS, *(
            name for name in EXTERNAL_SECTIONS if name != "FILE_EXTERNAL"
        ))
    )
    if expectation_count != 1:
        raise HarnessError(
            "expected exactly one --EXPECT--, --EXPECTF--, --EXPECTREGEX--, "
            "or corresponding _EXTERNAL section"
        )

    for external_name, resolved_name in EXTERNAL_SECTIONS.items():
        if external_name not in sections:
            continue
        external_text = bytes(sections[external_name]).decode(
            "utf-8", "surrogateescape"
        ).strip()
        external_path = Path(external_text)
        if (
            not external_text
            or external_path.is_absolute()
            or ".." in external_path.parts
        ):
            raise HarnessError(f"unsafe {external_name} path: {external_text}")
        resolved_path = path.parent / external_path
        try:
            sections[resolved_name] = bytearray(resolved_path.read_bytes())
        except OSError as error:
            raise HarnessError(
                f"could not load --{external_name}-- {resolved_path}"
            ) from error
    if not sections["TEST"]:
        raise HarnessError("empty --TEST-- section")

    return PhptCase(path=path, sections={key: bytes(value) for key, value in sections.items()})


def parse_ini(
    section: bytes | None,
    test_directory: Path,
    temporary_directory: Path,
    host_environment: dict[str, str],
) -> list[tuple[str, str]]:
    """Parse INI assignments with php-src's PWD, TMP, and ENV substitutions."""
    if not section:
        return []
    text = section.decode("utf-8", "surrogateescape")
    text = text.replace("{PWD}", str(test_directory))
    text = text.replace("{TMP}", str(temporary_directory))

    def replace_environment(match: re.Match[str]) -> str:
        """Resolve one mandatory host environment placeholder."""
        name = match.group(1)
        if name not in host_environment:
            raise HarnessError(f"environment variable {name} is not set")
        return host_environment[name]

    text = ENV_PLACEHOLDER_RE.sub(replace_environment, text)
    settings: list[tuple[str, str]] = []
    for line in re.split(r"[\r\n]+", text):
        if "=" not in line:
            continue
        name, value = line.split("=", 1)
        name = name.strip()
        if name:
            settings.append((name, value.strip()))
    return settings


def parse_environment(
    section: bytes | None,
    test_directory: Path,
    base_environment: dict[str, str],
) -> dict[str, str]:
    """Apply a PHPT ENV section to a copy of the process environment."""
    environment = dict(base_environment)
    if not section:
        return environment
    text = section.decode("utf-8", "surrogateescape").replace(
        "{PWD}", str(test_directory)
    )
    for line in text.splitlines():
        line = line.strip()
        if not line or "=" not in line:
            continue
        name, value = line.split("=", 1)
        if name:
            environment[name] = value
    return environment


def isolated_environment(
    section: bytes | None,
    test_directory: Path,
    temporary_directory: Path,
    base_environment: dict[str, str],
) -> dict[str, str]:
    """Build one PHPT environment with a runtime-private temporary directory."""
    temporary_directory.mkdir(parents=True, exist_ok=True)
    environment = dict(base_environment)
    for name in ("TMPDIR", "TMP", "TEMP"):
        environment[name] = str(temporary_directory)
    return parse_environment(section, test_directory, environment)


def compiler_environment(
    runtime_environment: dict[str, str],
    base_environment: dict[str, str],
) -> dict[str, str]:
    """Keep native-toolchain fingerprints stable while preserving other PHPT variables."""
    environment = dict(runtime_environment)
    for name in ("PATH", "TMPDIR", "TMP", "TEMP", "SYSTEMROOT"):
        if name in base_environment:
            environment[name] = base_environment[name]
        else:
            environment.pop(name, None)
    return environment


def validate_args(section: bytes | None) -> str:
    """Return raw POSIX-shell ARGS after rejecting malformed quoting or NULs."""
    if not section:
        return ""
    text = section.decode("utf-8", "surrogateescape").strip()
    if "\x00" in text:
        raise HarnessError("ARGS contains a NUL byte")
    try:
        shlex.split(text, posix=True)
    except ValueError as error:
        raise HarnessError(f"invalid ARGS quoting: {error}") from error
    return text


def required_extensions(section: bytes | None) -> list[str]:
    """Return extension names from an EXTENSIONS section in source order."""
    if not section:
        return []
    return [
        line.strip()
        for line in section.decode("ascii", "strict").splitlines()
        if line.strip()
    ]


def classify_skipif(output: bytes) -> tuple[str, str]:
    """Classify SKIPIF output according to PHP 8.5.8 run-tests.php."""
    text = normalize_output(output).decode("utf-8", "replace")
    lowered = text.lower()
    if lowered.startswith("skip"):
        return "skip", text[4:].strip()
    if lowered.startswith("info"):
        return "run", text[4:].strip()
    if lowered.startswith("warn") and len(text) > 4 and text[4].isspace():
        return "run", text[4:].strip()
    if lowered.startswith("xfail"):
        return "xfail", text[5:].strip()
    if lowered.startswith("xleak"):
        return "xleak", text[5:].strip()
    if lowered.startswith("flaky"):
        return "flaky", text[5:].strip()
    if text:
        return "invalid", text
    return "run", ""


def run_process(
    command: Sequence[str],
    *,
    cwd: Path,
    environment: dict[str, str],
    timeout_seconds: float,
    stdin: bytes | None = None,
) -> Execution:
    """Run a process with merged raw output and a hard timeout."""
    started = time.monotonic()
    try:
        completed = subprocess.run(
            list(command),
            cwd=cwd,
            env=environment,
            input=stdin,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
            timeout=timeout_seconds,
        )
        return Execution(
            command=tuple(str(part) for part in command),
            returncode=completed.returncode,
            output=completed.stdout,
            timed_out=False,
            duration_seconds=time.monotonic() - started,
        )
    except subprocess.TimeoutExpired as error:
        partial = error.output if isinstance(error.output, bytes) else b""
        return Execution(
            command=tuple(str(part) for part in command),
            returncode=None,
            output=partial,
            timed_out=True,
            duration_seconds=time.monotonic() - started,
        )


def shell_execution_command(command: Sequence[str], raw_args: str) -> list[str]:
    """Build the POSIX-shell command used by php-src to append raw ARGS."""
    shell_command = "exec " + shlex.join([str(part) for part in command])
    if raw_args:
        shell_command += " " + raw_args
    return ["/bin/sh", "-c", shell_command]


def oracle_base_command(oracle: Path, oracle_arguments: Sequence[str]) -> list[str]:
    """Build a deterministic no-php.ini oracle command prefix."""
    return [str(oracle), "-n", *oracle_arguments]


def ini_arguments(settings: Sequence[tuple[str, str]]) -> list[str]:
    """Convert parsed settings to repeated PHP/Elephc -d/--ini arguments later."""
    arguments: list[str] = []
    for name, value in settings:
        arguments.extend(["-d", f"{name}={value}"])
    return arguments


def execute_oracle_program(
    oracle: Path,
    oracle_arguments: Sequence[str],
    source_path: Path,
    work_directory: Path,
    settings: Sequence[tuple[str, str]],
    raw_args: str,
    environment: dict[str, str],
    timeout_seconds: float,
) -> Execution:
    """Execute one extracted section from php-src's source-root working directory."""
    command = [
        *oracle_base_command(oracle, oracle_arguments),
        *ini_arguments(settings),
        "-f",
        str(source_path),
    ]
    if raw_args:
        command.append("--")
    return run_process(
        shell_execution_command(command, raw_args),
        cwd=work_directory,
        environment=environment,
        timeout_seconds=timeout_seconds,
    )


def compile_elephc_program(
    elephc: Path,
    elephc_arguments: Sequence[str],
    source_path: Path,
    settings: Sequence[tuple[str, str]],
    target: str | None,
    repo_root: Path,
    environment: dict[str, str],
    timeout_seconds: float,
) -> Execution:
    """Compile one extracted PHP section through the DOM bridge."""
    command = [
        str(elephc),
        "--strict-php",
        "--php-version=8.5",
        "--with-dom",
        "--quiet",
        *elephc_arguments,
    ]
    if target:
        command.extend(["--target", target])
    for name, value in settings:
        command.extend(["--ini", f"{name}={value}"])
    command.append(str(source_path))
    return run_process(
        command,
        cwd=repo_root,
        environment=environment,
        timeout_seconds=timeout_seconds,
    )


def execute_elephc_program(
    binary_path: Path,
    work_directory: Path,
    execution_prefix: Sequence[str],
    raw_args: str,
    environment: dict[str, str],
    timeout_seconds: float,
) -> Execution:
    """Execute a linked binary from php-src's source-root working directory."""
    command = [*execution_prefix, str(binary_path)]
    return run_process(
        shell_execution_command(command, raw_args),
        cwd=work_directory,
        environment=environment,
        timeout_seconds=timeout_seconds,
    )


def execution_record(execution: Execution | None) -> dict[str, Any] | None:
    """Serialize a raw execution without lossy UTF-8 decoding."""
    if execution is None:
        return None
    normalized = normalize_output(execution.output)
    return {
        "command": list(execution.command),
        "returncode": execution.returncode,
        "timed_out": execution.timed_out,
        "duration_seconds": round(execution.duration_seconds, 6),
        "output_sha256": sha256_bytes(execution.output),
        "output_base64": base64.b64encode(execution.output).decode("ascii"),
        "normalized_output_sha256": sha256_bytes(normalized),
        "normalized_output_base64": base64.b64encode(normalized).decode("ascii"),
    }


def git_query(source_root: Path, *arguments: str) -> str:
    """Run a read-only Git provenance query in php-src."""
    try:
        completed = subprocess.run(
            ["git", *arguments],
            cwd=source_root,
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except subprocess.CalledProcessError as error:
        raise HarnessError(
            f"php-src Git query failed: {' '.join(arguments)}: {error.stderr.strip()}"
        ) from error
    return completed.stdout.strip()


def validate_source_root(source_root: Path, lock: dict[str, Any]) -> None:
    """Require the exact pinned php-src commit and locked extension trees."""
    if not (source_root / ".git").exists():
        raise HarnessError("--php-src must be a Git checkout, not an unverified export")
    actual_commit = git_query(source_root, "rev-parse", "HEAD")
    expected_commit = lock["php"]["commit"]
    if actual_commit != expected_commit:
        raise HarnessError(
            f"php-src commit mismatch: expected {expected_commit}, got {actual_commit}"
        )
    for relative_path, expected_tree in lock["php"]["trees"].items():
        actual_tree = git_query(
            source_root, "rev-parse", f"HEAD^{{tree}}:{relative_path}"
        )
        if actual_tree != expected_tree:
            raise HarnessError(
                f"php-src tree mismatch for {relative_path}: "
                f"expected {expected_tree}, got {actual_tree}"
            )


def validate_ledger(
    source_root: Path,
    ledger_path: Path,
    component_lock: dict[str, Any],
) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    """Validate ledger metadata and every PHPT hash before selecting tests."""
    ledger = json.loads(ledger_path.read_text())
    entries = ledger.get("entries")
    if (
        ledger.get("schema") != 1
        or not isinstance(entries, list)
        or ledger.get("phpt_count") != component_lock["phpt_count"]
        or len(entries) != component_lock["phpt_count"]
        or ledger.get("all_file_count") != component_lock["all_file_count"]
        or ledger.get("sorted_phpt_digest")
        != component_lock["sorted_phpt_digest"]
    ):
        raise HarnessError(f"invalid or stale ledger metadata: {ledger_path}")

    records: list[str] = []
    paths: list[str] = []
    for entry in entries:
        relative = entry.get("path")
        expected_digest = entry.get("sha256")
        if not isinstance(relative, str) or not isinstance(expected_digest, str):
            raise HarnessError(f"invalid ledger entry in {ledger_path}")
        source_path = source_root / relative
        if not source_path.is_file():
            raise HarnessError(f"ledger source is missing: {relative}")
        actual_digest = sha256_file(source_path)
        if actual_digest != expected_digest:
            raise HarnessError(
                f"PHPT digest mismatch for {relative}: "
                f"expected {expected_digest}, got {actual_digest}"
            )
        paths.append(relative)
        records.append(f"{actual_digest}  {relative}\n")

    if paths != sorted(paths) or len(paths) != len(set(paths)):
        raise HarnessError("ledger PHPT paths are duplicated or unsorted")
    aggregate = sha256_bytes("".join(records).encode())
    if aggregate != component_lock["sorted_phpt_digest"]:
        raise HarnessError(
            f"ledger aggregate digest mismatch: expected "
            f"{component_lock['sorted_phpt_digest']}, got {aggregate}"
        )

    component_root = source_root / ledger["source_root"]
    fixture_changes = git_query(
        source_root,
        "status",
        "--porcelain=v1",
        "--untracked-files=all",
        "--",
        ledger["source_root"],
    )
    if fixture_changes:
        raise HarnessError(
            "component fixtures differ from the pinned commit; first change: "
            + fixture_changes.splitlines()[0]
        )
    actual_file_count = sum(1 for path in component_root.rglob("*") if path.is_file())
    if actual_file_count != component_lock["all_file_count"]:
        raise HarnessError(
            f"fixture inventory mismatch for {component_root}: "
            f"expected {component_lock['all_file_count']}, got {actual_file_count}"
        )
    return ledger, entries


def validate_oracle(
    oracle: Path,
    oracle_arguments: Sequence[str],
    lock: dict[str, Any],
    environment: dict[str, str],
    timeout_seconds: float,
) -> dict[str, Any]:
    """Verify PHP, libxml, and base extension identity before any PHPT runs."""
    script = (
        "$v=["
        "'php_version'=>PHP_VERSION,"
        "'libxml_dotted_version'=>defined('LIBXML_DOTTED_VERSION')?LIBXML_DOTTED_VERSION:null,"
        "'libxml_version'=>defined('LIBXML_VERSION')?LIBXML_VERSION:null,"
        "'extensions'=>array_values(array_filter(['dom','libxml','simplexml'],"
        "fn($e)=>extension_loaded($e)))];"
        "echo json_encode($v, JSON_UNESCAPED_SLASHES);"
    )
    execution = run_process(
        [*oracle_base_command(oracle, oracle_arguments), "-r", script],
        cwd=oracle.parent,
        environment=environment,
        timeout_seconds=timeout_seconds,
    )
    if execution.timed_out or execution.returncode != 0:
        raise HarnessError(
            "PHP oracle preflight failed: "
            + normalize_output(execution.output).decode("utf-8", "replace")
        )
    try:
        metadata = json.loads(execution.output)
    except json.JSONDecodeError as error:
        raise HarnessError("PHP oracle preflight did not return clean JSON") from error

    expected = {
        "php_version": lock["php"]["version"],
        "libxml_dotted_version": lock["libxml2"]["version"],
        "libxml_version": lock["libxml2"]["version_number"],
        "extensions": ["dom", "libxml", "simplexml"],
    }
    if metadata != expected:
        raise HarnessError(
            "PHP oracle identity mismatch: "
            f"expected {json.dumps(expected, sort_keys=True)}, "
            f"got {json.dumps(metadata, sort_keys=True)}"
        )
    return metadata


def oracle_extension_probe(
    oracle: Path,
    oracle_arguments: Sequence[str],
    extensions: Sequence[str],
    settings: Sequence[tuple[str, str]],
    environment: dict[str, str],
    timeout_seconds: float,
) -> tuple[dict[str, Any], Execution | None]:
    """Return loaded extensions and shared-module discovery data from the oracle."""
    if not extensions:
        return {"missing": [], "extension_dir": None, "suffix": None}, None
    encoded = json.dumps(list(extensions))
    script = (
        f"$e=json_decode({json.dumps(encoded)},true);"
        "$m=array_values(array_filter($e,fn($n)=>!extension_loaded($n)));"
        "echo json_encode(['missing'=>$m,'extension_dir'=>ini_get('extension_dir'),"
        "'suffix'=>PHP_SHLIB_SUFFIX],JSON_UNESCAPED_SLASHES);"
    )
    execution = run_process(
        [
            *oracle_base_command(oracle, oracle_arguments),
            *ini_arguments(settings),
            "-r",
            script,
        ],
        cwd=oracle.parent,
        environment=environment,
        timeout_seconds=timeout_seconds,
    )
    if execution.timed_out or execution.returncode != 0:
        raise HarnessError("required-extension oracle probe failed")
    try:
        metadata = json.loads(execution.output)
    except json.JSONDecodeError as error:
        raise HarnessError("required-extension probe did not return clean JSON") from error
    missing = metadata.get("missing") if isinstance(metadata, dict) else None
    if not isinstance(missing, list) or not all(isinstance(name, str) for name in missing):
        raise HarnessError("required-extension probe returned invalid JSON")
    return metadata, execution


def resolve_oracle_extensions(
    oracle: Path,
    oracle_arguments: Sequence[str],
    extensions: Sequence[str],
    environment: dict[str, str],
    timeout_seconds: float,
) -> tuple[list[tuple[str, str]], list[str], list[Execution]]:
    """Auto-load readable EXTENSIONS modules exactly like php-src's CLI runner."""
    initial, initial_execution = oracle_extension_probe(
        oracle,
        oracle_arguments,
        extensions,
        [],
        environment,
        timeout_seconds,
    )
    executions = [initial_execution] if initial_execution is not None else []
    missing = initial["missing"]
    if not missing:
        return [], [], executions

    extension_dir = initial.get("extension_dir")
    suffix = initial.get("suffix")
    if not isinstance(extension_dir, str) or not isinstance(suffix, str):
        return [], missing, executions

    load_settings: list[tuple[str, str]] = []
    unavailable: list[str] = []
    prefix = "php_" if os.name == "nt" else ""
    for name in missing:
        module_path = Path(extension_dir) / f"{prefix}{name}.{suffix}"
        if not module_path.is_file() or not os.access(module_path, os.R_OK):
            unavailable.append(name)
            continue
        directive = "zend_extension" if name in {"opcache", "xdebug"} else "extension"
        load_settings.append((directive, str(module_path)))

    if unavailable:
        return load_settings, unavailable, executions
    verified, verified_execution = oracle_extension_probe(
        oracle,
        oracle_arguments,
        extensions,
        load_settings,
        environment,
        timeout_seconds,
    )
    if verified_execution is not None:
        executions.append(verified_execution)
    return load_settings, verified["missing"], executions


def copy_fixture(source_root: Path, destination_root: Path, relative: str) -> None:
    """Copy one explicitly authorized fixture while preserving its php-src path."""
    relative_path = Path(relative)
    if relative_path.is_absolute() or ".." in relative_path.parts:
        raise HarnessError(f"unsafe external fixture path: {relative}")
    fixture_changes = git_query(
        source_root,
        "status",
        "--porcelain=v1",
        "--untracked-files=all",
        "--",
        relative,
    )
    if fixture_changes:
        raise HarnessError(
            f"external fixture differs from the pinned commit: {relative}"
        )
    source = source_root / relative_path
    destination = destination_root / relative_path
    if source.is_dir():
        shutil.copytree(source, destination, dirs_exist_ok=True, symlinks=True)
    elif source.is_file():
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination, follow_symlinks=False)
    else:
        raise HarnessError(f"external fixture is missing: {relative}")


def stage_sandbox(
    source_root: Path,
    component_root_relative: str,
    entry_path: str,
    case: PhptCase,
    destination_root: Path,
    extra_fixtures: Sequence[str],
) -> Sandbox:
    """Create a complete component sandbox and extracted executable sections."""
    component_source = source_root / component_root_relative
    component_destination = destination_root / component_root_relative
    shutil.copytree(
        component_source,
        component_destination,
        dirs_exist_ok=True,
        symlinks=True,
    )
    fixtures = [*KNOWN_EXTERNAL_FIXTURES.get(entry_path, ()), *extra_fixtures]
    for fixture in fixtures:
        copy_fixture(source_root, destination_root, fixture)

    phpt_path = destination_root / entry_path
    source_path = phpt_path.with_suffix(".php")
    source_path.write_bytes(case.source)
    skipif_path = None
    if "SKIPIF" in case.sections:
        skipif_path = phpt_path.with_name(phpt_path.stem + ".skip.php")
        skipif_path.write_bytes(case.sections["SKIPIF"])
    clean_path = None
    if "CLEAN" in case.sections:
        clean_path = phpt_path.with_name(phpt_path.stem + ".clean.php")
        clean_path.write_bytes(case.sections["CLEAN"].strip(b" \t\n\r\x00\x0b"))
    return Sandbox(
        root=destination_root,
        phpt_path=phpt_path,
        source_path=source_path,
        skipif_path=skipif_path,
        clean_path=clean_path,
    )


def stage_elephc_native_project(repo_root: Path, destination_root: Path) -> None:
    """Stage the pinned managed-PCRE2 project required by regex/eval PHPTs."""
    fixture_root = repo_root / ELEPHC_NATIVE_PROJECT_FIXTURE
    for name in ("elephc.toml", "elephc.lock"):
        source = fixture_root / name
        if not source.is_file():
            raise HarnessError(f"Elephc native project fixture is missing: {source}")
        shutil.copy2(source, destination_root / name)


def snapshot_tree(root: Path) -> dict[str, dict[str, str]]:
    """Snapshot file contents and symlink targets for side-effect comparison."""
    snapshot: dict[str, dict[str, str]] = {}
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root).as_posix()
        if path.is_symlink():
            snapshot[relative] = {"kind": "symlink", "target": os.readlink(path)}
        elif path.is_file():
            snapshot[relative] = {"kind": "file", "sha256": sha256_file(path)}
    return snapshot


def tree_delta(
    before: dict[str, dict[str, str]],
    after: dict[str, dict[str, str]],
) -> list[dict[str, Any]]:
    """Describe added, removed, and modified sandbox paths deterministically."""
    delta: list[dict[str, Any]] = []
    for path in sorted(set(before) | set(after)):
        old = before.get(path)
        new = after.get(path)
        if old == new:
            continue
        change = "added" if old is None else "removed" if new is None else "modified"
        delta.append({"path": path, "change": change, "before": old, "after": new})
    return delta


def match_output(
    oracle: Path,
    oracle_arguments: Sequence[str],
    matcher_path: Path,
    mode: str,
    expected: bytes,
    actual: bytes,
    work_directory: Path,
    environment: dict[str, str],
    timeout_seconds: float,
) -> tuple[bool, Execution]:
    """Use the pinned oracle's PCRE engine for all PHPT output matching."""
    expected_path = work_directory / f"match-{mode.lower()}.expected"
    actual_path = work_directory / f"match-{mode.lower()}.actual"
    expected_path.write_bytes(expected)
    actual_path.write_bytes(actual)
    execution = run_process(
        [
            *oracle_base_command(oracle, oracle_arguments),
            str(matcher_path),
            mode,
            str(expected_path),
            str(actual_path),
        ],
        cwd=work_directory,
        environment=environment,
        timeout_seconds=timeout_seconds,
    )
    if execution.timed_out or execution.returncode not in (0, 1):
        message = normalize_output(execution.output).decode("utf-8", "replace")
        raise HarnessError(f"{mode} matcher failed: {message}")
    return execution.returncode == 0, execution


def compile_sections(
    sandbox: Sandbox,
    elephc: Path,
    elephc_arguments: Sequence[str],
    settings: Sequence[tuple[str, str]],
    target: str | None,
    repo_root: Path,
    environment: dict[str, str],
    timeout_seconds: float,
) -> dict[str, Execution]:
    """Compile FILE and optional SKIPIF/CLEAN payloads before observable execution."""
    sources = {"file": sandbox.source_path}
    if sandbox.skipif_path is not None:
        sources["skipif"] = sandbox.skipif_path
    if sandbox.clean_path is not None:
        sources["clean"] = sandbox.clean_path
    return {
        name: compile_elephc_program(
            elephc,
            elephc_arguments,
            source,
            settings,
            target,
            repo_root,
            environment,
            timeout_seconds,
        )
        for name, source in sources.items()
    }


def compiled_binary(source_path: Path) -> Path:
    """Return Elephc's executable output path for one PHP source."""
    return source_path.with_suffix("")


def successful_compilation(execution: Execution, binary: Path) -> bool:
    """Require a zero compiler exit and the expected linked executable."""
    return not execution.timed_out and execution.returncode == 0 and binary.is_file()


def clean_is_silent(execution: Execution | None) -> bool:
    """Require CLEAN to terminate successfully without normalized output."""
    return execution is None or (
        not execution.timed_out
        and execution.returncode == 0
        and normalize_output(execution.output) == b""
    )


def result_skeleton(entry_path: str, case: PhptCase | None) -> dict[str, Any]:
    """Create a stable result record before execution details are known."""
    return {
        "path": entry_path,
        "title": case.title if case else None,
        "status": "borked",
        "reason": None,
        "sections": sorted(case.sections) if case else [],
        "workspace": None,
        "oracle": {},
        "elephc": {},
        "oracle_file_delta": [],
        "elephc_file_delta": [],
        "oracle_post_clean_delta": [],
        "elephc_post_clean_delta": [],
    }


def run_case(
    *,
    entry_path: str,
    source_root: Path,
    component_root_relative: str,
    oracle: Path,
    oracle_arguments: Sequence[str],
    elephc: Path,
    elephc_arguments: Sequence[str],
    execution_prefix: Sequence[str],
    target: str | None,
    repo_root: Path,
    matcher_path: Path,
    extra_fixtures: Sequence[str],
    timeout_seconds: float,
    keep_workspace: bool,
) -> dict[str, Any]:
    """Run one PHPT through isolated PHP and Elephc sandboxes."""
    source_phpt = source_root / entry_path
    case: PhptCase | None = None
    try:
        case = parse_phpt(source_phpt)
    except (HarnessError, OSError) as error:
        result = result_skeleton(entry_path, case)
        result["reason"] = str(error)
        return result

    result = result_skeleton(entry_path, case)
    workspace = Path(tempfile.mkdtemp(prefix=f"elephc-phpt-{source_phpt.stem}-"))
    if keep_workspace:
        result["workspace"] = str(workspace)
    try:
        oracle_sandbox = stage_sandbox(
            source_root,
            component_root_relative,
            entry_path,
            case,
            workspace / "oracle",
            extra_fixtures,
        )
        elephc_sandbox = stage_sandbox(
            source_root,
            component_root_relative,
            entry_path,
            case,
            workspace / "elephc",
            extra_fixtures,
        )
        stage_elephc_native_project(repo_root, elephc_sandbox.root)

        base_environment = dict(os.environ)
        oracle_temporary = oracle_sandbox.root / ".phpt-tmp"
        elephc_temporary = elephc_sandbox.root / ".phpt-tmp"
        oracle_environment = isolated_environment(
            case.sections.get("ENV"),
            oracle_sandbox.source_path.parent,
            oracle_temporary,
            base_environment,
        )
        elephc_environment = isolated_environment(
            case.sections.get("ENV"),
            elephc_sandbox.source_path.parent,
            elephc_temporary,
            base_environment,
        )
        oracle_test_settings = parse_ini(
            case.sections.get("INI"),
            oracle_sandbox.source_path.parent,
            oracle_temporary,
            base_environment,
        )
        elephc_settings = parse_ini(
            case.sections.get("INI"),
            elephc_sandbox.source_path.parent,
            elephc_temporary,
            base_environment,
        )
        raw_args = validate_args(case.sections.get("ARGS"))

        extensions = required_extensions(case.sections.get("EXTENSIONS"))
        oracle_extension_settings, missing, extension_probes = resolve_oracle_extensions(
            oracle,
            oracle_arguments,
            extensions,
            oracle_environment,
            timeout_seconds,
        )
        result["oracle"]["extension_probes"] = [
            execution_record(probe) for probe in extension_probes
        ]
        result["oracle"]["extension_settings"] = [
            {"name": name, "value": value}
            for name, value in oracle_extension_settings
        ]
        if missing:
            result["status"] = "unavailable_extension"
            result["reason"] = "required oracle extension(s) unavailable: " + ", ".join(missing)
            return result
        oracle_settings = [*oracle_extension_settings, *oracle_test_settings]

        compilations = compile_sections(
            elephc_sandbox,
            elephc,
            elephc_arguments,
            elephc_settings,
            target,
            repo_root,
            compiler_environment(elephc_environment, base_environment),
            timeout_seconds,
        )
        result["elephc"]["compile"] = {
            name: execution_record(execution) for name, execution in compilations.items()
        }

        skipif_oracle: Execution | None = None
        skipif_elephc: Execution | None = None
        dynamic_xfail = ""
        if oracle_sandbox.skipif_path is not None:
            skipif_oracle = execute_oracle_program(
                oracle,
                oracle_arguments,
                oracle_sandbox.skipif_path,
                oracle_sandbox.root,
                oracle_settings,
                "",
                oracle_environment,
                timeout_seconds,
            )
            skip_compile = compilations["skipif"]
            if successful_compilation(
                skip_compile, compiled_binary(elephc_sandbox.skipif_path)
            ):
                skipif_elephc = execute_elephc_program(
                    compiled_binary(elephc_sandbox.skipif_path),
                    elephc_sandbox.root,
                    execution_prefix,
                    "",
                    elephc_environment,
                    timeout_seconds,
                )
            result["oracle"]["skipif"] = execution_record(skipif_oracle)
            result["elephc"]["skipif"] = execution_record(skipif_elephc)

            if skipif_oracle.timed_out or skipif_oracle.returncode != 0:
                result["reason"] = "oracle SKIPIF did not terminate successfully"
                return result
            oracle_action, oracle_reason = classify_skipif(skipif_oracle.output)
            if oracle_action == "invalid":
                result["reason"] = f"invalid oracle SKIPIF output: {oracle_reason}"
                return result
            if oracle_action == "skip":
                result["status"] = "oracle_skip"
                result["reason"] = oracle_reason or "oracle SKIPIF requested a skip"
                if skipif_elephc is None:
                    result["status"] = "skip_control_mismatch"
                    result["reason"] += "; Elephc SKIPIF did not compile"
                else:
                    elephc_action, _ = classify_skipif(skipif_elephc.output)
                    if elephc_action != "skip":
                        result["status"] = "skip_control_mismatch"
                        result["reason"] += "; Elephc SKIPIF classification differs"
                return result
            if oracle_action == "xfail":
                dynamic_xfail = oracle_reason
            if skipif_elephc is None:
                result["status"] = "failed"
                result["reason"] = "Elephc SKIPIF did not compile"
                return result
            elephc_action, elephc_reason = classify_skipif(skipif_elephc.output)
            if elephc_action != oracle_action:
                result["status"] = "failed"
                result["reason"] = (
                    f"SKIPIF classification mismatch: oracle={oracle_action}, "
                    f"Elephc={elephc_action} ({elephc_reason})"
                )
                return result

        file_compile = compilations["file"]
        elephc_binary = compiled_binary(elephc_sandbox.source_path)
        file_compiled = successful_compilation(file_compile, elephc_binary)
        clean_compiled = elephc_sandbox.clean_path is None or successful_compilation(
            compilations["clean"], compiled_binary(elephc_sandbox.clean_path)
        )

        oracle_before = snapshot_tree(oracle_sandbox.root)
        oracle_file = execute_oracle_program(
            oracle,
            oracle_arguments,
            oracle_sandbox.source_path,
            oracle_sandbox.root,
            oracle_settings,
            raw_args,
            oracle_environment,
            timeout_seconds,
        )
        oracle_after_file = snapshot_tree(oracle_sandbox.root)
        oracle_delta = tree_delta(oracle_before, oracle_after_file)
        result["oracle_file_delta"] = oracle_delta
        result["oracle"]["file"] = execution_record(oracle_file)

        oracle_clean: Execution | None = None
        if oracle_sandbox.clean_path is not None:
            oracle_clean = execute_oracle_program(
                oracle,
                oracle_arguments,
                oracle_sandbox.clean_path,
                oracle_sandbox.root,
                oracle_settings,
                "",
                oracle_environment,
                timeout_seconds,
            )
        result["oracle"]["clean"] = execution_record(oracle_clean)
        oracle_post_clean_delta = tree_delta(
            oracle_before, snapshot_tree(oracle_sandbox.root)
        )
        result["oracle_post_clean_delta"] = oracle_post_clean_delta

        if oracle_file.timed_out:
            result["status"] = "oracle_timeout"
            result["reason"] = "oracle FILE timed out"
            return result

        mode = case.expectation_mode
        expected = case.sections[mode]
        oracle_match, oracle_match_execution = match_output(
            oracle,
            oracle_arguments,
            matcher_path,
            mode,
            expected,
            oracle_file.output,
            workspace,
            oracle_environment,
            timeout_seconds,
        )
        result["oracle"]["match"] = execution_record(oracle_match_execution)

        if not oracle_match:
            result["status"] = "oracle_failure"
            result["reason"] = "pinned oracle output does not satisfy the frozen expectation"
            return result
        if not clean_is_silent(oracle_clean):
            result["status"] = "oracle_borked"
            result["reason"] = "oracle CLEAN failed, timed out, or produced output"
            return result
        if not file_compiled:
            result["status"] = "failed"
            result["reason"] = "Elephc FILE compilation failed or produced no executable"
            return result
        if not clean_compiled:
            result["status"] = "failed"
            result["reason"] = "Elephc CLEAN compilation failed or produced no executable"
            return result

        elephc_before = snapshot_tree(elephc_sandbox.root)
        elephc_file = execute_elephc_program(
            elephc_binary,
            elephc_sandbox.root,
            execution_prefix,
            raw_args,
            elephc_environment,
            timeout_seconds,
        )
        elephc_after_file = snapshot_tree(elephc_sandbox.root)
        elephc_delta = tree_delta(elephc_before, elephc_after_file)
        result["elephc_file_delta"] = elephc_delta
        result["elephc"]["file"] = execution_record(elephc_file)

        elephc_clean: Execution | None = None
        if elephc_sandbox.clean_path is not None:
            elephc_clean = execute_elephc_program(
                compiled_binary(elephc_sandbox.clean_path),
                elephc_sandbox.root,
                execution_prefix,
                "",
                elephc_environment,
                timeout_seconds,
            )
        result["elephc"]["clean"] = execution_record(elephc_clean)
        elephc_post_clean_delta = tree_delta(
            elephc_before, snapshot_tree(elephc_sandbox.root)
        )
        result["elephc_post_clean_delta"] = elephc_post_clean_delta

        if elephc_file.timed_out:
            result["status"] = "timed_out"
            result["reason"] = "Elephc FILE timed out"
            return result

        elephc_match, elephc_match_execution = match_output(
            oracle,
            oracle_arguments,
            matcher_path,
            mode,
            expected,
            elephc_file.output,
            workspace,
            oracle_environment,
            timeout_seconds,
        )
        result["elephc"]["match"] = execution_record(elephc_match_execution)

        reasons: list[str] = []
        if not elephc_match:
            reasons.append("Elephc output does not satisfy the frozen expectation")
        if oracle_file.returncode != elephc_file.returncode:
            reasons.append(
                f"FILE exit mismatch: oracle={oracle_file.returncode}, "
                f"Elephc={elephc_file.returncode}"
            )
        if oracle_delta != elephc_delta:
            reasons.append("observable FILE filesystem delta differs from the oracle")
        if oracle_post_clean_delta != elephc_post_clean_delta:
            reasons.append("post-CLEAN filesystem delta differs from the oracle")
        if not clean_is_silent(elephc_clean):
            reasons.append("Elephc CLEAN failed, timed out, or produced output")

        xfail_reason = dynamic_xfail or case.sections.get("XFAIL", b"").decode(
            "utf-8", "replace"
        ).strip()
        if reasons:
            if xfail_reason:
                result["status"] = "xfail"
                result["reason"] = xfail_reason + "; " + "; ".join(reasons)
            else:
                result["status"] = "failed"
                result["reason"] = "; ".join(reasons)
        elif xfail_reason:
            result["status"] = "xpass"
            result["reason"] = f"unexpected pass for XFAIL: {xfail_reason}"
        else:
            result["status"] = "passed"
            result["reason"] = "oracle and Elephc satisfy expectation, exit, and file-delta parity"
        return result
    except (HarnessError, OSError, UnicodeError, ValueError) as error:
        result["status"] = "borked"
        result["reason"] = str(error)
        return result
    finally:
        if not keep_workspace:
            shutil.rmtree(workspace, ignore_errors=True)


def parse_arguments(arguments: Sequence[str] | None = None) -> argparse.Namespace:
    """Parse the focused DOM PHPT runner command line."""
    parser = argparse.ArgumentParser(
        description="Run a frozen PHP 8.5.8 DOM-family PHPT ledger against Elephc."
    )
    parser.add_argument("--php-src", type=Path, required=True)
    parser.add_argument("--oracle", type=Path, required=True)
    parser.add_argument("--elephc", type=Path, required=True)
    parser.add_argument("--component", choices=COMPONENTS, default="simplexml")
    parser.add_argument("--ledger", type=Path)
    parser.add_argument(
        "--filter",
        default="",
        help="regular expression matched against PHPT paths",
    )
    parser.add_argument("--limit", type=int)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--report-json", type=Path, required=True)
    parser.add_argument(
        "--target",
        choices=("macos-aarch64", "linux-aarch64", "linux-x86_64"),
    )
    parser.add_argument(
        "--oracle-arg",
        action="append",
        default=[],
        help="argument inserted after php -n (repeatable; use --oracle-arg=VALUE)",
    )
    parser.add_argument(
        "--elephc-arg",
        action="append",
        default=[],
        help="additional compiler argument (repeatable; use --elephc-arg=VALUE)",
    )
    parser.add_argument(
        "--execute-prefix",
        action="append",
        default=[],
        help="emulator/runner prefix before a linked Elephc binary (repeatable)",
    )
    parser.add_argument(
        "--external-fixture",
        action="append",
        default=[],
        help="extra php-src-relative fixture copied into every sandbox",
    )
    parser.add_argument("--keep-workspace", action="store_true")
    parsed = parser.parse_args(arguments)
    if parsed.timeout <= 0:
        parser.error("--timeout must be positive")
    if parsed.limit is not None and parsed.limit <= 0:
        parser.error("--limit must be positive")
    try:
        re.compile(parsed.filter)
    except re.error as error:
        parser.error(f"invalid --filter regular expression: {error}")
    return parsed


def main(arguments: Sequence[str] | None = None) -> int:
    """Validate provenance, run selected PHPTs, and write a complete JSON report."""
    parsed = parse_arguments(arguments)
    repo_root = Path(__file__).resolve().parents[2]
    matcher_path = repo_root / "tools/php-dom/phpt_match.php"
    lock_path = repo_root / "tools/php-dom/source-lock.json"
    lock = json.loads(lock_path.read_text())
    source_root = parsed.php_src.resolve()
    oracle = parsed.oracle.resolve()
    elephc = parsed.elephc.resolve()
    ledger_path = (
        parsed.ledger.resolve()
        if parsed.ledger
        else repo_root / f"tests/php_dom/upstream/{parsed.component}-php-8.5.8.json"
    )
    report_path = parsed.report_json.resolve()

    try:
        if not oracle.is_file() or not os.access(oracle, os.X_OK):
            raise HarnessError(f"PHP oracle is not executable: {oracle}")
        if not elephc.is_file() or not os.access(elephc, os.X_OK):
            raise HarnessError(f"Elephc binary is not executable: {elephc}")
        if not matcher_path.is_file():
            raise HarnessError(f"PHPT matcher is missing: {matcher_path}")
        validate_source_root(source_root, lock)
        ledger, entries = validate_ledger(
            source_root, ledger_path, lock["ledgers"][parsed.component]
        )
        oracle_metadata = validate_oracle(
            oracle,
            parsed.oracle_arg,
            lock,
            dict(os.environ),
            parsed.timeout,
        )
    except (HarnessError, OSError, KeyError, json.JSONDecodeError) as error:
        print(f"PHPT harness preflight failed: {error}", file=sys.stderr)
        return 2

    selected = [
        entry
        for entry in entries
        if not parsed.filter or re.search(parsed.filter, entry["path"])
    ]
    if parsed.limit is not None:
        selected = selected[: parsed.limit]
    if not selected:
        print("PHPT harness preflight failed: no tests selected", file=sys.stderr)
        return 2

    report: dict[str, Any] = {
        "schema": 1,
        "php_source_commit": lock["php"]["commit"],
        "php_version": lock["php"]["version"],
        "libxml_version": lock["libxml2"]["version"],
        "component": parsed.component,
        "ledger": str(ledger_path),
        "ledger_closed": ledger["closed"],
        "oracle": str(oracle),
        "oracle_metadata": oracle_metadata,
        "elephc": str(elephc),
        "elephc_sha256": sha256_file(elephc),
        "target": parsed.target,
        "filter": parsed.filter,
        "selected": len(selected),
        "summary": {},
        "results": [],
    }
    for index, entry in enumerate(selected, start=1):
        print(f"[{index}/{len(selected)}] {entry['path']}", flush=True)
        result = run_case(
            entry_path=entry["path"],
            source_root=source_root,
            component_root_relative=ledger["source_root"],
            oracle=oracle,
            oracle_arguments=parsed.oracle_arg,
            elephc=elephc,
            elephc_arguments=parsed.elephc_arg,
            execution_prefix=parsed.execute_prefix,
            target=parsed.target,
            repo_root=repo_root,
            matcher_path=matcher_path,
            extra_fixtures=parsed.external_fixture,
            timeout_seconds=parsed.timeout,
            keep_workspace=parsed.keep_workspace,
        )
        report["results"].append(result)
        status = result["status"]
        report["summary"][status] = report["summary"].get(status, 0) + 1
        print(f"  {status}: {result['reason']}", flush=True)

    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
    failing = [
        result for result in report["results"] if result["status"] not in PASSING_STATUSES
    ]
    print(
        f"wrote {report_path}; summary: {json.dumps(report['summary'], sort_keys=True)}"
    )
    return 1 if failing else 0


if __name__ == "__main__":
    raise SystemExit(main())
