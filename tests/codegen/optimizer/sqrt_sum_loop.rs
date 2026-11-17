//! Purpose:
//! End-to-end coverage for the scalar `sqrt($i + c)` range reducer.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - Runtime bounds keep the range reducer alive through AST folding.
//! - Negative square roots stay NAN, an empty range keeps the initial accumulator, and
//!   `--ir-opt=off` keeps the original boxed loop with the same stdout.

use crate::support::*;

/// The range reducer matches PHP for runtime bounds, negative roots and empty ranges.
#[test]
fn sqrt_sum_loop_matches_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$iters = (int)($argc + 7);
$sum = 0.0;
for ($i = 0; $i < $iters; $i++) { $sum += sqrt($i + 1); }
echo $sum, "|";
$sum2 = 0.0;
for ($i = 0; $i < (int)$argc; $i++) { $sum2 += sqrt($i + 4); }
echo $sum2, "|";
$sum3 = 0.0;
for ($i = -3; $i < 2; $i++) { $sum3 += sqrt($i + 1); }
echo $sum3, "|";
$sum4 = 1.5;
for ($i = 0; $i < 0; $i++) { $sum4 += sqrt($i + 1); }
echo $sum4, "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "16.306000526036|2|NAN|1.5\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Optimized EIR selects the range reducer instead of the per-iteration sqrt call.
#[test]
fn sqrt_sum_loop_emits_the_range_reducer() {
    let dir = make_cli_test_dir("elephc_sqrt_sum_loop_ir");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
$iters = (int)($argc + 7);
$sum = 0.0;
for ($i = 0; $i < $iters; $i++) { $sum += sqrt($i + 1); }
echo $sum;
"#,
    )
    .expect("write sqrt range fixture");
    let output = elephc_cli_command(&dir)
        .arg("--emit-ir")
        .arg("--ir-opt=on")
        .arg(&php_path)
        .output()
        .expect("run elephc --emit-ir");
    assert!(
        output.status.success(),
        "emit-ir failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).expect("EIR is UTF-8");
    assert!(text.contains("runtime.sqrt_sum_range"), "{text}");
}

/// Disabling EIR optimization leaves the boxed loop and matches the reducer's output.
#[test]
fn sqrt_sum_loop_no_ir_opt_parity() {
    without_ir_opt(|| {
        let out = compile_and_run(
            r#"<?php
$iters = (int)($argc + 7);
$sum = 0.0;
for ($i = 0; $i < $iters; $i++) { $sum += sqrt($i + 1); }
echo $sum, "|";
$sum2 = 1.5;
for ($i = 0; $i < 0; $i++) { $sum2 += sqrt($i + 1); }
echo $sum2, "\n";
"#,
        );
        assert_eq!(out, "16.306000526036|1.5\n");
    });
}
