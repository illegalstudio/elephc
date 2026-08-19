//! Purpose:
//! End-to-end tests for `--emit cdylib`: compile PHP with `#[Export]` functions
//! into a shared library, load it from a C host via dlopen, and assert the
//! exported C ABI behaves per the scalar and owned-string contracts.
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
        cmd.arg("-Wl,-rpath,$ORIGIN");
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
function symbol_string(string $input): string {
    return $input;
}
"#;

const HOST_C: &str = r#"
#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>
#include <stddef.h>

int main(int argc, char **argv) {
    if (argc != 2) return 1;
    void *lib = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (!lib) { fprintf(stderr, "dlopen: %s\n", dlerror()); return 2; }
    int32_t (*init)(void) = (int32_t (*)(void))dlsym(lib, "elephc_init");
    int64_t (*add)(int64_t, int64_t) = (int64_t (*)(int64_t, int64_t))dlsym(lib, "add_i64");
    int32_t (*vt)(const char *, size_t) =
        (int32_t (*)(const char *, size_t))dlsym(lib, "validate_token");
    void (*shutdown)(void) = (void (*)(void))dlsym(lib, "elephc_shutdown");
    if (!init || !add || !vt || !shutdown) { fprintf(stderr, "dlsym failed\n"); return 3; }
    if (init() != 0) return 4;
    printf("%lld %d %d\n", (long long)add(40, 2), vt("supersecret", 11), vt("nope", 4));
    shutdown();
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
typedef const char *(*last_error_fn)(void);
typedef void (*free_fn)(void *);
typedef int32_t (*string_export_fn)(const char *, size_t, char **, size_t *);
typedef int64_t (*add_fn)(int64_t, int64_t);

int main(int argc, char **argv) {
    if (argc != 2) return 1;
    void *lib = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (!lib) { fprintf(stderr, "dlopen: %s\n", dlerror()); return 2; }

    LOAD(elephc_abi_version, p_abi_version, abi_version_fn);
    LOAD(elephc_init, p_init, init_fn);
    LOAD(elephc_shutdown, p_shutdown, shutdown_fn);
    LOAD(elephc_last_error, p_last_error, last_error_fn);
    LOAD(elephc_free, p_free, free_fn);
    LOAD(roundtrip, p_roundtrip, string_export_fn);
    LOAD(maybe_throw, p_maybe_throw, string_export_fn);
    LOAD(empty_throw, p_empty_throw, string_export_fn);
    LOAD(cleanup_throw, p_cleanup_throw, string_export_fn);
    LOAD(concat_success, p_concat_success, string_export_fn);
    LOAD(force_allocation_failure, p_force_allocation_failure, string_export_fn);
    LOAD(add_after_failure, p_add_after_failure, add_fn);
    if (!p_abi_version || !p_init || !p_shutdown || !p_last_error || !p_free ||
        !p_roundtrip || !p_maybe_throw || !p_empty_throw || !p_cleanup_throw ||
        !p_concat_success || !p_force_allocation_failure || !p_add_after_failure)
        return 3;
    if (p_abi_version() != ELEPHC_ABI_VERSION ||
        p_init() != ELEPHC_STATUS_OK) return 4;

    const unsigned char binary[] = {'A', 0, 'B', 0xff, 'Z'};
    char *out = (char *)(uintptr_t)1;
    size_t out_len = 99;
    if (p_roundtrip((const char *)binary, sizeof(binary), &out, &out_len) != ELEPHC_STATUS_OK)
        return 5;
    if (!out || out_len != sizeof(binary) || memcmp(out, binary, sizeof(binary)) != 0 ||
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
        out_len != 0 || !p_last_error()) return 8;
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
        out != NULL || out_len != 0) return 19;
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
            ELEPHC_STATUS_ALLOCATION_FAILURE || out != NULL || out_len != 0)
        return 23;
    error = p_last_error();
    if (!error || !strstr(error, "allocation failed")) return 24;
    if (p_add_after_failure(20, 22) != 42 || p_last_error() != NULL) return 25;

    out = NULL;
    out_len = 0;
    if (p_roundtrip("alive", 5, &out, &out_len) != ELEPHC_STATUS_OK ||
        out_len != 5 || memcmp(out, "alive", 5) != 0 || p_last_error() != NULL)
        return 26;
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
        "elephc_last_error",
        "elephc_free",
        "add_i64",
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
        "elephc_abi_version",
        "elephc_free",
        "elephc_init",
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

/// Verifies that `#[Export]` signatures outside the v1 scalar set are rejected
/// with a compile error instead of producing a trampoline with an undefined
/// C ABI (arrays have no defined marshaling in v1).
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
        "expected the v1 scalar-set diagnostic, got:\n{}",
        stderr
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
