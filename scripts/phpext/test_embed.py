#!/usr/bin/env python3
"""Run native producer lifecycle tests in a real PHP embed SAPI, optionally under Valgrind."""

import argparse
import os
from pathlib import Path
import shlex
import subprocess
import tempfile


def checked(command, **kwargs):
    """Surface complete diagnostics while keeping successful test logs compact."""
    result = subprocess.run(command, capture_output=True, text=True, **kwargs)
    if result.returncode or result.stderr:
        raise RuntimeError(f"Command failed ({result.returncode}): {shlex.join(map(str, command))}\n{result.stdout}\n{result.stderr}")
    return result.stdout


def main():
    """Compile an extension and harness with the same SDK and verify complete request cleanup."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--php-config", required=True)
    parser.add_argument("--compiler", default="target/debug/elephc")
    parser.add_argument("--valgrind", action="store_true")
    parser.add_argument("--leaks", action="store_true")
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
                   PHP_INI_SCAN_DIR="")
        checked([compiler, "--emit", "phpext", "--heap-size=524288", source], env=env)
        ini = work / "php.ini"
        ini.write_text(f'extension="{source.with_suffix(".so")}"\n')
        executable = work / "embed-test"
        cc = shlex.split(os.environ.get("CC", "cc"))
        checked(cc + includes + ["-std=c11", "-O1", "-g", "-pthread", str(root / "tests/phpext/embed.c"),
                                str(library), f"-Wl,-rpath,{library.parent}", "-o", str(executable)])
        command = [str(executable), str(ini)]
        if args.valgrind:
            env["USE_ZEND_ALLOC"] = "0"
            env["ELEPHC_EMBED_ITERATIONS"] = "3"
            command = ["valgrind", "--quiet", "--leak-check=full", "--show-leak-kinds=definite,indirect",
                       "--errors-for-leak-kinds=definite,indirect", "--error-exitcode=99"] + command
        if args.leaks:
            env["MallocStackLogging"] = "1"
            result = subprocess.run(["leaks", "--atExit", "--"] + command, env=env,
                                    capture_output=True, text=True, timeout=240)
            if result.returncode or "0 leaks" not in result.stdout or "request isolation ok" not in result.stdout:
                raise RuntimeError(f"Apple leak check failed ({result.returncode}):\n{result.stdout}\n{result.stderr}")
            print(result.stdout, end="")
            return
        output = checked(command, env=env, timeout=240)
        if output != "request isolation ok\n":
            raise RuntimeError(f"Unexpected embed output: {output}")
        print(output, end="")


if __name__ == "__main__":
    main()
