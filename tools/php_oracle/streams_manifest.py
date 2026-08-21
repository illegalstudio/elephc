#!/usr/bin/env python3
"""Generate and validate frozen PHP 8.5.6 stream build manifests."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

PHP_RELEASE = "8.5.6"
PHP_SRC_COMMIT = "fcc29c8d6d6ee6f5ba2d941f0a2a6ea6aa6ee633"
SCHEMA_VERSION = 1
SUPPORTED_TARGETS = ("macos-aarch64", "linux-aarch64", "linux-x86_64")
EXPLICIT_INI = {
    "date.timezone": "UTC",
    "display_errors": "stderr",
    "error_reporting": "-1",
    "html_errors": "0",
    "log_errors": "0",
}
ROOT = Path(__file__).resolve().parents[2]
PROBE = Path(__file__).with_name("manifest_probe.php")
MANIFEST_ROOT = (
    ROOT / "tests" / "php_oracle" / "manifests" / "streams" / f"php-{PHP_RELEASE}"
)


def parse_args() -> argparse.Namespace:
    """Parse generator and checked-in validation arguments."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--php-bin", type=Path)
    parser.add_argument("--php-src", type=Path)
    parser.add_argument("--target")
    parser.add_argument("--build-profile")
    parser.add_argument("--reachability", type=Path)
    parser.add_argument(
        "--binary-attestation",
        choices=("external-unverified", "source-build"),
        default="external-unverified",
    )
    parser.add_argument("--build-dir", type=Path)
    parser.add_argument("--compile-log", type=Path)
    parser.add_argument("--drift-ledger", type=Path)
    parser.add_argument("--corpus-index", type=Path)
    parser.add_argument("--output", type=Path)
    action = parser.add_mutually_exclusive_group(required=True)
    action.add_argument("--write", action="store_true")
    action.add_argument("--check", action="store_true")
    action.add_argument("--validate-all", action="store_true")
    return parser.parse_args()


def canonical_bytes(value: Any) -> bytes:
    """Serialize JSON deterministically with UTF-8, sorted keys, and final LF."""
    return (
        json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + "\n"
    ).encode()


def sha256_bytes(value: bytes) -> str:
    """Return the lowercase SHA-256 digest for arbitrary bytes."""
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    """Return the SHA-256 digest of one file."""
    return sha256_bytes(path.read_bytes())


def git_head(repo: Path) -> str:
    """Resolve one checkout's current commit."""
    result = subprocess.run(
        ["git", "-C", str(repo), "rev-parse", "HEAD"],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    return result.stdout.strip()


def git_tree(repo: Path) -> str:
    """Resolve the immutable tree object for one checkout's current commit."""
    result = subprocess.run(
        ["git", "-C", str(repo), "rev-parse", "HEAD^{tree}"],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    return result.stdout.strip()


def git_tracked_clean(repo: Path) -> bool:
    """Return whether tracked files exactly match the checked-out commit."""
    result = subprocess.run(
        ["git", "-C", str(repo), "status", "--porcelain", "--untracked-files=no"],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return result.stdout == b""


def php_command(php_bin: Path, script: Path) -> list[str]:
    """Build the hermetic no-INI PHP command used for manifest capture."""
    command = [str(php_bin), "-n"]
    for name, value in EXPLICIT_INI.items():
        command.extend(["-d", f"{name}={value}"])
    command.append(str(script))
    return command


def recorded_php_argv() -> list[str]:
    """Return the checkout-independent PHP argument vector recorded in artifacts."""
    arguments = ["-n"]
    for name, value in EXPLICIT_INI.items():
        arguments.extend(["-d", f"{name}={value}"])
    arguments.append(PROBE.resolve().relative_to(ROOT).as_posix())
    return arguments


def oracle_environment() -> dict[str, str]:
    """Return the deterministic environment for PHP oracle processes."""
    environment = os.environ.copy()
    environment.update({"LC_ALL": "C", "LANG": "C", "TZ": "UTC"})
    return environment


def run_php_json(
    php_bin: Path, script: Path, reachability: Path | None = None
) -> dict[str, Any]:
    """Run one PHP JSON probe and reject diagnostics or malformed output."""
    environment = oracle_environment()
    if reachability is not None:
        environment["ELEPHC_STREAM_REACHABILITY"] = str(reachability.resolve())
    result = subprocess.run(
        php_command(php_bin, script),
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=environment,
    )
    if result.returncode != 0:
        raise SystemExit(
            f"PHP manifest probe exited {result.returncode}:\n"
            + result.stderr.decode("utf-8", errors="replace")
        )
    if result.stderr:
        raise SystemExit(
            "PHP manifest probe emitted diagnostics:\n"
            + result.stderr.decode("utf-8", errors="replace")
        )
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise SystemExit(f"PHP manifest probe emitted invalid JSON: {error}") from error


def configure_command(php_bin: Path) -> list[str]:
    """Read the configured PHP build command from `php -i` when available."""
    result = subprocess.run(
        [str(php_bin), "-n", "-i"],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=oracle_environment(),
        text=True,
    )
    match = re.search(r"(?m)^Configure Command => (.*)$", result.stdout)
    if match is None:
        return []
    return [match.group(1)]


def target_for_runtime(runtime: dict[str, Any]) -> str:
    """Map the PHP runtime OS/architecture pair to Elephc's target spelling."""
    os_family = str(runtime["os_family"])
    machine = str(runtime["uname_machine"]).lower()
    if os_family == "Darwin" and machine in {"arm64", "aarch64"}:
        return "macos-aarch64"
    if os_family == "Linux" and machine in {"arm64", "aarch64"}:
        return "linux-aarch64"
    if os_family == "Linux" and machine in {"x86_64", "amd64"}:
        return "linux-x86_64"
    raise SystemExit(f"unsupported PHP oracle target: {os_family}/{machine}")


def require_generation_args(args: argparse.Namespace) -> None:
    """Reject incomplete generation/check invocations."""
    missing = [
        name
        for name in ("php_bin", "php_src", "target", "build_profile")
        if getattr(args, name) in (None, "")
    ]
    if missing:
        raise SystemExit(
            "--write/--check requires: " + ", ".join(f"--{name.replace('_', '-')}" for name in missing)
        )
    if args.binary_attestation == "source-build":
        source_missing = [
            name
            for name in ("build_dir", "compile_log")
            if getattr(args, name) is None
        ]
        if source_missing:
            raise SystemExit(
                "--binary-attestation source-build requires: "
                + ", ".join(
                    f"--{name.replace('_', '-')}" for name in source_missing
                )
            )


def default_output(target: str, profile: str) -> Path:
    """Return the specification-mandated checked-in manifest path."""
    return MANIFEST_ROOT / target / f"{profile}.json"


def target_profile_is_available(target: str, profile: str) -> bool:
    """Return whether one sibling profile has usable frozen source provenance."""
    path = default_output(target, profile)
    if not path.is_file():
        return False
    try:
        manifest = json.loads(path.read_bytes())
    except (OSError, json.JSONDecodeError):
        return False
    manifest_profile = manifest.get("profile", {})
    return (
        manifest_profile.get("target") == target
        and manifest_profile.get("name") == profile
        and manifest_profile.get("php_release") == PHP_RELEASE
        and manifest_profile.get("php_src_commit") == PHP_SRC_COMMIT
        and manifest.get("build", {}).get("binary_source_attestation")
        == "source-build"
    )


def missing_target_profile_requirements(target: str, profile: str) -> list[str]:
    """List supported-target siblings that lack an authoritative source profile."""
    return [
        f"{candidate}-profile"
        for candidate in SUPPORTED_TARGETS
        if candidate != target and not target_profile_is_available(candidate, profile)
    ]


def normalize_build_text(content: str, php_src: Path, build_dir: Path) -> str:
    """Replace ephemeral checkout/build roots in captured build evidence."""
    replacements = (
        (str(build_dir), "${BUILD_DIR}"),
        (str(build_dir.resolve()), "${BUILD_DIR}"),
        (str(php_src), "${PHP_SRC}"),
        (str(php_src.resolve()), "${PHP_SRC}"),
        (str(ROOT), "${ELEPHC_ROOT}"),
        (str(ROOT.resolve()), "${ELEPHC_ROOT}"),
        (str(php_src.parent), "${ORACLE_ROOT}"),
        (str(php_src.resolve().parent), "${ORACLE_ROOT}"),
    )
    normalized = content
    for original, replacement in replacements:
        normalized = normalized.replace(original, replacement)
        normalized = normalized.replace(
            original.replace("/private/tmp/", "/tmp/"),
            replacement,
        )
    return normalized


def build_input_record(path: Path, php_src: Path, build_dir: Path) -> dict[str, Any]:
    """Describe one build input using raw and path-normalized digests."""
    content = path.read_bytes()
    normalized = normalize_build_text(
        content.decode("utf-8", errors="surrogateescape"),
        php_src,
        build_dir,
    ).encode("utf-8", errors="surrogateescape")
    return {
        "path": path.resolve().relative_to(build_dir.resolve()).as_posix(),
        "sha256": sha256_bytes(content),
        "normalized_sha256": sha256_bytes(normalized),
    }


def compile_capture_record(
    path: Path, php_src: Path, build_dir: Path
) -> dict[str, Any]:
    """Summarize a compiler JSONL capture with ephemeral roots normalized."""
    records: list[dict[str, Any]] = []
    translation_units: list[str] = []
    compilers: set[str] = set()
    for line_number, line in enumerate(path.read_text().splitlines(), start=1):
        try:
            record = json.loads(line)
        except json.JSONDecodeError as error:
            raise SystemExit(
                f"invalid compile capture JSON at line {line_number}: {error}"
            ) from error
        compiler = record.get("compiler")
        if isinstance(compiler, str):
            compilers.add(compiler)
        normalized = json.loads(
            normalize_build_text(
                json.dumps(record, sort_keys=True, ensure_ascii=False),
                php_src,
                build_dir,
            )
        )
        records.append(normalized)
        arguments = normalized.get("arguments", [])
        if "-c" in arguments:
            index = arguments.index("-c")
            if index + 1 < len(arguments):
                translation_units.append(str(arguments[index + 1]))
    return {
        "sha256": sha256_file(path),
        "normalized_sha256": sha256_bytes(canonical_bytes(records)),
        "records": len(records),
        "translation_units": len(translation_units),
        "unique_translation_units": len(set(translation_units)),
        "compilers": [tool_record(Path(compiler)) for compiler in sorted(compilers)],
    }


def command_output(arguments: list[str]) -> str:
    """Run one provenance command and preserve combined textual output."""
    result = subprocess.run(
        arguments,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        env=oracle_environment(),
        text=True,
    )
    return result.stdout.strip()


def tool_record(path: Path) -> dict[str, Any]:
    """Describe one build tool by resolved path, binary digest, and version."""
    resolved = path.resolve()
    return {
        "path": str(resolved),
        "sha256": sha256_file(resolved) if resolved.is_file() else None,
        "version": command_output([str(resolved), "--version"]),
    }


def source_build_environment(php_bin: Path) -> dict[str, Any]:
    """Capture target OS, build-tool, and dynamic-link provenance."""
    os_release = Path("/etc/os-release")
    if os_release.is_file():
        operating_system = os_release.read_text()
    else:
        operating_system = command_output(["sw_vers"])
    if sys.platform == "darwin":
        libraries = command_output(["otool", "-L", str(php_bin)])
    else:
        libraries = command_output(["ldd", str(php_bin)])
        libraries = re.sub(r"0x[0-9A-Fa-f]+", "${ADDRESS}", libraries)
    libraries = libraries.replace(str(php_bin), "${PHP_BINARY}")
    return {
        "uname": command_output(["uname", "-srmv"]),
        "operating_system": operating_system.strip(),
        "make": command_output(["make", "--version"]).splitlines()[0],
        "linker": command_output(["ld", "--version"]).splitlines()[0],
        "linked_libraries": libraries.splitlines(),
    }


def source_line_number(content: str, offset: int) -> int:
    """Return the one-based source line containing one character offset."""
    return content.count("\n", 0, offset) + 1


def normalize_c_statement(statement: str) -> str:
    """Collapse insignificant C whitespace for stable source evidence."""
    return " ".join(statement.split())


def wrapper_argument_initializers(
    function_prefix: str,
    argument_storage: str,
    arity: int,
) -> list[dict[str, Any]]:
    """Extract source initializers for one user-wrapper callback invocation."""
    if arity == 0:
        return []
    if argument_storage == "args":
        storage_pattern = r"args\[(?P<position>\d+)\]"
    else:
        storage_pattern = re.escape(argument_storage.removeprefix("&"))
    pattern = re.compile(
        rf"(?P<statement>ZVAL_(?P<operation>[A-Z0-9_]+)\s*"
        rf"\(\s*&(?P<storage>{storage_pattern})\s*,.*?\);)",
        re.DOTALL,
    )
    by_position: dict[int, dict[str, Any]] = {}
    for match in pattern.finditer(function_prefix):
        position_text = match.groupdict().get("position")
        position = int(position_text) if position_text is not None else 0
        if position >= arity:
            continue
        entry = by_position.setdefault(
            position,
            {
                "position": position,
                "by_reference": False,
                "initializers": [],
            },
        )
        operation = match.group("operation")
        entry["by_reference"] |= operation in {
            "NEW_REF",
            "MAKE_REF",
            "REF",
        }
        statement = normalize_c_statement(match.group("statement"))
        if statement not in entry["initializers"]:
            entry["initializers"].append(statement)
    return [by_position[position] for position in sorted(by_position)]


def extract_user_wrapper_protocol(php_src: Path) -> dict[str, Any]:
    """Extract callback names, arities, references, and argument producers from php-src."""
    relative_source = Path("main/streams/userspace.c")
    source = php_src / relative_source
    content = source.read_text()
    macro_pattern = re.compile(
        r'(?m)^#define\s+(?P<macro>USERSTREAM_[A-Z_]+)\s+"(?P<name>[^"]+)"'
    )
    callbacks = {
        match.group("macro"): {
            "name": match.group("name"),
            "macro": match.group("macro"),
            "definition_line": source_line_number(content, match.start()),
            "invocations": [],
        }
        for match in macro_pattern.finditer(content)
    }
    assignment_pattern = re.compile(
        r"(?:zend_string\s+\*)?func_name\s*=\s*"
        r"ZSTR_INIT_LITERAL\((?P<macro>USERSTREAM_[A-Z_]+),\s*false\);"
    )
    call_pattern = re.compile(
        r"zend_call_method_if_exists\((?P<arguments>.*?)\);",
        re.DOTALL,
    )
    for assignment in assignment_pattern.finditer(content):
        call = call_pattern.search(content, assignment.end())
        if call is None:
            raise SystemExit(
                f"wrapper callback {assignment.group('macro')} has no invocation"
            )
        call_arguments = call.group("arguments")
        tail = re.search(
            r",\s*(?P<arity>\d+)\s*,\s*(?P<storage>args|&[A-Za-z_][A-Za-z0-9_]*|NULL)\s*$",
            call_arguments,
        )
        if tail is None:
            raise SystemExit(
                f"cannot parse wrapper callback invocation at "
                f"{relative_source}:{source_line_number(content, call.start())}"
            )
        macro = assignment.group("macro")
        if macro not in callbacks:
            raise SystemExit(f"undefined wrapper callback macro: {macro}")
        arity = int(tail.group("arity"))
        storage = tail.group("storage")
        function_start = content.rfind("\nstatic ", 0, assignment.start())
        if function_start < 0:
            raise SystemExit(f"cannot find C function for wrapper callback {macro}")
        function_prefix = content[function_start:call.start()]
        initializers = wrapper_argument_initializers(
            function_prefix,
            storage,
            arity,
        )
        initialized_positions = {entry["position"] for entry in initializers}
        if initialized_positions != set(range(arity)):
            raise SystemExit(
                f"incomplete callback argument evidence for {macro}: "
                f"expected {arity}, got {sorted(initialized_positions)}"
            )
        callbacks[macro]["invocations"].append(
            {
                "source_line": source_line_number(content, call.start()),
                "arity": arity,
                "argument_storage": storage,
                "arguments": initializers,
            }
        )
    missing = [macro for macro, callback in callbacks.items() if not callback["invocations"]]
    if missing:
        raise SystemExit("wrapper callback macros have no invocation: " + ", ".join(missing))
    return {
        "source": relative_source.as_posix(),
        "source_sha256": sha256_file(source),
        "callbacks": sorted(
            callbacks.values(),
            key=lambda callback: str(callback["name"]).lower(),
        ),
    }


def source_build_evidence(
    args: argparse.Namespace, php_src: Path, php_bin: Path
) -> dict[str, Any]:
    """Validate and describe a PHP binary built from the frozen checkout."""
    build_dir = args.build_dir.resolve()
    compile_log = args.compile_log.resolve()
    if build_dir not in php_bin.parents:
        raise SystemExit("source-built PHP binary must be below --build-dir")
    if not compile_log.is_file():
        raise SystemExit(f"compile capture missing: {compile_log}")
    input_names = ("config.nice", "main/php_config.h", "Makefile", "Makefile.objects")
    missing = [name for name in input_names if not (build_dir / name).is_file()]
    if missing:
        raise SystemExit("source build inputs missing: " + ", ".join(missing))
    if not git_tracked_clean(php_src):
        raise SystemExit("php-src tracked files differ from the frozen commit")
    return {
        "binary_source_attestation": "source-build",
        "binary_path": php_bin.relative_to(build_dir).as_posix(),
        "environment": source_build_environment(php_bin),
        "build_inputs": [
            build_input_record(build_dir / name, php_src, build_dir)
            for name in input_names
        ],
        "compile_capture": compile_capture_record(
            compile_log,
            php_src,
            build_dir,
        ),
    }


def build_manifest(args: argparse.Namespace) -> dict[str, Any]:
    """Capture and wrap one exact php-src/PHP build profile."""
    require_generation_args(args)
    php_bin = args.php_bin.resolve()
    php_src = args.php_src.resolve()
    if git_head(php_src) != PHP_SRC_COMMIT:
        raise SystemExit(
            f"php-src must be at frozen commit {PHP_SRC_COMMIT}, "
            f"got {git_head(php_src)}"
        )
    observed = run_php_json(php_bin, PROBE, args.reachability)
    runtime = observed["runtime"]
    if runtime["php_version"] != PHP_RELEASE:
        raise SystemExit(
            f"PHP oracle must be {PHP_RELEASE}, got {runtime['php_version']}"
        )
    observed_target = target_for_runtime(runtime)
    if observed_target != args.target:
        raise SystemExit(
            f"requested target {args.target} does not match oracle {observed_target}"
        )
    source_build = args.binary_attestation == "source-build"
    if source_build:
        build = source_build_evidence(args, php_src, php_bin)
        captured_configure = [
            normalize_build_text(value, php_src, args.build_dir.resolve())
            for value in configure_command(php_bin)
        ]
    else:
        build = {
            "binary_source_attestation": "external-unverified",
            "binary_path": str(php_bin),
        }
        captured_configure = configure_command(php_bin)
    reachability = None
    if args.reachability is not None:
        reachability_manifest = json.loads(args.reachability.read_bytes())
        reachability_profile = reachability_manifest.get("profile", {})
        if reachability_manifest.get("gate", {}).get("status") != "candidate":
            raise SystemExit("reachability manifest is not a Gate 0 candidate")
        if reachability_profile.get("php_src_commit") != PHP_SRC_COMMIT:
            raise SystemExit("reachability manifest uses the wrong php-src commit")
        if reachability_profile.get("target") != args.target:
            raise SystemExit("reachability manifest uses the wrong target")
        if reachability_profile.get("build_profile") != args.build_profile:
            raise SystemExit("reachability manifest uses the wrong build profile")
        reachability = {
            "path": args.reachability.resolve().relative_to(ROOT).as_posix(),
            "sha256": sha256_file(args.reachability),
        }
    binary_path = Path(runtime["php_binary"]).resolve()
    runtime["php_binary"] = build["binary_path"]
    runtime["php_binary_sha256"] = sha256_file(binary_path)
    open_requirements = missing_target_profile_requirements(
        args.target,
        args.build_profile,
    )
    if reachability is None:
        open_requirements.insert(0, "authoritative-clang-source-reachability")
    if args.drift_ledger is None:
        open_requirements.insert(0, "elephc-classified-drift-ledger")
    if args.corpus_index is None:
        open_requirements.insert(0, "differential-oracle-corpus")
    if not source_build:
        open_requirements.insert(0, "profile-binary-source-attestation")
    companion_evidence = {}
    for key, path in (
        ("drift_ledger", args.drift_ledger),
        ("corpus_index", args.corpus_index),
    ):
        if path is not None:
            companion_evidence[key] = {
                "path": path.resolve().relative_to(ROOT).as_posix(),
                "sha256": sha256_file(path),
            }
    return {
        "schema_version": SCHEMA_VERSION,
        "kind": "php-src-stream-build-manifest",
        "gate": {
            "number": 0,
            "status": "candidate" if not open_requirements else "partial",
            "acceptance_scope": [
                "build-profile",
                "configured-capabilities",
                "public-stream-constants",
                *(
                    ["reachable-function-and-class-reflection"]
                    if reachability is not None
                    else []
                ),
            ],
            "open_requirements": open_requirements,
        },
        "profile": {
            "name": args.build_profile,
            "target": args.target,
            "php_release": PHP_RELEASE,
            "php_src_tag": f"php-{PHP_RELEASE}",
            "php_src_commit": PHP_SRC_COMMIT,
            "php_src_tree": git_tree(php_src),
            "php_src_tracked_clean": git_tracked_clean(php_src),
            "configure_argv": captured_configure,
            "php_argv": recorded_php_argv(),
            "environment": {"LC_ALL": "C", "LANG": "C", "TZ": "UTC"},
            "explicit_ini": EXPLICIT_INI,
            "elephc_args": [],
            "elephc_features": [],
            "elephc_bridges": [],
        },
        "build": build,
        "companion_evidence": companion_evidence,
        "oracle": runtime,
        "surface": observed["surface"],
        "wrapper_protocol": extract_user_wrapper_protocol(php_src),
        "reachability": reachability,
        "generator": {
            "script": Path(__file__).resolve().relative_to(ROOT).as_posix(),
            "script_sha256": sha256_file(Path(__file__)),
            "probe": PROBE.resolve().relative_to(ROOT).as_posix(),
            "probe_sha256": sha256_file(PROBE),
        },
    }


def validate_manifest(path: Path, manifest: dict[str, Any]) -> list[str]:
    """Return structural/canonical validation errors for one checked-in manifest."""
    errors: list[str] = []
    if manifest.get("schema_version") != SCHEMA_VERSION:
        errors.append("unsupported schema_version")
    profile = manifest.get("profile", {})
    if profile.get("php_release") != PHP_RELEASE:
        errors.append("wrong php_release")
    if profile.get("php_src_commit") != PHP_SRC_COMMIT:
        errors.append("wrong php_src_commit")
    if not profile.get("php_src_tree"):
        errors.append("php_src_tree missing")
    if profile.get("php_src_tracked_clean") is not True:
        errors.append("php_src tracked state is not clean")
    build = manifest.get("build", {})
    attestation = build.get("binary_source_attestation")
    if attestation not in {"external-unverified", "source-build"}:
        errors.append("invalid binary source attestation")
    gate = manifest.get("gate", {})
    open_requirements = gate.get("open_requirements", [])
    expected_target_requirements = set(
        missing_target_profile_requirements(
            str(profile.get("target", "")),
            str(profile.get("name", "")),
        )
    )
    actual_target_requirements = {
        requirement
        for requirement in open_requirements
        if requirement.endswith("-profile")
    }
    if actual_target_requirements != expected_target_requirements:
        errors.append("supported-target profile requirements are stale")
    expected_status = "candidate" if not open_requirements else "partial"
    if gate.get("status") != expected_status:
        errors.append(f"gate status must be {expected_status}")
    if (
        attestation == "external-unverified"
        and "profile-binary-source-attestation" not in open_requirements
    ):
        errors.append("unverified binary attestation is not an open requirement")
    if (
        attestation == "source-build"
        and "profile-binary-source-attestation" in open_requirements
    ):
        errors.append("source-built profile still reports missing attestation")
    if attestation == "source-build":
        compile_capture = build.get("compile_capture", {})
        if compile_capture.get("translation_units", 0) < 1:
            errors.append("source build has no captured translation units")
        if (
            compile_capture.get("translation_units")
            != compile_capture.get("unique_translation_units")
        ):
            errors.append("source build compile capture contains duplicate units")
        if not compile_capture.get("compilers"):
            errors.append("source build compiler provenance is missing")
        environment = build.get("environment", {})
        for key in ("uname", "operating_system", "make", "linker", "linked_libraries"):
            if not environment.get(key):
                errors.append(f"source build environment.{key} is missing")
    reachability = manifest.get("reachability")
    if reachability is not None:
        evidence_path = ROOT / reachability.get("path", "")
        if not evidence_path.is_file():
            errors.append("reachability evidence is missing")
        elif sha256_file(evidence_path) != reachability.get("sha256"):
            errors.append("reachability evidence digest mismatch")
    companions = manifest.get("companion_evidence", {})
    for requirement, available in (
        ("authoritative-clang-source-reachability", reachability is not None),
        ("elephc-classified-drift-ledger", "drift_ledger" in companions),
        ("differential-oracle-corpus", "corpus_index" in companions),
    ):
        if available and requirement in open_requirements:
            errors.append(f"{requirement} is open despite checked-in evidence")
        if not available and requirement not in open_requirements:
            errors.append(f"{requirement} is hidden without checked-in evidence")
    for name, evidence in companions.items():
        evidence_path = ROOT / evidence.get("path", "")
        if not evidence_path.is_file():
            errors.append(f"{name} evidence is missing")
        elif sha256_file(evidence_path) != evidence.get("sha256"):
            errors.append(f"{name} evidence digest mismatch")
    try:
        expected_path = default_output(profile["target"], profile["name"]).resolve()
        if path.resolve() != expected_path:
            errors.append(f"path does not match profile: expected {expected_path}")
    except KeyError:
        errors.append("profile target/name missing")
    surface = manifest.get("surface", {})
    for key in ("wrappers", "transports", "filters"):
        values = surface.get(key)
        if not isinstance(values, list) or not values:
            errors.append(f"surface.{key} must be a non-empty list")
        elif len(values) != len(set(values)):
            errors.append(f"surface.{key} contains duplicates")
    constants = surface.get("constants")
    if not isinstance(constants, dict) or not constants:
        errors.append("surface.constants must be a non-empty object")
    functions = {
        function.get("canonical_name"): function
        for function in surface.get("functions", [])
        if isinstance(function, dict)
    }
    if functions:
        for alias, canonical in (
            ("fputs", "fwrite"),
            ("set_file_buffer", "stream_set_write_buffer"),
            ("socket_get_status", "stream_get_meta_data"),
            ("stream_register_wrapper", "stream_wrapper_register"),
        ):
            if functions.get(alias, {}).get("alias_of") != canonical:
                errors.append(f"{alias} alias target must be {canonical}")
    wrapper_protocol = manifest.get("wrapper_protocol", {})
    callbacks = wrapper_protocol.get("callbacks")
    if not wrapper_protocol.get("source_sha256"):
        errors.append("wrapper protocol source digest is missing")
    if not isinstance(callbacks, list) or not callbacks:
        errors.append("wrapper protocol callbacks must be a non-empty list")
    elif len(callbacks) != len(
        {callback.get("name") for callback in callbacks if isinstance(callback, dict)}
    ):
        errors.append("wrapper protocol callback names contain duplicates")
    elif any(not callback.get("invocations") for callback in callbacks):
        errors.append("wrapper protocol callback has no invocation evidence")
    for forbidden in (
        "STREAM_FROM_START",
        "STREAM_FROM_CUR",
        "STREAM_FROM_END",
        "STREAM_META_MODIFIED",
        "STREAM_OPTION_CHUNK_SIZE",
    ):
        if isinstance(constants, dict) and forbidden in constants:
            errors.append(f"internal constant leaked: {forbidden}")
    for required, expected in (
        ("STREAM_CLIENT_PERSISTENT", 1),
        ("STREAM_CLIENT_ASYNC_CONNECT", 2),
        ("STREAM_CLIENT_CONNECT", 4),
        ("FILE_BINARY", 0),
        ("FILE_TEXT", 0),
    ):
        value = constants.get(required) if isinstance(constants, dict) else None
        if value != {"type": "int", "value": expected}:
            errors.append(f"{required} must equal integer {expected}")
    if path.exists() and path.read_bytes() != canonical_bytes(manifest):
        errors.append("file is not canonical JSON")
    return errors


def validate_all() -> int:
    """Validate every checked-in stream manifest without invoking PHP."""
    paths = sorted(MANIFEST_ROOT.glob("*/*.json"))
    if not paths:
        print(f"no manifests found under {MANIFEST_ROOT}", file=sys.stderr)
        return 1
    failed = False
    for target in SUPPORTED_TARGETS:
        required_profile = default_output(target, "streams-full")
        if not required_profile.is_file():
            print(
                f"missing supported-target source profile: {required_profile}",
                file=sys.stderr,
            )
            failed = True
    for path in paths:
        try:
            manifest = json.loads(path.read_bytes())
        except (OSError, json.JSONDecodeError) as error:
            print(f"{path}: {error}", file=sys.stderr)
            failed = True
            continue
        errors = validate_manifest(path, manifest)
        for error in errors:
            print(f"{path}: {error}", file=sys.stderr)
        failed |= bool(errors)
    return int(failed)


def atomic_write(path: Path, content: bytes) -> None:
    """Replace one generated artifact atomically."""
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(content)
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def main() -> int:
    """Dispatch manifest generation, byte verification, or offline validation."""
    args = parse_args()
    if args.validate_all:
        return validate_all()
    manifest = build_manifest(args)
    output = args.output or default_output(args.target, args.build_profile)
    content = canonical_bytes(manifest)
    if args.check:
        if not output.exists():
            print(f"missing checked-in manifest: {output}", file=sys.stderr)
            return 1
        if output.read_bytes() != content:
            print(f"manifest drift: regenerate {output}", file=sys.stderr)
            return 1
        return 0
    atomic_write(output, content)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
