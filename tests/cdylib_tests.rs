//! Purpose:
//! End-to-end tests for library emission: compile PHP with `#[Export]` functions
//! into a shared library or static archive and assert the exported C ABI behaves
//! per the scalar and owned-string contracts.
//!
//! Called from:
//! - `cargo test --test cdylib_tests` through Rust's test harness.
//!
//! Key details:
//! - Tests invoke the elephc CLI binary as a subprocess (CARGO_BIN_EXE_elephc)
//!   inside an isolated temp dir with an isolated runtime cache, then compile
//!   a minimal C host with the system C compiler and run it.
//! - The platform symbol table is also asserted: internal globals stay private,
//!   while only documented boundary symbols and `#[Export]` trampolines remain.
//! - Host-target only: each platform/arch covers itself (macOS aarch64 runs
//!   locally, Linux x86_64/aarch64 run through the Docker test scripts).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

#[path = "cdylib/mbstring.rs"]
mod mbstring;

/// Creates an isolated temporary directory for one cdylib test, unique across
/// parallel test threads and processes.
fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("{}_{}_{:?}_{}", prefix, pid, tid, id));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Resolves the elephc CLI binary path for integration tests: prefers the
/// cargo-provided env var and falls back to locating the binary next to the
/// test executable (some environments do not propagate CARGO_BIN_EXE_* into
/// the test process environment).
fn elephc_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("failed to resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    })
}

/// Builds a `Command` for the elephc CLI rooted in `dir` with an isolated
/// runtime cache so parallel tests never share cached runtime objects.
fn elephc_command(dir: &Path) -> Command {
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd
}

/// Compiles one source as a cdylib and returns its expected failure diagnostic.
fn compile_cdylib_failure(prefix: &str, source: &str) -> String {
    let dir = make_test_dir(prefix);
    fs::write(dir.join("failure.php"), source).unwrap();
    let output = elephc_command(&dir)
        .args(["--emit", "cdylib", "failure.php"])
        .output()
        .expect("failed to run elephc");
    assert!(
        !output.status.success(),
        "unsafe cdylib source unexpectedly compiled"
    );
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    fs::remove_dir_all(&dir).ok();
    stderr
}

/// Compiles a multi-file cdylib fixture and returns its expected failure diagnostic.
fn compile_cdylib_files_failure(prefix: &str, files: &[(&str, &str)], entry: &str) -> String {
    let dir = make_test_dir(prefix);
    for (name, source) in files {
        fs::write(dir.join(name), source).unwrap();
    }
    let output = elephc_command(&dir)
        .args(["--emit", "cdylib", entry])
        .output()
        .expect("failed to run elephc");
    assert!(
        !output.status.success(),
        "unsafe multi-file cdylib source unexpectedly compiled"
    );
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    fs::remove_dir_all(&dir).ok();
    stderr
}

/// Runs one alternate cdylib validation terminal path and returns its failure diagnostic.
fn compile_cdylib_mode_failure(prefix: &str, source: &str, args: &[&str]) -> String {
    let dir = make_test_dir(prefix);
    fs::write(dir.join("failure.php"), source).unwrap();
    let output = elephc_command(&dir)
        .args(args)
        .arg("failure.php")
        .output()
        .expect("failed to run elephc");
    assert!(
        !output.status.success(),
        "unsafe cdylib source unexpectedly passed alternate validation mode"
    );
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    fs::remove_dir_all(&dir).ok();
    stderr
}

/// Returns the platform-conventional shared-library file name for `stem`.
fn shared_lib_name(stem: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("lib{}.dylib", stem)
    } else {
        format!("lib{}.so", stem)
    }
}

/// Compiles a C host program with the system C compiler, linking libdl on
/// Linux where `dlopen` lives outside libc on glibc systems.
fn compile_c_host(dir: &Path, source: &str, out_name: &str) -> PathBuf {
    let c_path = dir.join("host.c");
    fs::write(&c_path, source).unwrap();
    let out_path = dir.join(out_name);
    let mut cmd = Command::new("cc");
    cmd.arg("-o").arg(&out_path).arg(&c_path);
    if cfg!(target_os = "linux") {
        cmd.arg("-ldl");
    }
    let output = cmd.output().expect("failed to spawn the system C compiler");
    assert!(
        output.status.success(),
        "C host compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    out_path
}

/// Compiles a C host that links the generated library normally and embeds a
/// loader search path pointing beside the host executable.
fn compile_linked_c_host(dir: &Path, source: &str, out_name: &str, library: &str) -> PathBuf {
    let c_path = dir.join("linked-host.c");
    fs::write(&c_path, source).unwrap();
    let out_path = dir.join(out_name);
    let mut cmd = Command::new("cc");
    cmd.arg("-o")
        .arg(&out_path)
        .arg(&c_path)
        .arg("-I")
        .arg(dir)
        .arg("-L")
        .arg(dir)
        .arg(format!("-l{library}"));
    if cfg!(target_os = "macos") {
        cmd.arg("-Wl,-rpath,@loader_path");
    } else {
        cmd.arg("-Wl,-rpath,$ORIGIN").arg("-lm");
    }
    let output = cmd.output().expect("failed to spawn the system C compiler");
    assert!(
        output.status.success(),
        "linked C host compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    out_path
}

const EXPORT_PHP: &str = r#"<?php
function token_min_length(): int {
    return 8;
}

#[Export]
function validate_token(string $token): int {
    if (strlen($token) >= token_min_length()) {
        return 0;
    }
    return 1;
}

#[Export]
function add_i64(int $a, int $b): int {
    return $a + $b;
}

#[Export]
function scalar_throw(int $value): int {
    throw new RuntimeException("scalar boom");
}

#[Export]
function symbol_string(string $input): string {
    return $input;
}

#[Export]
function fixed_label(): string {
    return "fixed";
}

#[Export]
function fixed_throw(): string {
    throw new RuntimeException("fixed boom");
}

#[Export]
function compose_label(
    string $left,
    int $count,
    float $ratio,
    bool $enabled,
    string $right,
    int $extra_a,
    int $extra_b,
    int $extra_c,
): string {
    if ($count !== 7 || $ratio !== 1.5 || !$enabled ||
        $extra_a !== 11 || $extra_b !== 13 || $extra_c !== 17) {
        return "bad";
    }
    return $left . $right;
}
"#;

const HOST_C: &str = r#"
#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>
#include <stddef.h>
#include <string.h>

int main(int argc, char **argv) {
    if (argc != 2) return 1;
    void *lib = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (!lib) { fprintf(stderr, "dlopen: %s\n", dlerror()); return 2; }
    int32_t (*init)(void) = (int32_t (*)(void))dlsym(lib, "elephc_init");
    int32_t (*last_status)(void) = (int32_t (*)(void))dlsym(lib, "elephc_last_status");
    const char *(*last_error)(void) = (const char *(*)(void))dlsym(lib, "elephc_last_error");
    int64_t (*add)(int64_t, int64_t) = (int64_t (*)(int64_t, int64_t))dlsym(lib, "add_i64");
    int64_t (*scalar_throw)(int64_t) =
        (int64_t (*)(int64_t))dlsym(lib, "scalar_throw");
    int32_t (*vt)(const char *, size_t) =
        (int32_t (*)(const char *, size_t))dlsym(lib, "validate_token");
    int32_t (*fixed_label)(char **, size_t *) =
        (int32_t (*)(char **, size_t *))dlsym(lib, "fixed_label");
    int32_t (*fixed_throw)(char **, size_t *) =
        (int32_t (*)(char **, size_t *))dlsym(lib, "fixed_throw");
    int32_t (*compose_label)(const char *, size_t, int64_t, double, int64_t,
                             const char *, size_t, int64_t, int64_t, int64_t,
                             char **, size_t *) =
        (int32_t (*)(const char *, size_t, int64_t, double, int64_t,
                    const char *, size_t, int64_t, int64_t, int64_t,
                    char **, size_t *))dlsym(lib, "compose_label");
    void (*efree)(void *) = (void (*)(void *))dlsym(lib, "elephc_free");
    void (*shutdown)(void) = (void (*)(void))dlsym(lib, "elephc_shutdown");
    if (!init || !last_status || !last_error || !add || !scalar_throw || !vt ||
        !fixed_label || !fixed_throw || !compose_label || !efree || !shutdown) {
        fprintf(stderr, "dlsym failed\n"); return 3;
    }
    if (init() != 0) return 4;
    if (scalar_throw(7) != 0 || last_status() != 2 || !last_error()) return 5;
    if (add(40, 2) != 42 || last_status() != 0 || last_error() != NULL) return 6;
    char *output = (char *)(uintptr_t)1;
    size_t output_len = 99;
    if (fixed_throw(&output, &output_len) != 2 || output != NULL || output_len != 0 ||
        last_status() != 2 || !last_error() || !strstr(last_error(), "fixed boom")) return 7;
    output = NULL;
    output_len = 0;
    if (fixed_label(&output, &output_len) != 0 || !output || output_len != 5 ||
        memcmp(output, "fixed", 5) != 0 || last_error() != NULL) return 8;
    efree(output);
    output = NULL;
    output_len = 0;
    if (compose_label("left", 4, 7, 1.5, 1, "right", 5, 11, 13, 17,
                      &output, &output_len) != 0 ||
        !output || output_len != 9 || memcmp(output, "leftright", 9) != 0) return 9;
    efree(output);
    output = (char *)(uintptr_t)1;
    output_len = 99;
    if (compose_label(NULL, 1, 7, 1.5, 1, "right", 5, 11, 13, 17,
                      &output, &output_len) != 1 ||
        output != NULL || output_len != 0 || last_status() != 1 || !last_error()) return 10;
    printf("%lld %d %d\n", (long long)add(40, 2), vt("supersecret", 11), vt("nope", 4));
    shutdown();
    return 0;
}
"#;

const STATICLIB_HOST_C: &str = r#"
#include "libauth.h"
#include <stdint.h>
#include <stdio.h>
#include <string.h>

int main(void) {
    if (elephc_abi_version() != ELEPHC_ABI_VERSION ||
        elephc_init() != ELEPHC_STATUS_OK) return 1;
    char *output = NULL;
    size_t output_len = 0;
    if (symbol_string("static", 6, &output, &output_len) != ELEPHC_STATUS_OK ||
        !output || output_len != 6 || memcmp(output, "static", 6) != 0) return 2;
    elephc_free(output);
    output = NULL;
    output_len = 0;
    if (fixed_label(&output, &output_len) != ELEPHC_STATUS_OK ||
        !output || output_len != 5 || memcmp(output, "fixed", 5) != 0) return 3;
    elephc_free(output);
    output = NULL;
    output_len = 0;
    if (compose_label("left", 4, 7, 1.5, 1, "right", 5, 11, 13, 17,
                      &output, &output_len) !=
            ELEPHC_STATUS_OK ||
        !output || output_len != 9 || memcmp(output, "leftright", 9) != 0) return 4;
    printf("%.*s %lld\n", (int)output_len, output, (long long)add_i64(40, 2));
    elephc_free(output);
    elephc_shutdown();
    return 0;
}
"#;

const STRING_EXPORT_PHP: &str = r#"<?php
#[Export]
function roundtrip(string $input): string {
    return $input;
}

function stage_b_throw_helper(string $message): string {
    throw new RuntimeException($message);
}

function stage_b_cleanup_throw_helper(string $input): string {
    $owned = str_repeat($input, 1024);
    if (strlen($owned) > 0) {
        throw new RuntimeException("cleanup boom");
    }
    return $owned;
}

#[Export]
function maybe_throw(string $input): string {
    if ($input === "throw") {
        return stage_b_throw_helper("stage-b boom");
    }
    return $input;
}

#[Export]
function empty_throw(string $input): string {
    throw new RuntimeException("");
}

#[Export]
function cleanup_throw(string $input): string {
    return stage_b_cleanup_throw_helper($input);
}

#[Export]
function concat_success(string $input): string {
    return $input . "!";
}

#[Export]
function force_allocation_failure(string $input): string {
    return str_repeat($input, 70000);
}

#[Export]
function fixed_allocation_failure(): string {
    return str_repeat("x", 70000);
}

#[Export]
function add_after_failure(int $a, int $b): int {
    return $a + $b;
}
"#;

const STRING_HOST_C: &str = r#"
#include "libstrings.h"
#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define LOAD(symbol, variable, type) type variable = (type)dlsym(lib, #symbol)

typedef uint32_t (*abi_version_fn)(void);
typedef int32_t (*init_fn)(void);
typedef void (*shutdown_fn)(void);
typedef int32_t (*last_status_fn)(void);
typedef const char *(*last_error_fn)(void);
typedef void (*free_fn)(void *);
typedef int32_t (*string_export_fn)(const char *, size_t, char **, size_t *);
typedef int32_t (*zero_string_export_fn)(char **, size_t *);
typedef int64_t (*add_fn)(int64_t, int64_t);

int main(int argc, char **argv) {
    if (argc != 2) return 1;
    void *lib = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (!lib) { fprintf(stderr, "dlopen: %s\n", dlerror()); return 2; }

    LOAD(elephc_abi_version, p_abi_version, abi_version_fn);
    LOAD(elephc_init, p_init, init_fn);
    LOAD(elephc_shutdown, p_shutdown, shutdown_fn);
    LOAD(elephc_last_status, p_last_status, last_status_fn);
    LOAD(elephc_last_error, p_last_error, last_error_fn);
    LOAD(elephc_free, p_free, free_fn);
    LOAD(roundtrip, p_roundtrip, string_export_fn);
    LOAD(maybe_throw, p_maybe_throw, string_export_fn);
    LOAD(empty_throw, p_empty_throw, string_export_fn);
    LOAD(cleanup_throw, p_cleanup_throw, string_export_fn);
    LOAD(concat_success, p_concat_success, string_export_fn);
    LOAD(force_allocation_failure, p_force_allocation_failure, string_export_fn);
    LOAD(fixed_allocation_failure, p_fixed_allocation_failure, zero_string_export_fn);
    LOAD(add_after_failure, p_add_after_failure, add_fn);
    if (!p_abi_version || !p_init || !p_shutdown || !p_last_status || !p_last_error || !p_free ||
        !p_roundtrip || !p_maybe_throw || !p_empty_throw || !p_cleanup_throw ||
        !p_concat_success || !p_force_allocation_failure || !p_fixed_allocation_failure ||
        !p_add_after_failure)
        return 3;
    if (p_abi_version() != ELEPHC_ABI_VERSION ||
        p_init() != ELEPHC_STATUS_OK) return 4;

    const unsigned char binary[] = {'A', 0, 'B', 0xff, 'Z'};
    char *out = (char *)(uintptr_t)1;
    size_t out_len = 99;
    if (p_roundtrip((const char *)binary, sizeof(binary), &out, &out_len) != ELEPHC_STATUS_OK)
        return 5;
    if (!out || out_len != sizeof(binary) || memcmp(out, binary, sizeof(binary)) != 0 ||
        p_last_status() != ELEPHC_STATUS_OK ||
        out[out_len] != '\0') return 6;
    p_free(out);

    out = (char *)(uintptr_t)1;
    out_len = 99;
    if (p_roundtrip(NULL, 0, &out, &out_len) != ELEPHC_STATUS_OK || !out || out_len != 0 ||
        out[0] != '\0') return 7;
    p_free(out);
    p_free(NULL);

    out_len = 99;
    if (p_roundtrip("x", 1, NULL, &out_len) != ELEPHC_STATUS_INVALID_ARGUMENT ||
        p_last_status() != ELEPHC_STATUS_INVALID_ARGUMENT || out_len != 0 ||
        !p_last_error()) return 8;
    out = (char *)(uintptr_t)1;
    if (p_roundtrip("x", 1, &out, NULL) != ELEPHC_STATUS_INVALID_ARGUMENT ||
        out != NULL || !p_last_error()) return 9;

    const unsigned char utf8[] = {0xe2, 0x82, 0xac, 0xf0, 0x9f, 0x98, 0x80};
    out = NULL;
    out_len = 0;
    if (p_roundtrip((const char *)utf8, sizeof(utf8), &out, &out_len) !=
            ELEPHC_STATUS_OK || out_len != sizeof(utf8) ||
        memcmp(out, utf8, sizeof(utf8)) != 0) return 10;
    p_free(out);

    size_t long_len = 8192;
    char *long_input = malloc(long_len);
    if (!long_input) return 11;
    for (size_t i = 0; i < long_len; ++i) long_input[i] = (char)(i * 37u);
    out = NULL;
    out_len = 0;
    if (p_roundtrip(long_input, long_len, &out, &out_len) != ELEPHC_STATUS_OK ||
        out_len != long_len || memcmp(out, long_input, long_len) != 0) return 12;
    p_free(out);
    free(long_input);

    for (int iteration = 0; iteration < 10000; ++iteration) {
        out = NULL;
        out_len = 0;
        if (p_roundtrip("repeat", 6, &out, &out_len) != ELEPHC_STATUS_OK ||
            out_len != 6 || memcmp(out, "repeat", 6) != 0) return 13;
        p_free(out);
    }

    for (int iteration = 0; iteration < 10000; ++iteration) {
        out = NULL;
        out_len = 0;
        if (p_concat_success("x", 1, &out, &out_len) != ELEPHC_STATUS_OK ||
            out_len != 2 || memcmp(out, "x!", 2) != 0) return 14;
        p_free(out);
    }

    out = (char *)(uintptr_t)1;
    out_len = 99;
    if (p_roundtrip(NULL, 1, &out, &out_len) != ELEPHC_STATUS_INVALID_ARGUMENT ||
        out != NULL || out_len != 0 || !p_last_error()) return 15;

    out = (char *)(uintptr_t)1;
    out_len = 99;
    if (p_maybe_throw("throw", 5, &out, &out_len) != ELEPHC_STATUS_PHP_EXCEPTION ||
        out != NULL || out_len != 0) return 16;
    const char *error = p_last_error();
    if (!error || !strstr(error, "stage-b boom")) return 17;
    if (p_add_after_failure(40, 2) != 42 || p_last_error() != NULL) return 18;

    out = (char *)(uintptr_t)1;
    out_len = 99;
    if (p_empty_throw("ignored", 7, &out, &out_len) != ELEPHC_STATUS_PHP_EXCEPTION ||
        p_last_status() != ELEPHC_STATUS_PHP_EXCEPTION || out != NULL || out_len != 0)
        return 19;
    error = p_last_error();
    if (!error || strcmp(error, "") != 0) return 20;

    for (int iteration = 0; iteration < 256; ++iteration) {
        out = (char *)(uintptr_t)1;
        out_len = 99;
        int32_t cleanup_status = p_cleanup_throw("z", 1, &out, &out_len);
        if (cleanup_status != ELEPHC_STATUS_PHP_EXCEPTION || out != NULL || out_len != 0) {
            fprintf(stderr, "cleanup iteration=%d status=%d out=%p len=%zu error=%s\n",
                    iteration, cleanup_status, (void *)out, out_len,
                    p_last_error() ? p_last_error() : "(null)");
            return 21;
        }
        error = p_last_error();
        if (!error || !strstr(error, "cleanup boom")) return 22;
    }

    out = (char *)(uintptr_t)1;
    out_len = 99;
    if (p_force_allocation_failure("x", 1, &out, &out_len) !=
            ELEPHC_STATUS_ALLOCATION_FAILURE ||
        p_last_status() != ELEPHC_STATUS_ALLOCATION_FAILURE || out != NULL || out_len != 0)
        return 23;
    error = p_last_error();
    if (!error || !strstr(error, "allocation failed")) return 24;

    out = (char *)(uintptr_t)1;
    out_len = 99;
    if (p_fixed_allocation_failure(&out, &out_len) !=
            ELEPHC_STATUS_ALLOCATION_FAILURE ||
        p_last_status() != ELEPHC_STATUS_ALLOCATION_FAILURE || out != NULL || out_len != 0)
        return 25;
    error = p_last_error();
    if (!error || !strstr(error, "allocation failed")) return 26;
    if (p_add_after_failure(20, 22) != 42 || p_last_error() != NULL) return 27;

    out = NULL;
    out_len = 0;
    if (p_roundtrip("alive", 5, &out, &out_len) != ELEPHC_STATUS_OK ||
        out_len != 5 || memcmp(out, "alive", 5) != 0 || p_last_error() != NULL)
        return 28;
    p_free(out);
    p_shutdown();
    dlclose(lib);
    return 0;
}
"#;

const LINKED_STRING_HOST_C: &str = r#"
#include "libstrings.h"
#include <stdint.h>
#include <string.h>

int main(void) {
    const char input[] = {'l', 'i', 'n', 'k', 0, 'o', 'k'};
    char *output = NULL;
    size_t output_len = 0;
    if (elephc_init() != ELEPHC_STATUS_OK ||
        roundtrip(input, sizeof(input), &output, &output_len) != ELEPHC_STATUS_OK ||
        output_len != sizeof(input) || memcmp(input, output, sizeof(input)) != 0)
        return 1;
    elephc_free(output);
    elephc_shutdown();
    return 0;
}
"#;

const STACK_GUARD_EXPORT_PHP: &str = r#"<?php
function deep(int $depth): int {
    if ($depth >= 100000) {
        return $depth;
    }
    return deep($depth + 1);
}

#[Export]
function deep_probe(string $input): string {
    return (string) deep(0);
}

#[Export]
function after_overflow(string $input): string {
    return "HOST-ALIVE";
}
"#;

const STACK_GUARD_HOST_C: &str = r#"
#include "libstack_guard.h"
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/resource.h>

static int constrain_stack_for_guard_test(void) {
    struct rlimit limit;
    const rlim_t test_budget = 512 * 1024;
    if (getrlimit(RLIMIT_STACK, &limit) != 0) return 0;
    if (limit.rlim_max != RLIM_INFINITY && limit.rlim_max < test_budget) return 0;
    limit.rlim_cur = test_budget;
    return setrlimit(RLIMIT_STACK, &limit) == 0;
}

int main(void) {
    if (!constrain_stack_for_guard_test()) return 90;
    if (elephc_init() != ELEPHC_STATUS_OK) return 1;

    char *output = (char *)(uintptr_t)1;
    size_t output_len = 99;
    int32_t status = deep_probe("", 0, &output, &output_len);
    if (status != ELEPHC_STATUS_RUNTIME_FAILURE ||
        elephc_last_status() != ELEPHC_STATUS_RUNTIME_FAILURE ||
        output != NULL || output_len != 0 || elephc_last_error() == NULL) return 2;

    output = NULL;
    output_len = 0;
    status = after_overflow("", 0, &output, &output_len);
    if (status != ELEPHC_STATUS_OK || output == NULL || output_len != 10 ||
        memcmp(output, "HOST-ALIVE", 10) != 0) return 3;

    puts("HOST-ALIVE");
    elephc_free(output);
    elephc_shutdown();
    return 0;
}
"#;

const STACK_GUARD_SKIP_INIT_HOST_C: &str = r#"
#include "libstack_guard.h"
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/resource.h>

static int constrain_stack_for_guard_test(void) {
    struct rlimit limit;
    const rlim_t test_budget = 512 * 1024;
    if (getrlimit(RLIMIT_STACK, &limit) != 0) return 0;
    if (limit.rlim_max != RLIM_INFINITY && limit.rlim_max < test_budget) return 0;
    limit.rlim_cur = test_budget;
    return setrlimit(RLIMIT_STACK, &limit) == 0;
}

int main(void) {
    if (!constrain_stack_for_guard_test()) return 90;
    char *output = (char *)(uintptr_t)1;
    size_t output_len = 99;
    int32_t status = deep_probe("", 0, &output, &output_len);
    if (status != ELEPHC_STATUS_RUNTIME_FAILURE ||
        elephc_last_status() != ELEPHC_STATUS_RUNTIME_FAILURE ||
        output != NULL || output_len != 0 || elephc_last_error() == NULL) return 1;

    output = NULL;
    output_len = 0;
    status = after_overflow("", 0, &output, &output_len);
    if (status != ELEPHC_STATUS_OK || output == NULL || output_len != 10 ||
        memcmp(output, "HOST-ALIVE", 10) != 0) return 2;

    puts("HOST-ALIVE");
    elephc_free(output);
    elephc_shutdown();
    return 0;
}
"#;

const BUFFER_FAILURE_EXPORT_PHP: &str = r#"<?php
#[Export]
function buffer_uaf(int $value): int {
    buffer<int> $buffer = buffer_new<int>(1);
    buffer_free($buffer);
    $buffer[0] = $value;
    return 99;
}

#[Export]
function buffer_oob(int $value): int {
    buffer<int> $buffer = buffer_new<int>(1);
    $buffer[5] = $value;
    return 99;
}

#[Export]
function after_buffer_failure(int $value): int {
    return $value + 1;
}
"#;

const BUFFER_FAILURE_HOST_C: &str = r#"
#include "libbuffer_failures.h"
#include <stdint.h>
#include <stdio.h>

int main(void) {
    if (after_buffer_failure(41) != 42 ||
        elephc_last_status() != ELEPHC_STATUS_OK ||
        elephc_last_error() != NULL) return 1;

    if (buffer_uaf(7) != 0 ||
        elephc_last_status() != ELEPHC_STATUS_RUNTIME_FAILURE ||
        elephc_last_error() == NULL) return 2;
    if (after_buffer_failure(41) != 42 ||
        elephc_last_status() != ELEPHC_STATUS_OK ||
        elephc_last_error() != NULL) return 3;

    if (buffer_oob(7) != 0 ||
        elephc_last_status() != ELEPHC_STATUS_RUNTIME_FAILURE ||
        elephc_last_error() == NULL) return 4;
    if (after_buffer_failure(41) != 42 ||
        elephc_last_status() != ELEPHC_STATUS_OK ||
        elephc_last_error() != NULL) return 5;

    puts("HOST-ALIVE");
    elephc_shutdown();
    return 0;
}
"#;

const BUFFER_ALLOCATION_FAILURE_EXPORT_PHP: &str = r#"<?php
#[Export]
function buffer_size_fail(int $length): int {
    buffer<int> $buffer = buffer_new<int>($length);
    return buffer_len($buffer);
}

#[Export]
function buffer_registry_exhaust(int $value): int {
    for ($i = 0; $i < 4097; $i++) {
        buffer_new<int>(1);
    }
    return $value;
}

#[Export]
function after_buffer_allocation_failure(int $value): int {
    return $value + 1;
}
"#;

const BUFFER_ALLOCATION_FAILURE_HOST_C: &str = r#"
#include "libbuffer_allocation_failures.h"
#include <stdint.h>
#include <stdio.h>

int main(void) {
    if (elephc_init() != ELEPHC_STATUS_OK) return 1;

    if (buffer_size_fail(-1) != 0 ||
        elephc_last_status() != ELEPHC_STATUS_RUNTIME_FAILURE ||
        elephc_last_error() == NULL) return 2;
    if (after_buffer_allocation_failure(41) != 42 ||
        elephc_last_status() != ELEPHC_STATUS_OK ||
        elephc_last_error() != NULL) return 3;

    if (buffer_size_fail(INT64_C(2305843009213693952)) != 0 ||
        elephc_last_status() != ELEPHC_STATUS_RUNTIME_FAILURE ||
        elephc_last_error() == NULL) return 4;
    if (after_buffer_allocation_failure(41) != 42 ||
        elephc_last_status() != ELEPHC_STATUS_OK ||
        elephc_last_error() != NULL) return 5;

    if (buffer_registry_exhaust(7) != 0 ||
        elephc_last_status() != ELEPHC_STATUS_RUNTIME_FAILURE ||
        elephc_last_error() == NULL) return 6;
    if (after_buffer_allocation_failure(41) != 42 ||
        elephc_last_status() != ELEPHC_STATUS_OK ||
        elephc_last_error() != NULL) return 7;

    puts("HOST-ALIVE");
    elephc_shutdown();
    return 0;
}
"#;

const NAMESPACED_EXPORT_PHP: &str = r#"<?php
namespace Demo;

#[Export]
function add(int $left, int $right): int {
    return $left + $right;
}

#[Export]
function roundtrip(string $input): string {
    return $input;
}
"#;

const NAMESPACED_HOST_C: &str = r#"
#include "libnamespaced.h"
#include <string.h>

int main(void) {
    char *output = NULL;
    size_t output_len = 0;
    if (elephc_init() != ELEPHC_STATUS_OK || Demo_add(19, 23) != 42 ||
        Demo_roundtrip("namespace", 9, &output, &output_len) != ELEPHC_STATUS_OK ||
        !output || output_len != 9 || memcmp(output, "namespace", 9) != 0)
        return 1;
    elephc_free(output);
    elephc_shutdown();
    return 0;
}
"#;

/// Verifies the full cdylib path on the host target: `--emit cdylib` produces
/// a conventionally named shared library, a C host can dlopen it, resolve the
/// lifecycle entry points plus both `#[Export]` trampolines, and the exported
/// functions compute correct results for int and (ptr, len) string arguments.
#[test]
fn test_cdylib_builds_and_host_calls_exports() {
    let dir = make_test_dir("elephc_cdylib_e2e");
    fs::write(dir.join("auth.php"), EXPORT_PHP).unwrap();

    let output = elephc_command(&dir)
        .args(["--emit", "cdylib", "auth.php"])
        .output()
        .expect("failed to run elephc");
    assert!(
        output.status.success(),
        "cdylib compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let lib_path = dir.join(shared_lib_name("auth"));
    assert!(lib_path.exists(), "expected shared library at {:?}", lib_path);

    let host = compile_c_host(&dir, HOST_C, "host");
    let run = Command::new(&host)
        .arg(&lib_path)
        .output()
        .expect("failed to run the C host");
    assert!(
        run.status.success(),
        "C host run failed (exit {:?}):\n{}",
        run.status.code(),
        String::from_utf8_lossy(&run.stderr)
    );

    assert_eq!(String::from_utf8_lossy(&run.stdout), "42 0 1\n");

    fs::remove_dir_all(&dir).ok();
}

/// Verifies `--emit staticlib` produces an indexed archive and matching C header
/// that can be linked directly into a native host without `dlopen`.
#[test]
fn test_staticlib_links_directly_into_a_host_binary() {
    let dir = make_test_dir("elephc_staticlib_e2e");
    fs::write(dir.join("auth.php"), EXPORT_PHP).unwrap();

    let output = elephc_command(&dir)
        .args(["--emit", "staticlib", "auth.php"])
        .output()
        .expect("failed to run elephc");
    assert!(
        output.status.success(),
        "staticlib compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let archive = dir.join("libauth.a");
    let header = dir.join("libauth.h");
    assert!(archive.exists(), "expected archive at {archive:?}");
    assert!(header.exists(), "expected generated header at {header:?}");

    let listing = Command::new("ar")
        .arg("t")
        .arg(&archive)
        .output()
        .expect("failed to list generated archive");
    assert!(listing.status.success(), "ar failed to inspect {archive:?}");
    let members = String::from_utf8_lossy(&listing.stdout);
    assert!(members.contains("auth.o"), "missing user object: {members}");
    assert!(members.contains("runtime-"), "missing runtime object: {members}");

    let host = compile_linked_c_host(&dir, STATICLIB_HOST_C, "static-host", "auth");
    let run = Command::new(&host)
        .output()
        .expect("failed to run statically linked C host");
    assert!(
        run.status.success(),
        "static host failed (exit {:?}):\n{}",
        run.status.code(),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "leftright 42\n");

    fs::remove_dir_all(&dir).ok();
}

/// Regenerates both iOS showcase headers and compiles their Swift bridging
/// wrappers against ABI v3 so no host can retain a copied obsolete signature.
#[test]
fn test_ios_showcase_bridging_headers_compile_against_generated_abi_v3() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (label, source_path, wrapper_path, expected_prototype) in [
        (
            "view",
            "examples/swiftui-view-protocol/main.php",
            "examples/swiftui-view-protocol/elephc_abi.h",
            "int32_t render_view(char **output_ptr, size_t *output_len);",
        ),
        (
            "probe",
            "examples/ios-device-probe/main.php",
            "examples/ios-device-probe/probe_abi.h",
            "int32_t probe(const char *writableDir_ptr, size_t writableDir_len, char **output_ptr, size_t *output_len);",
        ),
    ] {
        let dir = make_test_dir(&format!("elephc_ios_{label}_header"));
        let source_name = "main.php";
        fs::write(dir.join(&source_name), fs::read(root.join(source_path)).unwrap()).unwrap();
        let wrapper_name = Path::new(wrapper_path).file_name().unwrap();
        fs::write(dir.join(wrapper_name), fs::read(root.join(wrapper_path)).unwrap()).unwrap();

        let output = elephc_command(&dir)
            .args(["--emit", "staticlib", &source_name])
            .output()
            .expect("failed to run elephc");
        assert!(
            output.status.success(),
            "{label} staticlib compilation failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let generated = fs::read_to_string(dir.join("libmain.h")).unwrap();
        assert!(
            generated.contains("#define ELEPHC_ABI_VERSION UINT32_C(3)")
                && generated.contains(expected_prototype),
            "generated {label} header did not expose the expected ABI-v3 contract:\n{generated}"
        );

        let host = dir.join("header-host.c");
        fs::write(
            &host,
            format!("#include \"{}\"\nint main(void) {{ return 0; }}\n", wrapper_name.to_string_lossy()),
        )
        .unwrap();
        let compile = Command::new("cc")
            .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-fsyntax-only"])
            .arg("-I")
            .arg(&dir)
            .arg(&host)
            .output()
            .expect("failed to compile the showcase bridging header");
        assert!(
            compile.status.success(),
            "{wrapper_path} disagrees with the generated header:\n{}",
            String::from_utf8_lossy(&compile.stderr)
        );

        fs::remove_dir_all(&dir).ok();
    }
}

/// Verifies host-process builtins are rejected for both iOS target variants.
#[test]
fn test_host_process_builtins_are_refused_for_ios_targets() {
    let dir = make_test_dir("elephc_ios_capability");

    for (builtin, source) in [
        ("system", r#"<?php system("ls");"#),
        ("passthru", r#"<?php passthru("ls");"#),
        ("exec", r#"<?php exec("ls");"#),
        ("shell_exec", r#"<?php shell_exec("ls");"#),
        ("popen", r#"<?php popen("ls", "r");"#),
        ("pclose", r#"<?php $handle = fopen("php://memory", "r+"); pclose($handle);"#),
        ("pcntl_fork", r#"<?php pcntl_fork();"#),
        ("pcntl_signal", r#"<?php pcntl_signal(15, 0);"#),
        ("pcntl_exec", r#"<?php pcntl_exec("/bin/true");"#),
        ("pcntl_wait", r#"<?php $status = 0; pcntl_wait($status);"#),
        ("pcntl_alarm", r#"<?php pcntl_alarm(1);"#),
        ("pcntl_daemon", r#"<?php pcntl_daemon(true, true);"#),
        ("posix_setpgid", r#"<?php posix_setpgid(0, 0);"#),
        ("posix_setsid", r#"<?php posix_setsid();"#),
    ] {
        fs::write(dir.join("spawn.php"), source).unwrap();

        for target in ["ios-arm64", "ios-sim-arm64"] {
            let refused = elephc_command(&dir)
                .args(["--check", "--target", target, "spawn.php"])
                .output()
                .expect("failed to run elephc");
            assert!(
                !refused.status.success(),
                "{builtin} must not type-check for {target}"
            );
            let message = String::from_utf8_lossy(&refused.stderr);
            assert!(
                message.contains(&format!("{builtin}()")),
                "{builtin}: diagnostic must name the builtin, got: {message}"
            );
            assert!(
                message.contains(target),
                "{builtin}: diagnostic must name {target}, got: {message}"
            );
            assert!(
                message.contains("error["),
                "{builtin}: diagnostic must carry a source position, got: {message}"
            );
        }

        let accepted = elephc_command(&dir)
            .args(["--check", "spawn.php"])
            .output()
            .expect("failed to run elephc");
        assert!(
            accepted.status.success(),
            "{builtin} must still type-check for the host:\n{}",
            String::from_utf8_lossy(&accepted.stderr)
        );
    }

    fs::remove_dir_all(&dir).ok();
}

/// Verifies an iOS target cannot accidentally produce Elephc's standalone CLI
/// executable shape; consumers must choose a host-linked library artifact.
#[test]
fn test_ios_targets_reject_executable_output_with_library_guidance() {
    let dir = make_test_dir("elephc_ios_executable_refuse");
    fs::write(dir.join("main.php"), "<?php echo 'not an app bundle';").unwrap();

    for target in ["ios-arm64", "ios-sim-arm64"] {
        let output = elephc_command(&dir)
            .args(["--target", target, "--emit", "executable", "main.php"])
            .output()
            .expect("failed to run elephc");
        assert!(!output.status.success(), "{target} executable must be rejected");
        let message = String::from_utf8_lossy(&output.stderr);
        assert!(
            message.contains("iOS targets do not emit standalone executables")
                && message.contains("--emit staticlib"),
            "{target}: missing actionable diagnostic: {message}"
        );
    }

    let check = elephc_command(&dir)
        .args(["--target", "ios-arm64", "--check", "main.php"])
        .output()
        .expect("failed to check iOS source");
    assert!(
        check.status.success(),
        "analysis-only iOS mode must remain available:\n{}",
        String::from_utf8_lossy(&check.stderr)
    );

    fs::remove_dir_all(&dir).ok();
}

/// Both iOS targets diagnose unsupported Async execution while preserving analysis-only mode.
#[test]
fn test_ios_targets_reject_async_library_execution_but_allow_analysis() {
    let dir = make_test_dir("elephc_ios_async_refuse");
    fs::write(
        dir.join("main.php"),
        "<?php use Elephc\\Async\\TaskGroup; use function Elephc\\Async\\run; run(function (TaskGroup $tasks): void {});",
    )
    .unwrap();

    for target in ["ios-arm64", "ios-sim-arm64"] {
        let emitted = elephc_command(&dir)
            .args(["--target", target, "--emit", "staticlib", "main.php"])
            .output()
            .expect("failed to compile Async for iOS");
        assert!(!emitted.status.success(), "{target} must reject Async execution");
        let message = String::from_utf8_lossy(&emitted.stderr);
        assert!(
            message.contains("Elephc Async execution is unavailable")
                && message.contains("host-driven Fiber scheduler contract")
                && message.contains("Use --check"),
            "{target}: missing actionable Async diagnostic: {message}"
        );

        let checked = elephc_command(&dir)
            .args(["--target", target, "--check", "main.php"])
            .output()
            .expect("failed to analyze Async for iOS");
        assert!(
            checked.status.success(),
            "{target} analysis must remain available:\n{}",
            String::from_utf8_lossy(&checked.stderr)
        );
    }

    fs::remove_dir_all(&dir).ok();
}

/// Both iOS targets diagnose unsupported worker execution while preserving analysis-only mode.
#[test]
fn test_ios_targets_reject_parallel_library_execution_but_allow_analysis() {
    let dir = make_test_dir("elephc_ios_parallel_refuse");
    fs::write(
        dir.join("main.php"),
        "<?php use Elephc\\Parallel\\TaskGroup; use function Elephc\\Parallel\\run; run(static function (TaskGroup $tasks): int { return $tasks->spawn(static fn (): int => 1)->join(); });",
    )
    .unwrap();

    for target in ["ios-arm64", "ios-sim-arm64"] {
        let emitted = elephc_command(&dir)
            .args(["--target", target, "--emit", "staticlib", "main.php"])
            .output()
            .expect("failed to compile Parallel for iOS");
        assert!(!emitted.status.success(), "{target} must reject Parallel execution");
        let message = String::from_utf8_lossy(&emitted.stderr);
        assert!(
            message.contains("Elephc Parallel execution is unavailable")
                && message.contains("host-driven worker-thread contract")
                && message.contains("Use --check"),
            "{target}: missing actionable Parallel diagnostic: {message}"
        );

        let checked = elephc_command(&dir)
            .args(["--target", target, "--check", "main.php"])
            .output()
            .expect("failed to analyze Parallel for iOS");
        assert!(
            checked.status.success(),
            "{target} Parallel analysis must remain available:\n{}",
            String::from_utf8_lossy(&checked.stderr)
        );
    }

    fs::remove_dir_all(&dir).ok();
}

/// Cross-emits an iOS AArch64 static-library boundary and pins both zero-input
/// and mixed-input string-return shapes without relying on the CI host ABI.
#[test]
fn test_ios_aarch64_emit_asm_pins_string_return_out_parameters() {
    let dir = make_test_dir("elephc_ios_aarch64_string_abi");
    fs::write(
        dir.join("main.php"),
        r#"<?php
#[Export]
function render_view(): string {
    return "view";
}

#[Export]
function dispatch(string $action, int $count, float $ratio, bool $enabled): string {
    return $action . $count . $ratio . $enabled;
}
"#,
    )
    .unwrap();

    let output = elephc_command(&dir)
        .args([
            "--target",
            "ios-arm64",
            "--emit",
            "staticlib",
            "--emit-asm",
            "main.php",
        ])
        .output()
        .expect("failed to cross-emit the iOS library assembly");
    assert!(
        output.status.success(),
        "iOS AArch64 assembly emission failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let asm = fs::read_to_string(dir.join("main.s")).expect("missing iOS AArch64 assembly");
    let dispatch_start = asm
        .find(".globl _dispatch\n_dispatch:")
        .expect("missing public dispatch boundary");
    let render_start = asm
        .find(".globl _render_view\n_render_view:")
        .expect("missing public render_view boundary");
    let lifecycle_start = asm
        .find(".globl _elephc_abi_version\n_elephc_abi_version:")
        .expect("missing public ABI-version boundary");
    assert!(dispatch_start < render_start && render_start < lifecycle_start);

    let dispatch = &asm[dispatch_start..render_start];
    for required in [
        "stur x0, [x29, #-8]",
        "stur x1, [x29, #-16]",
        "stur x2, [x29, #-24]",
        "stur d0, [x29, #-32]",
        "stur x3, [x29, #-40]",
        "stur x4, [x29, #-48]",
        "stur x5, [x29, #-56]",
    ] {
        assert!(
            dispatch.contains(required),
            "dispatch boundary is missing `{required}`"
        );
    }

    let render = &asm[render_start..lifecycle_start];
    for required in [
        "stur x0, [x29, #-8]",
        "stur x1, [x29, #-16]",
    ] {
        assert!(
            render.contains(required),
            "render_view boundary is missing `{required}`"
        );
    }
    assert!(asm[lifecycle_start..].contains("mov w0, #3"));

    fs::remove_dir_all(&dir).ok();
}

/// Keeps a backtrace-triggered frame collector internal to the exported PHP invocation.
#[test]
fn test_unrelated_backtrace_keeps_export_header_and_trampoline_fixed() {
    let dir = make_test_dir("elephc_cdylib_hidden_collector");
    fs::write(
        dir.join("collector.php"),
        r#"<?php
#[Export]
function roundtrip(string $input): string {
    return $input;
}

function unrelated_trace(): array {
    return debug_backtrace();
}
"#,
    )
    .unwrap();

    let output = elephc_command(&dir)
        .args(["--emit", "staticlib", "collector.php"])
        .output()
        .expect("failed to compile the collector-bearing static library");
    assert!(
        output.status.success(),
        "collector-bearing library compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let header = fs::read_to_string(dir.join("libcollector.h"))
        .expect("missing generated collector header");
    assert!(header.contains(
        "int32_t roundtrip(const char *input_ptr, size_t input_len, char **output_ptr, size_t *output_len);"
    ));
    assert!(!header.contains("__elephc_func_args"));

    let asm = fs::read_to_string(dir.join("collector.s"))
        .expect("missing collector-bearing library assembly");
    let public_label = if cfg!(target_os = "macos") {
        "\n_roundtrip:"
    } else {
        "\nroundtrip:"
    };
    let lifecycle_label = if cfg!(target_os = "macos") {
        "\n_elephc_abi_version:"
    } else {
        "\nelephc_abi_version:"
    };
    let start = asm.find(public_label).expect("missing public roundtrip wrapper");
    let end = asm[start..]
        .find(lifecycle_label)
        .map(|offset| start + offset)
        .expect("missing lifecycle boundary after roundtrip");
    let wrapper = &asm[start..end];
    let setjmp = wrapper.find("setjmp").expect("missing recoverable boundary");
    let allocate = wrapper
        .find("__rt_array_new")
        .expect("missing empty internal collector allocation");
    let publish = wrapper
        .find("__rt_cleanup_call_operand_owner")
        .expect("missing collector unwind owner");
    let invoke = wrapper
        .find("_fn_roundtrip")
        .expect("missing internal PHP invocation");
    let release = wrapper[invoke..]
        .find("__rt_decref_any")
        .map(|offset| invoke + offset)
        .expect("missing normal collector release");
    assert!(setjmp < allocate && allocate < publish && publish < invoke && invoke < release);

    fs::remove_dir_all(&dir).ok();
}

/// Verifies the Stage B owned-string ABI with a real C host, including embedded
/// NUL bytes, empty/long buffers, repeated allocation/free, structured argument
/// errors, an escaping PHP exception, last-error reset, and post-failure reuse.
#[test]
fn test_cdylib_owned_string_boundary_is_binary_safe_and_recoverable() {
    let dir = make_test_dir("elephc_cdylib_strings");
    fs::write(dir.join("strings.php"), STRING_EXPORT_PHP).unwrap();

    let output = elephc_command(&dir)
        .args(["--emit", "cdylib", "--heap-size=65536", "strings.php"])
        .output()
        .expect("failed to run elephc");
    assert!(
        output.status.success(),
        "cdylib compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let lib_path = dir.join(shared_lib_name("strings"));
    let header_path = dir.join("libstrings.h");
    assert!(lib_path.exists(), "expected shared library at {:?}", lib_path);
    assert!(header_path.exists(), "expected generated header at {:?}", header_path);
    let first_header = fs::read(&header_path).expect("failed to read generated header");
    let repeated = elephc_command(&dir)
        .args([
            "--emit",
            "cdylib",
            "--heap-debug",
            "--heap-size=65536",
            "strings.php",
        ])
        .output()
        .expect("failed to repeat elephc header generation");
    assert!(
        repeated.status.success(),
        "repeated cdylib compilation failed:\n{}",
        String::from_utf8_lossy(&repeated.stderr)
    );
    assert_eq!(
        fs::read(&header_path).expect("failed to reread generated header"),
        first_header,
        "generated C header changed across identical compilations"
    );

    let host = compile_c_host(&dir, STRING_HOST_C, "string-host");
    let run = Command::new(&host)
        .arg(&lib_path)
        .output()
        .expect("failed to run the Stage B C host");
    assert!(
        run.status.success(),
        "Stage B C host failed (exit {:?}):\nstdout:\n{}\nstderr:\n{}",
        run.status.code(),
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );

    let linked_host =
        compile_linked_c_host(&dir, LINKED_STRING_HOST_C, "linked-string-host", "strings");
    let linked_run = Command::new(&linked_host)
        .output()
        .expect("failed to run the normally linked Stage B C host");
    assert!(
        linked_run.status.success(),
        "linked Stage B C host failed (exit {:?}):\n{}",
        linked_run.status.code(),
        String::from_utf8_lossy(&linked_run.stderr)
    );

    fs::remove_dir_all(&dir).ok();
}

/// Constrains the host stack independently of runner defaults, arms the floor through
/// `elephc_init()`, and proves deep recursion returns without killing the C host.
#[test]
fn test_cdylib_stack_overflow_returns_runtime_failure_and_keeps_host_alive() {
    let dir = make_test_dir("elephc_cdylib_stack_guard");
    fs::write(dir.join("stack_guard.php"), STACK_GUARD_EXPORT_PHP).unwrap();

    let output = elephc_command(&dir)
        .args(["--emit", "cdylib", "stack_guard.php"])
        .output()
        .expect("failed to run elephc");
    assert!(
        output.status.success(),
        "stack-guard cdylib compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let host =
        compile_linked_c_host(&dir, STACK_GUARD_HOST_C, "stack-guard-host", "stack_guard");
    let run = Command::new(&host)
        .output()
        .expect("failed to run the stack-guard C host");
    assert!(
        run.status.success(),
        "stack-guard C host failed (exit {:?}):\nstdout:\n{}\nstderr:\n{}",
        run.status.code(),
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "HOST-ALIVE\n");

    fs::remove_dir_all(&dir).ok();
}

/// Constrains the host stack independently of runner defaults, then lazily arms the
/// floor when the C host omits `elephc_init()` and recovers from deep recursion.
#[test]
fn test_cdylib_stack_overflow_without_init_keeps_host_alive() {
    let dir = make_test_dir("elephc_cdylib_stack_guard_lazy");
    fs::write(dir.join("stack_guard.php"), STACK_GUARD_EXPORT_PHP).unwrap();

    let output = elephc_command(&dir)
        .args(["--emit", "cdylib", "stack_guard.php"])
        .output()
        .expect("failed to run elephc");
    assert!(
        output.status.success(),
        "lazy stack-guard cdylib compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let host = compile_linked_c_host(
        &dir,
        STACK_GUARD_SKIP_INIT_HOST_C,
        "stack-guard-lazy-host",
        "stack_guard",
    );
    let run = Command::new(&host)
        .output()
        .expect("failed to run the skip-init stack-guard C host");
    assert!(
        run.status.success(),
        "skip-init stack-guard C host failed (exit {:?}):\nstdout:\n{}\nstderr:\n{}",
        run.status.code(),
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "HOST-ALIVE\n");

    fs::remove_dir_all(&dir).ok();
}

/// Lazily initializes through a scalar export, converts buffer use-after-free and
/// bounds fatals into runtime-failure status, and keeps the same C host reusable.
#[test]
fn test_cdylib_buffer_fatals_keep_host_alive() {
    let dir = make_test_dir("elephc_cdylib_buffer_failures");
    fs::write(
        dir.join("buffer_failures.php"),
        BUFFER_FAILURE_EXPORT_PHP,
    )
    .unwrap();

    let output = elephc_command(&dir)
        .args(["--emit", "cdylib", "buffer_failures.php"])
        .output()
        .expect("failed to run elephc");
    assert!(
        output.status.success(),
        "buffer-failure cdylib compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let host = compile_linked_c_host(
        &dir,
        BUFFER_FAILURE_HOST_C,
        "buffer-failure-host",
        "buffer_failures",
    );
    let run = Command::new(&host)
        .output()
        .expect("failed to run the buffer-failure C host");
    assert!(
        run.status.success(),
        "buffer-failure C host failed (exit {:?}):\nstdout:\n{}\nstderr:\n{}",
        run.status.code(),
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "HOST-ALIVE\n");

    fs::remove_dir_all(&dir).ok();
}

/// Converts invalid buffer allocation sizes and descriptor-registry exhaustion
/// into runtime-failure status, with a successful export after every escape.
#[test]
fn test_cdylib_buffer_allocation_fatals_keep_host_alive() {
    let dir = make_test_dir("elephc_cdylib_buffer_allocation_failures");
    fs::write(
        dir.join("buffer_allocation_failures.php"),
        BUFFER_ALLOCATION_FAILURE_EXPORT_PHP,
    )
    .unwrap();

    let output = elephc_command(&dir)
        .args([
            "--emit",
            "cdylib",
            "--heap-size=1048576",
            "buffer_allocation_failures.php",
        ])
        .output()
        .expect("failed to run elephc");
    assert!(
        output.status.success(),
        "buffer-allocation cdylib compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let host = compile_linked_c_host(
        &dir,
        BUFFER_ALLOCATION_FAILURE_HOST_C,
        "buffer-allocation-failure-host",
        "buffer_allocation_failures",
    );
    let run = Command::new(&host)
        .output()
        .expect("failed to run the buffer-allocation C host");
    assert!(
        run.status.success(),
        "buffer-allocation C host failed (exit {:?}):\nstdout:\n{}\nstderr:\n{}",
        run.status.code(),
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "HOST-ALIVE\n");

    fs::remove_dir_all(&dir).ok();
}

/// Compiles namespaced scalar and string exports to stable C identifiers and
/// calls both prototypes through the generated header in a normally linked host.
#[test]
fn test_cdylib_namespaced_exports_use_stable_c_symbols() {
    let dir = make_test_dir("elephc_cdylib_namespaced");
    fs::write(dir.join("namespaced.php"), NAMESPACED_EXPORT_PHP).unwrap();

    let output = elephc_command(&dir)
        .args(["--emit", "cdylib", "namespaced.php"])
        .output()
        .expect("failed to run elephc");
    assert!(
        output.status.success(),
        "namespaced cdylib compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let header = fs::read_to_string(dir.join("libnamespaced.h")).unwrap();
    assert!(header.contains("int64_t Demo_add("));
    assert!(header.contains("int32_t Demo_roundtrip("));
    assert!(!header.contains("Demo\\add"));

    let host = compile_linked_c_host(
        &dir,
        NAMESPACED_HOST_C,
        "namespaced-host",
        "namespaced",
    );
    let run = Command::new(&host)
        .output()
        .expect("failed to run namespaced C host");
    assert!(
        run.status.success(),
        "namespaced C host failed (exit {:?}):\n{}",
        run.status.code(),
        String::from_utf8_lossy(&run.stderr)
    );

    fs::remove_dir_all(&dir).ok();
}

/// Compiles the generated header as C++ when PHP parameter names are C++ keywords.
#[test]
fn test_cdylib_header_escapes_c_and_cpp_parameter_keywords() {
    let dir = make_test_dir("elephc_cdylib_cpp_header");
    fs::write(
        dir.join("keywords.php"),
        r#"<?php
#[Export]
function keyword_args(int $class, int $new, int $template): int {
    return $class + $new + $template;
}
"#,
    )
    .unwrap();

    let output = elephc_command(&dir)
        .args(["--emit", "cdylib", "keywords.php"])
        .output()
        .expect("failed to run elephc");
    assert!(
        output.status.success(),
        "cdylib compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::write(
        dir.join("header.cpp"),
        "#include \"libkeywords.h\"\nint main() { return 0; }\n",
    )
    .unwrap();
    let cxx = Command::new("c++")
        .current_dir(&dir)
        .args(["-std=c++17", "-fsyntax-only", "header.cpp"])
        .output()
        .expect("failed to spawn the system C++ compiler");
    assert!(
        cxx.status.success(),
        "generated header is not valid C++:\n{}",
        String::from_utf8_lossy(&cxx.stderr)
    );

    fs::remove_dir_all(&dir).ok();
}

/// Verifies that the ELF dynamic symbol table contains exactly the documented
/// boundary entry points and `#[Export]` trampolines.
#[test]
#[cfg(target_os = "linux")]
fn test_cdylib_dynamic_symbols_expose_only_public_abi_on_linux() {
    let dir = make_test_dir("elephc_cdylib_dynsym");
    fs::write(dir.join("auth.php"), EXPORT_PHP).unwrap();

    let output = elephc_command(&dir)
        .args(["--emit", "cdylib", "auth.php"])
        .output()
        .expect("failed to run elephc");
    assert!(
        output.status.success(),
        "cdylib compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let readelf = Command::new("readelf")
        .args(["--dyn-syms", "--wide"])
        .arg(dir.join(shared_lib_name("auth")))
        .output()
        .expect("failed to run readelf");
    assert!(readelf.status.success(), "readelf failed");
    let actual = String::from_utf8_lossy(&readelf.stdout)
        .lines()
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            (fields.len() >= 8
                && matches!(fields[4], "GLOBAL" | "WEAK")
                && fields[6] != "UND")
                .then(|| fields[7].split('@').next().unwrap().to_string())
        })
        .collect::<BTreeSet<_>>();
    let expected = [
        "elephc_abi_version",
        "elephc_init",
        "elephc_shutdown",
        "elephc_last_status",
        "elephc_last_error",
        "elephc_free",
        "add_i64",
        "compose_label",
        "fixed_label",
        "fixed_throw",
        "scalar_throw",
        "symbol_string",
        "validate_token",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<BTreeSet<_>>();
    assert_eq!(actual, expected, "unexpected ELF public symbol surface");

    fs::remove_dir_all(&dir).ok();
}

/// Verifies Mach-O exports exactly the documented cdylib ABI and keeps every
/// compiler/runtime implementation symbol private.
#[test]
#[cfg(target_os = "macos")]
fn test_cdylib_dynamic_symbols_expose_only_public_abi_on_macos() {
    let dir = make_test_dir("elephc_cdylib_macho_symbols");
    fs::write(dir.join("auth.php"), EXPORT_PHP).unwrap();

    let output = elephc_command(&dir)
        .args(["--emit", "cdylib", "auth.php"])
        .output()
        .expect("failed to run elephc");
    assert!(
        output.status.success(),
        "cdylib compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let nm = Command::new("nm")
        .args(["-gU"])
        .arg(dir.join(shared_lib_name("auth")))
        .output()
        .expect("failed to run nm");
    assert!(nm.status.success(), "nm failed");
    let actual = String::from_utf8_lossy(&nm.stdout)
        .lines()
        .filter_map(|line| line.split_whitespace().last())
        .map(|symbol| symbol.trim_start_matches('_').to_string())
        .collect::<BTreeSet<_>>();
    let expected = [
        "add_i64",
        "compose_label",
        "fixed_label",
        "fixed_throw",
        "scalar_throw",
        "elephc_abi_version",
        "elephc_free",
        "elephc_init",
        "elephc_last_status",
        "elephc_last_error",
        "elephc_shutdown",
        "symbol_string",
        "validate_token",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<BTreeSet<_>>();
    assert_eq!(actual, expected, "unexpected Mach-O public symbol surface");

    fs::remove_dir_all(&dir).ok();
}

/// Verifies that `#[Export]` signatures outside the scalar set are rejected
/// with a compile error instead of producing a trampoline with an undefined
/// C ABI (arrays have no defined marshaling).
#[test]
fn test_export_with_unsupported_parameter_type_is_rejected() {
    let dir = make_test_dir("elephc_cdylib_badsig");
    fs::write(
        dir.join("bad.php"),
        "<?php\n#[Export]\nfunction sum_all(array $values): int {\n    return 0;\n}\n",
    )
    .unwrap();

    let output = elephc_command(&dir)
        .args(["--emit", "cdylib", "bad.php"])
        .output()
        .expect("failed to run elephc");
    assert!(
        !output.status.success(),
        "compilation must fail for an array parameter in an exported function"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unsupported type for --emit cdylib"),
        "expected the scalar-set diagnostic, got:\n{}",
        stderr
    );

    fs::remove_dir_all(&dir).ok();
}

/// Rejects scalar functions declared to return by reference because the public
/// C ABI exposes only values and cannot preserve PHP reference identity.
#[test]
fn test_export_with_by_reference_return_is_rejected() {
    let dir = make_test_dir("elephc_cdylib_by_ref_return");
    fs::write(
        dir.join("bad.php"),
        "<?php\n#[Export]\nfunction &borrowed(): int {\n    $value = 1;\n    return $value;\n}\n",
    )
    .unwrap();

    let output = elephc_command(&dir)
        .args(["--emit", "cdylib", "bad.php"])
        .output()
        .expect("failed to run elephc");
    assert!(
        !output.status.success(),
        "compilation must fail for a by-reference exported result"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("returns by reference")
            && stderr.contains("#[Export] accepts only by-value results"),
        "expected the by-value result diagnostic, got:\n{stderr}"
    );

    fs::remove_dir_all(&dir).ok();
}

/// Rejects `exit`/`die` when transitively reachable from a string-returning
/// export, because those process-termination constructs cannot produce a
/// recoverable cdylib status for the embedding host.
#[test]
fn test_cdylib_string_export_rejects_reachable_exit() {
    let dir = make_test_dir("elephc_cdylib_exit_restriction");
    fs::write(
        dir.join("exit.php"),
        r#"<?php
function stop_host(): string {
    exit(7);
}

#[Export]
function roundtrip(string $input): string {
    if ($input === "stop") {
        return stop_host();
    }
    return $input;
}
"#,
    )
    .unwrap();

    let output = elephc_command(&dir)
        .args(["--emit", "cdylib", "exit.php"])
        .output()
        .expect("failed to run elephc");
    assert!(!output.status.success(), "reachable exit() must be rejected");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("exit/die cannot return through the cdylib error boundary"),
        "expected the precise exit/die restriction, got:\n{stderr}"
    );

    fs::remove_dir_all(&dir).ok();
}

/// Rejects the `die` alias directly, independently of the transitive `exit` fixture.
#[test]
fn test_cdylib_string_export_rejects_reachable_die() {
    let stderr = compile_cdylib_failure(
        "elephc_cdylib_die_restriction",
        r#"<?php
#[Export]
function roundtrip(string $input): string {
    die(9);
}
"#,
    );
    assert!(
        stderr.contains("exit/die cannot return through the cdylib error boundary"),
        "expected the die restriction, got:\n{stderr}"
    );
}

/// Traverses fixed-class constructors and rejects process termination in their bodies.
#[test]
fn test_cdylib_string_export_rejects_constructor_exit() {
    let stderr = compile_cdylib_failure(
        "elephc_cdylib_constructor_exit",
        r#"<?php
class Killer {
    public function __construct() {
        exit(7);
    }
}

#[Export]
function roundtrip(string $input): string {
    new Killer();
    return $input;
}
"#,
    );
    assert!(
        stderr.contains("Killer::__construct")
            && stderr.contains("exit/die cannot return through the cdylib error boundary"),
        "expected the constructor call path and exit restriction, got:\n{stderr}"
    );
}

/// Traverses statically invoked closure bodies instead of treating them as unrelated EIR.
#[test]
fn test_cdylib_string_export_rejects_closure_exit() {
    let stderr = compile_cdylib_failure(
        "elephc_cdylib_closure_exit",
        r#"<?php
#[Export]
function roundtrip(string $input): string {
    $stop = function (): string {
        exit(7);
    };
    return $stop();
}
"#,
    );
    assert!(
        stderr.contains("exit/die cannot return through the cdylib error boundary"),
        "expected the closure-body exit restriction, got:\n{stderr}"
    );
}

/// Rejects runtime eval because its dynamically compiled body may terminate the host.
#[test]
fn test_cdylib_string_export_rejects_eval() {
    let stderr = compile_cdylib_failure(
        "elephc_cdylib_eval_restriction",
        r#"<?php
#[Export]
function roundtrip(string $input): string {
    eval($input);
    return $input;
}
"#,
    );
    assert!(
        stderr.contains("eval cannot return through the cdylib error boundary"),
        "expected the eval restriction, got:\n{stderr}"
    );
}

/// Rejects PCNTL process and signal operations from both hosted-library artifact kinds.
#[test]
fn test_hosted_libraries_reject_reachable_pcntl_operations() {
    let cases = [
        (
            "fork",
            r#"<?php
#[Export]
function mutate_host(): int {
    return pcntl_fork();
}
"#,
            "pcntl.fork",
        ),
        (
            "signal",
            r#"<?php
#[Export]
function mutate_host(): bool {
    return pcntl_signal(SIGCHLD, SIG_IGN);
}
"#,
            "pcntl.signal",
        ),
        (
            "exec",
            r#"<?php
#[Export]
function mutate_host(): bool {
    return pcntl_exec("/bin/true");
}
"#,
            "pcntl.exec",
        ),
        (
            "wait",
            r#"<?php
#[Export]
function mutate_host(): int {
    return pcntl_wait($status, WNOHANG);
}
"#,
            "pcntl.wait",
        ),
        (
            "alarm",
            r#"<?php
#[Export]
function mutate_host(): int {
    return pcntl_alarm(1);
}
"#,
            "pcntl.alarm",
        ),
        (
            "daemon",
            r#"<?php
#[Export]
function mutate_host(): bool {
    return pcntl_daemon(true, true);
}
"#,
            "pcntl.daemon",
        ),
        (
            "setpgid",
            r#"<?php
#[Export]
function mutate_host(): bool {
    return posix_setpgid(0, 0);
}
"#,
            "posix.setpgid",
        ),
        (
            "setsid",
            r#"<?php
#[Export]
function mutate_host(): int {
    return posix_setsid();
}
"#,
            "posix.setsid",
        ),
    ];

    for emit in ["cdylib", "staticlib"] {
        for (operation, source, eir_name) in cases {
            let dir = make_test_dir(&format!("elephc_{emit}_pcntl_{operation}"));
            fs::write(dir.join("unsafe.php"), source).unwrap();
            let output = elephc_command(&dir)
                .args(["--emit", emit, "unsafe.php"])
                .output()
                .expect("failed to run elephc");
            assert!(
                !output.status.success(),
                "{emit} unexpectedly accepted {operation}"
            );
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                stderr.contains(eir_name)
                    && stderr.contains("--emit cdylib/staticlib")
                    && stderr.contains("embedding host process"),
                "expected the hosted-library PCNTL diagnostic, got:\n{stderr}"
            );
            fs::remove_dir_all(&dir).ok();
        }
    }
}

/// Rejects a runtime-selected callable whose body cannot be proven free of termination.
#[test]
fn test_cdylib_string_export_rejects_opaque_callable_dispatch() {
    let stderr = compile_cdylib_failure(
        "elephc_cdylib_opaque_callable",
        r#"<?php
#[Export]
function roundtrip(string $input): string {
    return call_user_func($input, $input);
}
"#,
    );
    assert!(
        stderr.contains("opaque invocation"),
        "expected the opaque invocation restriction, got:\n{stderr}"
    );
}

/// Rejects a runtime-selected class constructor before it can reach backend dispatch.
#[test]
fn test_cdylib_string_export_rejects_dynamic_object_construction() {
    let stderr = compile_cdylib_failure(
        "elephc_cdylib_dynamic_new",
        r#"<?php
#[Export]
function roundtrip(string $input): string {
    new $input();
    return $input;
}
"#,
    );
    assert!(
        stderr.contains("opaque invocation 'dynamic_object_new"),
        "expected the dynamic-constructor restriction, got:\n{stderr}"
    );
}

/// Rejects a reachable EIR `Fatal` terminator produced by an implicitly returning `never` body.
#[test]
fn test_cdylib_export_rejects_reachable_fatal_terminator() {
    let stderr = compile_cdylib_failure(
        "elephc_cdylib_fatal_terminator",
        r#"<?php
function never_returns(): never {
}

#[Export]
function roundtrip(string $input): string {
    never_returns();
}
"#,
    );
    assert!(
        stderr.contains("fatal terminator")
            && stderr.contains("roundtrip -> never_returns"),
        "expected the fatal terminator and complete call path, got:\n{stderr}"
    );
}

/// Traverses destructors that a fixed-class allocation can run during export cleanup.
#[test]
fn test_cdylib_export_rejects_destructor_exit() {
    let stderr = compile_cdylib_failure(
        "elephc_cdylib_destructor_exit",
        r#"<?php
class Killer {
    public function __destruct() {
        exit(7);
    }
}

#[Export]
function roundtrip(string $input): string {
    $killer = new Killer();
    return $input;
}
"#,
    );
    assert!(
        stderr.contains("Killer::__destruct")
            && stderr.contains("exit/die cannot return through the cdylib error boundary"),
        "expected the destructor call path and exit restriction, got:\n{stderr}"
    );
}

/// Traverses user bodies reached by implicit object operations whose EIR does not
/// carry a method-call target, including string conversion, property access,
/// ArrayAccess, Countable, JsonSerializable, and foreach iterator dispatch.
#[test]
fn test_cdylib_export_rejects_exit_in_implicitly_invoked_object_bodies() {
    let cases = [
        (
            "tostring_concat",
            "__toString",
            r#"<?php
class Boom {
    public function __toString(): string {
        exit(42);
    }
}

#[Export]
function probe(string $input): string {
    $boom = new Boom();
    return "x" . $boom;
}
"#,
        ),
        (
            "tostring_echo",
            "__toString",
            r#"<?php
class Boom {
    public function __toString(): string {
        exit(43);
    }
}

#[Export]
function probe(string $input): string {
    $boom = new Boom();
    echo $boom;
    return $input;
}
"#,
        ),
        (
            "offset_get",
            "offsetGet",
            r#"<?php
class Boom implements ArrayAccess {
    public function offsetExists(mixed $offset): bool { return true; }
    public function offsetGet(mixed $offset): mixed { exit(45); }
    public function offsetSet(mixed $offset, mixed $value): void {}
    public function offsetUnset(mixed $offset): void {}
}

#[Export]
function probe(string $input): string {
    $boom = new Boom();
    return (string) $boom["k"];
}
"#,
        ),
        (
            "offset_set",
            "offsetSet",
            r#"<?php
class Boom implements ArrayAccess {
    public function offsetExists(mixed $offset): bool { return true; }
    public function offsetGet(mixed $offset): mixed { return null; }
    public function offsetSet(mixed $offset, mixed $value): void { exit(48); }
    public function offsetUnset(mixed $offset): void {}
}

#[Export]
function probe(string $input): string {
    $boom = new Boom();
    $boom["k"] = 1;
    return $input;
}
"#,
        ),
        (
            "offset_exists",
            "offsetExists",
            r#"<?php
class Boom implements ArrayAccess {
    public function offsetExists(mixed $offset): bool { exit(52); }
    public function offsetGet(mixed $offset): mixed { return null; }
    public function offsetSet(mixed $offset, mixed $value): void {}
    public function offsetUnset(mixed $offset): void {}
}

#[Export]
function probe(string $input): string {
    $boom = new Boom();
    if (isset($boom["k"])) {
        return $input;
    }
    return $input;
}
"#,
        ),
        (
            "offset_unset",
            "offsetUnset",
            r#"<?php
class Boom implements ArrayAccess {
    public function offsetExists(mixed $offset): bool { return true; }
    public function offsetGet(mixed $offset): mixed { return null; }
    public function offsetSet(mixed $offset, mixed $value): void {}
    public function offsetUnset(mixed $offset): void { exit(53); }
}

#[Export]
function probe(string $input): string {
    $boom = new Boom();
    unset($boom["k"]);
    return $input;
}
"#,
        ),
        (
            "magic_get",
            "__get",
            r#"<?php
class Boom {
    public function __get(string $name): mixed {
        exit(46);
    }
}

#[Export]
function probe(string $input): string {
    $boom = new Boom();
    return (string) $boom->missing;
}
"#,
        ),
        (
            "countable",
            "count",
            r#"<?php
class Boom implements Countable {
    public function count(): int {
        exit(49);
    }
}

#[Export]
function probe(string $input): string {
    $boom = new Boom();
    return (string) count($boom);
}
"#,
        ),
        (
            "json_serialize",
            "jsonSerialize",
            r#"<?php
class Boom implements JsonSerializable {
    public function jsonSerialize(): mixed {
        exit(47);
    }
}

#[Export]
function probe(string $input): string {
    $boom = new Boom();
    return (string) json_encode($boom);
}
"#,
        ),
        (
            "iterator_rewind",
            "rewind",
            r#"<?php
class Boom implements Iterator {
    public function rewind(): void { exit(50); }
    public function valid(): bool { return false; }
    public function current(): mixed { return null; }
    public function key(): mixed { return null; }
    public function next(): void {}
}

#[Export]
function probe(string $input): string {
    $boom = new Boom();
    foreach ($boom as $value) {}
    return $input;
}
"#,
        ),
        (
            "iterator_aggregate",
            "getIterator",
            r#"<?php
class Boom implements IteratorAggregate {
    public function getIterator(): Traversable {
        exit(51);
    }
}

#[Export]
function probe(string $input): string {
    $boom = new Boom();
    foreach ($boom as $value) {}
    return $input;
}
"#,
        ),
    ];

    for (name, method, source) in cases {
        let stderr =
            compile_cdylib_failure(&format!("elephc_cdylib_implicit_{name}"), source);
        assert!(
            stderr.contains(method)
                && stderr.contains("exit/die cannot return through the cdylib error boundary"),
            "expected the implicit {method} call path and exit restriction, got:\n{stderr}"
        );
    }
}

/// Traverses every body behind an include-variant dispatcher, including an unloaded fatal arm.
#[test]
fn test_cdylib_export_rejects_exit_in_include_variant() {
    let stderr = compile_cdylib_files_failure(
        "elephc_cdylib_include_variant_exit",
        &[
            (
                "main.php",
                r#"<?php
if ($argc > 1) {
    include 'safe.php';
} else {
    include 'fatal.php';
}

#[Export]
function roundtrip(string $input): string {
    return included_helper($input);
}
"#,
            ),
            (
                "safe.php",
                "<?php function included_helper(string $input): string { return $input; }",
            ),
            (
                "fatal.php",
                "<?php function included_helper(string $input): string { exit(7); }",
            ),
        ],
        "main.php",
    );
    assert!(
        stderr.contains("included_helper")
            && stderr.contains("exit/die cannot return through the cdylib error boundary"),
        "expected every include variant to be traversed, got:\n{stderr}"
    );
}

/// Rejects only runtime builtin argument shapes that can still reach raw process exits.
#[test]
fn test_cdylib_export_rejects_fatal_builtin_subsets() {
    let cases = [
        (
            "str_repeat",
            r#"<?php
#[Export]
function roundtrip(string $input): string {
    return str_repeat($input, strlen($input) - 2);
}
"#,
        ),
        (
            "dirname",
            r#"<?php
#[Export]
function roundtrip(string $input): string {
    return dirname($input, strlen($input));
}
"#,
        ),
        (
            "php_uname",
            r#"<?php
#[Export]
function roundtrip(string $input): string {
    return php_uname($input);
}
"#,
        ),
        (
            "sprintf",
            r#"<?php
#[Export]
function roundtrip(string $input): string {
    return sprintf($input);
}
"#,
        ),
    ];
    for (builtin, source) in cases {
        let stderr = compile_cdylib_failure(
            &format!("elephc_cdylib_fatal_builtin_{builtin}"),
            source,
        );
        assert!(
            stderr.contains(builtin)
                && stderr.contains("process-fatal runtime path")
                && stderr.contains("roundtrip"),
            "expected a targeted {builtin} safety diagnostic, got:\n{stderr}"
        );
    }
}

/// Keeps statically proven safe builtin subsets available instead of banning whole builtins.
#[test]
fn test_cdylib_export_accepts_proven_safe_builtin_subsets() {
    let dir = make_test_dir("elephc_cdylib_safe_builtin_subsets");
    fs::write(
        dir.join("safe.php"),
        r#"<?php
#[Export]
function repeat_twice(string $input): string {
    return str_repeat($input, 2);
}

#[Export]
function literal_format(string $input): string {
    return sprintf("safe %% literal");
}
"#,
    )
    .unwrap();
    let output = elephc_command(&dir)
        .args(["--emit", "cdylib", "safe.php"])
        .output()
        .expect("failed to run elephc");
    assert!(
        output.status.success(),
        "proven-safe builtin subsets must compile:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::remove_dir_all(&dir).ok();
}

/// Follows a statically resolved callback body passed through a runtime builtin.
#[test]
fn test_cdylib_export_rejects_fatal_runtime_callback_body() {
    let stderr = compile_cdylib_failure(
        "elephc_cdylib_runtime_callback_exit",
        r#"<?php
#[Export]
function roundtrip(string $input): string {
    $callback = function (string $value): string {
        exit(7);
    };
    array_map($callback, [$input]);
    return $input;
}
"#,
    );
    assert!(
        stderr.contains("exit/die cannot return through the cdylib error boundary"),
        "expected the runtime callback body to be traversed, got:\n{stderr}"
    );
}

/// Runs the same call-graph restriction for `--check --emit cdylib` and `--emit-ir`.
#[test]
fn test_cdylib_safety_runs_in_check_and_emit_ir_modes() {
    let source = r#"<?php
#[Export]
function roundtrip(string $input): string {
    exit(7);
}
"#;
    for (mode, args) in [
        ("check", &["--check"][..]),
        ("emit_ir", &["--emit-ir"][..]),
    ] {
        let stderr = compile_cdylib_mode_failure(
            &format!("elephc_cdylib_safety_{mode}"),
            source,
            args,
        );
        assert!(
            stderr.contains("exit/die cannot return through the cdylib error boundary"),
            "expected {mode} to run cdylib call-graph safety, got:\n{stderr}"
        );
    }
}

/// Verifies that executable mode still compiles a program containing
/// `#[Export]` attributes but warns that the exports are ignored, so users
/// know the attribute only takes effect under `--emit cdylib`.
#[test]
fn test_export_attribute_warns_and_is_ignored_in_executable_mode() {
    let dir = make_test_dir("elephc_cdylib_execwarn");
    fs::write(
        dir.join("main.php"),
        "<?php\n#[Export]\nfunction add_i64(int $a, int $b): int {\n    return $a + $b;\n}\necho add_i64(40, 2);\n",
    )
    .unwrap();

    let output = elephc_command(&dir)
        .args(["main.php"])
        .output()
        .expect("failed to run elephc");
    assert!(
        output.status.success(),
        "executable compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ignoring #[Export]"),
        "expected the ignored-exports warning, got:\n{}",
        stderr
    );

    let run = Command::new(dir.join("main"))
        .output()
        .expect("failed to run the compiled executable");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "42");

    fs::remove_dir_all(&dir).ok();
}

/// A C host that parks a sentinel in the reserved ctx register, calls across the
/// library boundary, and reports whether it came back.
///
/// The probe is module-level assembly, not inline asm with a register variable:
/// clang happily spills an explicit `register ... asm("x28")` around the call
/// and reloads it afterwards, so the C-level version of this test passes even
/// against a library that clobbers the register.
const CTX_ABI_HOST_C: &str = r#"
#include <stdio.h>
#include <stdint.h>

int32_t elephc_init(void);
uint64_t probe_add_i64(int64_t a, int64_t b);

#define SENTINEL 0x1234567890ABCDEFULL

/* Mach-O prefixes C symbols with an underscore and ELF does not, so the name the
   assembly calls differs by platform. Hardcoding the Mach-O spelling passed on macOS
   and failed the linux-aarch64 shard with
   `undefined reference to \`_add_i64\`` — a link error, so the sentinel never ran at all. */
#if defined(__APPLE__)
#define CSYM(name) "_" name
#else
#define CSYM(name) name
#endif

#if defined(__aarch64__)
__asm__(
"    .text\n"
"    .globl _probe_add_i64\n"
"    .globl probe_add_i64\n"
"    .align 2\n"
"_probe_add_i64:\n"
"probe_add_i64:\n"
"    sub sp, sp, #32\n"
"    stp x29, x30, [sp, #16]\n"
"    add x29, sp, #16\n"
"    str x28, [sp, #0]\n"
"    movz x28, #0xCDEF\n"
"    movk x28, #0x90AB, lsl #16\n"
"    movk x28, #0x5678, lsl #32\n"
"    movk x28, #0x1234, lsl #48\n"
"    bl " CSYM("add_i64") "\n"
"    mov x0, x28\n"
"    ldr x28, [sp, #0]\n"
"    ldp x29, x30, [sp, #16]\n"
"    add sp, sp, #32\n"
"    ret\n"
);
#elif defined(__x86_64__)
__asm__(
"    .text\n"
"    .globl probe_add_i64\n"
"probe_add_i64:\n"
"    push %rbp\n"
"    mov %rsp, %rbp\n"
"    push %r14\n"
"    push %r15\n"
"    movabs $0x1234567890ABCDEF, %r14\n"
"    call " CSYM("add_i64") "\n"
"    mov %r14, %rax\n"
"    pop %r15\n"
"    pop %r14\n"
"    pop %rbp\n"
"    ret\n"
);
#endif

int main(void) {
    if (elephc_init() != 0) {
        fprintf(stderr, "init failed\n");
        return 1;
    }
    uint64_t after = probe_add_i64(40, 2);
    if (after != SENTINEL) {
        printf("CLOBBERED %016llx\n", (unsigned long long) after);
        return 2;
    }
    printf("PRESERVED\n");
    return 0;
}
"#;

/// A C host that exercises the context pool directly, since no PHP program can reach it.
const CTX_POOL_HOST_C: &str = r#"
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <pthread.h>
#include <dirent.h>
#include <glob.h>
#include <errno.h>
#include <fcntl.h>
#include <unistd.h>

int32_t elephc_init(void);

/* The runtime defines these with their names spelled exactly, so an ordinary C
   declaration misses on Mach-O, where the compiler prepends its own underscore:
   `__rt_ctx_release` in C becomes `___rt_ctx_release`. An explicit asm label pins the
   symbol on both formats. */
void *__rt_ctx_acquire(void *arena_base, uint64_t arena_size, void *object_index,
                       void *object_free, void *buffer_registry,
                       void *serialize_object_ptrs, void *serialize_object_indexes,
                       void *unserialize_values)
    __asm__("__rt_ctx_acquire");
int64_t __rt_ctx_release(void *ctx) __asm__("__rt_ctx_release");
int64_t __rt_stream_owner_mark(void) __asm__("__rt_stream_owner_mark");
void *__rt_resource_inventory_register(int64_t id, int64_t payload, int64_t kind)
    __asm__("__rt_resource_inventory_register");
extern uint64_t resource_id_cursor __asm__("_resource_id_next");
void __rt_exit_or_parallel_fatal(int64_t status)
    __asm__("__rt_exit_or_parallel_fatal") __attribute__((noreturn));
void __rt_cstr_to_str(void) __asm__("__rt_cstr_to_str");
void (*zlib_close_hook)(int fd) __asm__("_zlib_close_fn");
void (*bz2_close_hook)(int fd) __asm__("_bz2_close_fn");
void (*iconv_close_hook)(int fd) __asm__("_iconv_close_fn");
void (*tls_close_hook)(void *handle) __asm__("_elephc_tls_close_fn");
typedef int64_t (*worker_callback)(void *job);
int64_t native_parallel_worker_entry(void *ctx, worker_callback callback, void *job,
                                   uint64_t stack_size, int64_t *released)
    __asm__("__rt_parallel_worker_entry");

/* The bridge needs both callback status and clean-release status before freeing
   its backing arrays. Preserve that composed ABI in every native pool probe. */
static int64_t run_worker_entry(void *ctx, worker_callback callback,
                                        void *job, uint64_t stack_size) {
    int64_t released = -1;
    int64_t status = native_parallel_worker_entry(ctx, callback, job, stack_size, &released);
    if (released != 1) {
        printf("WORKER-CONTEXT-NOT-CLEANLY-RELEASED-%lld\n", (long long)released);
        return -98;
    }
    return status;
}

#define SLOTS 8
#define ARENA 4096
#define STORAGE_SETS (SLOTS + 16)
#define RACE_WORKERS SLOTS
#define OBJECT_INDEX_WORDS ((ARENA + 15) / 16)
#define OBJECT_FREE_WORDS ((ARENA + 23) / 24)
#define BUFFER_REGISTRY_WORDS (((4096 + 1) * 48) / 8)
#define SERIALIZE_OBJECT_TABLE_WORDS 65536
#define UNSERIALIZE_VALUE_TABLE_WORDS 65536
/* Pinned by ctx_layout_offsets_are_ordered_and_size_is_aligned. */
#define REQUEST_STATE_BASE 280
#define REQUEST_STATE_WORDS 45
#define REQUEST_STATE_BYTES (REQUEST_STATE_WORDS * 8)
#define USER_WRAPPER_HANDLES_OFFSET (20616 + REQUEST_STATE_BYTES)
#define EOF_FLAGS_OFFSET (5512 + REQUEST_STATE_BYTES)
#define POPEN_FILES_OFFSET (5768 + REQUEST_STATE_BYTES)
#define DIR_HANDLES_OFFSET (7816 + REQUEST_STATE_BYTES)
#define GLOB_HANDLES_OFFSET (9864 + REQUEST_STATE_BYTES)
#define BZSTREAM_HANDLES_OFFSET (11912 + REQUEST_STATE_BYTES)
#define STREAM_READ_FILTERS_OFFSET (13960 + REQUEST_STATE_BYTES)
#define STREAM_WRITE_FILTERS_OFFSET (14216 + REQUEST_STATE_BYTES)
#define ZSTREAM_HANDLES_OFFSET (14472 + REQUEST_STATE_BYTES)
#define ICONV_HANDLES_OFFSET (16520 + REQUEST_STATE_BYTES)
#define TLS_SESSIONS_OFFSET (18568 + REQUEST_STATE_BYTES)
#define USER_FILTER_INSTANCES_OFFSET (22664 + REQUEST_STATE_BYTES)
#define STREAM_CHUNK_SIZE_OFFSET (26760 + REQUEST_STATE_BYTES)
#define STREAM_CONNECT_HOST_OFFSET (28808 + REQUEST_STATE_BYTES)
#define STREAM_OWNED_FDS_OFFSET (32904 + REQUEST_STATE_BYTES)
/* Non-resource scalar fields pinned by ctx_layout_offsets_are_ordered_and_size_is_aligned. */
#define HEAP_OFF_OFFSET 8
#define HEAP_BASE_OFFSET 56
#define EXC_VALUE_OFFSET 72
#define GC_ALLOCS_OFFSET 144
#define GC_FREES_OFFSET 152
#define GC_PEAK_OFFSET 168
#define SER_VALUE_COUNTER_OFFSET 184
#define SER_OBJECT_COUNT_OFFSET 192
#define UNSER_DEPTH_OFFSET 200
#define UNSER_ACTIVE_OFFSET 232
#define UNSER_CONTEXT_OFFSET 240
#define PRINT_R_MODE_OFFSET 256
#define PRINT_R_OFF_OFFSET 264
#define OB_LEVEL_OFFSET 272

static unsigned char arenas[STORAGE_SETS][ARENA];
static uint32_t object_indexes[STORAGE_SETS][OBJECT_INDEX_WORDS];
static uint32_t object_frees[STORAGE_SETS][OBJECT_FREE_WORDS];
static uint64_t buffer_registries[STORAGE_SETS][BUFFER_REGISTRY_WORDS];
static uint64_t serialize_object_ptrs[STORAGE_SETS][SERIALIZE_OBJECT_TABLE_WORDS];
static uint64_t serialize_object_indexes[STORAGE_SETS][SERIALIZE_OBJECT_TABLE_WORDS];
static uint64_t unserialize_values[STORAGE_SETS][UNSERIALIZE_VALUE_TABLE_WORDS];
static pthread_mutex_t race_lock = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t race_ready_changed = PTHREAD_COND_INITIALIZER;
static pthread_cond_t race_start = PTHREAD_COND_INITIALIZER;
static int race_ready = 0;
static int race_go = 0;
static pthread_mutex_t isolation_lock = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t isolation_ready_changed = PTHREAD_COND_INITIALIZER;
static pthread_cond_t isolation_start = PTHREAD_COND_INITIALIZER;
static int isolation_ready = 0;
static int isolation_go = 0;

static void *acquire_context(int storage) {
    return __rt_ctx_acquire(arenas[storage], ARENA, object_indexes[storage],
                            object_frees[storage], buffer_registries[storage],
                            serialize_object_ptrs[storage], serialize_object_indexes[storage],
                            unserialize_values[storage]);
}

struct race_job {
    int storage;
    void *context;
};

static void *race_acquire(void *opaque) {
    struct race_job *job = opaque;
    pthread_mutex_lock(&race_lock);
    race_ready++;
    pthread_cond_signal(&race_ready_changed);
    while (!race_go) {
        pthread_cond_wait(&race_start, &race_lock);
    }
    pthread_mutex_unlock(&race_lock);
    job->context = acquire_context(job->storage);
    return NULL;
}

#define ISOLATION_SLOT 17

static const size_t ISOLATION_WORD_TABLE_OFFSETS[] = {
    POPEN_FILES_OFFSET, DIR_HANDLES_OFFSET, GLOB_HANDLES_OFFSET,
    BZSTREAM_HANDLES_OFFSET, ZSTREAM_HANDLES_OFFSET, ICONV_HANDLES_OFFSET,
    TLS_SESSIONS_OFFSET,
};

static uint64_t *native_handle_table_for(void *ctx, size_t offset);

static const size_t ISOLATION_SCALAR_OFFSETS[] = {
    HEAP_OFF_OFFSET, HEAP_BASE_OFFSET, EXC_VALUE_OFFSET, GC_ALLOCS_OFFSET,
    GC_FREES_OFFSET, GC_PEAK_OFFSET, SER_VALUE_COUNTER_OFFSET,
    SER_OBJECT_COUNT_OFFSET, UNSER_DEPTH_OFFSET, UNSER_ACTIVE_OFFSET,
    UNSER_CONTEXT_OFFSET, PRINT_R_MODE_OFFSET, PRINT_R_OFF_OFFSET, OB_LEVEL_OFFSET,
};

static void write_context_marker(void *ctx, uint64_t marker) {
    unsigned char *bytes = ctx;
    for (size_t index = 0; index < REQUEST_STATE_WORDS; index++) {
        *(uint64_t *)(bytes + REQUEST_STATE_BASE + index * 8) = marker;
    }
    for (size_t index = 0; index < sizeof(ISOLATION_SCALAR_OFFSETS) / sizeof(ISOLATION_SCALAR_OFFSETS[0]); index++) {
        *(uint64_t *)(bytes + ISOLATION_SCALAR_OFFSETS[index]) = marker;
    }
    bytes[EOF_FLAGS_OFFSET + ISOLATION_SLOT] = (unsigned char)marker;
    bytes[STREAM_READ_FILTERS_OFFSET + ISOLATION_SLOT] = (unsigned char)marker;
    bytes[STREAM_WRITE_FILTERS_OFFSET + ISOLATION_SLOT] = (unsigned char)marker;
    bytes[STREAM_OWNED_FDS_OFFSET + ISOLATION_SLOT] = (unsigned char)marker;
    for (size_t index = 0; index < sizeof(ISOLATION_WORD_TABLE_OFFSETS) / sizeof(ISOLATION_WORD_TABLE_OFFSETS[0]); index++) {
        native_handle_table_for(ctx, ISOLATION_WORD_TABLE_OFFSETS[index])[ISOLATION_SLOT] = marker;
    }
    native_handle_table_for(ctx, USER_WRAPPER_HANDLES_OFFSET)[ISOLATION_SLOT] = marker;
    native_handle_table_for(ctx, USER_FILTER_INSTANCES_OFFSET)[ISOLATION_SLOT * 2] = marker;
    native_handle_table_for(ctx, USER_FILTER_INSTANCES_OFFSET)[ISOLATION_SLOT * 2 + 1] = marker;
    native_handle_table_for(ctx, STREAM_CHUNK_SIZE_OFFSET)[ISOLATION_SLOT] = marker;
    native_handle_table_for(ctx, STREAM_CONNECT_HOST_OFFSET)[ISOLATION_SLOT * 2] = marker;
    native_handle_table_for(ctx, STREAM_CONNECT_HOST_OFFSET)[ISOLATION_SLOT * 2 + 1] = marker + 1;
}

static int context_marker_matches(void *ctx, uint64_t marker) {
    unsigned char *bytes = ctx;
    for (size_t index = 0; index < REQUEST_STATE_WORDS; index++) {
        if (*(uint64_t *)(bytes + REQUEST_STATE_BASE + index * 8) != marker) return 0;
    }
    for (size_t index = 0; index < sizeof(ISOLATION_SCALAR_OFFSETS) / sizeof(ISOLATION_SCALAR_OFFSETS[0]); index++) {
        if (*(uint64_t *)(bytes + ISOLATION_SCALAR_OFFSETS[index]) != marker) return 0;
    }
    if (bytes[EOF_FLAGS_OFFSET + ISOLATION_SLOT] != marker ||
        bytes[STREAM_READ_FILTERS_OFFSET + ISOLATION_SLOT] != marker ||
        bytes[STREAM_WRITE_FILTERS_OFFSET + ISOLATION_SLOT] != marker ||
        bytes[STREAM_OWNED_FDS_OFFSET + ISOLATION_SLOT] != marker) return 0;
    for (size_t index = 0; index < sizeof(ISOLATION_WORD_TABLE_OFFSETS) / sizeof(ISOLATION_WORD_TABLE_OFFSETS[0]); index++) {
        if (native_handle_table_for(ctx, ISOLATION_WORD_TABLE_OFFSETS[index])[ISOLATION_SLOT] != marker) return 0;
    }
    return native_handle_table_for(ctx, USER_WRAPPER_HANDLES_OFFSET)[ISOLATION_SLOT] == marker &&
        native_handle_table_for(ctx, USER_FILTER_INSTANCES_OFFSET)[ISOLATION_SLOT * 2] == marker &&
        native_handle_table_for(ctx, USER_FILTER_INSTANCES_OFFSET)[ISOLATION_SLOT * 2 + 1] == marker &&
        native_handle_table_for(ctx, STREAM_CHUNK_SIZE_OFFSET)[ISOLATION_SLOT] == marker &&
        native_handle_table_for(ctx, STREAM_CONNECT_HOST_OFFSET)[ISOLATION_SLOT * 2] == marker &&
        native_handle_table_for(ctx, STREAM_CONNECT_HOST_OFFSET)[ISOLATION_SLOT * 2 + 1] == marker + 1;
}

static void clear_context_marker(void *ctx) {
    write_context_marker(ctx, 0);
    native_handle_table_for(ctx, STREAM_CONNECT_HOST_OFFSET)[ISOLATION_SLOT * 2 + 1] = 0;
}

static int64_t worker_probe(void *expected) {
    void *actual = NULL;
#if defined(__aarch64__)
    __asm__ volatile("mov %0, x28" : "=r"(actual));
#elif defined(__x86_64__)
    __asm__ volatile("mov %%r14, %0" : "=r"(actual));
#else
#error unsupported worker-entry test architecture
#endif
    return actual == expected ? 37 : -7;
}

/* `__rt_stream_owner_mark` takes and returns its descriptor through the runtime
   result register instead of the C ABI argument register. Keep this tiny shim
   in the C host so the test exercises the production ownership path unchanged. */
static int64_t mark_stream_owner_from_worker(int64_t descriptor) {
#if defined(__aarch64__)
    register int64_t result __asm__("x0") = descriptor;
    register void *marker __asm__("x16") = (void *)(uintptr_t)__rt_stream_owner_mark;
    __asm__ volatile("blr x16" : "+r"(result) : "r"(marker) : "x9", "x10", "x30", "memory");
    return result;
#elif defined(__x86_64__)
    register int64_t result __asm__("rax") = descriptor;
    void *marker = (void *)(uintptr_t)__rt_stream_owner_mark;
    __asm__ volatile("call *%1" : "+a"(result) : "r"(marker) : "rcx", "rdx", "rsi", "rdi", "r8", "r9", "r10", "r11", "memory", "cc");
    return result;
#else
#error unsupported worker-entry test architecture
#endif
}

static void *runtime_owned_string(const char *source) {
#if defined(__aarch64__)
    register const char *input __asm__("x0") = source;
    register void *result __asm__("x1");
    register void *converter __asm__("x16") = (void *)(uintptr_t)__rt_cstr_to_str;
    __asm__ volatile("blr x16" : "+r"(input), "=r"(result) : "r"(converter) : "x2", "x3", "x9", "x10", "x11", "x12", "x13", "x14", "x15", "x17", "x30", "memory");
    return result;
#elif defined(__x86_64__)
    register const char *input __asm__("rax") = source;
    void *converter = (void *)(uintptr_t)__rt_cstr_to_str;
    __asm__ volatile("call *%1" : "+a"(input) : "r"(converter) : "rcx", "rdx", "rsi", "rdi", "r8", "r9", "r10", "r11", "memory", "cc");
    return (void *)input;
#else
#error unsupported worker-entry test architecture
#endif
}

static int zlib_closed_fd = -1;
static int bz2_closed_fd = -1;
static int iconv_closed_fd = -1;
static void *tls_closed_handle = NULL;

static void *active_worker_context(void) {
    void *ctx = NULL;
#if defined(__aarch64__)
    __asm__ volatile("mov %0, x28" : "=r"(ctx));
#elif defined(__x86_64__)
    __asm__ volatile("mov %%r14, %0" : "=r"(ctx));
#else
#error unsupported worker-entry test architecture
#endif
    return ctx;
}

static uint64_t *native_handle_table_for(void *ctx, size_t offset) {
    return (uint64_t *)((unsigned char *)ctx + offset);
}

static void host_zlib_close(int fd) {
    zlib_closed_fd = fd;
    native_handle_table_for(active_worker_context(), ZSTREAM_HANDLES_OFFSET)[fd] = 0;
}

static void host_bz2_close(int fd) {
    bz2_closed_fd = fd;
    native_handle_table_for(active_worker_context(), BZSTREAM_HANDLES_OFFSET)[fd] = 0;
}

static void host_iconv_close(int fd) {
    iconv_closed_fd = fd;
    native_handle_table_for(active_worker_context(), ICONV_HANDLES_OFFSET)[fd] = 0;
}

static uint64_t *inventory_node_seen;
static int inventory_retired_before_native_close;
static uint64_t inventory_node_snapshot[6];
static uint64_t inventory_registered_snapshot[6];
static int tls_close_calls;

static void host_tls_close(void *handle) {
    tls_close_calls++;
    tls_closed_handle = handle;
    if (inventory_node_seen != NULL) {
        for (size_t index = 0; index < 6; index++) inventory_node_snapshot[index] = inventory_node_seen[index];
    }
    inventory_retired_before_native_close = inventory_node_seen != NULL &&
        inventory_node_seen[2] == (uint64_t)-99 && inventory_node_seen[4] == 1;
}

struct worker_stream_owner_job {
    int fd;
    int attach_native_handles;
};

static int64_t worker_stream_owner_fatal_probe(void *opaque) {
    struct worker_stream_owner_job *job = opaque;
    job->fd = open("/dev/null", O_RDONLY);
    if (job->fd < 0) { return -11; }
    if (mark_stream_owner_from_worker(job->fd) != job->fd) {
        close(job->fd);
        return -12;
    }
    if (job->fd >= 256) {
        close(job->fd);
        return -13;
    }
    if (job->attach_native_handles) {
        inventory_node_seen = __rt_resource_inventory_register(99, job->fd, 1);
        if (inventory_node_seen == NULL) return -14;
        for (size_t index = 0; index < 6; index++) inventory_registered_snapshot[index] = inventory_node_seen[index];
        void *ctx = active_worker_context();
        ((unsigned char *)ctx)[EOF_FLAGS_OFFSET + job->fd] = 1;
        ((unsigned char *)ctx)[STREAM_READ_FILTERS_OFFSET + job->fd] = 1;
        ((unsigned char *)ctx)[STREAM_WRITE_FILTERS_OFFSET + job->fd] = 1;
        native_handle_table_for(ctx, STREAM_CHUNK_SIZE_OFFSET)[job->fd] = 4096;
        native_handle_table_for(ctx, STREAM_CONNECT_HOST_OFFSET)[job->fd * 2] = 0xC0DE;
        native_handle_table_for(ctx, STREAM_CONNECT_HOST_OFFSET)[job->fd * 2 + 1] = 4;
        native_handle_table_for(ctx, BZSTREAM_HANDLES_OFFSET)[job->fd] = 1;
        native_handle_table_for(ctx, ZSTREAM_HANDLES_OFFSET)[job->fd] = 1;
        native_handle_table_for(ctx, ICONV_HANDLES_OFFSET)[job->fd] = 1;
        native_handle_table_for(ctx, TLS_SESSIONS_OFFSET)[job->fd] = 0x5A17;
    }
    __rt_exit_or_parallel_fatal(7);
}

struct worker_popen_job {
    int fd;
};

static int64_t worker_popen_fatal_probe(void *opaque) {
    struct worker_popen_job *job = opaque;
    FILE *pipe = popen("printf x", "r");
    if (pipe == NULL) { return -21; }
    job->fd = fileno(pipe);
    if (job->fd < 0 || job->fd >= 256 || mark_stream_owner_from_worker(job->fd) != job->fd) {
        pclose(pipe);
        return -22;
    }
    native_handle_table_for(active_worker_context(), POPEN_FILES_OFFSET)[job->fd] = (uintptr_t)pipe;
    __rt_exit_or_parallel_fatal(7);
}

struct worker_directory_job {
    int fd;
};

static int64_t worker_directory_fatal_probe(void *opaque) {
    struct worker_directory_job *job = opaque;
    DIR *directory = opendir(".");
    if (directory == NULL) { return -31; }
    job->fd = dirfd(directory);
    if (job->fd < 0 || job->fd >= 256 || mark_stream_owner_from_worker(job->fd) != job->fd) {
        closedir(directory);
        return -32;
    }
    native_handle_table_for(active_worker_context(), DIR_HANDLES_OFFSET)[job->fd] = (uintptr_t)directory;
    __rt_exit_or_parallel_fatal(7);
}

struct worker_glob_job {
    int fd;
};

struct worker_glob_state {
    unsigned char header[24];
    glob_t glob;
};

static int64_t worker_glob_fatal_probe(void *opaque) {
    struct worker_glob_job *job = opaque;
    struct worker_glob_state *state = calloc(1, sizeof(*state));
    if (state == NULL) { return -41; }
    if (glob("*", 0, NULL, &state->glob) != 0) {
        free(state);
        return -42;
    }
    job->fd = open("/dev/null", O_RDONLY);
    if (job->fd < 0 || job->fd >= 256 || mark_stream_owner_from_worker(job->fd) != job->fd) {
        if (job->fd >= 0) { close(job->fd); }
        globfree(&state->glob);
        free(state);
        return -43;
    }
    native_handle_table_for(active_worker_context(), GLOB_HANDLES_OFFSET)[job->fd] = (uintptr_t)state;
    __rt_exit_or_parallel_fatal(7);
}

struct worker_wrapper_job {
    void *value;
};

static int64_t worker_wrapper_fatal_probe(void *opaque) {
    struct worker_wrapper_job *job = opaque;
    job->value = runtime_owned_string("fatal-wrapper-owner");
    if (job->value == NULL) { return -51; }
    native_handle_table_for(active_worker_context(), USER_WRAPPER_HANDLES_OFFSET)[0] = (uintptr_t)job->value;
    __rt_exit_or_parallel_fatal(7);
}

struct worker_filter_job {
    int fd;
    void *value;
};

static int64_t worker_filter_fatal_probe(void *opaque) {
    struct worker_filter_job *job = opaque;
    job->fd = open("/dev/null", O_RDONLY);
    if (job->fd < 0 || job->fd >= 256 || mark_stream_owner_from_worker(job->fd) != job->fd) {
        if (job->fd >= 0) { close(job->fd); }
        return -61;
    }
    job->value = runtime_owned_string("fatal-filter-owner");
    if (job->value == NULL) {
        close(job->fd);
        return -62;
    }
    uint64_t *instances = native_handle_table_for(active_worker_context(), USER_FILTER_INSTANCES_OFFSET);
    instances[job->fd * 2] = (uintptr_t)job->value;
    instances[job->fd * 2 + 1] = (uintptr_t)job->value;
    __rt_exit_or_parallel_fatal(7);
}

struct worker_probe_job {
    void *ctx;
    int64_t status;
};

static void *run_worker_probe(void *opaque) {
    struct worker_probe_job *job = opaque;
    job->status = run_worker_entry(
        job->ctx, worker_probe, job->ctx, 8 * 1024 * 1024
    );
    return NULL;
}

struct isolation_job {
    void *ctx;
    uint64_t marker;
    int64_t status;
};

static int64_t worker_context_isolation_probe(void *opaque) {
    struct isolation_job *job = opaque;
    if (active_worker_context() != job->ctx) return -71;
    uint64_t request_state_snapshot[REQUEST_STATE_WORDS];
    for (size_t index = 0; index < REQUEST_STATE_WORDS; index++) {
        request_state_snapshot[index] = *(uint64_t *)((unsigned char *)job->ctx + REQUEST_STATE_BASE + index * 8);
    }
    uint64_t arena_base = *(uint64_t *)((unsigned char *)job->ctx + HEAP_BASE_OFFSET);
    write_context_marker(job->ctx, job->marker);
    pthread_mutex_lock(&isolation_lock);
    isolation_ready++;
    pthread_cond_signal(&isolation_ready_changed);
    while (!isolation_go) {
        pthread_cond_wait(&isolation_start, &isolation_lock);
    }
    pthread_mutex_unlock(&isolation_lock);
    int ok = context_marker_matches(job->ctx, job->marker);
    clear_context_marker(job->ctx);
    for (size_t index = 0; index < REQUEST_STATE_WORDS; index++) {
        *(uint64_t *)((unsigned char *)job->ctx + REQUEST_STATE_BASE + index * 8) = request_state_snapshot[index];
    }
    *(uint64_t *)((unsigned char *)job->ctx + HEAP_BASE_OFFSET) = arena_base;
    return ok ? 73 : -72;
}

static void *run_context_isolation_probe(void *opaque) {
    struct isolation_job *job = opaque;
    job->status = run_worker_entry(
        job->ctx, worker_context_isolation_probe, job, 8 * 1024 * 1024
    );
    return NULL;
}

int main(void) {
    if (elephc_init() != 0) { printf("INIT-FAILED\n"); return 1; }

    if (__rt_ctx_acquire(arenas[0], ARENA, NULL, object_frees[0],
                         buffer_registries[0], serialize_object_ptrs[0],
                         serialize_object_indexes[0], unserialize_values[0]) != NULL) {
        printf("NULL-METADATA-ACCEPTED\n"); return 1;
    }

    void *first = acquire_context(0);
    void *second = acquire_context(1);
    if (first == NULL || second == NULL) { printf("ACQUIRE-NULL\n"); return 1; }
    if (first == second) { printf("SAME-SLOT-TWICE\n"); return 1; }

    /* Releasing hands the slot back; the next acquire must be able to take it. */
    if (__rt_ctx_release(first) != 1) { printf("CLEAN-RELEASE-REJECTED\n"); return 1; }
    void *again = acquire_context(2);
    if (again != first) { printf("RELEASED-SLOT-NOT-REUSED\n"); return 1; }
    pthread_attr_t worker_attr;
    pthread_t worker;
    struct worker_probe_job worker_job = { again, 0 };
    if (pthread_attr_init(&worker_attr) != 0 ||
        pthread_attr_setstacksize(&worker_attr, 8 * 1024 * 1024) != 0 ||
        pthread_create(&worker, &worker_attr, run_worker_probe, &worker_job) != 0 ||
        pthread_join(worker, NULL) != 0) {
        printf("WORKER-THREAD-FAILED\n"); return 1;
    }
    pthread_attr_destroy(&worker_attr);
    if (worker_job.status != 37) {
        printf("WORKER-CONTEXT-NOT-INSTALLED\n"); return 1;
    }
    /* The entry consumed this lease. Reject reuse before acquiring a new one. */
    int64_t stale_release = 92;
    if (native_parallel_worker_entry(again, worker_probe, again, 8 * 1024 * 1024, &stale_release) != -1 || stale_release != 92) {
        printf("RELEASED-WORKER-CONTEXT-ACCEPTED\n"); return 1;
    }
    again = acquire_context(2);
    if (again != first) { printf("WORKER-SLOT-NOT-REACQUIRED\n"); return 1; }

    /* Mark a real descriptor from inside an installed worker context, then
       enter the production fatal trampoline. It must close the descriptor,
       return a typed PHP-fatal status, and leave this exact slot reusable. */
    zlib_close_hook = host_zlib_close;
    bz2_close_hook = host_bz2_close;
    iconv_close_hook = host_iconv_close;
    tls_close_hook = host_tls_close;
    zlib_closed_fd = -1;
    bz2_closed_fd = -1;
    iconv_closed_fd = -1;
    tls_closed_handle = NULL;
    inventory_node_seen = NULL;
    inventory_retired_before_native_close = 0;
    resource_id_cursor = 77;
    struct worker_stream_owner_job stream_owner_job = { -1, 1 };
    int64_t fatal_status = run_worker_entry(
        again, worker_stream_owner_fatal_probe, &stream_owner_job, 8 * 1024 * 1024
    );
    if (fatal_status != -5) { printf("STREAM-OWNER-FATAL-STATUS-%lld\n", (long long)fatal_status); return 1; }
    if (!inventory_retired_before_native_close) {
        printf("INVENTORY-INCARNATION-NOT-RETIRED\n");
        fprintf(stderr, "inventory id=%lld payload=%lld kind=%llu closed=%llu weak=%llu fd=%d\n",
            (long long)inventory_node_snapshot[1], (long long)inventory_node_snapshot[2],
            (unsigned long long)inventory_node_snapshot[3], (unsigned long long)inventory_node_snapshot[4],
            (unsigned long long)inventory_node_snapshot[5], stream_owner_job.fd);
        fprintf(stderr, "registered id=%lld payload=%lld kind=%llu closed=%llu tls-calls=%d zlib=%d bz2=%d iconv=%d\n",
            (long long)inventory_registered_snapshot[1], (long long)inventory_registered_snapshot[2],
            (unsigned long long)inventory_registered_snapshot[3], (unsigned long long)inventory_registered_snapshot[4],
            tls_close_calls, zlib_closed_fd, bz2_closed_fd, iconv_closed_fd);
        return 1;
    }
    if (resource_id_cursor != 77) { printf("WORKER-RESET-GLOBAL-RESOURCE-CURSOR\n"); return 1; }
    if (stream_owner_job.fd < 0) { printf("STREAM-OWNER-OPEN-FAILED\n"); return 1; }
    if (zlib_closed_fd != stream_owner_job.fd) { printf("ZLIB-CLOSE-HOOK-MISSED\n"); return 1; }
    if (bz2_closed_fd != stream_owner_job.fd) { printf("BZ2-CLOSE-HOOK-MISSED\n"); return 1; }
    if (iconv_closed_fd != stream_owner_job.fd) { printf("ICONV-CLOSE-HOOK-MISSED\n"); return 1; }
    if (tls_closed_handle != (void *)(uintptr_t)0x5A17) { printf("TLS-CLOSE-HOOK-MISSED\n"); return 1; }
    if (((unsigned char *)again)[EOF_FLAGS_OFFSET + stream_owner_job.fd] != 0) { printf("EOF-FLAG-STILL-LIVE\n"); return 1; }
    if (((unsigned char *)again)[STREAM_READ_FILTERS_OFFSET + stream_owner_job.fd] != 0) { printf("READ-FILTER-ID-STILL-LIVE\n"); return 1; }
    if (((unsigned char *)again)[STREAM_WRITE_FILTERS_OFFSET + stream_owner_job.fd] != 0) { printf("WRITE-FILTER-ID-STILL-LIVE\n"); return 1; }
    if (native_handle_table_for(again, STREAM_CHUNK_SIZE_OFFSET)[stream_owner_job.fd] != 0) { printf("CHUNK-SIZE-STILL-LIVE\n"); return 1; }
    if (native_handle_table_for(again, STREAM_CONNECT_HOST_OFFSET)[stream_owner_job.fd * 2] != 0 || native_handle_table_for(again, STREAM_CONNECT_HOST_OFFSET)[stream_owner_job.fd * 2 + 1] != 0) { printf("CONNECT-HOST-STILL-LIVE\n"); return 1; }
    if (native_handle_table_for(again, BZSTREAM_HANDLES_OFFSET)[stream_owner_job.fd] != 0) { printf("BZ2-HANDLE-STILL-LIVE\n"); return 1; }
    if (native_handle_table_for(again, ZSTREAM_HANDLES_OFFSET)[stream_owner_job.fd] != 0) { printf("ZLIB-HANDLE-STILL-LIVE\n"); return 1; }
    if (native_handle_table_for(again, ICONV_HANDLES_OFFSET)[stream_owner_job.fd] != 0) { printf("ICONV-HANDLE-STILL-LIVE\n"); return 1; }
    if (native_handle_table_for(again, TLS_SESSIONS_OFFSET)[stream_owner_job.fd] != 0) { printf("TLS-HANDLE-STILL-LIVE\n"); return 1; }
    errno = 0;
    if (fcntl(stream_owner_job.fd, F_GETFD) != -1 || errno != EBADF) {
        printf("STREAM-OWNER-FATAL-FD-STILL-OPEN\n"); return 1;
    }
    zlib_close_hook = NULL;
    bz2_close_hook = NULL;
    iconv_close_hook = NULL;
    tls_close_hook = NULL;
    if (__rt_ctx_release(again) != 1) { printf("STREAM-OWNER-RELEASE-REJECTED\n"); return 1; }
    again = acquire_context(2);
    if (again != first) { printf("STREAM-OWNER-SLOT-NOT-REUSED\n"); return 1; }

    struct worker_popen_job popen_job = { -1 };
    fatal_status = run_worker_entry(
        again, worker_popen_fatal_probe, &popen_job, 8 * 1024 * 1024
    );
    if (fatal_status != -5) { printf("POPEN-FATAL-STATUS-%lld\n", (long long)fatal_status); return 1; }
    if (popen_job.fd < 0) { printf("POPEN-OPEN-FAILED\n"); return 1; }
    if (native_handle_table_for(again, POPEN_FILES_OFFSET)[popen_job.fd] != 0) { printf("POPEN-HANDLE-STILL-LIVE\n"); return 1; }
    errno = 0;
    if (fcntl(popen_job.fd, F_GETFD) != -1 || errno != EBADF) { printf("POPEN-FD-STILL-OPEN\n"); return 1; }
    if (__rt_ctx_release(again) != 1) { printf("POPEN-RELEASE-REJECTED\n"); return 1; }
    again = acquire_context(2);
    if (again != first) { printf("POPEN-SLOT-NOT-REUSED\n"); return 1; }

    struct worker_directory_job directory_job = { -1 };
    fatal_status = run_worker_entry(
        again, worker_directory_fatal_probe, &directory_job, 8 * 1024 * 1024
    );
    if (fatal_status != -5) { printf("DIR-FATAL-STATUS-%lld\n", (long long)fatal_status); return 1; }
    if (directory_job.fd < 0) { printf("DIR-OPEN-FAILED\n"); return 1; }
    if (native_handle_table_for(again, DIR_HANDLES_OFFSET)[directory_job.fd] != 0) { printf("DIR-HANDLE-STILL-LIVE\n"); return 1; }
    errno = 0;
    if (fcntl(directory_job.fd, F_GETFD) != -1 || errno != EBADF) { printf("DIR-FD-STILL-OPEN\n"); return 1; }
    if (__rt_ctx_release(again) != 1) { printf("DIR-RELEASE-REJECTED\n"); return 1; }
    again = acquire_context(2);
    if (again != first) { printf("DIR-SLOT-NOT-REUSED\n"); return 1; }

    struct worker_glob_job glob_job = { -1 };
    fatal_status = run_worker_entry(
        again, worker_glob_fatal_probe, &glob_job, 8 * 1024 * 1024
    );
    if (fatal_status != -5) { printf("GLOB-FATAL-STATUS-%lld\n", (long long)fatal_status); return 1; }
    if (glob_job.fd < 0) { printf("GLOB-OPEN-FAILED\n"); return 1; }
    if (native_handle_table_for(again, GLOB_HANDLES_OFFSET)[glob_job.fd] != 0) { printf("GLOB-HANDLE-STILL-LIVE\n"); return 1; }
    errno = 0;
    if (fcntl(glob_job.fd, F_GETFD) != -1 || errno != EBADF) { printf("GLOB-FD-STILL-OPEN\n"); return 1; }
    if (__rt_ctx_release(again) != 1) { printf("GLOB-RELEASE-REJECTED\n"); return 1; }
    again = acquire_context(2);
    if (again != first) { printf("GLOB-SLOT-NOT-REUSED\n"); return 1; }

    struct worker_wrapper_job wrapper_job = { NULL };
    fatal_status = run_worker_entry(
        again, worker_wrapper_fatal_probe, &wrapper_job, 8 * 1024 * 1024
    );
    if (fatal_status != -5) { printf("WRAPPER-FATAL-STATUS-%lld\n", (long long)fatal_status); return 1; }
    if (wrapper_job.value == NULL) { printf("WRAPPER-ALLOC-FAILED\n"); return 1; }
    if (native_handle_table_for(again, USER_WRAPPER_HANDLES_OFFSET)[0] != 0) { printf("WRAPPER-HANDLE-STILL-LIVE\n"); return 1; }
    if (__rt_ctx_release(again) != 1) { printf("WRAPPER-RELEASE-REJECTED\n"); return 1; }
    again = acquire_context(2);
    if (again != first) { printf("WRAPPER-SLOT-NOT-REUSED\n"); return 1; }

    struct worker_filter_job filter_job = { -1, NULL };
    fatal_status = run_worker_entry(
        again, worker_filter_fatal_probe, &filter_job, 8 * 1024 * 1024
    );
    if (fatal_status != -5) { printf("FILTER-FATAL-STATUS-%lld\n", (long long)fatal_status); return 1; }
    if (filter_job.fd < 0 || filter_job.value == NULL) { printf("FILTER-ALLOC-FAILED\n"); return 1; }
    uint64_t *filter_instances = native_handle_table_for(again, USER_FILTER_INSTANCES_OFFSET);
    if (filter_instances[filter_job.fd * 2] != 0 || filter_instances[filter_job.fd * 2 + 1] != 0) { printf("FILTER-HANDLE-STILL-LIVE\n"); return 1; }
    errno = 0;
    if (fcntl(filter_job.fd, F_GETFD) != -1 || errno != EBADF) { printf("FILTER-FD-STILL-OPEN\n"); return 1; }
    if (__rt_ctx_release(again) != 1) { printf("FILTER-RELEASE-REJECTED\n"); return 1; }
    again = acquire_context(2);
    if (again != first) { printf("FILTER-SLOT-NOT-REUSED\n"); return 1; }

    struct isolation_job isolation_a = { again, 0x31, 0 };
    struct isolation_job isolation_b = { second, 0x52, 0 };
    pthread_t isolation_threads[2];
    pthread_attr_t isolation_attr;
    if (pthread_attr_init(&isolation_attr) != 0 ||
        pthread_attr_setstacksize(&isolation_attr, 8 * 1024 * 1024) != 0 ||
        pthread_create(&isolation_threads[0], &isolation_attr, run_context_isolation_probe, &isolation_a) != 0 ||
        pthread_create(&isolation_threads[1], &isolation_attr, run_context_isolation_probe, &isolation_b) != 0) {
        printf("ISOLATION-THREAD-CREATE-FAILED\n"); return 1;
    }
    pthread_mutex_lock(&isolation_lock);
    while (isolation_ready != 2) {
        pthread_cond_wait(&isolation_ready_changed, &isolation_lock);
    }
    isolation_go = 1;
    pthread_cond_broadcast(&isolation_start);
    pthread_mutex_unlock(&isolation_lock);
    if (pthread_join(isolation_threads[0], NULL) != 0 || pthread_join(isolation_threads[1], NULL) != 0) {
        printf("ISOLATION-THREAD-JOIN-FAILED\n"); return 1;
    }
    pthread_attr_destroy(&isolation_attr);
    if (isolation_a.status != 73 || isolation_b.status != 73) { printf("ISOLATION-STATE-CROSSTALK\n"); return 1; }
    if (__rt_ctx_release(again) != 1 || __rt_ctx_release(second) != 1) { printf("ISOLATION-RELEASE-REJECTED\n"); return 1; }
    again = acquire_context(2);
    second = acquire_context(1);
    if (again != first || second == NULL || again == second) { printf("ISOLATION-SLOT-REUSE-FAILED\n"); return 1; }
    int64_t invalid_release = 91;
    if (native_parallel_worker_entry(NULL, worker_probe, again, 8 * 1024 * 1024, &invalid_release) != -1 || invalid_release != 91) {
        printf("NULL-WORKER-CONTEXT-ACCEPTED\n"); return 1;
    }

    /* Fill the pool: two are held (again, second), so SLOTS-2 remain. */
    void *held[SLOTS];
    int n = 0;
    for (int i = 0; i < SLOTS; i++) {
        void *c = acquire_context(3 + i);
        if (c == NULL) { break; }
        held[n++] = c;
    }
    if (n != SLOTS - 2) { printf("EXPECTED-%d-FREE-GOT-%d\n", SLOTS - 2, n); return 1; }
    if (acquire_context(SLOTS + 3) != NULL) { printf("OVERSUBSCRIBED\n"); return 1; }

    /* Release exactly one slot, then race every worker contender for it. */
    if (__rt_ctx_release(held[0]) != 1) { printf("RECOVERY-RELEASE-REJECTED\n"); return 1; }
    pthread_t racers[RACE_WORKERS];
    struct race_job jobs[RACE_WORKERS];
    for (int i = 0; i < RACE_WORKERS; i++) {
        jobs[i].storage = SLOTS + 3 + i;
        jobs[i].context = NULL;
        if (pthread_create(&racers[i], NULL, race_acquire, &jobs[i]) != 0) {
            printf("RACE-THREAD-CREATE-FAILED\n"); return 1;
        }
    }
    pthread_mutex_lock(&race_lock);
    while (race_ready != RACE_WORKERS) {
        pthread_cond_wait(&race_ready_changed, &race_lock);
    }
    race_go = 1;
    pthread_cond_broadcast(&race_start);
    pthread_mutex_unlock(&race_lock);
    int winners = 0;
    void *winner = NULL;
    for (int i = 0; i < RACE_WORKERS; i++) {
        if (pthread_join(racers[i], NULL) != 0) { printf("RACE-THREAD-JOIN-FAILED\n"); return 1; }
        if (jobs[i].context != NULL) { winners++; winner = jobs[i].context; }
    }
    if (winners != 1 || winner != held[0]) { printf("LAST-SLOT-RACE-FAILED-%d\n", winners); return 1; }
    if (__rt_ctx_release(winner) != 1) { printf("RACE-WINNER-RELEASE-REJECTED\n"); return 1; }
    if (acquire_context(SLOTS + 12) == NULL) { printf("NO-RACE-RECOVERY\n"); return 1; }

    /* A context with a surviving owner must stay claimed instead of exposing backing
       storage that its Rust WorkerStorage owner is about to release. The pool is full
       here, so any successful acquire would necessarily mean the dirty slot escaped. */
    *(uint64_t *)((unsigned char *)held[1] + USER_WRAPPER_HANDLES_OFFSET) = 1;
    if (__rt_ctx_release(held[1]) != 0) { printf("DIRTY-RELEASE-ACCEPTED\n"); return 1; }
    if (acquire_context(SLOTS + 13) != NULL) { printf("DIRTY-SLOT-REUSED\n"); return 1; }

    printf("POOL-OK\n");
    return 0;
}
"#;

/// THE CONTEXT POOL HANDS OUT DISTINCT SLOTS, REUSES RELEASED ONES, AND REFUSES WHEN FULL.
///
/// `__rt_ctx_acquire` / `__rt_ctx_release` are new assembly that no PHP program can reach,
/// so the only honest test drives them from C against the static archive. What it pins is
/// the contract the M1 bridge will depend on: two acquires never return the same slot, a
/// released slot becomes available again, the pool refuses rather than overcommitting, and
/// it recovers after a release.
///
/// What it CANNOT test is the part the atomics are for — two threads racing for the last
/// slot. That is why the claim uses `ldaxr`/`stlxr` and `lock cmpxchg` now rather than when
/// M1 arrives: a plain load/store pair would pass this test exactly as well.
#[test]
fn test_rt_ctx_pool_hands_out_distinct_slots_and_reuses_released_ones() {
    let dir = make_test_dir("elephc_ctx_pool");
    fs::write(dir.join("auth.php"), EXPORT_PHP).unwrap();

    let output = elephc_command(&dir)
        .args(["--emit", "staticlib", "auth.php"])
        .output()
        .expect("failed to run elephc");
    assert!(
        output.status.success(),
        "--rt-ctx staticlib compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let host = compile_linked_c_host(&dir, CTX_POOL_HOST_C, "ctx-pool-host", "auth");
    let run = Command::new(&host).output().expect("failed to run the ctx pool host");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout).trim(),
        "POOL-OK",
        "the context pool broke its contract (exit {:?}):\n{}",
        run.status.code(),
        String::from_utf8_lossy(&run.stderr)
    );

    fs::remove_dir_all(&dir).ok();
}

/// Produces current Linux assembly and the same native pool fixture without a Docker Cargo rebuild.
/// Run explicitly, then assemble/link/execute each bundle in its matching Linux image.
#[test]
#[ignore = "manual cross-target fixture emission; requires subsequent native Linux execution"]
fn emit_linux_context_pool_probe_bundles() {
    use elephc::codegen_support::{generate_runtime_with_features_mode, platform::Target, RuntimeFeatures};
    let root = make_test_dir("elephc_linux_context_pool_probes");
    for name in ["linux-aarch64", "linux-x86_64"] {
        let dir = root.join(name);
        fs::create_dir_all(&dir).unwrap();
        // A scalar export requires no optional bridge; the pool/runtime helpers are real,
        // not substitutes for the PHP/eval descriptor or Rust Parallel bridge gates.
        fs::write(dir.join("auth.php"), "<?php\n#[Export]\nfunction add_i64(int $a, int $b): int { return $a + $b; }\n").unwrap();
        let output = elephc_command(&dir)
            .args(["--target", name, "--emit", "staticlib", "--emit-asm", "auth.php"])
            .output().expect("cross-target assembly emission");
        assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
        let runtime = generate_runtime_with_features_mode(
            8 * 1024 * 1024, Target::parse(name).unwrap(), RuntimeFeatures::none(), false, true,
        );
        assert!(runtime.contains("__rt_resource_inventory_abandon_fd") && runtime.contains("__rt_parallel_worker_entry"), "{name}: missing real runtime controls");
        fs::write(dir.join("runtime.s"), runtime).unwrap();
        fs::write(dir.join("pool-host.c"), CTX_POOL_HOST_C).unwrap();
        println!("Linux context-pool bundle: {}", dir.display());
    }
}

/// THE STATICLIB ARM CARRIES THE SAME ABI CONTRACT, AND WAS NEVER PROBED.
///
/// The cdylib arm has been green since round 3, and `.plans/sandbox-threads.md` listed
/// the staticlib as still unprobed — the two emit kinds build their export wrappers
/// through different paths, so "the cdylib preserves the register" says nothing about
/// the archive a host links directly.
///
/// Same sentinel as the cdylib test: a module-level assembly probe parks a known value
/// in the ctx register, calls the export, and reports whether the value survived. It is
/// module-level assembly rather than a C `register … asm("x28")` variable because clang
/// spills and reloads such a variable around the call, which makes the C version pass
/// against a library that clobbers the register.
#[test]
fn test_rt_ctx_staticlib_export_preserves_the_hosts_ctx_register() {
    let dir = make_test_dir("elephc_staticlib_rt_ctx_abi");
    fs::write(dir.join("auth.php"), EXPORT_PHP).unwrap();

    let output = elephc_command(&dir)
        .args(["--emit", "staticlib", "auth.php"])
        .output()
        .expect("failed to run elephc");
    assert!(
        output.status.success(),
        "--rt-ctx staticlib compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        dir.join("libauth.a").exists(),
        "expected the archive the host links directly against"
    );

    let host = compile_linked_c_host(&dir, CTX_ABI_HOST_C, "ctx-abi-static-host", "auth");
    let run = Command::new(&host)
        .output()
        .expect("failed to run the ctx ABI static host");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout).trim(),
        "PRESERVED",
        "a --rt-ctx staticlib export clobbered the host's callee-saved ctx register \
         (exit {:?}):\n{}",
        run.status.code(),
        String::from_utf8_lossy(&run.stderr)
    );

    fs::remove_dir_all(&dir).ok();
}

/// The ctx register (`x28` AArch64 / `r14` x86_64) is callee-saved, so a library
/// export that publishes elephc's `_rt_ctx` pointer into it borrows a register
/// the HOST owns. Every return path must hand the host's value back — the same
/// contract the FFI callback trampoline already carries.
///
/// Without the save/restore pair the host gets elephc's context pointer in place
/// of its own value, corrupting whatever the caller kept there: an ABI break no
/// PHP-level test can see.
#[test]
fn test_rt_ctx_cdylib_export_preserves_the_hosts_ctx_register() {
    let dir = make_test_dir("elephc_cdylib_rt_ctx_abi");
    fs::write(dir.join("auth.php"), EXPORT_PHP).unwrap();

    let output = elephc_command(&dir)
        .args(["--emit", "cdylib", "auth.php"])
        .output()
        .expect("failed to run elephc");
    assert!(
        output.status.success(),
        "--rt-ctx cdylib compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let host = compile_linked_c_host(&dir, CTX_ABI_HOST_C, "ctx-abi-host", "auth");
    let run = Command::new(&host)
        .output()
        .expect("failed to run the ctx ABI host");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout).trim(),
        "PRESERVED",
        "a --rt-ctx export clobbered the host's callee-saved ctx register (exit {:?}):\n{}",
        run.status.code(),
        String::from_utf8_lossy(&run.stderr)
    );

    fs::remove_dir_all(&dir).ok();
}
