//! Purpose:
//! Checks the foreign-callable capability guard at the native eval boundary.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval_escape_guard`.
//!
//! Key details:
//! - Dynamic fragments exercise Magician with both optimizer settings.
//! - Detached foreign handlers still require transitive escape validation.

use crate::support::*;

/// Compiles a dynamic eval fixture and captures its native boundary behavior.
fn run_eval_fixture(source: &str, optimized: bool) -> std::process::Output {
    let dir = make_cli_test_dir("elephc_eval_escape_guard");
    let php = dir.join("main.php");
    fs::write(&php, source).expect("write eval escape fixture");
    let mode = if optimized { "--ir-opt=on" } else { "--ir-opt=off" };
    let compiled = elephc_cli_command(&dir).args([mode, "--heap-debug"])
        .arg(&php).output().expect("compile eval escape fixture");
    if !compiled.status.success() {
        fs::remove_dir_all(&dir).expect("remove failed eval fixture");
        panic!("eval compile failed: {}", String::from_utf8_lossy(&compiled.stderr));
    }
    let output = run_binary(&dir.join("main"), &dir);
    fs::remove_dir_all(dir).expect("remove eval fixture");
    output
}

/// Skipping foreign-callable scans preserves nested eval and partial writes before a throw.
#[test]
fn test_eval_escape_guard_preserves_nested_scope_writes_and_throw() {
    let source = r#"<?php
function updateThroughEval(string $code): void {
    $sum = 0;
    $label = "before";
    try { eval($code); }
    catch (RuntimeException $error) { echo $error->getMessage(), '|'; }
    echo $sum, '|', $label;
}
updateThroughEval('$sum += 3; eval("\$sum += 6;"); $label = "after"; throw new RuntimeException("caught");');
"#;
    for optimized in [false, true] {
        let output = run_eval_fixture(source, optimized);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{stderr}");
        assert_eq!(output.stdout, b"caught|9|after");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    }
    if std::env::var_os("ELEPHC_PHP_CHECK").is_some() {
        let php = Command::new("php").args(["-n", "-r", &source[6..]])
            .output().expect("run PHP eval oracle");
        assert!(php.status.success(), "{}", String::from_utf8_lossy(&php.stderr));
        assert_eq!(php.stdout, b"caught|9|after");
    }
}

/// A cyclic object returned by nested eval cannot hide an imported detached handler.
#[test]
fn test_eval_escape_guard_rejects_foreign_handler_in_cyclic_nested_result() {
    let source = r#"<?php
function registerForeignHandler(): void {
    eval('pcntl_signal(SIGUSR1, function(): void {});');
}
registerForeignHandler();
$code = $argc > 0 ? '
    $handler = pcntl_signal_get_handler(SIGUSR1);
    pcntl_signal(SIGUSR1, SIG_DFL);
    $node = new stdClass();
    $node->next = $node;
    $node->handler = $handler;
    return eval("return [\$node];");
' : '';
echo "before|";
$result = eval($code);
echo "after";
"#;
    for optimized in [false, true] {
        let output = run_eval_fixture(source, optimized);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "foreign handler escaped");
        assert_eq!(output.stdout, b"before|");
        assert!(stderr.contains("PCNTL handler/closure cannot escape its eval context"), "{stderr}");
    }
}

/// A thrown outcome still validates foreign callable writes to native-visible scope.
#[test]
fn test_eval_escape_guard_rejects_foreign_scope_write_before_throw() {
    let source = r#"<?php
function registerThrowingForeignHandler(): void {
    eval('pcntl_signal(SIGUSR1, function(): void {});');
}
registerThrowingForeignHandler();
$handler = null;
$written = 0;
$code = $argc > 0 ? '
    $written = 7;
    $handler = pcntl_signal_get_handler(SIGUSR1);
    pcntl_signal(SIGUSR1, SIG_DFL);
    throw new RuntimeException("stop");
' : '';
echo "before|";
try { eval($code); }
catch (RuntimeException $error) { echo "after"; }
"#;
    for optimized in [false, true] {
        let output = run_eval_fixture(source, optimized);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "foreign scope write escaped");
        assert_eq!(output.stdout, b"before|");
        assert!(stderr.contains("PCNTL handler/closure cannot escape its eval context"), "{stderr}");
    }
}
