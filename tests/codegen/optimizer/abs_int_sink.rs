//! Purpose:
//! End-to-end coverage for the allocation-free `abs()` int sink.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - Small runtime values exercise the raw scalar path; `INT_MIN` exercises PHP's float
//!   promotion plus the warned float-to-int cast; a float operand keeps the boxed result.

use crate::support::*;

/// Cast-observed integer abs values match PHP and stay heap-clean.
#[test]
fn abs_int_sink_matches_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [1, -2, 3, -4];
$n = 0;
for ($i = 0; $i < 2001; $i++) { $n ^= (int)abs($a[$i & 3]); }
echo $n, '|';
$m = PHP_INT_MIN;
echo (int)abs($m), '|';
$f = -1.5;
echo abs($f), '|';
$v = -7;
echo abs($v), "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "1|-9223372036854775808|1.5|7\n");
    assert!(
        out.stderr
            .contains("The float 9.223372036854776E+18 is not representable as an int"),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Disabling EIR optimization leaves the boxed call and produces the same output.
#[test]
fn abs_int_sink_no_ir_opt_parity() {
    without_ir_opt(|| {
        let out = compile_and_run(
            r#"<?php
$v = -7;
$a = [1, -2, 3, -4];
echo (int)abs($v), '|', (int)abs($a[1]), "\n";
"#,
        );
        assert_eq!(out, "7|2\n");
    });
}
