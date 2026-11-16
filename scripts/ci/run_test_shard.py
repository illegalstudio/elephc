"""Run the existing shard families identically on original and recovery runners."""

import json
import os
from pathlib import Path
import shutil
import subprocess

FILTERS = {
    "non-codegen": "not binary(codegen_tests) and not binary(web_tests) and not binary(web_session_tests) and not (binary(/^(opcache_|eval_|php_profile_)/) or binary(extension_loaded_tests) or binary(php_version_surface_tests))",
    "cli": "binary(/^(opcache_|eval_|php_profile_)/) or binary(extension_loaded_tests) or binary(php_version_surface_tests)",
    "web": "binary(web_tests) or binary(web_session_tests)",
    "codegen": "binary(codegen_tests) and not test(~codegen::eval) and not test(~codegen::pdo) and not test(~codegen::curl)",
}


def execute(arguments, env):
    """Propagate every setup or test failure without retrying tests here."""
    subprocess.run([str(arg) for arg in arguments], env=env, check=True)


def run(family, target, shard, count, root, env):
    """Preserve filters, worker limits, native setup and the single-process helpers."""
    root = Path(root)
    extracted = root / "nextest"
    extracted.mkdir(parents=True, exist_ok=True)
    common = [root / "cargo-nextest", "nextest", "run", "--profile", "ci",
              "--archive-file", root / "nextest-archive.tar.zst", "--workspace-remap", ".",
              "--extract-to", extracted, "--extract-overwrite"]
    env = dict(env)
    macos = target == "macos-aarch64"
    if family == "pdo":
        env.update(CARGO_NET_OFFLINE="true", ELEPHC_TEST_PREBUILT_BRIDGES="1")
    if family == "curl":
        cache = root / "elephc-native"
        cache.mkdir(parents=True, exist_ok=True)
        execute(["tar", "-xzf", root / f"managed-native-curl-{target}.tar.gz",
                 "-C", cache, "--strip-components=1"], env)
    if family in {"codegen", "pdo", "curl", "eval"}:
        execute(common + ["--no-run", "-E", "binary(codegen_tests)"], env)
    compiler = extracted / "target/debug/elephc"
    if family == "codegen":
        execute([compiler, "native", "install", "--locked", "--target", target,
                 "--manifest-path", "examples/xml/elephc.toml"], env)
        if not macos:
            for tool in ("m4", "gdb"):
                if not shutil.which(tool):
                    raise ValueError(f"CI image is missing preinstalled {tool}")
        for example in ("gmp", "openssl-cipher", "fileinfo", "tidy"):
            execute([compiler, "native", "install", "--locked", "--target", target,
                     "--manifest-path", f"examples/{example}/elephc.toml"], env)
        env.update({f"ELEPHC_TEST_REQUIRE_{package}_NATIVE": "1"
                    for package in ("XML", "GMP", "FILEINFO", "OPENSSL", "TIDY")})
        phases = json.loads(Path("scripts/ci/shard_fixture_phases.json").read_text())
        env["ELEPHC_FIXTURE_PHASE_TIMING"] = phases["macos" if macos else "linux"]
    if family == "curl":
        env["ELEPHC_NATIVE_CACHE"] = str(cache)
        execute([compiler, "native", "install", "--locked", "--offline", "--target", target,
                 "--manifest-path", "examples/curl-get/elephc.toml"], env)
        env["ELEPHC_TEST_BINARY_TIMEOUT_SECS"] = "120"
    if family in {"codegen", "pdo", "curl", "eval"}:
        env.update(CARGO_NET_OFFLINE="true", ELEPHC_TEST_PREBUILT_BRIDGES="1")
    if family in {"pdo", "curl", "eval"}:
        binaries = [path for path in (extracted / "target/debug/deps").glob("codegen_tests-*")
                    if path.is_file() and os.access(path, os.X_OK)]
        if len(binaries) != 1:
            raise ValueError("Expected one archived codegen test executable")
        if family == "eval":
            env.update(EVAL_SHARD_NEXTEST_BIN=str(root / "cargo-nextest"),
                       EVAL_SHARD_NEXTEST_ARCHIVE=str(root / "nextest-archive.tar.zst"),
                       EVAL_SHARD_NEXTEST_EXTRACT_DIR=str(extracted),
                       EVAL_SHARD_JUNIT_FILE="target/nextest/ci/junit-pass1.xml")
        execute(["bash", f"scripts/ci/run_{family}_codegen_shard.sh", binaries[0], shard, count], env)
        return
    workers = {"non-codegen": 3 if macos else 4, "cli": 2, "web": 1,
               "codegen": 3 if macos else 4}[family]
    arguments = common + ["-E", FILTERS[family], "--partition", f"hash:{shard}/{count}"]
    if family in {"non-codegen", "cli"}:
        arguments += ["--status-level", "pass"]
    execute(arguments + ["--no-fail-fast", "--retries", "1", "--flaky-result", "pass", "-j", workers], env)


if __name__ == "__main__":
    run(os.environ["FAMILY"], os.environ["TARGET"], os.environ["SHARD"],
        os.environ["SHARDS"], os.environ["RUNNER_TEMP"], os.environ)
