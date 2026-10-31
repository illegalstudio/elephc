//! Purpose:
//! End-to-end coverage for the fused `(int)sqrt(int + int)` reducer.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - Runtime-seeded values keep the checked add and the sink alive through AST folding.
//! - A negative sum keeps PHP's NAN cast warning and zero; an overflowing sum keeps the
//!   checked-add float promotion; float addends keep the boxed path.

use crate::support::*;

/// Fused integer square roots match PHP and stay heap-clean.
#[test]
fn sqrt_add_int_sink_matches_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$base = $argc - 6;
echo (int)sqrt($base + 1), '|';
$big = PHP_INT_MAX - $argc + 1;
echo (int)sqrt($big + $big), '|';
$p = $argc + 9;
echo (int)sqrt($p + 3.5), '|';
$i = $argc + 4;
echo (int)sqrt($i + 1), '|';
$f = 2.25;
echo (int)sqrt($f + 1), '|';
$n = 0;
$iters = $argc + 63;
for ($k = 0; $k < $iters; $k++) { $n ^= (int)sqrt($k + 1); }
echo $n, "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "0|4294967296|3|2|1|8\n");
    assert!(
        out.stderr.contains("The float NAN is not representable as an int"),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Disabling EIR optimization leaves the boxed checked-add/sqrt graph and matches PHP.
#[test]
fn sqrt_add_int_sink_no_ir_opt_parity() {
    without_ir_opt(|| {
        let out = compile_and_run(
            r#"<?php
$big = PHP_INT_MAX - $argc + 1;
echo (int)sqrt($big + $big), '|';
$i = $argc + 4;
echo (int)sqrt($i + 2), "\n";
"#,
        );
        assert_eq!(out, "4294967296|2\n");
    });
}
