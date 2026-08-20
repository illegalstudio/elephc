//! Purpose:
//! End-to-end regression coverage for issue #623 checked integer sink specialization.
//!
//! Called from:
//! - `cargo test --test codegen_tests optimizer::checked_int_sink`.
//!
//! Key details:
//! - CLI `--emit-ir` comparisons pin CSE and LICM structure with optimization on/off.
//! - Runtime fixtures pin exact overflow casts, Mixed-observer preservation, and heap cleanup.

use super::*;

/// Emits optimized or unoptimized EIR and returns only the `main` function body.
fn emit_main_ir(source: &str, extra_args: &[&str]) -> String {
    let dir = make_cli_test_dir("elephc_issue_623_ir");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write issue-623 EIR fixture");
    let mut command = elephc_cli_command(&dir);
    command.arg("--emit-ir").args(extra_args).arg(&php_path);
    let output = command.output().expect("run elephc --emit-ir");
    assert!(
        output.status.success(),
        "emit-ir failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).expect("EIR is UTF-8");
    let main = text
        .split("function main(")
        .nth(1)
        .expect("EIR contains main")
        .split("\n  function ")
        .next()
        .expect("main body")
        .to_string();
    let _ = fs::remove_dir_all(dir);
    main
}

/// Compiles and runs one CLI fixture with the requested optimizer flags.
fn compile_and_run_cli_variant(source: &str, extra_args: &[&str]) -> String {
    let dir = make_cli_test_dir("elephc_issue_623_run");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write issue-623 runtime fixture");
    let mut command = elephc_cli_command(&dir);
    command.args(extra_args).arg(&php_path);
    let compile = command.output().expect("compile issue-623 fixture");
    assert!(
        compile.status.success(),
        "compile failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = run_binary(&dir.join("main"), &dir);
    assert!(
        run.status.success(),
        "fixture failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8(run.stdout).expect("program stdout is UTF-8");
    let _ = fs::remove_dir_all(dir);
    stdout
}

/// Repeated immutable buffer indices collapse from three checked boxes to one scalar value.
#[test]
fn test_issue_623_cse_collapses_integer_only_index_arithmetic() {
    let source = r#"<?php
buffer<int> $values = buffer_new<int>(16);
int $index = $argc;
int $offset = 2;
$values[$index + $offset] = $values[$index + $offset] + 1;
echo $values[$index + $offset];
buffer_free($values);
"#;
    let unoptimized = emit_main_ir(source, &["--no-ir-opt"]);
    let optimized = emit_main_ir(source, &[]);

    assert_eq!(
        unoptimized.matches("= ichecked_add ").count(),
        4,
        "three indices plus the loaded-value increment start boxed"
    );
    assert_eq!(optimized.matches("= ichecked_add ").count(), 0);
    assert_eq!(
        optimized.matches("= ichecked_add_to_int ").count(),
        2,
        "CSE shares one index expression while the value increment remains distinct"
    );
    assert_eq!(compile_and_run(source), "1");
}

/// An invariant `$argc + 2` index and its constant operand move into the loop preheader.
#[test]
fn test_issue_623_licm_hoists_integer_only_index_arithmetic() {
    let source = r#"<?php
buffer<int> $values = buffer_new<int>(16);
$values[3] = 7;
int $i = 0;
while ($i < 3) {
    echo $values[$argc + 2];
    $i++;
}
buffer_free($values);
"#;
    let unoptimized = emit_main_ir(source, &["--no-ir-opt"]);
    let optimized = emit_main_ir(source, &[]);
    let preheader = optimized.split("while.cond:").next().expect("loop preheader");
    let body = optimized
        .split("while.body:")
        .nth(1)
        .expect("loop body")
        .split("while.exit:")
        .next()
        .expect("body before exit");

    assert!(unoptimized.contains("while.body:"));
    assert!(unoptimized.contains("= ichecked_add "));
    assert!(preheader.contains("= ichecked_add_to_int "));
    assert!(preheader.contains("origin: licm"));
    // The loop counter's own `$i++` is scalar as well, so an `ichecked_add_to_int` in the
    // body is expected and says nothing about hoisting. What must not be there is the
    // invariant index computation, which LICM stamps with its own origin when it moves it.
    assert!(!body.contains("origin: licm"));
    assert_eq!(compile_and_run(source), "777");
}

/// Overflow follows PHP's double promotion and exact float-to-int cast with optimization on/off.
#[test]
fn test_issue_623_overflow_cast_matches_unoptimized_semantics() {
    let source = r#"<?php
int $one = $argc;
echo (int) (PHP_INT_MAX + $one), "|";
echo (int) (PHP_INT_MIN - $one), "|";
int $three = $argc + 2;
echo (int) (PHP_INT_MAX * $three);
"#;
    let expected = "-9223372036854775808|-9223372036854775808|-9223372036854775808";
    assert_eq!(compile_and_run_cli_variant(source, &["--no-ir-opt"]), expected);
    assert_eq!(compile_and_run_cli_variant(source, &[]), expected);
}

/// A direct Mixed observer keeps the boxed checked operation and its float result on overflow.
#[test]
fn test_issue_623_mixed_observer_remains_boxed() {
    let source = "<?php int $one = $argc; echo PHP_INT_MAX + $one;";
    let optimized = emit_main_ir(source, &[]);
    assert!(optimized.contains("= ichecked_add "));
    assert!(!optimized.contains("= ichecked_add_to_int "));
    assert_eq!(
        compile_and_run_cli_variant(source, &[]),
        compile_and_run_cli_variant(source, &["--no-ir-opt"])
    );
}

/// The optimized loop eliminates transient index boxes without upsetting heap ownership.
#[test]
fn test_issue_623_integer_sink_loop_is_heap_debug_clean() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
buffer<int> $values = buffer_new<int>(16);
$values[3] = 7;
int $i = 0;
while ($i < 20) {
    echo $values[$argc + 2];
    $i++;
}
buffer_free($values);
"#,
    );
    assert!(output.success, "program failed: {}", output.stderr);
    assert_eq!(output.stdout, "7".repeat(20));
    assert!(
        output.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        output.stderr
    );
}
