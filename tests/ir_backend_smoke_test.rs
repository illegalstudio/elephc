//! Purpose:
//! Integration smoke tests for the opt-in EIR backend CLI path.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - These tests exercise the binary-level `--ir-backend` path instead of only
//!   testing library helpers.

use std::fs;
use std::process::Command;

/// Returns the path to the cargo-built `elephc` binary.
fn elephc_cli_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("failed to resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    })
}

/// Verifies the IR backend compiles, links, and runs straight-line scalar echo programs.
#[test]
fn ir_backend_echoes_scalar_literals() {
    for (name, source, expected) in [
        ("int", "<?php echo 42;", "42"),
        ("string", "<?php echo \"hi\";", "hi"),
        ("bool_true", "<?php echo true;", "1"),
        ("bool_false", "<?php echo false;", ""),
        ("null", "<?php echo null;", ""),
        ("float", "<?php echo 1.5;", "1.5"),
        ("local_store", "<?php $x = 40; echo $x;", "40"),
        ("argc_load", "<?php echo $argc;", "1"),
    ] {
        let output = compile_and_run_ir_backend(name, source);
        assert_eq!(output, expected, "unexpected stdout for {name}");
    }
}

/// Compiles `source` with `--ir-backend`, runs the output binary, and returns stdout.
fn compile_and_run_ir_backend(name: &str, source: &str) -> String {
    let dir = std::env::temp_dir().join(format!(
        "elephc_ir_backend_{}_{}_{}",
        name,
        std::process::id(),
        unique_test_id()
    ));
    fs::create_dir_all(&dir).expect("failed to create IR backend hello directory");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("failed to write IR backend PHP fixture");

    let compile = Command::new(elephc_cli_bin())
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .current_dir(&dir)
        .arg("--ir-backend")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI with --ir-backend");
    assert!(
        compile.status.success(),
        "elephc --ir-backend failed: stderr={}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(dir.join("main"))
        .current_dir(&dir)
        .output()
        .expect("failed to run IR backend binary");
    assert!(run.status.success(), "IR backend binary failed");
    let stdout = String::from_utf8(run.stdout).unwrap();

    let _ = fs::remove_dir_all(&dir);
    stdout
}

/// Returns a coarse unique suffix for temporary test directories.
fn unique_test_id() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos()
}
