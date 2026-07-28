#!/usr/bin/env python3
"""Verify that bridge C ABI exports survive in a built static archive.

The source scan establishes the intended ``#[no_mangle] extern \"C\"`` surface;
``nm`` establishes what the linker can actually resolve from the archive.  This
is especially useful for PE/COFF cross-builds, where successfully compiling a
Rust crate alone does not prove that all bridge entry points reached the final
archive.
"""

from __future__ import annotations

import argparse
import re
import shutil
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BRIDGES = ("image", "pdo", "phar", "crypto", "tz", "tls", "web")
EXPORT_RE = re.compile(
    r'#\s*\[\s*no_mangle\s*\]\s*'
    r'pub\s+(?:unsafe\s+)?extern\s+"C"\s+fn\s+(elephc_[A-Za-z0-9_]+)'
)
NM_SYMBOL_RE = re.compile(r"^_?(elephc_[A-Za-z0-9_]+)$")


def source_exports(source_dir: Path) -> set[str]:
    """Return the no-mangle elephc C ABI functions declared below source_dir."""
    exports: set[str] = set()
    for path in sorted(source_dir.rglob("*.rs")):
        exports.update(EXPORT_RE.findall(path.read_text(encoding="utf-8")))
    return exports


def archive_exports(nm: str, archive: Path) -> set[str]:
    """Return elephc symbols defined globally by archive according to nm."""
    result = subprocess.run(
        [nm, "-g", "--defined-only", str(archive)],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        sys.stderr.write(result.stderr)
        raise RuntimeError(f"{nm} failed for {archive} with status {result.returncode}")

    exports: set[str] = set()
    for line in result.stdout.splitlines():
        if not line.split():
            continue
        match = NM_SYMBOL_RE.match(line.split()[-1])
        if match:
            exports.add(match.group(1))
    return exports


def default_nm(target: str) -> str:
    """Choose an nm implementation appropriate for target, if available."""
    candidates = []
    if target:
        candidates.append(f"{target}-nm")
    if target == "x86_64-pc-windows-gnu":
        candidates.append("x86_64-w64-mingw32-nm")
    candidates.extend(("llvm-nm", "nm"))
    for candidate in candidates:
        resolved = shutil.which(candidate)
        if resolved:
            return resolved
    raise RuntimeError("no nm implementation found; pass --nm explicitly")


def archive_path(target_dir: Path, target: str, profile: str, bridge: str) -> Path:
    """Resolve Cargo's staticlib archive path for one bridge build."""
    base = target_dir / target if target else target_dir
    directory = base / profile
    candidates = (
        directory / f"libelephc_{bridge}.a",
        directory / f"elephc_{bridge}.lib",
        directory / f"libelephc_{bridge}.lib",
    )
    return next((path for path in candidates if path.is_file()), candidates[0])


def check_bridge(bridge: str, target_dir: Path, target: str, profile: str, nm: str) -> bool:
    """Compare one bridge's source ABI with its built archive, printing drift."""
    source_dir = ROOT / "crates" / f"elephc-{bridge}" / "src"
    archive = archive_path(target_dir, target, profile, bridge)
    if not archive.is_file():
        print(f"ERROR [{bridge}]: archive not found: {archive}", file=sys.stderr)
        return False

    intended = source_exports(source_dir)
    if not intended:
        print(f"ERROR [{bridge}]: no C ABI exports found below {source_dir}", file=sys.stderr)
        return False
    actual = archive_exports(nm, archive)
    missing = sorted(intended - actual)
    unexpected = sorted(actual - intended)

    for symbol in missing:
        print(f"ERROR [{bridge}]: archive is missing {symbol}", file=sys.stderr)
    for symbol in unexpected:
        print(f"ERROR [{bridge}]: archive has undeclared export {symbol}", file=sys.stderr)
    print(
        f"[{bridge}] source={len(intended)} archive={len(actual)} "
        f"missing={len(missing)} unexpected={len(unexpected)}"
    )
    return not missing and not unexpected


def parse_args() -> argparse.Namespace:
    """Parse command-line options for archive location and bridge selection."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", default="", help="Cargo target triple")
    parser.add_argument("--profile", default="debug", help="Cargo profile directory")
    parser.add_argument("--target-dir", type=Path, default=ROOT / "target")
    parser.add_argument("--nm", help="nm-compatible executable")
    parser.add_argument("--bridge", action="append", choices=BRIDGES, dest="bridges")
    return parser.parse_args()


def main() -> int:
    """Check the requested bridge archives and return nonzero on any ABI drift."""
    args = parse_args()
    try:
        nm = args.nm or default_nm(args.target)
        results = [
            check_bridge(bridge, args.target_dir, args.target, args.profile, nm)
            for bridge in (args.bridges or BRIDGES)
        ]
        ok = all(results)
    except (OSError, RuntimeError) as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return 1
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
