//! Purpose:
//! Tests configured mbstring through real static and shared library host boundaries.
//!
//! Called from:
//! - The library integration test binary with the actual compiler and managed PCRE2 fixture.
//!
//! Key details:
//! - Hosts exercise lazy scalar/string entry, explicit init, saved C arguments, and returned ownership.
//! - Failure injection supplies an empty argument list or missing provider to the real startup ABI.

use super::*;
use elephc::codegen::platform::{Arch, Target};

#[path = "../support/managed_pcre2.rs"]
mod managed_pcre2;

const SOURCE: &str = include_str!("mbstring/library.php");

/// Preserves host arguments and live settings across lazy exports and repeated explicit initialization.
#[test]
fn test_mbstring_library_initialization() {
    for mode in ["staticlib", "cdylib"] {
        let dir = make_test_dir("mbstring_library");
        let cache = managed_pcre2::prepare_managed_pcre2_cli_project(&dir, Target::detect_host());
        fs::write(dir.join("text.php"), SOURCE).unwrap();
        compile(&dir, &cache, mode, &[]);
        let host = compile_host(&dir, include_str!("mbstring/host.c"), mode == "staticlib");
        for first in ["scalar", "string", "init"] {
            let result = Command::new(&host).arg(first).output().unwrap();
            assert!(result.status.success(), "{mode}/{first}: {}\n{}\n{}",
                result.status, String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
            assert_eq!(result.stdout, b"configured:5:4:preserved\n");
        }
        fs::remove_dir_all(dir).unwrap();
    }
}

/// Reports provider and configuration validation failures through init, scalar, and string C returns.
#[test]
fn test_mbstring_library_configuration_failure() {
    let dir = make_test_dir("mbstring_library_failure");
    let cache = managed_pcre2::prepare_managed_pcre2_cli_project(&dir, Target::detect_host());
    fs::write(dir.join("text.php"), SOURCE).unwrap();
    compile(&dir, &cache, "staticlib", &[]);
    compile(&dir, &cache, "staticlib", &["--emit-asm"]);
    let assembly = fs::read_to_string(dir.join("text.s")).unwrap();
    let target = Target::detect_host();
    for (symbol, arm_argument, x86_argument) in [
        ("elephc_mbstring_configure_v1", "mov x1, #0", "xor esi, esi"),
        ("elephc_mbstring_mime_provider_v1", "mov x0, #0", "xor edi, edi"),
    ] {
        let (call, argument) = if target.arch == Arch::AArch64 {
            (format!("bl {}", target.extern_symbol(symbol)), arm_argument)
        } else { (format!("call {symbol}"), x86_argument) };
        assert_eq!(assembly.matches(&call).count(), 1);
        let patched = assembly.replace(&call, &format!("{argument}\n    {call}"));
        fs::write(dir.join("text.s"), patched).unwrap();
        let built = Command::new("cc").current_dir(&dir).args(["-c", "text.s", "-o", "text.o"]).output().unwrap();
        assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
        let replaced = Command::new("ar").current_dir(&dir).args(["r", "libtext.a", "text.o"]).output().unwrap();
        assert!(replaced.status.success(), "{}", String::from_utf8_lossy(&replaced.stderr));
        let host = compile_host(&dir, include_str!("mbstring/failure.c"), true);
        let result = Command::new(host).output().unwrap();
        assert!(result.status.success(), "{symbol}: {}\n{}\n{}", result.status,
            String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
        assert_eq!(result.stdout, b"configuration failure:host alive\n");
    }
    fs::remove_dir_all(dir).unwrap();
}

/// Compiles and assembles complete configured library user objects for all supported targets.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn test_mbstring_library_all_targets() {
    for (name, triple) in [("linux-x86_64", "x86_64-linux-gnu"),
        ("linux-aarch64", "aarch64-linux-gnu"), ("macos-aarch64", "arm64-apple-macos11"),
        ("ios-arm64", "arm64-apple-ios13"), ("ios-sim-arm64", "arm64-apple-ios13-simulator")] {
        let dir = make_test_dir("mbstring_library_target");
        fs::write(dir.join("text.php"), SOURCE).unwrap();
        compile(&dir, &dir.join("unused-native-cache"), "staticlib", &["--emit-asm", "--target", name]);
        let assembly = fs::read_to_string(dir.join("text.s")).unwrap();
        assert!(assembly.contains("__rt_mbstring_startup_status"), "{name}");
        let result = Command::new("clang").current_dir(&dir).args(["-target", triple, "-c", "text.s", "-o", "text.o"])
            .output().unwrap();
        assert!(result.status.success(), "{name}: {}", String::from_utf8_lossy(&result.stderr));
        fs::remove_dir_all(dir).unwrap();
    }
}

/// Invokes the real configured library compiler with a hermetic managed provider for final linking.
fn compile(dir: &Path, cache: &Path, mode: &str, flags: &[&str]) {
    let output = elephc_command(dir).env("ELEPHC_NATIVE_CACHE", cache)
        .args(["--emit", mode, "--ini", "default_charset=8bit"])
        .args(flags).arg("text.php").output().unwrap();
    assert!(output.status.success(), "{}: {}", dir.display(), String::from_utf8_lossy(&output.stderr));
}

/// Compiles a strict C host against the generated public header and selected library artifact.
fn compile_host(dir: &Path, source: &str, static_library: bool) -> PathBuf {
    fs::write(dir.join("host.c"), source).unwrap();
    let mut compiler = Command::new("cc");
    compiler.current_dir(dir).args(["-O2", "-Wall", "-Wextra", "-Werror", "host.c", "-I.", "-L.", "-ltext", "-o", "host"]);
    if static_library {
        // Static output contains user and runtime objects. Its consumer links the
        // bridge and native packages separately, as required by linker::archive.
        let bridge_dir = std::env::var_os("ELEPHC_MBSTRING_LIB_DIR")
            .filter(|directory| !directory.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(elephc_bin()).parent().unwrap().to_path_buf());
        let bridge = bridge_dir.join("libelephc_mbstring.a");
        assert!(bridge.is_file(), "build the mbstring bridge before this host test: {}", bridge.display());
        let target = Target::detect_host();
        compiler.arg(bridge).arg(managed_pcre2::test_pcre2_shim_archive(target));
        for library in ["pcre2-posix", "pcre2-8"] {
            compiler.arg(managed_pcre2::test_pcre2_static_archive_path(target, library));
        }
    }
    if cfg!(target_os = "macos") { compiler.arg("-Wl,-rpath,@loader_path"); }
    else { compiler.args(["-Wl,-rpath,$ORIGIN", "-lm", "-ldl", "-lpthread"]); }
    let result = compiler.output().unwrap();
    assert!(result.status.success(), "{}: {}", dir.display(), String::from_utf8_lossy(&result.stderr));
    dir.join("host")
}
