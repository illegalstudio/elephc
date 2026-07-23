//! Purpose:
//! End-to-end tests for `--emit cdylib`: compile PHP with `#[Export]` functions
//! into a shared library, load it from a C host via dlopen, and assert the
//! exported C ABI behaves per the v1 contract on the host target.
//!
//! Called from:
//! - `cargo test --test cdylib_tests` through Rust's test harness.
//!
//! Key details:
//! - Tests invoke the elephc CLI binary as a subprocess (CARGO_BIN_EXE_elephc)
//!   inside an isolated temp dir with an isolated runtime cache, then compile
//!   a minimal C host with the system C compiler and run it.
//! - On ELF targets the dynamic symbol table is also asserted: internal
//!   globals (e.g. `_concat_buf`) must be hidden, only the lifecycle entry
//!   points and `#[Export]` trampolines stay visible.
//! - Host-target only: each platform/arch covers itself (macOS aarch64 runs
//!   locally, Linux x86_64/aarch64 run through the Docker test scripts).

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

/// Verifies that ELF cdylibs export only the public ABI: the dynamic symbol
/// table must contain the lifecycle entry points and `#[Export]` trampolines
/// while internal runtime globals stay hidden. Linux-only because Mach-O
/// dylibs bind same-image references through the two-level namespace instead.
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
    let dynsyms = String::from_utf8_lossy(&readelf.stdout);
    for public in ["elephc_init", "elephc_shutdown", "add_i64", "validate_token"] {
        assert!(
            dynsyms.contains(public),
            "public symbol '{}' missing from dynamic symbol table",
            public
        );
    }
    for internal in ["_concat_buf", "_concat_off", "_fn_token_u_min_u_length"] {
        assert!(
            !dynsyms.contains(&format!(" {}\n", internal)),
            "internal symbol '{}' leaked into the dynamic symbol table",
            internal
        );
    }

    fs::remove_dir_all(&dir).ok();
}

/// Verifies the Windows GNU cross path produces a PE DLL, a conventional
/// import library, undecorated exports, and an import library consumable by a
/// separately compiled C executable. Runtime execution remains a Wine/Windows
/// runner gate rather than a host-independent structural check.
#[test]
fn test_windows_cdylib_cross_links_a_c_consumer_when_mingw_is_available() {
    let mingw = "x86_64-w64-mingw32-gcc";
    if Command::new(mingw).arg("--version").output().is_err() {
        eprintln!("skipping Windows cdylib cross-link test: MinGW-w64 unavailable");
        return;
    }

    let dir = make_test_dir("elephc_windows_cdylib");
    fs::write(
        dir.join("native.php"),
        r#"<?php
#[Export]
function mixed(int $a, float $b, int $c, float $d, int $e): float {
    return $a + $b + $c + $d + $e;
}

#[Export]
function string_after_three(int $a, int $b, int $c, string $value): int {
    return $a + $b + $c + strlen($value);
}
"#,
    )
    .unwrap();

    let output = elephc_command(&dir)
        .args([
            "--target",
            "windows-x86_64",
            "--emit",
            "cdylib",
            "native.php",
        ])
        .output()
        .expect("failed to run elephc");
    assert!(
        output.status.success(),
        "Windows cdylib compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let dll = dir.join("native.dll");
    let import_lib = dir.join("libnative.dll.a");
    assert!(dll.exists(), "expected PE DLL at {dll:?}");
    assert!(import_lib.exists(), "expected import library at {import_lib:?}");
    let asm = fs::read_to_string(dir.join("native.s")).expect("expected generated DLL assembly");
    assert!(asm.contains("mov QWORD PTR [rbp - 48], rdi"), "{asm}");
    assert!(asm.contains("mov QWORD PTR [rbp - 56], rsi"), "{asm}");
    assert!(asm.contains("movdqu XMMWORD PTR [rbp - 80], xmm6"), "{asm}");
    assert!(asm.contains("movdqu XMMWORD PTR [rbp - 224], xmm15"), "{asm}");
    assert!(asm.contains("movdqu xmm15, XMMWORD PTR [rbp - 224]"), "{asm}");
    assert!(asm.contains("mov rsi, QWORD PTR [rbp - 56]"), "{asm}");
    assert!(asm.contains("mov rdi, QWORD PTR [rbp - 48]"), "{asm}");

    let consumer = dir.join("consumer.c");
    fs::write(
        &consumer,
        r#"#include <stddef.h>
__declspec(dllimport) double mixed(long long, double, long long, double, long long);
__declspec(dllimport) long long string_after_three(long long, long long, long long, const char *, size_t);
int main(void) {
    return mixed(1, 2.0, 3, 4.0, 5) == 15.0 &&
           string_after_three(1, 2, 3, "abcd", 4) == 10 ? 0 : 1;
}
"#,
    )
    .unwrap();
    let linked = Command::new(mingw)
        .current_dir(&dir)
        .args(["-o", "consumer.exe", "consumer.c", "-L.", "-lnative"])
        .output()
        .expect("failed to spawn MinGW C compiler");
    assert!(
        linked.status.success(),
        "MinGW C consumer link failed:\n{}",
        String::from_utf8_lossy(&linked.stderr)
    );
    assert!(dir.join("consumer.exe").exists());

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
