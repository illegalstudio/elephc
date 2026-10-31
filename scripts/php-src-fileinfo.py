#!/usr/bin/env python3
"""Replays php-src `ext/fileinfo/tests` through elephc and compares PHP's expectations.

Usage:
    scripts/php-src-fileinfo.py --php-src /path/to/php-src [--filter NAME]
                                [--elephc target/debug/elephc]
                                [--manifest-dir examples/fileinfo] [--jobs N]

The runner extracts each fixture's `--FILE--`, evaluates `--SKIPIF--` with the host
`php`, compiles the fixture with elephc, runs the produced binary with stdout and
stderr merged, and compares the result using php-src's `EXPECT`, `EXPECTF`, and
`EXPECTREGEX` rules. `--ENV--`, `--INI--`, `--CLEAN--`, and `--FILE_EXTERNAL--` are
honoured.

Two fixture quirks are normalized without changing the PHP program's behavior:

- elephc's AOT entry file accepts one `<?php` block, so closing tags and their inline
  output are rewritten to equivalent `echo` statements.
- a fixture that creates a directory named after itself would collide with the
  compiler's default binary name, so the binary is renamed to `program` before it runs.

`--EXTENSIONS--` follows php-src's rule: a fixture naming an extension elephc does not
provide is skipped. `intl` is left to the fixture's own `SKIPIF` because the one
fileinfo test that names it is gated on an installed locale, not on the extension API.

Requirements: a php-src checkout (the pinned PHP 8.5.10 tree is the compliance target),
a host `php` interpreter for `--SKIPIF--`, and a built `elephc` with the managed
`libmagic` and `pcre2` artifacts (`elephc native add libmagic` installs both).
"""

from __future__ import annotations

import argparse
import concurrent.futures
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

SECTION_RE = re.compile(r"^--([A-Z_]+)--$", re.MULTILINE)

# php-src extensions for which elephc always has the core runtime surface. `zip` is
# listed because fileinfo's read-only `zip://` wrapper handles the one fixture that
# names it; `intl` is intentionally absent so that fixture's SKIPIF decides.
CORE_EXTENSIONS = {
    "core", "standard", "spl", "pcre", "date", "json", "hash", "random",
    "reflection", "filter", "zlib", "session", "ctype", "posix", "pcntl", "zip",
    # The one fileinfo fixture that names intl is gated by its own SKIPIF on an installed
    # ru_RU.koi8r locale; let that environmental decision show through instead of hiding it.
    "intl",
}


def parse_phpt(text: str) -> dict[str, str]:
    """Splits a `.phpt` file into its named sections."""
    matches = list(SECTION_RE.finditer(text))
    sections: dict[str, str] = {}
    for index, match in enumerate(matches):
        start = match.end()
        if text[start:start + 1] == "\n":
            start += 1
        end = matches[index + 1].start() if index + 1 < len(matches) else len(text)
        sections[match.group(1)] = text[start:end]
    return sections


def expectf_to_regex(wanted: str) -> str:
    """Converts a php-src `EXPECTF` section into an anchored regular expression."""
    wanted = wanted.replace("\r\n", "\n")
    temp = ""
    start_offset = 0
    length = len(wanted)
    while start_offset < length:
        start = wanted.find("%r", start_offset)
        if start != -1:
            end = wanted.find("%r", start + 2)
            if end == -1:
                end = start = length
        else:
            start = end = length
        temp += re.escape(wanted[start_offset:start])
        if end > start:
            temp += "(" + wanted[start + 2:end] + ")"
        start_offset = end + 2
    replacements = {
        "%e": re.escape("/"),
        "%s": r"[^\r\n]+",
        "%S": r"[^\r\n]*",
        "%a": r".+?",
        "%A": r".*?",
        "%w": r"\s*",
        "%i": r"[+-]?\d+",
        "%d": r"\d+",
        "%x": r"[0-9a-fA-F]+",
        "%f": r"[+-]?(?:\d+|(?=\.\d))(?:\.\d+)?(?:[Ee][+-]?\d+)?",
        "%c": r".",
        "%0": r"\x00",
    }
    for key, value in replacements.items():
        temp = temp.replace(re.escape(key), value)
    return temp


def php_single_quote(text: str) -> str:
    """Encodes literal inline output as a PHP single-quoted string."""
    return "'" + text.replace("\\", "\\\\").replace("'", "\\'") + "'"


def normalize_tags(source: str) -> str:
    """Rewrites closing tags and inline HTML into equivalent `echo` statements.

    PHP swallows the newline directly after `?>`; the rewrite preserves that rule.
    """
    if "?>" not in source:
        return source
    out: list[str] = []
    pos = 0
    while True:
        close = source.find("?>", pos)
        if close == -1:
            out.append(source[pos:])
            break
        out.append(source[pos:close])
        after = close + 2
        if source[after:after + 1] == "\n":
            after += 1
        elif source[after:after + 2] == "\r\n":
            after += 2
        next_open = source.find("<?php", after)
        text_end = len(source) if next_open == -1 else next_open
        literal = source[after:text_end]
        if literal:
            out.append("echo " + php_single_quote(literal) + ";")
        if next_open == -1:
            break
        pos = next_open
    return "".join(out)


def ini_args(ini: str) -> list[str]:
    """Turns an `--INI--` section into `php -d` arguments."""
    args = []
    for line in ini.splitlines():
        line = line.strip()
        if not line or line.startswith(";"):
            continue
        key, _, value = line.partition("=")
        args.extend(["-d", f"{key.strip()}={value.strip()}"])
    return args


def env_from(text: str) -> dict[str, str]:
    """Parses an `--ENV--` section into environment variables."""
    env = {}
    for line in text.splitlines():
        line = line.strip()
        if not line:
            continue
        key, _, value = line.partition("=")
        env[key.strip()] = value.strip()
    return env


def prepare(test: Path, root: Path, work: Path, manifest_dir: Path) -> None:
    """Builds one isolated work directory with symlinks to every sibling asset."""
    work.mkdir(parents=True, exist_ok=True)
    for entry in root.iterdir():
        if entry == test or entry.suffix == ".phpt":
            continue
        target = work / entry.name
        if not target.exists():
            os.symlink(entry, target)
    for name in ("elephc.toml", "elephc.lock"):
        source = manifest_dir / name
        if source.exists():
            shutil.copyfile(source, work / name)


def missing_extension(sections: dict[str, str], available: set[str]) -> str | None:
    """Returns the first `--EXTENSIONS--` name elephc cannot provide, if any."""
    if "EXTENSIONS" not in sections:
        return None
    for name in sections["EXTENSIONS"].split():
        lowered = name.lower()
        if lowered in CORE_EXTENSIONS or lowered in available:
            continue
        return lowered
    return None


def run_one(test: Path, root: Path, elephc: str, php: str, outdir: Path,
            available: set[str], manifest_dir: Path) -> dict:
    """Compiles and runs one fixture, returning a result dictionary."""
    sections = parse_phpt(test.read_text(encoding="utf-8", errors="surrogateescape"))
    name = test.stem
    result = {"name": name, "status": "FAIL", "reason": "", "expected": "", "actual": ""}

    if "FILE" not in sections and "FILE_EXTERNAL" not in sections:
        result.update(status="SKIP", reason="no FILE section")
        return result
    absent = missing_extension(sections, available)
    if absent is not None:
        result.update(status="SKIP", reason=f"extension not available: {absent}")
        return result

    work = Path(tempfile.mkdtemp(prefix=f"phpt-{name[:40]}-"))
    try:
        prepare(test, root, work, manifest_dir)
        env = dict(os.environ)
        env.pop("PHP_INI_SCAN_DIR", None)
        if "ENV" in sections:
            env.update(env_from(sections["ENV"]))
        php_d = ini_args(sections.get("INI", ""))

        if "SKIPIF" in sections:
            skipif = work / "skipif.php"
            skipif.write_text(sections["SKIPIF"])
            proc = subprocess.run(
                [php, *php_d, str(skipif)],
                cwd=work, env=env, capture_output=True, text=True, timeout=120,
            )
            combined = (proc.stdout + proc.stderr).strip()
            if re.match(r"^skip", combined, re.IGNORECASE):
                result.update(status="SKIP", reason=combined.splitlines()[0] if combined else "skip")
                return result

        if "FILE_EXTERNAL" in sections:
            source = (root / sections["FILE_EXTERNAL"].strip()).read_text(
                encoding="utf-8", errors="surrogateescape")
        else:
            source = sections["FILE"]
        source = normalize_tags(source.replace("\r\n", "\n"))
        php_file = work / f"{name}.php"
        php_file.write_text(source)

        compile_cmd = [elephc, str(php_file)]
        for ini in php_d:
            if ini != "-d":
                compile_cmd.extend(["--ini", ini])
        proc = subprocess.run(
            compile_cmd, cwd=work, env=env, capture_output=True, text=True, timeout=600,
        )
        if proc.returncode != 0:
            result.update(status="FAIL", reason="compile error",
                          actual=(proc.stdout + proc.stderr).strip())
            return result
        binary = work / name
        if not binary.exists():
            result.update(status="FAIL", reason="no binary produced",
                          actual=(proc.stdout + proc.stderr).strip())
            return result
        # Keep the source stem (EXPECTF names `<test>.php`) but run under a neutral binary
        # name: several fixtures create a directory named after the test.
        program = work / "program"
        os.replace(binary, program)
        try:
            run = subprocess.run(
                [str(program)], cwd=work, env=env, stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT, timeout=120,
            )
        except subprocess.TimeoutExpired:
            result.update(status="FAIL", reason="timeout")
            return result
        actual = re.sub(r"\r\n", "\n", run.stdout.decode("utf-8", "surrogateescape")).strip()

        if "CLEAN" in sections:
            clean = work / "clean.php"
            clean.write_text(sections["CLEAN"])
            subprocess.run([php, str(clean)], cwd=work, env=env, capture_output=True, timeout=60)

        if "EXPECT" in sections:
            expected = re.sub(r"\r\n", "\n", sections["EXPECT"]).strip()
            if actual == expected:
                result.update(status="PASS")
            else:
                result.update(expected=expected, actual=actual)
        elif "EXPECTF" in sections:
            expected = sections["EXPECTF"].strip()
            if re.match(r"^" + expectf_to_regex(expected) + r"$", actual, re.S):
                result.update(status="PASS")
            else:
                result.update(expected=expected, actual=actual)
        elif "EXPECTREGEX" in sections:
            expected = sections["EXPECTREGEX"].strip()
            if re.match(r"^" + expected + r"$", actual, re.S):
                result.update(status="PASS")
            else:
                result.update(expected=expected, actual=actual)
        else:
            result.update(status="SKIP", reason="no expectation section")
        return result
    except Exception as exc:  # noqa: BLE001
        result.update(status="FAIL", reason=f"harness error: {exc}")
        return result
    finally:
        shutil.rmtree(work, ignore_errors=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--php-src", required=True, type=Path,
                        help="php-src checkout or its ext/fileinfo/tests directory")
    parser.add_argument("--elephc", default="target/debug/elephc")
    parser.add_argument("--php", default="php")
    parser.add_argument("--filter", default="")
    parser.add_argument("--jobs", type=int, default=4)
    parser.add_argument("--manifest-dir", type=Path, default=Path("examples/fileinfo"),
                        help="directory holding the libmagic/pcre2 elephc.toml and lock")
    parser.add_argument("--outdir", type=Path, default=Path("phpt-results"))
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()

    root = args.php_src / "ext" / "fileinfo" / "tests"
    if not root.is_dir():
        root = args.php_src
    tests = sorted(root.glob("*.phpt"))
    if args.filter:
        tests = [test for test in tests if args.filter in test.name]
    elephc = str(Path(args.elephc).resolve())
    args.outdir.mkdir(parents=True, exist_ok=True)

    capabilities = subprocess.run(
        [elephc, "--print-capabilities"], capture_output=True, text=True, check=False,
    )
    available = {line.split("\t")[1].lower() for line in capabilities.stdout.splitlines()
                 if line.startswith("bridge\t") or line.startswith("capability\t")}

    results = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        futures = [pool.submit(run_one, test, root, elephc, args.php, args.outdir,
                               available, args.manifest_dir) for test in tests]
        for future in concurrent.futures.as_completed(futures):
            results.append(future.result())
    results.sort(key=lambda result: result["name"])

    for result in results:
        line = f"{result['status']:5} {result['name']}"
        if result["status"] != "PASS":
            line += f"  ({result['reason'] or 'output mismatch'})"
        print(line)
        if result["status"] == "FAIL":
            (args.outdir / f"{result['name']}.out").write_text(
                result.get("actual", ""), errors="surrogateescape")
            (args.outdir / f"{result['name']}.exp").write_text(
                result.get("expected", ""), errors="surrogateescape")
            if args.verbose or result.get("reason") == "compile error":
                print("  --- actual ---")
                print("\n".join("  " + line for line in result.get("actual", "").splitlines()[:40]))
                if result.get("expected"):
                    print("  --- expected ---")
                    print("\n".join("  " + line for line in result["expected"].splitlines()[:40]))

    passed = sum(1 for result in results if result["status"] == "PASS")
    skipped = sum(1 for result in results if result["status"] == "SKIP")
    failed = len(results) - passed - skipped
    print(f"\n{passed} passed, {failed} failed, {skipped} skipped, {len(results)} total")
    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
