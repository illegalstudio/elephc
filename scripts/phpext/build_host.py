#!/usr/bin/env python3
"""Build a checksum-pinned real PHP CLI/embed SDK for producer lifecycle tests."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import urllib.request

VERSION = "8.5.11"
SHA256 = "d9be75c08e8c316f4c8f4194d8fbe1750a15f6a6d9d4e3fe72082abeeb800360"


def build(args):
    """Install an exclusively owned test SDK, cleaning source and partial installs on failure."""
    prefix = Path(args.prefix).resolve()
    identity = {"version": VERSION, "sha256": SHA256, "zts": args.zts, "debug": args.debug,
                "valgrind": args.with_valgrind}
    if prefix.exists():
        marker = prefix / "elephc-test-host.json"
        if marker.is_file() and json.loads(marker.read_text()) == identity:
            print(f"Using verified test SDK: {prefix}", flush=True)
            return
        raise SystemExit(f"Refusing to replace existing directory: {prefix}")
    prefix.parent.mkdir(parents=True, exist_ok=True)
    try:
        with tempfile.TemporaryDirectory(prefix="phpext-host-build-", dir=prefix.parent) as work:
            work = Path(work)
            archive = work / "php.tar.xz"
            print(f"Downloading checksum-pinned PHP {VERSION}", flush=True)
            urllib.request.urlretrieve(f"https://www.php.net/distributions/php-{VERSION}.tar.xz", archive)
            if hashlib.sha256(archive.read_bytes()).hexdigest() != SHA256:
                raise RuntimeError("PHP source checksum mismatch")
            with tarfile.open(archive) as source:
                source.extractall(work, filter="data")
            source = work / f"php-{VERSION}"
            configure = ["./configure", f"--prefix={prefix}", "--disable-all", "--enable-cli",
                         "--enable-embed=shared", "--disable-cgi", "--disable-phpdbg", "--without-pear"]
            if args.zts:
                configure.append("--enable-zts")
            if args.debug:
                configure.append("--enable-debug")
            if args.with_valgrind:
                configure.append("--with-valgrind")
            env = dict(os.environ, CFLAGS="-O1 -g")
            for name, command in [("configure", configure), ("build", ["make", f"-j{args.jobs}"]),
                                  ("install", ["make", "install"])]:
                print(f"PHP test SDK: {name}", flush=True)
                log = work / f"{name}.log"
                with log.open("w") as output:
                    result = subprocess.run(command, cwd=source, env=env, stdout=output, stderr=subprocess.STDOUT)
                if result.returncode:
                    raise RuntimeError(f"PHP {name} failed:\n" + "\n".join(log.read_text().splitlines()[-80:]))
            (prefix / "elephc-test-host.json").write_text(json.dumps(identity, sort_keys=True) + "\n")
    except BaseException:
        if prefix.exists():
            shutil.rmtree(prefix)
        raise
    print(f"Installed real PHP test SDK: {prefix}", flush=True)


def main():
    """Parse the explicit destination and host ABI configuration."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", required=True)
    parser.add_argument("--zts", action="store_true")
    parser.add_argument("--debug", action="store_true")
    parser.add_argument("--with-valgrind", action="store_true")
    parser.add_argument("--jobs", type=int, default=4)
    build(parser.parse_args())


if __name__ == "__main__":
    main()
