#!/usr/bin/env python3
"""Capture raw and instrumented PHP stream oracle runs as canonical JSON."""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

SCHEMA_VERSION = 1
ROOT = Path(__file__).resolve().parents[2]
BOOTSTRAP = Path(__file__).with_name("bootstrap.php")
CANONICAL_VALUE = Path(__file__).with_name("canonical_value.php")
ORACLE_ROOT = ROOT / "tests" / "php_oracle" / "oracles" / "streams"
SANDBOX_ROOT = Path("/tmp/elephc-php-oracle/streams")
BASE_INI = {
    "date.timezone": "UTC",
    "display_errors": "stderr",
    "display_startup_errors": "1",
    "error_reporting": "-1",
    "html_errors": "0",
    "log_errors": "0",
}


def parse_args() -> argparse.Namespace:
    """Parse one oracle-case capture or verification request."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--php-bin", required=True, type=Path)
    parser.add_argument("--profile-manifest", required=True, type=Path)
    parser.add_argument("--case-dir", required=True, type=Path)
    parser.add_argument("--output", type=Path)
    action = parser.add_mutually_exclusive_group(required=True)
    action.add_argument("--write", action="store_true")
    action.add_argument("--check", action="store_true")
    return parser.parse_args()


def sha256_bytes(content: bytes) -> str:
    """Return the lowercase SHA-256 digest for arbitrary bytes."""
    return hashlib.sha256(content).hexdigest()


def sha256_file(path: Path) -> str:
    """Return the SHA-256 digest of one file."""
    return sha256_bytes(path.read_bytes())


def canonical_bytes(value: Any) -> bytes:
    """Serialize canonical UTF-8 JSON with sorted keys and a final newline."""
    return (
        json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + "\n"
    ).encode()


def byte_record(content: bytes) -> dict[str, Any]:
    """Describe exact bytes without assuming UTF-8."""
    return {
        "base64": base64.b64encode(content).decode("ascii"),
        "length": len(content),
        "sha256": sha256_bytes(content),
    }


def php_arguments(ini: dict[str, str], script: str, argv: list[str]) -> list[str]:
    """Build a no-INI PHP argument vector in deterministic directive order."""
    arguments = ["-n"]
    for name, value in sorted(ini.items()):
        arguments.extend(["-d", f"{name}={value}"])
    arguments.extend([script, *argv])
    return arguments


def process_environment(case: dict[str, Any]) -> dict[str, str]:
    """Build the deterministic case environment plus declared additions."""
    environment = os.environ.copy()
    environment.update({"LC_ALL": "C", "LANG": "C", "TZ": "UTC"})
    for name, value in case.get("environment", {}).items():
        environment[str(name)] = str(value)
    return environment


def stdin_bytes(case: dict[str, Any]) -> bytes:
    """Decode a case's optional binary stdin declaration."""
    stdin = case.get("stdin")
    if stdin is None:
        return b""
    if stdin.get("encoding") == "base64":
        return base64.b64decode(stdin["data"], validate=True)
    if stdin.get("encoding") == "utf-8":
        return stdin["data"].encode()
    raise SystemExit("case stdin encoding must be base64 or utf-8")


def stable_sandbox(profile: dict[str, Any], case_id: str) -> Path:
    """Return the stable absolute sandbox path used in raw PHP diagnostics."""
    release = profile["php_release"]
    target = profile["target"]
    name = profile["name"]
    return SANDBOX_ROOT / f"php-{release}" / target / name / case_id


def prepare_sandbox(case_dir: Path, sandbox: Path) -> Path:
    """Recreate one validated temp sandbox and copy source/fixtures into it."""
    resolved_root = SANDBOX_ROOT.resolve()
    resolved = sandbox.resolve()
    if resolved_root not in resolved.parents:
        raise SystemExit(f"refusing unsafe sandbox path: {resolved}")
    if sandbox.exists():
        shutil.rmtree(sandbox)
    sandbox.mkdir(parents=True)
    source = case_dir / "main.php"
    if not source.is_file():
        raise SystemExit(f"case source missing: {source}")
    shutil.copy2(source, sandbox / "main.php")
    fixtures = case_dir / "fixtures"
    if fixtures.exists():
        shutil.copytree(fixtures, sandbox / "fixtures", symlinks=True)
    return sandbox / "main.php"


def filesystem_snapshot(root: Path) -> dict[str, Any]:
    """Capture a binary-safe recursive filesystem snapshot below a sandbox."""
    entries: dict[str, Any] = {}
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root).as_posix()
        if relative in {"main.php", ".oracle-telemetry.json"}:
            continue
        metadata = path.lstat()
        mode = stat.S_IMODE(metadata.st_mode)
        if path.is_symlink():
            entries[relative] = {
                "kind": "symlink",
                "mode": f"{mode:04o}",
                "target": os.readlink(path),
            }
        elif path.is_dir():
            entries[relative] = {"kind": "directory", "mode": f"{mode:04o}"}
        elif path.is_file():
            entries[relative] = {
                "kind": "file",
                "mode": f"{mode:04o}",
                "bytes": byte_record(path.read_bytes()),
            }
        else:
            entries[relative] = {"kind": "other", "mode": f"{mode:04o}"}
    return entries


def filesystem_diff(before: dict[str, Any], after: dict[str, Any]) -> dict[str, Any]:
    """Return created, deleted, and modified sandbox entries."""
    before_names = set(before)
    after_names = set(after)
    return {
        "created": {name: after[name] for name in sorted(after_names - before_names)},
        "deleted": {name: before[name] for name in sorted(before_names - after_names)},
        "modified": {
            name: {"before": before[name], "after": after[name]}
            for name in sorted(before_names & after_names)
            if before[name] != after[name]
        },
    }


def execute(
    command: list[str],
    cwd: Path,
    environment: dict[str, str],
    stdin: bytes,
    timeout_seconds: float,
) -> dict[str, Any]:
    """Run one process group and capture exact bytes, status, signal, or timeout."""
    process = subprocess.Popen(
        command,
        cwd=cwd,
        env=environment,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        start_new_session=True,
    )
    timed_out = False
    try:
        stdout, stderr = process.communicate(stdin, timeout=timeout_seconds)
    except subprocess.TimeoutExpired:
        timed_out = True
        os.killpg(process.pid, signal.SIGKILL)
        stdout, stderr = process.communicate()
    return_code = process.returncode
    return {
        "stdout": byte_record(stdout),
        "stderr": byte_record(stderr),
        "exit": {
            "code": return_code if return_code is not None and return_code >= 0 else None,
            "signal": -return_code if return_code is not None and return_code < 0 else None,
            "timeout": timed_out,
        },
    }


def run_raw(
    php_bin: Path,
    case: dict[str, Any],
    main: Path,
    sandbox: Path,
) -> tuple[dict[str, Any], dict[str, Any]]:
    """Execute the unmodified case; this run is authoritative for bytes and status."""
    ini = {**BASE_INI, **{str(k): str(v) for k, v in case.get("ini", {}).items()}}
    before = filesystem_snapshot(sandbox)
    command = [
        str(php_bin),
        *php_arguments(ini, "main.php", [str(value) for value in case.get("argv", [])]),
    ]
    result = execute(
        command,
        sandbox,
        process_environment(case),
        stdin_bytes(case),
        float(case.get("timeout_seconds", 10)),
    )
    after = filesystem_snapshot(sandbox)
    result["argv"] = ["php", *php_arguments(ini, "main.php", [str(value) for value in case.get("argv", [])])]
    return result, filesystem_diff(before, after)


def run_instrumented(
    php_bin: Path,
    case: dict[str, Any],
    main: Path,
    sandbox: Path,
) -> dict[str, Any]:
    """Execute the case through the telemetry bootstrap after a fresh reset."""
    telemetry_path = sandbox / ".oracle-telemetry.json"
    tools_dir = sandbox / ".oracle-tools"
    tools_dir.mkdir()
    sandbox_bootstrap = tools_dir / "bootstrap.php"
    shutil.copy2(BOOTSTRAP, sandbox_bootstrap)
    shutil.copy2(CANONICAL_VALUE, tools_dir / "canonical_value.php")
    environment = process_environment(case)
    environment["ELEPHC_ORACLE_TELEMETRY"] = str(telemetry_path)
    ini = {**BASE_INI, **{str(k): str(v) for k, v in case.get("ini", {}).items()}}
    command = [
        str(php_bin),
        *php_arguments(
            ini,
            str(sandbox_bootstrap),
            [str(main), *[str(value) for value in case.get("argv", [])]],
        ),
    ]
    result = execute(
        command,
        sandbox,
        environment,
        stdin_bytes(case),
        float(case.get("timeout_seconds", 10)),
    )
    result["argv"] = [
        "php",
        *php_arguments(
            ini,
            "tools/php_oracle/bootstrap.php",
            ["main.php", *[str(value) for value in case.get("argv", [])]],
        ),
    ]
    result["telemetry"] = (
        json.loads(telemetry_path.read_bytes()) if telemetry_path.exists() else None
    )
    return result


def case_source_digest(case_dir: Path) -> str:
    """Hash every versioned case input in relative-path/content order."""
    digest = hashlib.sha256()
    for path in sorted(item for item in case_dir.rglob("*") if item.is_file()):
        relative = path.relative_to(case_dir).as_posix().encode()
        content = path.read_bytes()
        digest.update(len(relative).to_bytes(8, "big"))
        digest.update(relative)
        digest.update(len(content).to_bytes(8, "big"))
        digest.update(content)
    return digest.hexdigest()


def output_path(profile: dict[str, Any], case_id: str) -> Path:
    """Return the specification-aligned oracle artifact path."""
    return (
        ORACLE_ROOT
        / f"php-{profile['php_release']}"
        / profile["target"]
        / profile["name"]
        / f"{case_id}.json"
    )


def build_artifact(args: argparse.Namespace) -> tuple[dict[str, Any], Path]:
    """Run both oracle modes and assemble one canonical artifact."""
    profile_bytes = args.profile_manifest.read_bytes()
    profile_manifest = json.loads(profile_bytes)
    profile = profile_manifest["profile"]
    php_bin = args.php_bin.resolve()
    if sha256_file(php_bin) != profile_manifest["oracle"]["php_binary_sha256"]:
        raise SystemExit("PHP binary SHA-256 does not match the selected profile")
    case_dir = args.case_dir.resolve()
    case = json.loads((case_dir / "case.json").read_bytes())
    if case.get("schema_version") != SCHEMA_VERSION:
        raise SystemExit("unsupported case schema_version")
    case_id = str(case["id"])
    sandbox = stable_sandbox(profile, case_id)
    main = prepare_sandbox(case_dir, sandbox)
    raw, side_effects = run_raw(php_bin, case, main, sandbox)
    main = prepare_sandbox(case_dir, sandbox)
    instrumented = run_instrumented(php_bin, case, main, sandbox)
    artifact = {
        "schema_version": SCHEMA_VERSION,
        "kind": "php-src-stream-oracle-artifact",
        "case": {
            "id": case_id,
            "source": case_dir.relative_to(ROOT).as_posix(),
            "source_sha256": case_source_digest(case_dir),
            "description": case.get("description", ""),
            "dependencies": case.get("dependencies", []),
            "normalization": case.get("normalization", []),
        },
        "profile": {
            "path": args.profile_manifest.resolve().relative_to(ROOT).as_posix(),
            "sha256": sha256_bytes(profile_bytes),
            "php_release": profile["php_release"],
            "php_src_commit": profile["php_src_commit"],
            "target": profile["target"],
            "name": profile["name"],
            "php_binary_sha256": profile_manifest["oracle"]["php_binary_sha256"],
        },
        "environment": {
            "base": {"LC_ALL": "C", "LANG": "C", "TZ": "UTC"},
            "case": case.get("environment", {}),
            "ini": {**BASE_INI, **case.get("ini", {})},
            "stdin": case.get("stdin"),
        },
        "raw": raw,
        "instrumented": instrumented,
        "filesystem_diff": side_effects,
    }
    return artifact, args.output or output_path(profile, case_id)


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
    """Capture or byte-verify one oracle artifact."""
    args = parse_args()
    artifact, output = build_artifact(args)
    content = canonical_bytes(artifact)
    if args.check:
        if not output.exists():
            print(f"missing oracle artifact: {output}", file=sys.stderr)
            return 1
        if output.read_bytes() != content:
            print(f"oracle drift: regenerate {output}", file=sys.stderr)
            return 1
        return 0
    atomic_write(output, content)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
