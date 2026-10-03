//! Purpose:
//! Verifies mixed-storage type guards use the guarded statement's physical source mode.
//!
//! Called from:
//! - The codegen CLI suite through its strict-PHP source-profile tests.
//!
//! Key details:
//! - LFC retains extension guards under a strict request.
//! - Calling a PHP body from LFC must not enable hidden guards inside the PHP body.

use crate::support::*;

/// Compiles a small mixed-source project, cleans its files, and returns stdout and diagnostics.
fn run_guard_project(files: &[(&str, &str)], entry: &str, strict_locals: bool) -> (String, String) {
    let dir = make_cli_test_dir("elephc_source_guard");
    for (name, source) in files { fs::write(dir.join(name), source).unwrap(); }
    let mut command = elephc_cli_command(&dir);
    command.arg("--strict-php");
    if strict_locals { command.arg("--strict-locals"); }
    let compile = command.arg(dir.join(entry)).output().unwrap();
    let run = compile.status.success().then(|| run_binary(&dir.join(entry).with_extension(""), &dir));
    fs::remove_dir_all(&dir).unwrap();
    let diagnostics = String::from_utf8_lossy(&compile.stderr).into_owned();
    assert!(compile.status.success(), "{diagnostics}");
    let run = run.unwrap();
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    (String::from_utf8(run.stdout).unwrap(), diagnostics)
}

/// A physical LFC guard narrows without spurious mixed-storage advice in either local mode.
#[test]
fn test_strict_php_source_guards_lfc_narrow_without_mixed_warning() {
    let source = r#"$value = "s";
if (is_real($value)) { $value = 1.5; }
echo $value;"#;
    for strict_locals in [false, true] {
        let (stdout, diagnostics) = run_guard_project(&[("main.lfc", source)], "main.lfc", strict_locals);
        assert_eq!(stdout, "s");
        assert!(!diagnostics.contains("boxed mixed storage"), "{diagnostics}");
    }
}

/// PHP bodies keep a user function's non-guard semantics even when first resolved from LFC.
#[test]
fn test_strict_php_source_guards_php_body_is_independent_of_lfc_caller() {
    let source = r#"<?php
function is_real(mixed $input): bool { return false; }
function php_body(): void {
    $value = "s";
    if (is_real($value)) { $value = 1.5; }
    echo $value;
}
require "demo.lfc";
"#;
    for call in ["php_body();", "demo();"] {
        let entry = format!("{source}\n{call}");
        let (stdout, diagnostics) = run_guard_project(
            &[("main.php", &entry), ("demo.lfc", "function demo(): void { php_body(); }")],
            "main.php",
            false,
        );
        assert_eq!(stdout, "s");
        assert!(diagnostics.contains("boxed mixed storage"), "{diagnostics}");
    }
}
