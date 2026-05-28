//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of runtime GC growth, including example copy-on-write compiles and runs, literal spread grows past initial capacity, and literal spread refcounted grows past initial capacity.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Uses checked-in example PHP fixtures through include_str! in addition to inline native-output assertions.

use crate::support::*;

/// Compiles and runs the cow example PHP, asserting stdout matches expected output.
/// Regression: ensures copy-on-write array behavior produces correct distinct values
/// when arrays share backing store then are written to.
#[test]
fn test_example_cow_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/cow/main.php"));
    assert_eq!(
        out,
        "left: 1 2 3 \nright: 99 2 3 4 \nouterA inner: 10 20 \nouterB inner: 10 77 \n"
    );
}

/// Compiles and runs inline PHP that uses spread operators with range() to fill
/// a 30-element array, asserting count and a specific index value.
/// Edge case: spread grows past the initial small-array capacity (no pre-allocated size hint).
#[test]
fn test_array_literal_spread_grows_past_initial_capacity() {
    let out = compile_and_run(
        r#"<?php
$nums = [...range(1, 10), ...range(11, 20), ...range(21, 30)];
echo count($nums) . "|" . $nums[25];
"#,
    );
    assert_eq!(out, "30|26");
}

/// Compiles and runs inline PHP that creates two refcounted arrays ($a, $b) filled
/// with a shared $inner reference, then spreads both into $c three times.
/// Regression: refcounted values must survive multiple spread expansions without
/// use-after-free or incorrect refcount when the destination grows past initial capacity.
#[test]
fn test_array_literal_spread_refcounted_grows_past_initial_capacity() {
    let out = compile_and_run(
        r#"<?php
$inner = [1];
$a = array_fill(0, 10, $inner);
$b = array_fill(0, 10, $inner);
$c = [...$a, ...$b, ...$a];
echo count($c) . "|" . count($c[25]);
"#,
    );
    assert_eq!(out, "30|1");
}
