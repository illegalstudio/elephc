//! Purpose:
//! Regressions for checker exception states at hidden switch comparisons.
//!
//! Called from:
//! - The codegen integration harness and the CLI's check mode.
//!
//! Key details:
//! - A throwing warning joins catch bindings before subsequent typed argument checks.
//! - Label writes precede that edge; arm writes follow it.

use super::*;

const CHECKER_SWITCH_PREFIX: &str = r#"<?php
function needSwitchInt(int $value): void { var_dump($value); }
function guardedSwitchCatch(int $e, object $subject): void {
    try { switch ($subject) { case 1: break; default: break; } }
    catch (Exception $e) { echo "caught|"; }
    if ($e instanceof Exception) { echo "object|"; }
    else { needSwitchInt($e); }
}
function switchLabelWrite(?int $value, object $subject): void {
    try { switch ($subject) { case ($value ??= 7): break; default: break; } }
    catch (Exception $caught) { echo "prefix|"; }
    needSwitchInt($value);
}
function terminatingSwitchCatch(int $e, object $subject): void {
    try { switch ($subject) { case 1: break; default: break; } }
    catch (Exception $e) { echo "return|"; return; }
    needSwitchInt($e);
}
set_error_handler(function(int $level, string $message): bool { throw new Exception($message); });
guardedSwitchCatch(1, new stdClass());
switchLabelWrite(null, new stdClass());
terminatingSwitchCatch(1, new stdClass());
restore_error_handler();
"#;

/// A catch replacement and a pre-arm nullable value cannot silently reach an int parameter.
#[test]
fn test_loose_comparison_batch_checker_switch_warning_joins() {
    let dir = make_cli_test_dir("elephc_switch_warning_checker_joins");
    let result = std::panic::catch_unwind(|| {
        let switched = "switch ($subject) { case 1: break; default: break; }";
        for comparison in [switched, "if ($subject == 1) {}"] {
            let source = format!(r#"<?php
function needComparisonInt(int $e): void {{}}
function checkCatch(int $e, object $subject): void {{
    try {{ {comparison} }} catch (Exception $e) {{}}
    needComparisonInt($e);
}}
checkCatch(1, new stdClass());
"#);
            fs::write(dir.join("main.php"), source).unwrap();
            let checked = elephc_cli_command(&dir).args(["--quiet", "--check", "main.php"]).output().unwrap();
            let error = String::from_utf8_lossy(&checked.stderr);
            assert!(!checked.status.success(), "unsafe catch binding was accepted: {comparison}");
            assert!(error.contains("needComparisonInt") && error.contains("expects int"), "{error:.2000}");
        }
        fs::write(dir.join("main.php"), r#"<?php
function needComparisonInt(int $value): void {}
function beforeArms(?int $value, object $subject): void {
    try {
        switch ($subject) {
            case 1: $value ??= 1; break;
            default: $value ??= 1;
        }
    } catch (Exception $caught) {}
    needComparisonInt($value);
}
beforeArms(null, new stdClass());
"#).unwrap();
        let checked = elephc_cli_command(&dir).args(["--quiet", "--check", "main.php"]).output().unwrap();
        let error = String::from_utf8_lossy(&checked.stderr);
        assert!(!checked.status.success(), "a warning before arm writes was treated as a post-arm throw");
        assert!(error.contains("needComparisonInt") && error.contains("expects int"), "{error:.2000}");
    });
    fs::remove_dir_all(&dir).unwrap();
    result.unwrap();
}

/// Label assignments and terminating catches retain their precise continuing views and owners.
#[test]
fn test_loose_comparison_batch_checker_switch_prefix_flow() {
    let output = compile_and_run_with_heap_debug(CHECKER_SWITCH_PREFIX);
    assert!(output.success, "{}: {:.2000}", output.status, output.stderr);
    assert!(output.stdout.len() < 8192, "unexpected stdout length {}", output.stdout.len());
    assert_eq!(output.stdout, "caught|object|prefix|int(7)\nreturn|");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{:.2000}", output.stderr);
}
