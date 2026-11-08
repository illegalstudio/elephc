#!/usr/bin/env python3
"""Create downloadable development packages from verified CI compiler and bridge outputs."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile

from build_artifacts import digest, host_files

NOTICES = {
    "LICENSE": "LICENSE",
    "crates/elephc-fileinfo/NOTICE.md": "fileinfo-notices/NOTICE.md",
    "crates/elephc-mbstring/NOTICE.md": "mbstring-notices/NOTICE.md",
    "crates/elephc-mbstring/src/unicode/data/LICENSE-UNICODE": "mbstring-notices/LICENSE-UNICODE",
    "crates/elephc-mbstring/src/detect/data/LICENSE-PHP": "mbstring-notices/LICENSE-PHP",
}


def package(root, output, target, sha, compiler_manifest, bridge_manifest):
    """Keep the install layout complete and include its original build provenance."""
    output.mkdir(parents=True, exist_ok=True)
    tarball = output / f"elephc-dev-{target}-{sha}.tar.gz"
    files = {name: Path(name).name for kind in ("compiler", "bridges")
             for name in host_files(root, "debug", kind)}
    files.update(NOTICES)
    for path in files:
        if not (root / path).is_file():
            raise ValueError(f"Incomplete development package: {path}")
    provenance = {"schema": 1, "sha": sha, "target": target, "profile": "debug",
                  "compiler": json.loads(compiler_manifest.read_text()),
                  "bridges": json.loads(bridge_manifest.read_text()),
                  "files": {name: digest(root / path) for path, name in files.items()}}
    with tempfile.TemporaryDirectory(prefix="elephc-development-") as temporary:
        staging = Path(temporary)
        for path, name in files.items():
            destination = staging / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(root / path, destination)
        (staging / "build-manifest.json").write_text(json.dumps(provenance, indent=2) + "\n")
        (staging / "README.txt").write_text(
            f"Elephc development build {sha} ({target}, debug profile).\n"
            "This Actions artifact is not a versioned release.\n"
            "Keep elephc and the bridge .a files together when installing.\n"
            "Linux builds require the Ubuntu 24.04/glibc 2.39 generation or newer.\n"
            "Building programs requires the platform assembler and C linker.\n"
            "macOS builds are unsigned development binaries.\n")
        with tarfile.open(tarball, "w:gz", compresslevel=1) as bundle:
            for path in sorted(staging.rglob("*")):
                if path.is_file():
                    bundle.add(path, arcname=str(path.relative_to(staging)), recursive=False)
    tarball.with_suffix(tarball.suffix + ".sha256").write_text(f"{digest(tarball)}  {tarball.name}\n")
    return tarball


def smoke(tarball):
    """Compile and link with a packaged bridge entirely outside the source checkout."""
    with tempfile.TemporaryDirectory(prefix="elephc-development-smoke-") as temporary:
        root = Path(temporary)
        with tarfile.open(tarball, "r:gz") as bundle:
            # This archive was just created from the validated file inventory above.
            bundle.extractall(root, filter="data")
        compiler = root / "elephc"
        environment = {key: value for key, value in os.environ.items()
                       if not (key.startswith("ELEPHC_") and key.endswith("_LIB_DIR"))
                       and key != "CARGO_TARGET_DIR"}
        capabilities = subprocess.check_output([str(compiler), "--print-capabilities"], text=True, env=environment)
        for line in capabilities.splitlines():
            for field in line.split("\t"):
                if field.endswith(".a") and not (root / field).is_file():
                    raise ValueError(f"Packaged capability is missing {field}")
        (root / "probe.php").write_text("<?php echo hash('sha256', 'artifact');")
        subprocess.run([str(compiler), "--with-crypto", "probe.php"], cwd=root, env=environment, check=True)
        actual = subprocess.check_output([str(root / "probe")], cwd=root, text=True, env=environment)
        if actual != hashlib.sha256(b"artifact").hexdigest():
            raise ValueError("Development package compile/link probe produced the wrong output")


def main():
    """Package only after both build manifests have been validated by the restore action."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True)
    parser.add_argument("--sha", required=True)
    parser.add_argument("--compiler-manifest", type=Path, required=True)
    parser.add_argument("--bridge-manifest", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=Path("target/development"))
    args = parser.parse_args()
    tarball = package(Path.cwd(), args.output, args.target, args.sha,
                      args.compiler_manifest, args.bridge_manifest)
    smoke(tarball)
    print(tarball)


if __name__ == "__main__":
    main()
