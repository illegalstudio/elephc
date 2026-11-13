//! Purpose:
//! Checks that overflow-promotable sum and counter loops gain a raw integer copy.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The fixtures use runtime bounds so the arithmetic is not folded before EIR.
//! - A call inside the body must keep the original mixed loop.

use std::path::Path;

use crate::ir::{Function, Op};

/// Lowers and optimizes one fixture, returning the named user function.
fn optimized(source: &str, name: &str) -> Function {
    let mut module = crate::ir_lower::tests::lower_source_at_for_target(
        source,
        Path::new("main.php"),
        Path::new("."),
        crate::codegen::platform::Target::parse("linux-x86_64").unwrap(),
    );
    crate::ir_passes::optimize_module(&mut module);
    module
        .functions
        .into_iter()
        .find(|function| function.name == name)
        .unwrap()
}

/// A plain `while` sum keeps a raw add/overflow path and the mixed fallback.
#[test]
fn raw_promotable_sum_loop_keeps_integer_and_mixed_paths() {
    let function = optimized(
        r#"<?php
function total(int $n): mixed {
    $sum = 0;
    $i = 1;
    while ($i <= $n) {
        $sum += $i;
        $i += 1;
    }
    return $sum;
}
echo total($argc);
"#,
        "total",
    );
    let text = crate::ir::print_function(&function);
    assert!(
        text.contains("rawint.fast.header"),
        "missing raw loop:\n{text}"
    );
    assert!(
        function.instructions.iter().any(|inst| inst.op == Op::IAdd),
        "{text}"
    );
    assert!(
        function.instructions.iter().any(|inst| inst.op == Op::IOverflows),
        "{text}"
    );
    assert!(
        function.instructions.iter().any(|inst| inst.op == Op::FAdd),
        "overflow continuation is not a raw float add:\n{text}"
    );
    assert!(
        text.contains("rawint.float.header"),
        "missing float continuation:\n{text}"
    );
    assert!(
        function.instructions.iter().any(|inst| inst.op == Op::MixedNumericBinop),
        "mixed fallback missing:\n{text}"
    );
    assert!(text.contains("rawint.slow"), "{text}");
}

/// A call in the body is observable, so both locals stay on the mixed loop.
#[test]
fn raw_promotable_loop_with_a_call_stays_mixed() {
    let function = optimized(
        r#"<?php
function noisy(int $n): mixed {
    $sum = 0;
    $i = 1;
    while ($i <= $n) {
        $sum += $i;
        echo $i;
        $i += 1;
    }
    return $sum;
}
echo noisy($argc);
"#,
        "noisy",
    );
    let text = crate::ir::print_function(&function);
    assert!(!text.contains("rawint."), "{text}");
    assert!(
        function.instructions.iter().any(|inst| inst.op == Op::MixedNumericBinop),
        "{text}"
    );
}

/// A `for` loop whose counter is already a raw integer still gets the raw accumulator copy.
#[test]
fn raw_promotable_for_loop_with_integer_counter_gets_raw_copy() {
    let function = optimized(
        r#"<?php
function total(int $n): mixed {
    $sum = 0;
    for ($i = 0; $i < $n; $i++) {
        $sum += $i * $i;
    }
    return $sum;
}
echo total($argc);
"#,
        "total",
    );
    let text = crate::ir::print_function(&function);
    assert!(text.contains("rawint.fast.header"), "missing raw loop:\n{text}");
    let overflow_tests = function
        .instructions
        .iter()
        .filter(|inst| inst.op == Op::IOverflows)
        .count();
    assert!(overflow_tests >= 2, "product and sum need one overflow test each:\n{text}");
    assert!(text.contains("rawint.slow"), "{text}");
}

/// A counter read after the loop is handed to the exit block as a parameter.
#[test]
fn raw_promotable_counter_read_after_the_loop_gets_raw_copy() {
    let function = optimized(
        r#"<?php
function total(int $n): mixed {
    $sum = 0;
    for ($i = 0; $i < $n; $i++) {
        $sum += $i;
    }
    return $sum + $i;
}
echo total($argc);
"#,
        "total",
    );
    let text = crate::ir::print_function(&function);
    assert!(text.contains("rawint.fast.header"), "missing raw loop:\n{text}");
    assert!(
        function.instructions.iter().any(|inst| inst.op == Op::IOverflows),
        "{text}"
    );
}
