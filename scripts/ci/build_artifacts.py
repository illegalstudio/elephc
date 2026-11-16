#!/usr/bin/env python3
"""Package and validate immutable CI outputs before consumers execute them."""

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tarfile

from build_environment import snapshot, validate as validate_environment


SCHEMA = 1
HOSTS = ("linux-x86_64", "linux-aarch64", "macos-aarch64")


def digest(path):
    """Hash large outputs without loading an archive into memory."""
    with path.open("rb") as stream:
        return stream_digest(stream)


def stream_digest(stream):
    """Support the Python 3.10 shipped by the release Linux runners."""
    result = hashlib.sha256()
    for chunk in iter(lambda: stream.read(1024 * 1024), b""):
        result.update(chunk)
    return result.hexdigest()


def relative_file(name):
    """Accept only canonical relative POSIX filenames."""
    path = PurePosixPath(name)
    if not name or path.is_absolute() or str(path) != name or ".." in path.parts:
        raise ValueError(f"Unsafe artifact path: {name!r}")
    return path


def checked_destination(root, name):
    """Reject existing symlinks, including parent symlinks, before writing."""
    destination = root.joinpath(*relative_file(name).parts)
    for path in (destination, *destination.parents):
        if path == root:
            break
        if path.is_symlink():
            raise ValueError(f"Artifact destination contains a symlink: {path}")
    if root not in destination.resolve().parents:
        raise ValueError(f"Artifact path escapes destination: {name}")
    return destination


def identity(sha, target, profile, features, kind, repository):
    """Describe a specific output configuration with an immutable source SHA."""
    if not re.fullmatch(r"[0-9a-f]{40}", sha):
        raise ValueError("Artifacts require a full lowercase source SHA")
    if target not in (*HOSTS, "ios-arm64", "ios-sim-arm64"):
        raise ValueError(f"Unsupported target: {target}")
    if profile not in ("debug", "release"):
        raise ValueError(f"Unsupported profile: {profile}")
    if kind not in ("compiler", "bridges", "tests", "tools", "ios", "development"):
        raise ValueError(f"Unsupported artifact kind: {kind}")
    return dict(sha=sha, target=target, profile=profile,
                features=sorted(set(filter(None, features.split(",")))),
                kind=kind, repository=repository)


def create(root, output, names, expected, producer):
    """Write a payload archive and a manifest after validating every input."""
    root = root.resolve()
    if output.exists():
        raise ValueError(f"Refusing to overwrite artifact directory: {output}")
    files = {}
    for name in sorted(set(names)):
        path = checked_destination(root, name)
        if not path.is_file() or path.stat().st_size == 0:
            raise ValueError(f"Required build output is missing or empty: {name}")
        files[name] = {"sha256": digest(path), "size": path.stat().st_size,
                       "mode": 0o755 if os.access(path, os.X_OK) else 0o644}
    if not files:
        raise ValueError("An empty build artifact cannot satisfy a producer")
    output.mkdir(parents=True)
    try:
        archive = output / "payload.tar.gz"
        with tarfile.open(archive, "w:gz", compresslevel=1) as bundle:
            for name, metadata in files.items():
                info = bundle.gettarinfo(str(root / name), arcname=name)
                info.uid = info.gid = 0
                info.uname = info.gname = ""
                info.mode = metadata["mode"]
                with (root / name).open("rb") as stream:
                    bundle.addfile(info, stream)
        manifest = dict(schema=SCHEMA, identity=expected, producer=producer,
                        archive_sha256=digest(archive), files=files)
        (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    except BaseException:
        shutil.rmtree(output)
        raise


def verify(artifact, expected, run_id=None, attempt=None):
    """Check configuration, provenance and all bytes before extracting anything."""
    manifest = json.loads((artifact / "manifest.json").read_text())
    if manifest.get("schema") != SCHEMA or manifest.get("identity") != expected:
        raise ValueError("Artifact schema or build identity does not match the consumer")
    producer = manifest.get("producer", {})
    if run_id is not None and str(producer.get("run_id")) != str(run_id):
        raise ValueError("Artifact belongs to a different producer run")
    if attempt is not None and str(producer.get("attempt")) != str(attempt):
        raise ValueError("Artifact belongs to a different producer attempt")
    archive = artifact / "payload.tar.gz"
    if digest(archive) != manifest["archive_sha256"]:
        raise ValueError("Artifact archive checksum mismatch")
    files = manifest.get("files", {})
    if not files:
        raise ValueError("Artifact manifest contains no outputs")
    seen = set()
    with tarfile.open(archive, "r:gz") as bundle:
        for member in bundle:
            relative_file(member.name)
            if not member.isfile() or member.name in seen or member.name not in files:
                raise ValueError(f"Unexpected artifact member: {member.name}")
            seen.add(member.name)
            metadata = files[member.name]
            if member.size != metadata["size"] or member.mode != metadata["mode"]:
                raise ValueError(f"Artifact metadata mismatch: {member.name}")
            if member.mode not in (0o644, 0o755):
                raise ValueError(f"Unsafe artifact mode: {member.name}")
            with bundle.extractfile(member) as stream:
                actual = stream_digest(stream)
            if actual != metadata["sha256"]:
                raise ValueError(f"Artifact file checksum mismatch: {member.name}")
    if seen != set(files):
        raise ValueError("Artifact is incomplete")
    return manifest


def restore(artifact, destination, expected, run_id=None, attempt=None):
    """Extract validated regular files while preserving executable permissions."""
    manifest = verify(artifact, expected, run_id, attempt)
    destination.mkdir(parents=True, exist_ok=True)
    destination = destination.resolve()
    paths = {name: checked_destination(destination, name) for name in manifest["files"]}
    with tarfile.open(artifact / "payload.tar.gz", "r:gz") as bundle:
        for member in bundle:
            path = paths[member.name]
            path.parent.mkdir(parents=True, exist_ok=True)
            with bundle.extractfile(member) as source, path.open("wb") as output:
                shutil.copyfileobj(source, output)
            path.chmod(member.mode)


def host_files(root, profile, kind):
    """Select complete host outputs without uploading unrelated Cargo state."""
    base = Path("target") / profile
    if kind == "compiler":
        return [str(base / "elephc")]
    if kind == "bridges":
        catalog = (root / "src/linker/bridges.rs").read_text()
        libraries = re.findall(r'lib_name: "(elephc_[a-z_]+)"', catalog)
        if not libraries:
            raise ValueError("No bridge archive inventory found")
        names = {str(base / f"lib{name}.a") for name in libraries}
        names.add(str(base / "libelephc_magician_curl.a"))
        names.add(str(base / "elephc-pdo-features.txt"))
        return sorted(names)
    raise ValueError("Specify --file for non-host payloads")


def bridge_environment(root, profile, target=None, artifact_root=None):
    """Pin CLI bridge lookup so checkout timestamps cannot trigger Cargo builds."""
    triple = {"ios-arm64": "aarch64-apple-ios", "ios-sim-arm64": "aarch64-apple-ios-sim"}.get(target)
    artifacts = artifact_root or root
    directory = ((artifacts / "target" / triple if triple else artifacts / "target") / profile).resolve()
    if "\n" in str(directory) or "\r" in str(directory):
        raise ValueError("Invalid bridge directory")
    catalog = (root / "src/linker/bridges.rs").read_text()
    variables = sorted(set(re.findall(r'env_var: "(ELEPHC_[A-Z_]+)"', catalog)))
    if not variables:
        raise ValueError("No bridge environment inventory found")
    return "".join(f"{name}={directory}\n" for name in variables)


def checkout_identity(root):
    """Bind outputs to tracked source content, including deliberate version stamping."""
    git = ["git", "-c", f"safe.directory={root.resolve()}"]
    tree = subprocess.check_output(git + ["rev-parse", "HEAD^{tree}"], cwd=root, text=True).strip()
    changes = subprocess.check_output(git + ["diff", "--no-ext-diff", "--binary", "HEAD", "--"], cwd=root)
    return {"tree": tree, "changes_sha256": hashlib.sha256(changes).hexdigest(),
            "lock_sha256": digest(root / "Cargo.lock"), "rust_toolchain": "1.98"}


def main():
    """Expose the same configuration contract to producer and consumer jobs."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("create", "verify", "restore"))
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--artifact", type=Path, required=True)
    parser.add_argument("--sha", required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--profile", default="debug")
    parser.add_argument("--features", default="")
    parser.add_argument("--kind", required=True)
    parser.add_argument("--repository", default=os.environ.get("GITHUB_REPOSITORY", ""))
    parser.add_argument("--run-id", default=os.environ.get("GITHUB_RUN_ID"))
    parser.add_argument("--attempt", default=os.environ.get("GITHUB_RUN_ATTEMPT"))
    parser.add_argument("--file", action="append", default=[])
    parser.add_argument("--github-env", type=Path)
    args = parser.parse_args()
    expected = identity(args.sha, args.target, args.profile, args.features, args.kind, args.repository)
    expected["source"] = checkout_identity(args.root)
    if args.command == "create":
        actual_sha = subprocess.check_output(["git", "-c", f"safe.directory={args.root.resolve()}", "rev-parse", "HEAD"], cwd=args.root, text=True).strip()
        if actual_sha != args.sha:
            raise ValueError("Checked-out source does not match the requested artifact SHA")
        producer = {"run_id": args.run_id, "attempt": args.attempt,
                    "workflow": os.environ.get("GITHUB_WORKFLOW_REF", ""),
                    "event": os.environ.get("GITHUB_EVENT_NAME", ""),
                    "rustc": subprocess.check_output(["rustc", "-vV"], text=True).strip(),
                    "image": os.environ.get("ImageVersion", ""),
                    "lock_sha256": digest(args.root / "Cargo.lock"),
                    "environment": snapshot(args.target)}
        create(args.root, args.artifact, args.file or host_files(args.root, args.profile, args.kind),
               expected, producer)
    elif args.command == "verify":
        validate_environment(json.loads((args.artifact / "manifest.json").read_text())["producer"]["environment"], args.target)
        verify(args.artifact, expected, args.run_id, args.attempt)
    else:
        validate_environment(json.loads((args.artifact / "manifest.json").read_text())["producer"]["environment"], args.target)
        restore(args.artifact, args.root, expected, args.run_id, args.attempt)
        if args.kind in ("bridges", "ios") and args.github_env:
            with args.github_env.open("a") as output:
                output.write(bridge_environment(args.root, args.profile, args.target))
    print(f"{args.command}: {args.kind} {args.target} {args.profile} {args.sha}")


if __name__ == "__main__":
    main()
