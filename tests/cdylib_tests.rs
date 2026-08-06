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

const STRING_RETURN_PHP: &str = r#"<?php
#[Export]
function greet(string $name): string {
    return "Hello, " . $name;
}

#[Export]
function echo_back(string $s): string {
    return $s;
}

#[Export]
function fixed_label(): string {
    return "fixed";
}
"#;

const STRING_RETURN_HOST_C: &str = r#"
#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>
#include <stddef.h>
#include <string.h>

typedef struct { const char *ptr; size_t len; } elephc_str;

int main(int argc, char **argv) {
    if (argc != 2) return 1;
    void *lib = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (!lib) { fprintf(stderr, "dlopen: %s\n", dlerror()); return 2; }
    int32_t (*init)(void) = (int32_t (*)(void))dlsym(lib, "elephc_init");
    void (*shutdown)(void) = (void (*)(void))dlsym(lib, "elephc_shutdown");
    void (*efree)(void *) = (void (*)(void *))dlsym(lib, "elephc_free");
    elephc_str (*greet)(const char *, size_t) =
        (elephc_str (*)(const char *, size_t))dlsym(lib, "greet");
    elephc_str (*echo_back)(const char *, size_t) =
        (elephc_str (*)(const char *, size_t))dlsym(lib, "echo_back");
    elephc_str (*fixed_label)(void) = (elephc_str (*)(void))dlsym(lib, "fixed_label");
    if (!init || !shutdown || !efree || !greet || !echo_back || !fixed_label) {
        fprintf(stderr, "dlsym failed\n"); return 3;
    }
    if (init() != 0) return 4;

    /* Buffer owned by the host: the string it returns must NOT come back as the
       same pointer, or `elephc_free` below would be freeing host memory. */
    char mine[] = "world";

    elephc_str g = greet(mine, 5);
    elephc_str e = echo_back(mine, 5);
    elephc_str f = fixed_label();

    printf("%.*s %zu %.*s %zu %.*s %zu %d\n",
           (int)g.len, g.ptr, g.len,
           (int)e.len, e.ptr, e.len,
           (int)f.len, f.ptr, f.len,
           (e.ptr == mine) ? 1 : 0);

    efree((void *)g.ptr);
    efree((void *)e.ptr);
    efree((void *)f.ptr);
    efree(NULL);
    shutdown();
    return 0;
}
"#;

const STATICLIB_HOST_C: &str = r#"
#include <stdint.h>
#include <stdio.h>
#include <stddef.h>

typedef struct { const char *ptr; size_t len; } elephc_str;

extern int32_t elephc_init(void);
extern void elephc_shutdown(void);
extern void elephc_free(void *);
extern elephc_str greet(const char *, size_t);
extern elephc_str fixed_label(void);

int main(void) {
    if (elephc_init() != 0) return 1;
    elephc_str g = greet("static", 6);
    elephc_str f = fixed_label();
    printf("%.*s %zu %.*s %zu\n", (int)g.len, g.ptr, g.len, (int)f.len, f.ptr, f.len);
    elephc_free((void *)g.ptr);
    elephc_free((void *)f.ptr);
    elephc_shutdown();
    return 0;
}
"#;

/// Verifies that every process-spawning builtin is refused at compile time for
/// an iOS target, and still accepted for macOS.
///
/// These functions exist as libSystem symbols on iOS and link happily, then fail
/// at run time inside the sandbox, which forbids `fork`. Catching that at build
/// time is the whole point, so the test asserts on the diagnostic rather than on
/// the exit status alone: it must name the builtin, name the target, and carry a
/// source position.
///
/// `proc_open` and its family are absent from this list because they do not
/// exist in the compiler at all. Whenever they are added they must adopt the
/// same guard.
#[test]
fn test_process_spawning_builtins_are_refused_for_ios_targets() {
    let dir = make_test_dir("elephc_ios_capability");

    for (builtin, source) in [
        ("system", r#"<?php system("ls");"#),
        ("passthru", r#"<?php passthru("ls");"#),
        ("exec", r#"<?php exec("ls");"#),
        ("shell_exec", r#"<?php shell_exec("ls");"#),
        ("popen", r#"<?php popen("ls", "r");"#),
        ("pclose", r#"<?php pclose(popen("ls", "r"));"#),
    ] {
        let php = dir.join("spawn.php");
        fs::write(&php, source).unwrap();

        let refused = elephc_command(&dir)
            .args(["--target", "ios-arm64", "--emit", "staticlib", "spawn.php"])
            .output()
            .expect("failed to run elephc");
        assert!(
            !refused.status.success(),
            "{builtin} must not compile for iOS"
        );
        let message = String::from_utf8_lossy(&refused.stderr);
        // `pclose` wraps `popen`, whose argument is evaluated first, so the
        // reported builtin is the inner one -- either name proves the gate ran.
        assert!(
            message.contains(&format!("{builtin}()")) || message.contains("popen()"),
            "{builtin}: diagnostic must name the builtin, got: {message}"
        );
        assert!(
            message.contains("ios-arm64"),
            "{builtin}: diagnostic must name the target, got: {message}"
        );
        assert!(
            message.contains("error["),
            "{builtin}: diagnostic must carry a source position, got: {message}"
        );

        // The same source stays valid for the host target: this is a target
        // capability gate, not a removal of the builtin.
        let accepted = elephc_command(&dir)
            .args(["--emit", "staticlib", "spawn.php"])
            .output()
            .expect("failed to run elephc");
        assert!(
            accepted.status.success(),
            "{builtin} must still compile for the host:\n{}",
            String::from_utf8_lossy(&accepted.stderr)
        );
    }

    fs::remove_dir_all(&dir).ok();
}

/// Verifies `--emit staticlib`: the archive links directly into a host binary,
/// with no `dlopen` and no runtime symbol resolution involved.
///
/// This is the delivery form an Xcode project consumes, and it exercises the
/// *non-PIC* codegen path — a staticlib takes `Emitter::new`, not
/// `Emitter::new_pic`, because GOT indirection exists for `dlopen`-time
/// resolution rather than for position independence. The host executable is PIE
/// regardless, so a link succeeding here is what proves that distinction holds.
#[test]
fn test_staticlib_links_directly_into_a_host_binary() {
    let dir = make_test_dir("elephc_staticlib");
    fs::write(dir.join("strret.php"), STRING_RETURN_PHP).unwrap();

    let output = elephc_command(&dir)
        .args(["--emit", "staticlib", "strret.php"])
        .output()
        .expect("failed to run elephc");
    assert!(
        output.status.success(),
        "staticlib compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let archive = dir.join("libstrret.a");
    assert!(archive.exists(), "expected an archive at {:?}", archive);

    // `ar rcs` must have written a symbol index, or the link below cannot
    // resolve anything out of the archive.
    let listing = Command::new("ar")
        .arg("t")
        .arg(&archive)
        .output()
        .expect("failed to run ar");
    let members = String::from_utf8_lossy(&listing.stdout);
    assert!(
        members.contains("strret.o"),
        "the user object must be a member: {members}"
    );
    assert!(
        members.contains("runtime-"),
        "the runtime object must be a member: {members}"
    );

    let c_path = dir.join("host.c");
    fs::write(&c_path, STATICLIB_HOST_C).unwrap();
    let host = dir.join("statichost");
    let compile = Command::new("cc")
        .arg("-o")
        .arg(&host)
        .arg(&c_path)
        .arg(&archive)
        .output()
        .expect("failed to spawn the system C compiler");
    assert!(
        compile.status.success(),
        "linking against the archive failed:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&host).output().expect("failed to run the host");
    assert!(
        run.status.success(),
        "host run failed (exit {:?}):\n{}",
        run.status.code(),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "Hello, static 13 fixed 5\n"
    );

    fs::remove_dir_all(&dir).ok();
}

/// Verifies the string-return half of the export ABI end to end.
///
/// Covers the three provenances that reach a `return` differently, because each
/// exercises a different part of the contract:
/// - `greet` concatenates, so lowering already treats the result as scratch and
///   persists it — the ordinary path;
/// - `fixed_label` returns a literal, which lives in `.rodata` and would be
///   illegal to `elephc_free` unless the export forces a persist;
/// - `echo_back` returns its parameter, the dangerous case: without the forced
///   persist the host would get back *its own* pointer and then free it.
///
/// The host asserts that last point directly (`e.ptr == mine` must be 0), which
/// is what makes this a test of ownership rather than of string contents. It
/// also frees every returned pointer plus a `NULL`, so a mis-wired
/// `elephc_free` shows up as a crash rather than passing quietly.
#[test]
fn test_cdylib_string_returns_transfer_ownership_to_the_host() {
    let dir = make_test_dir("elephc_cdylib_strret");
    fs::write(dir.join("strret.php"), STRING_RETURN_PHP).unwrap();

    let output = elephc_command(&dir)
        .args(["--emit", "cdylib", "strret.php"])
        .output()
        .expect("failed to run elephc");
    assert!(
        output.status.success(),
        "cdylib compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let lib_path = dir.join(shared_lib_name("strret"));
    let host = compile_c_host(&dir, STRING_RETURN_HOST_C, "strhost");
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
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "Hello, world 12 world 5 fixed 5 0\n"
    );

    fs::remove_dir_all(&dir).ok();
}

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
