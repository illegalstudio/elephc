//! Purpose:
//! Native binary runner helpers for assembling runtimes, linking objects, and executing codegen fixtures.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Handles platform-specific linker flags, qemu ARM64 execution, and runtime object caching.
//! - Per-test assembly is fed to `as` over stdin so no intermediate `test.s`
//!   file is written, which shaves ~1/3 of the file-system events the macOS
//!   `syspolicyd` / on-access AV scans inspect during a full `cargo test`.

use std::io::Write as _;
use std::process::Stdio;

use super::*;

/// Assemble `asm` to `obj_path` by piping the source through `as`'s stdin so
/// no intermediate `.s` file is created.
fn assemble_from_stdin(asm: &str, obj_path: &Path) {
    let mut cmd = Command::new(assembler_cmd());
    if target().platform == Platform::MacOS {
        cmd.args(["-arch", target().darwin_arch_name()]);
    }
    cmd.arg("-o").arg(obj_path).arg("-");
    cmd.stdin(Stdio::piped());
    let mut child = cmd.spawn().expect("failed to spawn assembler");
    child
        .stdin
        .as_mut()
        .expect("assembler stdin missing")
        .write_all(asm.as_bytes())
        .expect("failed to feed assembler");
    let status = child.wait().expect("failed to wait for assembler");
    assert!(status.success(), "assembler failed");
}

/// Returns the cached base runtime object path, assembling the runtime on first call.
/// Creates a temp directory, generates an 8_388_608-byte heap runtime without optional
/// feature families, assembles it with `as`, and caches the `.o` path for legacy tests.
pub(crate) fn get_runtime_obj() -> &'static Path {
    RUNTIME_OBJ.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("elephc_test_runtime_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let runtime_asm = elephc::codegen::generate_runtime_with_features(
            8_388_608,
            target(),
            elephc::codegen::RuntimeFeatures::none(),
        );
        let obj_path = dir.join("runtime.o");
        assemble_from_stdin(&runtime_asm, &obj_path);
        obj_path
    })
}

/// Returns a cached runtime object assembled from the exact runtime assembly string.
///
/// The cache key is an FNV-1a hash of the full runtime assembly, so feature-gated
/// runtimes and custom heap sizes get distinct objects while repeated tests can
/// still share the assembled output.
pub(crate) fn runtime_obj_for_asm(runtime_asm: &str) -> std::path::PathBuf {
    let hash = runtime_asm_hash(runtime_asm);
    let cache = RUNTIME_OBJS_BY_ASM.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    let mut cache = cache.lock().expect("runtime asm cache poisoned");
    if let Some(path) = cache.get(&hash) {
        return path.clone();
    }

    let dir = std::env::temp_dir().join(format!("elephc_test_runtime_{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let asm_path = dir.join(format!("runtime_{hash:016x}.s"));
    let obj_path = dir.join(format!("runtime_{hash:016x}.o"));
    fs::write(&asm_path, runtime_asm).unwrap();

    let mut cmd = Command::new(assembler_cmd());
    if target().platform == Platform::MacOS {
        cmd.args(["-arch", target().darwin_arch_name()]);
    }
    cmd.arg("-o").arg(&obj_path).arg(&asm_path);
    let status = cmd.status().expect("failed to assemble feature runtime");
    assert!(status.success(), "feature runtime assembler failed");
    cache.insert(hash, obj_path.clone());
    obj_path
}

/// Computes a stable FNV-1a hash for runtime assembly cache keys.
fn runtime_asm_hash(asm: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in asm.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Assembles a base runtime object with a custom heap size, writing the `.o` to `dir`.
/// Generates ARM64/x86_64 runtime assembly without optional feature families and assembles
/// it using `assembler_cmd()`. Used by tests that need non-default heap sizes.
pub(crate) fn assemble_custom_runtime(heap_size: usize, dir: &Path) -> std::path::PathBuf {
    let runtime_asm = elephc::codegen::generate_runtime_with_features(
        heap_size,
        target(),
        elephc::codegen::RuntimeFeatures::none(),
    );
    let obj_path = dir.join("runtime.o");
    assemble_from_stdin(&runtime_asm, &obj_path);
    obj_path
}


/// Links a user object file and a runtime object into a final native binary.
/// On macOS uses `ld` with SDK/platform_version flags; on Linux uses `gcc` with
/// static linking when no extra libs are needed. Adds `-lm -lpthread` on Linux.
pub(crate) fn link_binary(
    obj_path: &Path,
    runtime_obj: &Path,
    bin_path: &Path,
    extra_link_libs: &[String],
    extra_link_paths: &[String],
    extra_frameworks: &[String],
) {
    let actual_link_libs = effective_link_libs(extra_link_libs);

    // The elephc-tls staticlib lives in `<target>/debug` alongside the test
    // binaries; surface that path automatically when a program uses the
    // https:// wrapper so callers do not have to thread it through. The
    // Docker scripts override CARGO_TARGET_DIR to point at a shared volume,
    // so honour that envvar before falling back to the in-tree target/.
    let needs_elephc_tls = actual_link_libs.iter().any(|l| *l == "elephc_tls");
    let elephc_tls_dir = match std::env::var("CARGO_TARGET_DIR") {
        Ok(dir) if !dir.is_empty() => format!("{}/debug", dir),
        _ => format!("{}/target/debug", env!("CARGO_MANIFEST_DIR")),
    };

    match target().platform {
        Platform::MacOS => {
            let mut ld_cmd = Command::new("ld");
            ld_cmd.args(["-arch", target().darwin_arch_name(), "-e", "_main", "-o"]);
            ld_cmd.arg(bin_path);
            ld_cmd.arg(obj_path);
            ld_cmd.arg(runtime_obj);
            ld_cmd.args(["-lSystem", "-syslibroot"]);
            ld_cmd.arg(get_sdk_path());
            ld_cmd.args([
                "-platform_version",
                "macos",
                get_sdk_version(),
                get_sdk_version(),
            ]);
            if needs_elephc_tls {
                ld_cmd.arg(format!("-L{}", elephc_tls_dir));
            }
            for path in extra_link_paths {
                ld_cmd.arg(format!("-L{}", path));
            }
            for lib in &actual_link_libs {
                ld_cmd.arg(format!("-l{}", lib));
            }
            for framework in extra_frameworks {
                ld_cmd.args(["-framework", framework]);
            }
            // The PostgreSQL driver in the PDO bridge pulls in `whoami`, which
            // references CoreFoundation / SystemConfiguration on macOS.
            if actual_link_libs.iter().any(|lib| *lib == "elephc_pdo") {
                ld_cmd.args(["-framework", "CoreFoundation"]);
                ld_cmd.args(["-framework", "SystemConfiguration"]);
            }
            let ld_out = ld_cmd.output().expect("failed to run linker");
            assert!(
                ld_out.status.success(),
                "linker failed:\n{}",
                String::from_utf8_lossy(&ld_out.stderr)
            );
        }
        Platform::Linux => {
            let mut ld_cmd = Command::new(gcc_cmd());
            ld_cmd.arg("-o").arg(bin_path);
            ld_cmd.arg(obj_path);
            ld_cmd.arg(runtime_obj);
            if actual_link_libs.is_empty() {
                ld_cmd.arg("-static");
            }
            if !actual_link_libs.is_empty() {
                ld_cmd.arg("-Wl,--no-as-needed");
            }
            if needs_elephc_tls {
                ld_cmd.arg(format!("-L{}", elephc_tls_dir));
            }
            for path in extra_link_paths {
                ld_cmd.arg(format!("-L{}", path));
            }
            for lib in &actual_link_libs {
                ld_cmd.arg(format!("-l{}", lib));
            }
            if !actual_link_libs.is_empty() {
                ld_cmd.arg("-Wl,--as-needed");
            }
            // Math and POSIX regex libraries needed on Linux
            ld_cmd.args(["-lm", "-lpthread"]);
            // rustls (elephc-tls) and the elephc-pdo bridge staticlib (PDO)
            // both pull in the dynamic loader for the libc unwinder on Linux.
            if needs_elephc_tls || actual_link_libs.iter().any(|lib| *lib == "elephc_pdo") {
                ld_cmd.arg("-ldl");
            }
            let ld_out = ld_cmd.output().expect("failed to run linker");
            assert!(
                ld_out.status.success(),
                "linker failed:\n{}",
                String::from_utf8_lossy(&ld_out.stderr)
            );
        }
    }
}

/// Runs a compiled binary directly, using qemu on Linux x86_64 to emulate ARM64.
/// On other platform/arch combinations, execs the binary natively.
/// Used for post-link execution of already-assembled test binaries.
pub(crate) fn run_binary(bin_path: &Path, dir: &Path) -> std::process::Output {
    if target().platform == Platform::Linux
        && target().arch == Arch::AArch64
        && cfg!(target_arch = "x86_64")
    {
        let mut cmd = Command::new("qemu-aarch64-static");
        if let Some(sysroot) = qemu_sysroot() {
            cmd.args(["-L", sysroot]);
        }
        cmd.arg(bin_path)
            .current_dir(dir)
            .output()
            .expect("failed to run compiled binary via qemu")
    } else {
        Command::new(bin_path)
            .current_dir(dir)
            .output()
            .expect("failed to run compiled binary")
    }
}

/// Assembles user assembly, links it with a runtime object, runs the binary,
/// and returns stdout. Asserts the binary exits successfully. Used for happy-path codegen tests.
pub(crate) fn assemble_and_run(
    user_asm: &str,
    runtime_obj: &Path,
    dir: &Path,
    extra_link_libs: &[String],
    extra_link_paths: &[String],
    extra_frameworks: &[String],
) -> String {
    let obj_path = dir.join("test.o");
    let bin_path = dir.join("test");

    assemble_from_stdin(user_asm, &obj_path);

    link_binary(
        &obj_path,
        runtime_obj,
        &bin_path,
        extra_link_libs,
        extra_link_paths,
        extra_frameworks,
    );

    let output = run_binary(&bin_path, dir);
    assert!(
        output.status.success(),
        "binary exited with error: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8(output.stdout).unwrap()
}

// Captures stdout and stderr from a compiled binary, along with its exit status.
// Used by tests that need to inspect both output streams without asserting success,
// or by error/regression tests that need to validate stderr without requiring exit failure.
pub(crate) struct ProgramOutput {
    // Raw stdout bytes decoded as UTF-8.
    pub(crate) stdout: String,
    // Raw stderr bytes decoded as UTF-8.
    pub(crate) stderr: String,
    // true if the process exited with a successful (zero) exit code.
    pub(crate) success: bool,
}

/// Assembles user assembly, links it with a runtime object, runs the binary,
/// and captures stdout, stderr, and exit status. Asserts the binary exits successfully.
pub(crate) fn assemble_and_run_capture(
    user_asm: &str,
    runtime_obj: &Path,
    dir: &Path,
    extra_link_libs: &[String],
    extra_link_paths: &[String],
    extra_frameworks: &[String],
) -> ProgramOutput {
    let obj_path = dir.join("test.o");
    let bin_path = dir.join("test");

    assemble_from_stdin(user_asm, &obj_path);

    link_binary(
        &obj_path,
        runtime_obj,
        &bin_path,
        extra_link_libs,
        extra_link_paths,
        extra_frameworks,
    );

    let output = run_binary(&bin_path, dir);

    ProgramOutput {
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
        success: output.status.success(),
    }
}

/// Assembles user assembly, links it with a runtime object, runs the binary,
/// and returns stderr. Asserts the binary exits with failure. Used for error/regression tests.
pub(crate) fn assemble_and_run_expect_failure(
    user_asm: &str,
    runtime_obj: &Path,
    dir: &Path,
    extra_link_libs: &[String],
    extra_link_paths: &[String],
    extra_frameworks: &[String],
) -> String {
    let obj_path = dir.join("test.o");
    let bin_path = dir.join("test");

    assemble_from_stdin(user_asm, &obj_path);

    link_binary(
        &obj_path,
        runtime_obj,
        &bin_path,
        extra_link_libs,
        extra_link_paths,
        extra_frameworks,
    );

    let output = run_binary(&bin_path, dir);
    assert!(!output.status.success(), "binary unexpectedly succeeded");

    String::from_utf8(output.stderr).unwrap()
}
