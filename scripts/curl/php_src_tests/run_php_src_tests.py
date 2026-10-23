#!/usr/bin/env python3
"""Run php-src `ext/curl` .phpt tests against an elephc-compiled binary.

This is a developer audit tool, not part of `cargo test`. It compiles each
test's `--FILE--` section with elephc, runs the produced binary, and compares its
output against `--EXPECT--` / `--EXPECTF--` / `--EXPECTREGEX--` using php-src
`run-tests.php`'s own semantics (trim output, anchored regex, EXPECTF
placeholder expansion). `--SKIPIF--` is evaluated with a real PHP CLI binary so
environment-dependent skips are honoured.

The php-src tests use a PHP built-in dev server via `server.inc` /
`curl_cli_server_start()`. This harness starts that server once (`php -S`) and
shadows `server.inc` with a shim that returns the pre-started host, so the tests
exercise elephc's curl, not PHP's. It also rewrites inline-HTML `?>` segments to
equivalent `echo` statements, because elephc's frontend does not tokenize closing
tags.

Prerequisites and the managed-native-cache fingerprint caveat are documented in
this directory's README.md.

Usage:
    python3 run_php_src_tests.py <php-src>/ext/curl/tests \\
        --elephc ../../target/debug/elephc --jobs 8 --json results.json
"""
import argparse
import concurrent.futures
import json
import os
import re
import shutil
import signal
import socket
import subprocess
import sys
import time
from collections import Counter

SECTION_RE = re.compile(r"^--([A-Z_]+)--\s*$", re.M)

# Ordered so multi-char keys are not shadowed; PHP's strtr uses longest match.
EXPECTF_MAP = {
    "%e": None,  # platform directory separator, filled per-run
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


def parse_sections(text):
    """Split a .phpt file into its --NAME-- sections (last occurrence wins)."""
    sections = {}
    matches = list(SECTION_RE.finditer(text))
    for i, m in enumerate(matches):
        name = m.group(1)
        start = m.end()
        end = matches[i + 1].start() if i + 1 < len(matches) else len(text)
        body = text[start:end]
        # The newline right after the `--NAME--` marker is a separator, not
        # content.
        if body.startswith("\r\n"):
            body = body[2:]
        elif body.startswith("\n"):
            body = body[1:]
        sections[name] = body
    return sections


def php_quote(s, delimiter="/"):
    """Reproduce PHP preg_quote() for a single delimiter."""
    specials = ".\\+*?[^]$(){} =!<>|:-#/"
    out = []
    for ch in s:
        if ch in specials:
            out.append("\\" + ch)
        else:
            out.append(ch)
    return "".join(out)


def expectf_to_regex(wanted):
    """Convert an EXPECTF body to a regex exactly like run-tests.php."""
    wanted = wanted.replace("\r\n", "\n")
    # Handle %r...%r raw regex sections, quoting everything else.
    temp = []
    start_offset = 0
    length = len(wanted)
    while start_offset < length:
        start = wanted.find("%r", start_offset)
        if start != -1:
            end = wanted.find("%r", start + 2)
            if end == -1:
                start = end = length
        else:
            start = end = length
        temp.append(php_quote(wanted[start_offset:start], "/"))
        if end > start:
            temp.append("(" + wanted[start + 2 : end] + ")")
        start_offset = end + 2
    quoted = "".join(temp)
    # PHP strtr on the quoted string: placeholders remain literal because '%'
    # is not a preg_quote special. Longest keys first.
    for key, value in sorted(EXPECTF_MAP.items(), key=lambda kv: -len(kv[0])):
        if value is None:
            continue
        quoted = quoted.replace(key, value)
    return quoted


def expectf_to_regex_with_sep(wanted, sep):
    regex = expectf_to_regex(wanted)
    return regex.replace("%e", re.escape(sep))


def find_free_port():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def start_php_server(php, doc_root, log_path):
    """Start php -S and return (process, host:port).

    Binds and probes `localhost` (not `127.0.0.1`): the tests' `Host` header must read
    `localhost:<port>`, which a `127.0.0.1` bind would change.
    """
    port = find_free_port()
    log = open(log_path, "wb")
    proc = subprocess.Popen(
        [php, "-n", "-t", doc_root, "-S", f"localhost:{port}", "responder/get.inc"],
        cwd=doc_root,
        stdin=subprocess.DEVNULL,
        stdout=log,
        stderr=log,
    )
    host = f"localhost:{port}"
    # Wait for the port to accept connections. The window is generous: a heavily loaded
    # machine can take far longer than the 5s a 100-iteration loop allows.
    for _ in range(600):
        time.sleep(0.05)
        if proc.poll() is not None:
            raise RuntimeError("php dev server exited early")
        try:
            with socket.create_connection(("localhost", port), timeout=1):
                return proc, host
        except OSError:
            continue
    proc.terminate()
    raise RuntimeError("php dev server did not become ready")


SHIM_SERVER_INC = """<?php
function curl_cli_server_start() {
    return getenv('ELEPHC_CURL_TEST_HOST') ?: 'localhost:1';
}
"""


def php_single_quote(text):
    """Encode literal text as a single-quoted PHP string body."""
    return "'" + text.replace("\\", "\\\\").replace("'", "\\'") + "'"


def run_skipif(php, sections, workdir, tests_dir, timeout):
    skipif = sections.get("SKIPIF")
    if not skipif:
        return None
    path = os.path.join(workdir, "skipif.php")
    with open(path, "w") as fh:
        fh.write(skipif)
    env = dict(os.environ)
    env["TEST_PHP_EXECUTABLE"] = php
    try:
        out = subprocess.run(
            [php, "-n", path],
            cwd=tests_dir,
            capture_output=True,
            text=True,
            timeout=timeout,
            env=env,
        )
    except subprocess.TimeoutExpired:
        return "skip (skipif timeout)"
    combined = (out.stdout or "") + (out.stderr or "")
    if re.match(r"\s*skip", combined, re.IGNORECASE):
        first = combined.strip().splitlines()[0] if combined.strip() else "skip"
        return first
    return None


def compile_and_run(args, sections, test_dir, stem, host, timeout_compile, timeout_run):
    file_body = sections.get("FILE")
    if file_body is None:
        file_body = sections.get("FILEEOF")
    if file_body is None:
        return {"status": "error", "detail": "no FILE section"}
    if "--ENV--" in sections:
        for line in sections["--ENV--"].splitlines():
            pass
    php_path = os.path.join(test_dir, stem + ".php")
    # elephc parses PHP's `?>` close tag, inline HTML, and the implied statement
    # terminator like PHP does, so the FILE body compiles as-is.
    converted = file_body
    with open(php_path, "w") as fh:
        fh.write(converted)
    compile_env = dict(os.environ)
    # Pin PATH: the native toolchain fingerprint is derived partly from PATH, and
    # bootstrap_native.py materialized the cached archives for this exact PATH.
    compile_env["PATH"] = "/usr/bin:/bin:/usr/sbin:/sbin"
    # `--INI--` directives become compile-time `--ini KEY=VALUE` overrides, which is what
    # PHP's startup INI application is for a compiled program.
    ini_flags = []
    ini_body = sections.get("INI")
    if ini_body:
        for line in ini_body.splitlines():
            line = line.strip()
            if not line or line.startswith(";") or "=" not in line:
                continue
            key, _, value = line.partition("=")
            ini_flags += ["--ini", f"{key.strip()}={value.strip()}"]
    try:
        comp = subprocess.run(
            [args.elephc, *ini_flags, php_path],
            cwd=test_dir,
            capture_output=True,
            text=True,
            timeout=timeout_compile,
            env=compile_env,
        )
    except subprocess.TimeoutExpired:
        return {"status": "compile-error", "detail": "compile timeout"}
    if comp.returncode != 0:
        return {
            "status": "compile-error",
            "detail": (comp.stdout or "") + (comp.stderr or ""),
        }
    binary = os.path.join(test_dir, stem)
    if not os.path.exists(binary):
        return {"status": "compile-error", "detail": "no binary produced"}
    run_env = dict(os.environ)
    run_env["ELEPHC_CURL_TEST_HOST"] = host
    if "ENV" in sections:
        for line in sections["ENV"].splitlines():
            if "=" in line:
                k, v = line.split("=", 1)
                run_env[k.strip()] = v.strip()
    try:
        run = subprocess.run(
            [binary],
            cwd=test_dir,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=timeout_run,
            env=run_env,
        )
    except subprocess.TimeoutExpired:
        return {"status": "run-timeout", "detail": ""}
    out = run.stdout.decode("utf-8", "replace")
    return {"status": "ran", "stdout": out, "stderr": "", "returncode": run.returncode}


DIAG_PREFIXES = ("Notice: ", "Warning: ", "Deprecated: ", "Fatal error: ", "Parse error: ")
DIAG_START_RE = re.compile(r"(?m)^(?=(?:Notice|Warning|Deprecated|Fatal error|Parse error): )")


def php_display_errors_normalize(output):
    """Emulate PHP CLI's display_errors prefix.

    PHP's CLI SAPI writes exactly one `\\n` before every diagnostic it renders to
    stdout; elephc writes the same text to stderr without that separator. The
    harness merges the two streams (2>&1) and inserts the separator here so a
    missing/extra diagnostic is still a difference, but the stream/prefix shape
    is not.
    """
    return DIAG_START_RE.sub("\n", output)


def compare(sections, output):
    output = php_display_errors_normalize(output).replace("\r\n", "\n").strip()
    if "EXPECTF" in sections:
        wanted = sections["EXPECTF"].strip()
        regex = expectf_to_regex_with_sep(wanted, os.sep)
        try:
            ok = re.match("^" + regex + "$", output, re.DOTALL) is not None
        except re.error as exc:
            return False, f"regex error: {exc}", regex
        return ok, wanted, regex
    if "EXPECTREGEX" in sections:
        wanted = sections["EXPECTREGEX"].strip()
        try:
            ok = re.match("^" + wanted + "$", output, re.DOTALL) is not None
        except re.error as exc:
            return False, f"regex error: {exc}", wanted
        return ok, wanted, wanted
    wanted = sections.get("EXPECT", "").replace("\r\n", "\n").strip()
    return output == wanted, wanted, None


def run_one(args, tests_dir, workdir, host, name):
    """Runs one .phpt: SKIPIF, compile, execute, compare. Returns a result record."""
    path = os.path.join(tests_dir, name)
    with open(path, encoding="utf-8", errors="replace") as fh:
        sections = parse_sections(fh.read())
    stem = name[:-5]
    skip_dir = os.path.join(workdir, stem)
    os.makedirs(skip_dir, exist_ok=True)
    skip = run_skipif(args.php, sections, skip_dir, tests_dir, args.skipif_timeout)
    if skip:
        return {"name": name, "status": "skip", "detail": skip}
    outcome = compile_and_run(
        args, sections, tests_dir, stem, host,
        args.timeout_compile, args.timeout_run,
    )
    if outcome["status"] != "ran":
        return {"name": name, "status": outcome["status"], "detail": outcome.get("detail", "")}
    ok, wanted, regex = compare(sections, outcome["stdout"])
    return {
        "name": name,
        "status": "pass" if ok else "fail",
        "wanted": wanted,
        "got": outcome["stdout"],
        "stderr": outcome["stderr"],
        "regex": regex,
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("tests_dir")
    ap.add_argument("--elephc", required=True)
    ap.add_argument("--php", default="php")
    ap.add_argument("--filter", default="")
    ap.add_argument("--workdir", default=None)
    ap.add_argument("--manifest-source", default=None,
                    help="directory holding elephc.toml/elephc.lock to copy into tests_dir")
    ap.add_argument("--timeout-compile", type=float, default=120.0)
    ap.add_argument("--timeout-run", type=float, default=30.0)
    ap.add_argument("--skipif-timeout", type=float, default=60.0)
    ap.add_argument("--jobs", type=int, default=1)
    ap.add_argument("--json", default=None)
    args = ap.parse_args()

    tests_dir = os.path.abspath(args.tests_dir)
    workdir = os.path.abspath(args.workdir or os.path.join(tests_dir, "__elephc_run__"))
    os.makedirs(workdir, exist_ok=True)

    # A native project manifest is required for the compile path to resolve the
    # managed `curl` package. Copy one from an existing elephc project.
    manifest_source = args.manifest_source or os.path.join(
        os.path.dirname(os.path.abspath(__file__)), "audit-project"
    )
    for fname in ("elephc.toml", "elephc.lock"):
        src = os.path.join(manifest_source, fname)
        dst = os.path.join(tests_dir, fname)
        if os.path.exists(src) and not os.path.exists(dst):
            shutil.copy(src, dst)

    # Shadow server.inc with the shim (restored at exit).
    real_inc = os.path.join(tests_dir, "server.inc")
    inc_backup = os.path.join(tests_dir, "server.inc.elephc-backup")
    if os.path.exists(real_inc) and not os.path.exists(inc_backup):
        shutil.copy(real_inc, inc_backup)
        with open(real_inc, "w") as fh:
            fh.write(SHIM_SERVER_INC)

    server_proc, host = start_php_server(args.php, tests_dir, os.path.join(workdir, "php-server.log"))
    print(f"[harness] php dev server at {host}", file=sys.stderr)

    names = sorted(n for n in os.listdir(tests_dir) if n.endswith(".phpt"))
    if args.filter:
        names = [n for n in names if args.filter in n]

    results = []
    try:
        if args.jobs > 1:
            with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
                futures = {
                    pool.submit(run_one, args, tests_dir, workdir, host, name): name
                    for name in names
                }
                for future in concurrent.futures.as_completed(futures):
                    rec = future.result()
                    results.append(rec)
                    suffix = f": {rec['detail']}" if rec["status"] == "skip" else ""
                    print(rec["status"].upper() + " " + rec["name"] + suffix)
        else:
            for name in names:
                rec = run_one(args, tests_dir, workdir, host, name)
                results.append(rec)
                suffix = f": {rec['detail']}" if rec["status"] == "skip" else ""
                print(rec["status"].upper() + " " + rec["name"] + suffix)
                if rec["status"] == "compile-error" and rec.get("detail"):
                    print("   " + rec["detail"].strip().replace("\n", "\n   ")[:1500])
    finally:
        server_proc.terminate()
        try:
            server_proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            server_proc.kill()
        if os.path.exists(inc_backup):
            shutil.move(inc_backup, real_inc)

    counts = Counter(r["status"] for r in results)
    print("\n=== summary ===")
    for k in sorted(counts):
        print(f"{k}: {counts[k]}")
    print(f"total: {len(results)}")

    print("\n=== failures (expected vs got) ===")
    for r in results:
        if r["status"] != "fail":
            continue
        print(f"\n--- {r['name']} ---")
        print("EXPECTED:")
        print(r["wanted"][:2000])
        print("GOT:")
        print((r.get("got") or "")[:2000])
        if r.get("stderr"):
            print("STDERR:")
            print(r["stderr"][:800])

    if args.json:
        with open(args.json, "w") as fh:
            json.dump(results, fh, indent=2)


if __name__ == "__main__":
    main()
