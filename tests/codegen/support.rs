#![allow(dead_code)]

use std::collections::HashSet;
pub(crate) use std::fs;
pub(crate) use std::path::Path;
pub(crate) use std::process::Command;
pub(crate) use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

pub(crate) use elephc::codegen::platform::{Arch, Platform, Target};

pub(crate) static TEST_ID: AtomicU64 = AtomicU64::new(0);
pub(crate) static SDK_PATH: OnceLock<String> = OnceLock::new();
pub(crate) static SDK_VERSION: OnceLock<String> = OnceLock::new();
pub(crate) static RUNTIME_OBJ: OnceLock<std::path::PathBuf> = OnceLock::new();
pub(crate) static QEMU_SYSROOT: OnceLock<Option<String>> = OnceLock::new();
pub(crate) static TEST_TARGET: OnceLock<Target> = OnceLock::new();

pub(crate) fn target() -> Target {
    *TEST_TARGET.get_or_init(|| {
        std::env::var("ELEPHC_TEST_TARGET")
            .ok()
            .map(|value| Target::parse(&value).expect("invalid ELEPHC_TEST_TARGET"))
            .unwrap_or_else(Target::detect_host)
    })
}

pub(crate) fn get_sdk_path() -> &'static str {
    SDK_PATH.get_or_init(|| {
        Command::new("xcrun")
            .args(["--show-sdk-path"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    })
}

pub(crate) fn get_sdk_version() -> &'static str {
    SDK_VERSION.get_or_init(|| {
        match Command::new("xcrun")
            .args(["--sdk", "macosx", "--show-sdk-version"])
            .output()
        {
            Ok(output) => {
                let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if version.is_empty() {
                    "15.0".to_string()
                } else {
                    version
                }
            }
            Err(_) => "15.0".to_string(),
        }
    })
}

/// Get the assembler command for the current platform.
pub(crate) fn assembler_cmd() -> &'static str {
    target().assembler_cmd()
}

/// Get the linker/gcc command for the current platform.
pub(crate) fn gcc_cmd() -> &'static str {
    target().linker_cmd()
}

/// Pre-assemble the runtime into a cached .o file. Built once per test
/// session, reused by every test via two-object linking.
pub(crate) fn get_runtime_obj() -> &'static Path {
    RUNTIME_OBJ.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("elephc_test_runtime_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let runtime_asm = elephc::codegen::generate_runtime(8_388_608, target());
        let asm_path = dir.join("runtime.s");
        let obj_path = dir.join("runtime.o");
        fs::write(&asm_path, &runtime_asm).unwrap();

        let mut cmd = Command::new(assembler_cmd());
        if target().platform == Platform::MacOS {
            cmd.args(["-arch", target().darwin_arch_name()]);
        }
        cmd.arg("-o").arg(&obj_path).arg(&asm_path);
        let status = cmd.status().expect("failed to assemble runtime");
        assert!(status.success(), "runtime assembler failed");
        obj_path
    })
}

/// Assemble a custom runtime for tests that need a non-default heap size.
pub(crate) fn assemble_custom_runtime(heap_size: usize, dir: &Path) -> std::path::PathBuf {
    let runtime_asm = elephc::codegen::generate_runtime(heap_size, target());
    let asm_path = dir.join("runtime.s");
    let obj_path = dir.join("runtime.o");
    fs::write(&asm_path, &runtime_asm).unwrap();

    let mut cmd = Command::new(assembler_cmd());
    if target().platform == Platform::MacOS {
        cmd.args(["-arch", target().darwin_arch_name()]);
    }
    cmd.arg("-o").arg(&obj_path).arg(&asm_path);
    let status = cmd.status().expect("failed to assemble custom runtime");
    assert!(status.success(), "custom runtime assembler failed");
    obj_path
}

pub(crate) fn default_link_paths() -> Vec<String> {
    let mut paths = Vec::new();
    match target().platform {
        Platform::MacOS => {
            for candidate in ["/opt/homebrew/lib", "/usr/local/lib"] {
                if std::path::Path::new(candidate).exists() {
                    paths.push(candidate.to_string());
                }
            }
        }
        Platform::Linux => {
            for candidate in ["/usr/aarch64-linux-gnu/lib", "/usr/lib/aarch64-linux-gnu"] {
                if std::path::Path::new(candidate).exists() {
                    paths.push(candidate.to_string());
                }
            }
        }
    }
    paths
}

pub(crate) fn effective_link_libs(extra_link_libs: &[String]) -> Vec<&str> {
    extra_link_libs
        .iter()
        .map(String::as_str)
        .filter(|lib| *lib != "System")
        .collect()
}

pub(crate) fn qemu_sysroot() -> Option<&'static str> {
    QEMU_SYSROOT
        .get_or_init(|| match target().platform {
            Platform::Linux => {
                let compiler = gcc_cmd();
                if let Ok(output) = Command::new(compiler).arg("-print-sysroot").output() {
                    let sysroot = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if !sysroot.is_empty()
                        && sysroot != "/"
                        && std::path::Path::new(&sysroot).exists()
                    {
                        return Some(sysroot);
                    }
                }
                for candidate in ["/usr/aarch64-linux-gnu", "/usr/local/aarch64-linux-gnu"] {
                    if std::path::Path::new(candidate)
                        .join("lib/ld-linux-aarch64.so.1")
                        .exists()
                        || std::path::Path::new(candidate)
                            .join("lib64/ld-linux-aarch64.so.1")
                            .exists()
                    {
                        return Some(candidate.to_string());
                    }
                }
                None
            }
            Platform::MacOS => None,
        })
        .as_deref()
}

pub(crate) fn link_binary(
    obj_path: &Path,
    runtime_obj: &Path,
    bin_path: &Path,
    extra_link_libs: &[String],
    extra_link_paths: &[String],
    extra_frameworks: &[String],
) {
    let actual_link_libs = effective_link_libs(extra_link_libs);

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
            for path in extra_link_paths {
                ld_cmd.arg(format!("-L{}", path));
            }
            for lib in &actual_link_libs {
                ld_cmd.arg(format!("-l{}", lib));
            }
            for framework in extra_frameworks {
                ld_cmd.args(["-framework", framework]);
            }
            let ld_status = ld_cmd.status().expect("failed to run linker");
            assert!(ld_status.success(), "linker failed");
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
            let ld_status = ld_cmd.status().expect("failed to run linker");
            assert!(ld_status.success(), "linker failed");
        }
    }
}

/// Run a compiled binary, using qemu on Linux x86_64 for ARM64 binaries.
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

#[test]
fn test_effective_link_libs_ignores_system() {
    let libs = vec!["System".to_string(), "crypto".to_string()];
    assert_eq!(effective_link_libs(&libs), vec!["crypto"]);
}

pub(crate) fn assemble_and_run(
    user_asm: &str,
    runtime_obj: &Path,
    dir: &Path,
    extra_link_libs: &[String],
    extra_link_paths: &[String],
    extra_frameworks: &[String],
) -> String {
    let asm_path = dir.join("test.s");
    let obj_path = dir.join("test.o");
    let bin_path = dir.join("test");

    fs::write(&asm_path, user_asm).unwrap();

    let mut as_cmd = Command::new(assembler_cmd());
    if target().platform == Platform::MacOS {
        as_cmd.args(["-arch", target().darwin_arch_name()]);
    }
    as_cmd.arg("-o").arg(&obj_path).arg(&asm_path);
    let as_status = as_cmd.status().expect("failed to run assembler");
    assert!(as_status.success(), "assembler failed");

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

pub(crate) struct ProgramOutput {
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) success: bool,
}

pub(crate) fn assemble_and_run_capture(
    user_asm: &str,
    runtime_obj: &Path,
    dir: &Path,
    extra_link_libs: &[String],
    extra_link_paths: &[String],
    extra_frameworks: &[String],
) -> ProgramOutput {
    let asm_path = dir.join("test.s");
    let obj_path = dir.join("test.o");
    let bin_path = dir.join("test");

    fs::write(&asm_path, user_asm).unwrap();

    let mut as_cmd = Command::new(assembler_cmd());
    if target().platform == Platform::MacOS {
        as_cmd.args(["-arch", target().darwin_arch_name()]);
    }
    as_cmd.arg("-o").arg(&obj_path).arg(&asm_path);
    let as_status = as_cmd.status().expect("failed to run assembler");
    assert!(as_status.success(), "assembler failed");

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

pub(crate) fn assemble_and_run_expect_failure(
    user_asm: &str,
    runtime_obj: &Path,
    dir: &Path,
    extra_link_libs: &[String],
    extra_link_paths: &[String],
    extra_frameworks: &[String],
) -> String {
    let asm_path = dir.join("test.s");
    let obj_path = dir.join("test.o");
    let bin_path = dir.join("test");

    fs::write(&asm_path, user_asm).unwrap();

    let mut as_cmd = Command::new(assembler_cmd());
    if target().platform == Platform::MacOS {
        as_cmd.args(["-arch", target().darwin_arch_name()]);
    }
    as_cmd.arg("-o").arg(&obj_path).arg(&asm_path);
    let as_status = as_cmd.status().expect("failed to run assembler");
    assert!(as_status.success(), "assembler failed");

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

pub(crate) fn compile_source_to_asm_with_options(
    source: &str,
    dir: &Path,
    heap_size: usize,
    gc_stats: bool,
    heap_debug: bool,
) -> (String, String, Vec<String>) {
    compile_source_to_asm_with_defines(
        source,
        dir,
        &HashSet::new(),
        heap_size,
        gc_stats,
        heap_debug,
    )
}

pub(crate) fn compile_source_to_asm_with_defines(
    source: &str,
    dir: &Path,
    defines: &HashSet<String>,
    heap_size: usize,
    gc_stats: bool,
    heap_debug: bool,
) -> (String, String, Vec<String>) {
    let tokens = elephc::lexer::tokenize(source).expect("tokenize failed");
    let ast = elephc::parser::parse(&tokens).expect("parse failed");
    let ast = elephc::conditional::apply(ast, defines);
    let resolved = elephc::resolver::resolve(ast, dir).expect("resolve failed");
    let resolved = elephc::name_resolver::resolve(resolved).expect("name resolve failed");
    let check_result = elephc::types::check_with_target(&resolved, target()).expect("type check failed");
    let (user_asm, runtime_asm) = elephc::codegen::generate(
        &resolved,
        &check_result.global_env,
        &check_result.functions,
        &check_result.interfaces,
        &check_result.classes,
        &check_result.enums,
        &check_result.packed_classes,
        &check_result.extern_functions,
        &check_result.extern_classes,
        &check_result.extern_globals,
        heap_size,
        gc_stats,
        heap_debug,
        target(),
    );
    // user assembly is already platform-correct (emitters handle platform at emit time)
    (user_asm, runtime_asm, check_result.required_libraries)
}

pub(crate) fn inject_main_exit_harness(asm: &str, harness: &str) -> String {
    let needle = match (target().platform, target().arch) {
        (Platform::MacOS, Arch::AArch64) => "    mov x0, #0\n    mov x16, #1\n    svc #0x80",
        (Platform::Linux, Arch::AArch64) => "    mov x0, #0\n    mov x8, #93\n    svc #0",
        (Platform::Linux, Arch::X86_64) => "    mov edi, 0\n    mov eax, 60\n    syscall",
        (_, Arch::X86_64) => panic!(
            "main exit harness is not implemented yet for target {}",
            target()
        ),
    };
    // Harness strings are written in macOS assembly dialect; transform for Linux if needed
    let harness = target().transform_assembly(harness);
    let replacement = format!("{harness}\n{needle}");
    let patched = asm.replacen(needle, &replacement, 1);
    assert_ne!(patched, asm, "failed to inject main exit harness");
    patched
}

pub(crate) fn compile_harness_expect_failure(source: &str, heap_size: usize, harness: &str) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, heap_size, false, true);
    let runtime_obj = assemble_custom_runtime(heap_size, &dir);
    let patched = inject_main_exit_harness(&user_asm, harness);
    let stderr = assemble_and_run_expect_failure(
        &patched,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    let _ = fs::remove_dir_all(&dir);
    stderr
}

pub(crate) fn compile_harness_and_run(source: &str, heap_size: usize, harness: &str) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, heap_size, false, false);
    let runtime_obj = assemble_custom_runtime(heap_size, &dir);
    let patched = inject_main_exit_harness(&user_asm, harness);
    let stdout = assemble_and_run(
        &patched,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    let _ = fs::remove_dir_all(&dir);
    stdout
}

pub(crate) fn compile_harness_and_run_with_heap_debug(
    source: &str,
    heap_size: usize,
    harness: &str,
) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, heap_size, false, true);
    let runtime_obj = assemble_custom_runtime(heap_size, &dir);
    let patched = inject_main_exit_harness(&user_asm, harness);
    let stdout = assemble_and_run(
        &patched,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    let _ = fs::remove_dir_all(&dir);
    stdout
}

pub(crate) fn compile_and_run_with_gc_stats(source: &str) -> ProgramOutput {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, true, false);
    let output = assemble_and_run_capture(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    let _ = fs::remove_dir_all(&dir);
    output
}

pub(crate) fn compile_and_run_with_heap_debug(source: &str) -> ProgramOutput {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, true);
    let output = assemble_and_run_capture(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    let _ = fs::remove_dir_all(&dir);
    output
}

pub(crate) fn parse_gc_stats(stderr: &str) -> (u64, u64) {
    let line = stderr
        .lines()
        .find(|line| line.starts_with("GC: allocs="))
        .unwrap_or_else(|| panic!("missing gc stats line: {stderr}"));
    let allocs = line
        .split("allocs=")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or_else(|| panic!("missing alloc count: {stderr}"));
    let frees = line
        .split("frees=")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or_else(|| panic!("missing free count: {stderr}"));
    (allocs, frees)
}

/// Compile a PHP source string to a native binary, run it, and return stdout.
/// Uses the elephc library directly (no subprocess) for tokenize → parse → check → codegen.
/// Only spawns as + ld + binary execution.
pub(crate) fn compile_and_run_with_heap_size(source: &str, heap_size: usize) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, heap_size, false, false);

    let custom_rt;
    let runtime_obj: &Path = if heap_size == 8_388_608 {
        get_runtime_obj()
    } else {
        custom_rt = assemble_custom_runtime(heap_size, &dir);
        &custom_rt
    };

    let elephc_out = assemble_and_run(
        &user_asm,
        runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    // PHP cross-check (opt-in via ELEPHC_PHP_CHECK=1)
    if std::env::var("ELEPHC_PHP_CHECK").is_ok() {
        let php_path = dir.join("test.php");
        fs::write(&php_path, source).unwrap();
        if let Ok(php_output) = Command::new("php").arg(&php_path).output() {
            if php_output.status.success() {
                let php_out = String::from_utf8_lossy(&php_output.stdout);
                if elephc_out != php_out.as_ref() {
                    eprintln!(
                        "PHP compat note: output differs for test.\n  elephc: {:?}\n  php:    {:?}",
                        elephc_out, php_out
                    );
                }
            }
        }
    }

    let _ = fs::remove_dir_all(&dir);
    elephc_out
}

pub(crate) fn compile_and_run(source: &str) -> String {
    compile_and_run_with_heap_size(source, 8_388_608)
}

pub(crate) fn compile_and_run_with_defines(source: &str, defines: &[&str]) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let define_set: HashSet<String> = defines.iter().map(|define| (*define).to_string()).collect();
    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_defines(source, &dir, &define_set, 8_388_608, false, false);
    let elephc_out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    let _ = fs::remove_dir_all(&dir);
    elephc_out
}

pub(crate) fn compile_cli_file_and_run(source: &str, defines: &[&str]) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_cli_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let php_path = dir.join("main.php");
    fs::write(&php_path, source).unwrap();

    let elephc_bin = std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("failed to resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    });
    let mut compile_cmd = Command::new(elephc_bin);
    for define in defines {
        compile_cmd.arg("--define").arg(define);
    }
    compile_cmd.arg(&php_path).current_dir(&dir);
    let compile_out = compile_cmd.output().expect("failed to run elephc CLI");
    assert!(
        compile_out.status.success(),
        "elephc CLI failed: {}",
        String::from_utf8_lossy(&compile_out.stderr)
    );

    let bin_path = dir.join("main");
    let output = run_binary(&bin_path, &dir);
    assert!(
        output.status.success(),
        "CLI-compiled binary exited with error"
    );

    let _ = fs::remove_dir_all(&dir);
    String::from_utf8(output.stdout).unwrap()
}

/// Compile a PHP source string and assert the generated binary fails at runtime.
pub(crate) fn compile_and_run_expect_failure(source: &str) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    let output = assemble_and_run_expect_failure(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    let _ = fs::remove_dir_all(&dir);
    output
}

/// Compile a PHP project with multiple files using the library directly.
pub(crate) fn compile_and_run_files(files: &[(&str, &str)], main_file: &str) -> String {
    compile_and_run_files_with_defines(files, main_file, &[])
}

pub(crate) fn compile_and_run_files_with_defines(
    files: &[(&str, &str)],
    main_file: &str,
    defines: &[&str],
) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    for (path, content) in files {
        let full_path = dir.join(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full_path, content).unwrap();
    }

    let php_path = dir.join(main_file);
    let source = fs::read_to_string(&php_path).unwrap();
    let base_dir = php_path.parent().unwrap();

    let tokens = elephc::lexer::tokenize(&source).expect("tokenize failed");
    let ast = elephc::parser::parse(&tokens).expect("parse failed");
    let define_set: HashSet<String> = defines.iter().map(|define| (*define).to_string()).collect();
    let ast = elephc::conditional::apply(ast, &define_set);
    let resolved = elephc::resolver::resolve(ast, base_dir).expect("resolve failed");
    let resolved = elephc::name_resolver::resolve(resolved).expect("name resolve failed");
    let check_result =
        elephc::types::check_with_target(&resolved, target()).expect("type check failed");
    let (user_asm, _runtime_asm) = elephc::codegen::generate(
        &resolved,
        &check_result.global_env,
        &check_result.functions,
        &check_result.interfaces,
        &check_result.classes,
        &check_result.enums,
        &check_result.packed_classes,
        &check_result.extern_functions,
        &check_result.extern_classes,
        &check_result.extern_globals,
        8_388_608,
        false,
        false,
        target(),
    );
    // user assembly is already platform-correct (emitters handle platform at emit time)

    let elephc_out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &check_result.required_libraries,
        &default_link_paths(),
        &[],
    );
    let _ = fs::remove_dir_all(&dir);
    elephc_out
}

/// Write multiple files and attempt compilation. Returns true if compilation fails.
pub(crate) fn compile_files_fails(files: &[(&str, &str)], main_file: &str) -> bool {
    compile_files_fails_with_defines(files, main_file, &[])
}

pub(crate) fn compile_files_fails_with_defines(
    files: &[(&str, &str)],
    main_file: &str,
    defines: &[&str],
) -> bool {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    for (path, content) in files {
        let full_path = dir.join(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full_path, content).unwrap();
    }

    let php_path = dir.join(main_file);
    let source = fs::read_to_string(&php_path).unwrap();
    let base_dir = php_path.parent().unwrap();

    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let tokens = elephc::lexer::tokenize(&source)?;
        let ast = elephc::parser::parse(&tokens)?;
        let define_set: HashSet<String> =
            defines.iter().map(|define| (*define).to_string()).collect();
        let ast = elephc::conditional::apply(ast, &define_set);
        let resolved = elephc::resolver::resolve(ast, base_dir)?;
        let resolved = elephc::name_resolver::resolve(resolved)?;
        elephc::types::check_with_target(&resolved, target())?;
        Ok(())
    })();

    let _ = fs::remove_dir_all(&dir);
    result.is_err()
}

pub(crate) fn compile_and_run_with_stdin(source: &str, stdin_data: &str) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let tokens = elephc::lexer::tokenize(source).expect("tokenize failed");
    let ast = elephc::parser::parse(&tokens).expect("parse failed");
    let resolved = elephc::resolver::resolve(ast, &dir).expect("resolve failed");
    let resolved = elephc::name_resolver::resolve(resolved).expect("name resolve failed");
    let check_result = elephc::types::check_with_target(&resolved, target()).expect("type check failed");
    let (user_asm, _runtime_asm) = elephc::codegen::generate(
        &resolved,
        &check_result.global_env,
        &check_result.functions,
        &check_result.interfaces,
        &check_result.classes,
        &check_result.enums,
        &check_result.packed_classes,
        &check_result.extern_functions,
        &check_result.extern_classes,
        &check_result.extern_globals,
        8_388_608,
        false,
        false,
        target(),
    );
    // user assembly is already platform-correct (emitters handle platform at emit time)

    let asm_path = dir.join("test.s");
    let obj_path = dir.join("test.o");
    let bin_path = dir.join("test");

    fs::write(&asm_path, &user_asm).unwrap();

    let mut as_cmd = Command::new(assembler_cmd());
    if target().platform == Platform::MacOS {
        as_cmd.args(["-arch", target().darwin_arch_name()]);
    }
    as_cmd.arg("-o").arg(&obj_path).arg(&asm_path);
    let as_status = as_cmd.status().expect("failed to run assembler");
    assert!(as_status.success(), "assembler failed");

    link_binary(
        &obj_path,
        get_runtime_obj(),
        &bin_path,
        &check_result.required_libraries,
        &default_link_paths(),
        &[],
    );

    use std::io::Write;
    let bin_cmd = if target().platform == Platform::Linux
        && target().arch == Arch::AArch64
        && cfg!(target_arch = "x86_64")
    {
        "qemu-aarch64-static"
    } else {
        bin_path.to_str().unwrap()
    };
    let mut cmd = if target().platform == Platform::Linux
        && target().arch == Arch::AArch64
        && cfg!(target_arch = "x86_64")
    {
        let mut c = Command::new(bin_cmd);
        c.arg(&bin_path);
        c
    } else {
        Command::new(&bin_path)
    };
    let mut child = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn binary");

    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(stdin_data.as_bytes()).unwrap();
    }
    drop(child.stdin.take());

    let output = child.wait_with_output().expect("failed to wait for binary");
    assert!(output.status.success(), "binary exited with error");

    let _ = fs::remove_dir_all(&dir);
    String::from_utf8(output.stdout).unwrap()
}

/// Compile and run in a specific temp dir (returns dir path for file I/O tests).
pub(crate) fn compile_and_run_in_dir(source: &str) -> (String, std::path::PathBuf) {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let tokens = elephc::lexer::tokenize(source).expect("tokenize failed");
    let ast = elephc::parser::parse(&tokens).expect("parse failed");
    let resolved = elephc::resolver::resolve(ast, &dir).expect("resolve failed");
    let resolved = elephc::name_resolver::resolve(resolved).expect("name resolve failed");
    let check_result = elephc::types::check_with_target(&resolved, target()).expect("type check failed");
    let (user_asm, _runtime_asm) = elephc::codegen::generate(
        &resolved,
        &check_result.global_env,
        &check_result.functions,
        &check_result.interfaces,
        &check_result.classes,
        &check_result.enums,
        &check_result.packed_classes,
        &check_result.extern_functions,
        &check_result.extern_classes,
        &check_result.extern_globals,
        8_388_608,
        false,
        false,
        target(),
    );
    // user assembly is already platform-correct (emitters handle platform at emit time)

    let elephc_out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &check_result.required_libraries,
        &default_link_paths(),
        &[],
    );
    (elephc_out, dir)
}
