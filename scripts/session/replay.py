#!/usr/bin/env python3
"""Inventory every pinned session PHPT and replay its unchanged FILE over native HTTP.

The reference CLI must first match the upstream EXPECT/EXPECTF. CGI-only tests,
missing adapters, oracle skips and native failures stay visible in the denominator.
Reference CLI output and native HTTP output are distinct SAPI measurements; a
green subset is never a full session-compliance certificate.
"""
import argparse
import base64
from collections import Counter
import hashlib
import http.client
import json
import os
from pathlib import Path
import re
import shutil
import signal
import socket
import subprocess
import time

TAG = "php-8.5.11"
PIN = "678778973bb4d4185c06ef1320de50f1a975d3c8"
SOURCE_HASHES = {
    "ext/session/session.c": "1d77c098b03d43071c1d7dc9cfdf62b96b0f70902e5ae338e0b215d85ff683a9",
    "ext/session/session.stub.php": "a41f47ef5cc213f832bee85ec3b978a774a1b5e69f460d0e4b128fb1e00c4c16",
    "ext/session/mod_files.c": "2c3c625ca7f425878cd9a028a827fbeda307296192998d9c42571e3390f4cff4",
    "run-tests.php": "dacb8590091b147ffeda04d69a129c5f87ca3d301ab76e93a5c670dc09a9fe33",
}
SUPPORTED = {"TEST", "EXTENSIONS", "SKIPIF", "INI", "FILE", "EXPECT", "EXPECTF",
             "CLEAN", "XFAIL", "CREDITS", "DESCRIPTION", "CONFLICTS"}
DEFAULT_INI = [
    "output_handler=", "open_basedir=", "disable_functions=", "output_buffering=Off",
    "error_reporting=30719", "fatal_error_backtraces=Off", "display_errors=1",
    "display_startup_errors=1", "log_errors=0", "html_errors=0", "track_errors=0",
    "report_zend_debug=0", "docref_root=", "docref_ext=.html", "error_prepend_string=",
    "error_append_string=", "auto_prepend_file=", "auto_append_file=",
    "ignore_repeated_errors=0", "precision=14", "serialize_precision=-1", "memory_limit=128M",
    "opcache.fast_shutdown=0", "opcache.file_update_protection=0", "opcache.revalidate_freq=0",
    "opcache.jit_hot_loop=1", "opcache.jit_hot_func=1", "opcache.jit_hot_return=1",
    "opcache.jit_hot_side_exit=1", "opcache.jit_max_root_traces=100000",
    "opcache.jit_max_side_traces=100000", "opcache.jit_max_exit_counters=100000",
    "opcache.protect_memory=1", "zend.assertions=1", "zend.exception_ignore_args=0",
    "zend.exception_string_param_max_len=15", "short_open_tag=0", "date.timezone=UTC",
]


def digest(path):
    """Fingerprint an artifact without loading large archives into memory."""
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(block)
    return result.hexdigest()


def sections(data):
    """Extract PHPT sections byte-for-byte and reject duplicate boundaries."""
    marks = list(re.finditer(rb"^--([A-Z_]+)--[ \t]*\r?$", data, re.M))
    result = {}
    if not marks or marks[0][1] != b"TEST":
        raise ValueError("missing initial TEST section")
    for index, mark in enumerate(marks):
        name = mark[1].decode("ascii")
        if name in result:
            raise ValueError("duplicate section: " + name)
        begin = mark.end() + (data[mark.end():mark.end() + 1] == b"\n")
        end = marks[index + 1].start() if index + 1 < len(marks) else len(data)
        result[name] = data[begin:end]
    return result


def run(args, cwd, env, timeout):
    """Capture one phase with merged diagnostic ordering and an explicit timeout."""
    started = time.monotonic()
    try:
        process = subprocess.run(args, cwd=cwd, env=env, stdout=subprocess.PIPE,
                                 stderr=subprocess.STDOUT, timeout=timeout)
        return {"exit": process.returncode, "output": process.stdout,
                "seconds": round(time.monotonic() - started, 3)}
    except subprocess.TimeoutExpired as error:
        return {"exit": None, "output": error.stdout or b"", "timeout": True,
                "seconds": round(time.monotonic() - started, 3)}


def save_phase(folder, name, result):
    """Persist complete phase output separately from the compact JSON record."""
    (folder / (name + ".out")).write_bytes(result["output"])
    return {key: value for key, value in result.items() if key != "output"}


def ini_settings(parts, directory):
    """Keep upstream directive order and expand only documented PHPT placeholders."""
    settings = DEFAULT_INI.copy()
    text = parts.get("INI", b"").decode().replace("{PWD}", str(directory))
    text = text.replace("{TMP}", str(directory))
    for line in text.splitlines():
        if not line.strip() or line.lstrip().startswith(";"):
            continue
        if "=" not in line:
            raise ValueError("invalid INI line: " + line)
        name, value = line.split("=", 1)
        value = value.strip()
        settings.append(name.strip() + "=" + value)
    return settings


def php_args(php, settings, source):
    """Run the oracle without host php.ini or optional host extensions."""
    return [str(php), "-n"] + [arg for item in settings for arg in ("-d", item)] + [str(source)]


def make_matcher(source, destination):
    """Use the frozen php-src runner's own EXPECTF implementation through PHP/PCRE."""
    text = (source / "run-tests.php").read_text()
    begin = text.index("\nfunction expectf_to_regex(")
    end = text.index("\n/**", begin)
    destination.write_text("<?php\n" + text[begin:end] + r'''
$x = json_decode(stream_get_contents(STDIN), true, flags: JSON_THROW_ON_ERROR);
$want = trim(str_replace("\r\n", "\n", base64_decode($x['want'])));
$got = trim(str_replace("\r\n", "\n", base64_decode($x['got'])));
if ($x['kind'] === 'EXPECT') { echo $want === $got ? '1' : '0'; }
else { $r = preg_match('/^' . expectf_to_regex($want) . '$/s', $got);
       if ($r === false) { exit(2); } echo $r ? '1' : '0'; }
''')


def matches(php, matcher, kind, want, got):
    """Compare exact output using upstream matching; retain every diagnostic."""
    payload = json.dumps({"kind": kind, "want": base64.b64encode(want).decode(),
                          "got": base64.b64encode(got).decode()}).encode()
    result = subprocess.run([str(php), "-n", str(matcher)], input=payload,
                            capture_output=True, timeout=10)
    if result.returncode:
        raise ValueError("upstream matcher failed: " + result.stderr.decode(errors="replace"))
    return result.stdout == b"1"


def native_http(binary, directory, env, timeout, evidence):
    """Issue one native request and reap the complete prefork process group afterward."""
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        port = listener.getsockname()[1]
    with (evidence / "native.server.log").open("wb") as log:
        process = subprocess.Popen([str(binary), "--listen", f"127.0.0.1:{port}",
                                    "--workers", "1"], cwd=directory, env=env,
                                   stdout=log, stderr=log, start_new_session=True)
        try:
            deadline = time.monotonic() + timeout
            while True:
                if process.poll() is not None:
                    return {"exit": process.returncode, "output": b"", "server_failed": True}
                try:
                    with socket.create_connection(("127.0.0.1", port), timeout=0.1):
                        break
                except OSError:
                    if time.monotonic() >= deadline:
                        return {"exit": None, "output": b"", "timeout": True}
                    time.sleep(0.02)
            connection = http.client.HTTPConnection("127.0.0.1", port, timeout=timeout)
            try:
                connection.request("GET", "/", headers={"Connection": "close"})
                response = connection.getresponse()
                body = response.read()
                return {"exit": 0, "http_status": response.status,
                        "headers": response.getheaders(), "output": body}
            finally:
                connection.close()
        finally:
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()


def replay_case(path, options, matcher, extensions):
    """Validate the oracle before compiling unchanged source for the native web SAPI."""
    test_root = options.php_src / "ext/session/tests"
    relative = path.relative_to(test_root)
    evidence = options.output / "cases" / relative.with_suffix("")
    evidence.mkdir(parents=True)
    result = {"test": str(path.relative_to(options.php_src)), "sha256": digest(path)}
    try:
        parts = sections(path.read_bytes())
        unsupported = sorted(parts.keys() - SUPPORTED)
        if unsupported:
            return dict(result, status="adapter_error", reason="unsupported sections: " + ", ".join(unsupported))
        missing = set(parts.get("EXTENSIONS", b"").decode().split()) - extensions
        if missing:
            return dict(result, status="oracle_skip", reason="missing extensions: " + ", ".join(sorted(missing)))
        kind = next((name for name in ("EXPECT", "EXPECTF") if name in parts), None)
        if kind is None or "FILE" not in parts:
            raise ValueError("missing FILE/EXPECT")
        if any(token in parts["FILE"] for token in (b"TEST_PHP_EXECUTABLE", b"PHP_BINARY")):
            return dict(result, status="adapter_error", reason="PHP subprocess adapter required")
        if b"sapi/cgi/" in parts["FILE"] + parts.get("SKIPIF", b""):
            return dict(result, status="adapter_error", reason="upstream CGI fixture/SAPI adapter required")
        for engine in ("reference", "native"):
            fixtures = evidence / engine / "ext/session/tests"
            shutil.copytree(test_root, fixtures, ignore=shutil.ignore_patterns("*.phpt"))
            directory = fixtures / relative.parent
            source = directory / path.with_suffix(".php").name
            source.write_bytes(parts["FILE"])
            settings = ini_settings(parts, directory)
            env = os.environ.copy()
            # Preserve upstream's distinction between the test cwd and its temporary directory.
            # In particular, open_basedir=. must still reject the default session temp path.
            temporary = evidence / engine / "tmp"
            temporary.mkdir()
            env.update(TMPDIR=str(temporary), TMP=str(temporary), TEMP=str(temporary),
                       XDG_CACHE_HOME=str(options.output / "cache"))
            executable = options.php if engine == "reference" else options.output / "unsupported_php_subprocess.py"
            env["TEST_PHP_EXECUTABLE"] = str(executable)
            env["TEST_PHP_EXECUTABLE_ESCAPED"] = str(executable)
            marker = directory / ".unsupported-subprocess"
            env["ELEPHC_PHPT_SUBPROCESS_FLAG"] = str(marker)
            if engine == "reference" and "SKIPIF" in parts:
                skip = directory / "skip.php"
                skip.write_bytes(parts["SKIPIF"])
                checked = run(php_args(options.php, settings, skip), directory, env, options.timeout)
                result["skipif"] = save_phase(evidence, "skipif", checked)
                if checked["exit"] != 0:
                    return dict(result, status="oracle_error", reason="SKIPIF failed")
                if checked["output"].strip().lower().startswith(b"skip"):
                    return dict(result, status="oracle_skip", reason=checked["output"].decode(errors="replace").strip())
            if engine == "reference":
                actual = run(php_args(options.php, settings, source), directory, env, options.timeout)
            else:
                arguments = [str(options.elephc), "--quiet", "--web", "--php-version", "8.5"]
                arguments += [arg for setting in settings for arg in ("--ini", setting)]
                compiled = run(arguments + [str(source)], options.project, env, options.timeout)
                result["compile"] = save_phase(evidence, "compile", compiled)
                if compiled.get("timeout"):
                    return dict(result, status="compile_timeout")
                if compiled["exit"] != 0:
                    return dict(result, status="compile_error")
                try:
                    actual = native_http(source.with_suffix(""), directory, env, options.timeout, evidence)
                except (http.client.HTTPException, ConnectionError, TimeoutError) as error:
                    return dict(result, status="runtime_timeout" if isinstance(error, TimeoutError)
                                else "runtime_http_error", reason=str(error))
                if marker.exists():
                    return dict(result, status="adapter_error", reason="fixture requires a PHP subprocess")
            result[engine] = save_phase(evidence, engine, actual)
            if actual.get("timeout"):
                return dict(result, status="oracle_timeout" if engine == "reference" else "runtime_timeout")
            matched = matches(options.php, matcher, kind, parts[kind], actual["output"])
            if "CLEAN" in parts:
                clean = directory / "clean.php"
                clean.write_bytes(parts["CLEAN"])
                cleaned = run(php_args(options.php, settings, clean), directory, env, options.timeout)
                result[engine + "_clean"] = save_phase(evidence, engine + "_clean", cleaned)
                result["clean_adapter"] = "reference PHP, cleanup only"
                if cleaned["exit"] != 0:
                    return dict(result, status="adapter_error", reason="CLEAN failed")
            if engine == "reference" and not matched and "XFAIL" in parts:
                return dict(result, status="oracle_xfail", reason=parts["XFAIL"].decode().strip())
            # PHPT fatal-error tests intentionally exit 255; upstream scores their output.
            if engine == "reference" and (not matched or actual["exit"] < 0):
                return dict(result, status="oracle_error", reason="CLI reference did not match original expectation")
            if engine == "native":
                if actual.get("server_failed") or actual["exit"] < 0:
                    return dict(result, status="runtime_crash")
                if not matched:
                    return dict(result, status="mismatch")
                log = (evidence / "native.server.log").read_bytes()
                if re.search(rb"(?:Warning|Notice|Deprecated|Fatal error):", log):
                    return dict(result, status="diagnostic_mismatch")
                if actual["http_status"] != 200:
                    return dict(result, status="http_status_mismatch")
        return dict(result, status="xfail_match" if "XFAIL" in parts else "pass")
    except (ValueError, OSError, subprocess.SubprocessError, http.client.HTTPException) as error:
        return dict(result, status="adapter_error", reason=str(error))


def main():
    """Retain full inventory, exact artifacts and incremental results for every selected case."""
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("php-src", "php", "elephc", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--project", type=Path, default=Path.cwd())
    parser.add_argument("--filter", default="")
    parser.add_argument("--timeout", type=float, default=60)
    options = parser.parse_args()
    for name in ("php_src", "php", "elephc", "output", "project"):
        setattr(options, name, getattr(options, name).resolve())
    version = subprocess.check_output([str(options.php), "-n", "-r", "echo PHP_VERSION;"]).decode()
    if version != "8.5.11":
        parser.error("reference PHP must be exactly 8.5.11, found " + version)
    for name, expected in SOURCE_HASHES.items():
        if digest(options.php_src / name) != expected:
            parser.error("source differs from the frozen release: " + name)
    options.output.mkdir(parents=True, exist_ok=False)
    (options.output / "cases").mkdir()
    guard = options.output / "unsupported_php_subprocess.py"
    guard.write_text('#!/usr/bin/env python3\nimport os\nfrom pathlib import Path\n'
                     'Path(os.environ["ELEPHC_PHPT_SUBPROCESS_FLAG"]).touch()\nraise SystemExit(126)\n')
    guard.chmod(0o755)
    corpus = sorted((options.php_src / "ext/session/tests").rglob("*.phpt"))
    if len(corpus) != 263:
        parser.error("pinned session corpus must contain 263 tests")
    selected = [path for path in corpus if re.search(options.filter, path.name)]
    manifest = {"tag": TAG, "source_pin": PIN, "reference_version": version,
                "php_sha256": digest(options.php), "elephc_sha256": digest(options.elephc),
                "runner_sha256": digest(Path(__file__)), "source_hashes": SOURCE_HASHES,
                "filter": options.filter,
                "timeout": options.timeout, "corpus_count": len(corpus),
                "selected_count": len(selected), "complete": len(selected) == len(corpus),
                "tests": [{"path": str(path.relative_to(options.php_src)), "sha256": digest(path)} for path in corpus],
                "git_head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=options.project).decode().strip(),
                "source_diff_sha256": hashlib.sha256(subprocess.check_output(["git", "diff", "HEAD"], cwd=options.project)).hexdigest(),
                "sapi_adapter": "original oracle CLI vs native HTTP; unsupported request sections stay explicit"}
    changed = subprocess.check_output(["git", "diff", "HEAD", "--name-only"], cwd=options.project).decode().splitlines()
    changed += subprocess.check_output(["git", "ls-files", "--others", "--exclude-standard"], cwd=options.project).decode().splitlines()
    manifest["changed_source_hashes"] = {name: digest(options.project / name) for name in sorted(set(changed))
                                         if (options.project / name).is_file()}
    for name in ("libelephc_web.a", "libelephc_magician.a"):
        artifact = options.elephc.parent / name
        manifest[name + "_sha256"] = digest(artifact) if artifact.exists() else None
    (options.output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    matcher = options.output / "matcher.php"
    make_matcher(options.php_src, matcher)
    extensions = set(subprocess.check_output([str(options.php), "-n", "-r",
                                            'echo implode("\\n", get_loaded_extensions());']).decode().splitlines())
    counts = Counter()
    with (options.output / "results.jsonl").open("w") as output:
        for index, path in enumerate(selected, 1):
            result = replay_case(path, options, matcher, extensions)
            counts[result["status"]] += 1
            output.write(json.dumps(result) + "\n")
            output.flush()
            print(f"{index}/{len(selected)} {path.name}: {result['status']}", flush=True)
    artifact_end = {"php_sha256": digest(options.php), "elephc_sha256": digest(options.elephc)}
    for name in ("libelephc_web.a", "libelephc_magician.a"):
        artifact = options.elephc.parent / name
        artifact_end[name + "_sha256"] = digest(artifact) if artifact.exists() else None
    stable = all(manifest[key] == value for key, value in artifact_end.items())
    summary = {"complete": manifest["complete"], "artifacts_stable": stable,
               "artifact_end_sha256": artifact_end, "corpus_count": len(corpus),
               "selected_count": len(selected), "unselected_count": len(corpus) - len(selected),
               "counts": dict(counts), "acceptance": "open: request adapters, SAPI, profiles and target matrix"}
    (options.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))
    return 0 if stable and summary["complete"] and counts.get("pass", 0) == len(corpus) else 1


if __name__ == "__main__":
    raise SystemExit(main())
