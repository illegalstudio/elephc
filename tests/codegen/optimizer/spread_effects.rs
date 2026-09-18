//! Purpose:
//! Verifies discarded array spreads keep their validation and exception boundaries.
//!
//! Called from:
//! - The optimizer codegen integration suite.
//!
//! Key details:
//! - Runtime-dependent inputs exercise direct literals and summarized function calls.
//! - CI runs the fixture with EIR optimization enabled and disabled.

use super::*;

/// Runs the spread-effects fixture in one optimizer mode and reports native stdout.
fn run_spread_effect_fixture(source: &str, ir_opt: bool) -> String {
    let dir = make_cli_test_dir("elephc_spread_effects");
    let path = dir.join("main.php");
    fs::write(&path, source).expect("failed to write spread effects fixture");
    let mode = if ir_opt { "--ir-opt=on" } else { "--ir-opt=off" };
    let compile = elephc_cli_command(&dir).arg(mode).arg(&path).output()
        .expect("failed to compile spread effects fixture");
    assert!(compile.status.success(), "ir_opt={ir_opt}: {}", String::from_utf8_lossy(&compile.stderr));
    let output = Command::new(dir.join("main")).output().expect("failed to run spread effects fixture");
    assert!(output.status.success(), "ir_opt={ir_opt}: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    let _ = fs::remove_dir_all(&dir);
    stdout
}

/// Ignoring a spread result cannot remove a dynamic error, its catch, or its finally block.
#[test]
fn test_discarded_array_spreads_preserve_exceptions_and_catch_routing() {
    let source = r#"<?php
function discardSpreadExpression(mixed $items): void {
    try { [...$items]; }
    catch (Error $error) { echo "expression|"; }
    finally { echo "finally|"; }
}
function returnSpreadExpression(mixed $items): array { return [...$items]; }
discardSpreadExpression($argc);
try { returnSpreadExpression($argc); }
catch (Error $error) { echo "call|"; }
finally { echo "finally|"; }
discardSpreadExpression(["key" => $argc]);
returnSpreadExpression([$argc]);
echo "done";
"#;
    for ir_opt in [true, false] {
        assert_eq!(run_spread_effect_fixture(source, ir_opt),
            "expression|finally|call|finally|finally|done", "ir_opt={ir_opt}");
    }
}
