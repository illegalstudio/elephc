#!/usr/bin/env python3
"""Run native producer lifecycle tests in a real PHP embed SAPI, optionally under Valgrind."""

import argparse
import os
from pathlib import Path
import plistlib
import select
import shlex
import signal
import subprocess
import sys
import tempfile


def checked(command, allow_stderr=False, **kwargs):
    """Surface complete diagnostics while keeping successful test logs compact."""
    result = subprocess.run(command, capture_output=True, text=True, **kwargs)
    if result.returncode or (result.stderr and not allow_stderr):
        raise RuntimeError(f"Command failed ({result.returncode}): {shlex.join(map(str, command))}\n{result.stdout}\n{result.stderr}")
    return result.stdout


def check_apple_leaks(command, env, work):
    """Inspect the live harness after full PHP shutdown, without atExit injection or open pipe hangs."""
    entitlements = work / "leaks.entitlements"
    with entitlements.open("wb") as output:
        plistlib.dump({"com.apple.security.get-task-allow": True}, output)
    checked(["codesign", "--force", "--sign", "-", "--entitlements", str(entitlements), command[0]],
            allow_stderr=True)
    env.update(MallocStackLogging="1", ELEPHC_EMBED_ITERATIONS="3",
               ELEPHC_EMBED_LEAK_READY="1")
    with subprocess.Popen(command, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, text=True, start_new_session=True) as process:
        try:
            readable = select.select([process.stdout], [], [], 120)[0]
            ready = process.stdout.readline() if readable else ""
            if not readable or ready not in ("request isolation ok\n", "request isolation failed\n"):
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGKILL)
                stdout, stderr = process.communicate()
                reason = "failed before leak inspection" if readable else "did not reach the Apple leak inspection checkpoint"
                raise RuntimeError(f"PHP lifecycle {reason} ({process.returncode}):\n"
                                   f"{ready}{stdout}\n{stderr}")
            result = subprocess.run(["leaks", str(process.pid)], capture_output=True, text=True, timeout=120)
            stdout, stderr = process.communicate("\n", timeout=30)
            if result.returncode or "0 leaks" not in result.stdout:
                raise RuntimeError(f"Apple leak check failed ({result.returncode}):\n{result.stdout}\n"
                                   f"{result.stderr}\n{ready}{stdout}\n{stderr}")
            if process.returncode or stdout or ready != "request isolation ok\n":
                raise RuntimeError(f"PHP lifecycle failed after leak inspection ({process.returncode}):\n"
                                   f"{ready}{stdout}\n{stderr}\n{result.stdout}")
            print(ready + result.stdout, end="")
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGKILL)
                process.communicate()


def main():
    """Compile an extension and harness with the same SDK and verify complete request cleanup."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--php-config", required=True)
    parser.add_argument("--compiler", default="target/debug/elephc")
    parser.add_argument("--valgrind", action="store_true")
    parser.add_argument("--leaks", action="store_true")
    parser.add_argument("--asan", action="store_true", help="use PHP's ASan minimum reserved stack")
    args = parser.parse_args()
    config = Path(args.php_config).resolve()
    compiler = Path(args.compiler).resolve()
    root = Path(__file__).resolve().parents[2]
    php = Path(checked([config, "--php-binary"]).strip())
    includes = shlex.split(checked([config, "--includes"]))
    prefix = Path(checked([config, "--prefix"]).strip())
    libraries = [prefix / "lib" / name for name in ["libphp.so", "libphp.dylib"]]
    library = next((path for path in libraries if path.is_file()), None)
    if library is None:
        raise SystemExit("The lifecycle test requires a real PHP --enable-embed=shared SDK")
    with tempfile.TemporaryDirectory(prefix="phpext-embed-", dir=root / "target") as work:
        work = Path(work)
        source = work / "lifecycle.php"
        source.write_bytes((root / "tests/phpext/lifecycle.php").read_bytes())
        env = dict(os.environ, ELEPHC_PHPEXT_PHP_CONFIG=str(config), ELEPHC_PHPEXT_PHP_BINARY=str(php),
                   PHP_INI_SCAN_DIR="", ELEPHC_EMBED_FIBERS=str(root / "tests/phpext/fibers.php"))
        checked([compiler, "--emit", "phpext", "--heap-size=524288", source], env=env)
        helper = work / "late_resume.so"
        cc = shlex.split(os.environ.get("CC", "cc"))
        shared = ["-bundle", "-undefined", "dynamic_lookup"] if sys.platform == "darwin" else ["-shared"]
        checked(cc + includes + ["-std=gnu11", "-O1", "-g", "-fPIC", "-Wall", "-Wextra", "-Werror"]
                + shared + [str(root / "tests/phpext/late_resume.c"), "-o", str(helper)])
        ini = work / "php.ini"
        # Keep stack exhaustion ahead of the intentionally small native heap on NTS main threads too.
        reserved = 327680 if args.asan else 65536
        # ASan requires 320 KiB for PHP's error paths; retain the ordinary 192 KiB native budget.
        ini.write_text(f'extension="{helper}"\n'
                       f'extension="{source.with_suffix(".so")}"\n'
                       f'zend.max_allowed_stack_size={reserved + 196608}\n'
                       f'zend.reserved_stack_size={reserved}\n')
        executable = work / "embed-test"
        checked(cc + includes + ["-std=c11", "-O1", "-g", "-pthread", str(root / "tests/phpext/embed.c"),
                                str(library), f"-Wl,-rpath,{library.parent}", "-o", str(executable)])
        command = [str(executable), str(ini)]
        if args.valgrind:
            env["ELEPHC_EMBED_ITERATIONS"] = "3"
            command = ["valgrind", "--quiet", "--leak-check=full", "--show-leak-kinds=definite,indirect",
                       "--errors-for-leak-kinds=definite,indirect", "--error-exitcode=99"] + command
        # Ordinary requests expose every Zend allocation to the memory checker.
        # Fatal requests need Zend's arena reclamation for executor-owned objects
        # intentionally left alive after bailout. Native adapter/checkpoint mallocs
        # remain individually visible to ASan, Valgrind and leaks in both phases.
        # Keep the allocator oracle in its own process, preserving cold fatal-request caches.
        for phase, zend_alloc in [("allocator", "1"), ("ordinary", "0"), ("fatal", "1")]:
            phase_env = dict(env, USE_ZEND_ALLOC=zend_alloc)
            phase_env.pop("ELEPHC_EMBED_FATAL_PHASE", None)
            phase_env.pop("ELEPHC_EMBED_ALLOCATOR_PROBE", None)
            if phase == "allocator":
                phase_env["ELEPHC_EMBED_ALLOCATOR_PROBE"] = "1"
            elif phase == "fatal":
                phase_env["ELEPHC_EMBED_FATAL_PHASE"] = "1"
            if args.leaks:
                check_apple_leaks(command, phase_env, work)
            else:
                output = checked(command, env=phase_env, timeout=240)
                if output != "request isolation ok\n":
                    raise RuntimeError(f"Unexpected {phase} embed output: {output}")
                print(f"{phase}: {output}", end="")


if __name__ == "__main__":
    main()
