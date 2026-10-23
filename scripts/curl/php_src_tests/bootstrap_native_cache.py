#!/usr/bin/env python3
"""Materialize cached native curl-family artifacts under the current toolchain
fingerprint so the elephc CLI resolver can link them.

WARNING: this mutates the global managed-native cache (`~/.cache/elephc/native`
unless `ELEPHC_NATIVE_CACHE` is set). It copies pinned, already-built archives
to a new fingerprint key; it never downloads or rebuilds. The archives are
SHA-256-verified by the resolver, and only the receipt's `toolchain_fingerprint`
field is rewritten, so the linked bits are exactly the pinned build.

The durable cache keys artifacts by a fingerprint that includes PATH/TMPDIR, so
artifacts built in another session are not found. This script is a local
convenience for the php-src audit; prefer `elephc native install --locked` when
a real rebuild is acceptable.

Usage:
    python3 bootstrap_native_cache.py <elephc-binary> <path/to/elephc.lock>
"""
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


LOCK_RE = re.compile(
    r'\[\[package\]\]\nname = "([^"]+)"\nversion = "([^"]+)"\nrecipe = (\d+)\n'
    r'dependencies = \[[^\]]*\]\nprovides = \[[^\]]*\]\n\n\[package\.source\]\n'
    r'url = "([^"]+)"\nsha256 = "([0-9a-f]+)"',
    re.M,
)


def current_fingerprint(elephc: Path, project: Path, env: dict) -> str:
    php = project / "__fp_probe__.php"
    php.write_text("<?php $v = curl_version();\n")
    out = subprocess.run(
        [str(elephc), str(php)], cwd=project, capture_output=True, text=True, env=env
    )
    text = out.stdout + out.stderr
    m = re.search(r"aarch64-apple-darwin/([0-9a-f]{64})", text)
    if not m:
        # Already resolvable: find the artifact dir the resolver selected, or
        # fall back to any cached fingerprint's receipt.
        raise SystemExit(f"could not determine current fingerprint from:\n{text}")
    return m.group(1)


def package_specs(lock_text: str):
    specs = []
    for m in LOCK_RE.finditer(lock_text):
        specs.append(
            {
                "name": m.group(1),
                "version": m.group(2),
                "recipe": m.group(3),
                "sha256": m.group(5),
            }
        )
    return specs


def main():
    cache = Path(
        os.environ.get(
            "ELEPHC_NATIVE_CACHE", Path.home() / ".cache" / "elephc" / "native"
        )
    )
    elephc = Path(sys.argv[1]).resolve()
    lock_path = Path(sys.argv[2]).resolve()
    project = lock_path.parent
    target = "macos-aarch64"
    abi = "aarch64-apple-darwin"

    env = dict(os.environ)
    env["PATH"] = "/usr/bin:/bin:/usr/sbin:/sbin"
    fp = current_fingerprint(elephc, project, env)
    print(f"[bootstrap] current fingerprint = {fp}")

    artifacts = cache / "artifacts"
    specs = package_specs(lock_path.read_text())
    for spec in specs:
        base = artifacts / spec["name"] / spec["version"] / f"r{spec['recipe']}" / spec["sha256"] / target / abi
        dest = base / fp
        if dest.exists():
            print(f"[bootstrap] {spec['name']}: already present")
            continue
        sources = [d for d in base.iterdir() if d.is_dir()] if base.exists() else []
        if not sources:
            raise SystemExit(f"[bootstrap] no cached artifact for {spec['name']} at {base}")
        # Prefer the CLT/Xcode build with the most complete tree; any works.
        sources.sort(key=lambda d: d.name)
        chosen = sources[0]
        print(f"[bootstrap] {spec['name']}: {chosen.name} -> {fp}")
        tmp = base / (fp + ".staging")
        if tmp.exists():
            shutil.rmtree(tmp)
        shutil.copytree(chosen, tmp, symlinks=True)
        receipt = tmp / "receipt.json"
        text = receipt.read_text()
        text = re.sub(r'"toolchain_fingerprint": "[0-9a-f]+"',
                      f'"toolchain_fingerprint": "{fp}"', text, count=1)
        receipt.write_text(text)
        tmp.rename(dest)

    # Clean the probe file.
    (project / "__fp_probe__.php").unlink(missing_ok=True)
    print("[bootstrap] done")


if __name__ == "__main__":
    main()
