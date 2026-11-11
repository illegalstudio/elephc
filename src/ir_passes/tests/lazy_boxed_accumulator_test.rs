//! Purpose:
//! Checks which mixed locals the lazy boxing pass gives a raw integer companion.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The fixtures use runtime inputs so the arithmetic is not folded before EIR.
//! - A local read on every iteration gains nothing and must stay untouched.

use std::path::Path;

use crate::ir::{Function, Op};

/// Lowers and optimizes one fixture, returning the named user function.
fn optimized(source: &str, name: &str) -> Function {
    optimized_for_target(source, name, "linux-x86_64")
}

/// Lowers and optimizes one named function for the requested supported target.
fn optimized_for_target(source: &str, name: &str, target: &str) -> Function {
    let mut module = crate::ir_lower::tests::lower_source_at_for_target(
        source,
        Path::new("main.php"),
        Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    );
    crate::ir_passes::optimize_module(&mut module);
    module
        .functions
        .into_iter()
        .find(|function| function.name == name)
        .unwrap()
}

/// Returns true when the pass added a raw companion local to the function.
fn has_companion(function: &Function) -> bool {
    function
        .locals
        .iter()
        .any(|local| local.name.as_deref().is_some_and(|name| name.starts_with("lazyint")))
}

/// A `foreach` sum gets a companion, an overflow test, and keeps the mixed arithmetic.
#[test]
fn lazy_accumulator_foreach_sum_gets_raw_companion() {
    let function = optimized(
        r#"<?php
function total(array $a): mixed {
    $sum = 0;
    foreach ($a as $v) {
        $sum += $v;
    }
    return $sum;
}
echo total([1, 2, $argc]);
"#,
        "total",
    );
    let text = crate::ir::print_function(&function);
    assert!(has_companion(&function), "missing companion:\n{text}");
    assert!(
        function.instructions.iter().any(|inst| inst.op == Op::IOverflows),
        "{text}"
    );
    assert!(
        function.instructions.iter().any(|inst| inst.op == Op::MixedNumericBinop),
        "mixed fallback missing:\n{text}"
    );
}

/// A local concatenated on every iteration would be boxed again each time, so it stays mixed.
#[test]
fn lazy_accumulator_local_read_every_iteration_stays_mixed() {
    let function = optimized(
        r#"<?php
function trail(array $a): mixed {
    $sum = 0;
    $out = "";
    foreach ($a as $v) {
        $sum += $v;
        $out .= $sum . ",";
    }
    return $out;
}
echo trail([1, 2, $argc]);
"#,
        "trail",
    );
    let text = crate::ir::print_function(&function);
    assert!(!has_companion(&function), "{text}");
    assert!(
        !function.instructions.iter().any(|inst| inst.op == Op::IOverflows),
        "{text}"
    );
}

/// A compare of the accumulator inside the loop is modeled, so the local is still rewritten.
#[test]
fn lazy_accumulator_compared_in_the_loop_gets_raw_companion() {
    let function = optimized(
        r#"<?php
function capped(array $a): mixed {
    $sum = 0;
    foreach ($a as $v) {
        $sum += $v;
        if ($sum > 100) {
            break;
        }
    }
    return $sum;
}
echo capped([1, 2, $argc]);
"#,
        "capped",
    );
    let text = crate::ir::print_function(&function);
    assert!(has_companion(&function), "missing companion:\n{text}");
    assert!(
        function.instructions.iter().any(|inst| inst.op == Op::IOverflows),
        "{text}"
    );
}

/// Literal `eval` code reads locals by name, so the function is left alone.
#[test]
fn lazy_accumulator_function_with_literal_eval_stays_mixed() {
    let function = optimized(
        r#"<?php
function shown(int $n): void {
    $s = 0;
    for ($i = 0; $i < $n; $i++) {
        $s += 10;
        if ($i == 0) {
            echo "";
        }
    }
    eval('echo $s, "\n";');
}
shown($argc);
"#,
        "shown",
    );
    let text = crate::ir::print_function(&function);
    assert!(!has_companion(&function), "{text}");
}

/// Every supported target keeps proven buffer sums in frame cells while specializing dynamic sums.
#[test]
fn lazy_accumulator_frame_numeric_locals_keep_cells_on_every_target() {
    let source = r#"<?php
function bufferSum(buffer<int> $entries, int $n): int {
    $sum = 0;
    for ($i = 0; $i < $n; $i++) { $sum += $entries[$i]; }
    return $sum;
}
function dynamicSum(array $entries): mixed {
    $sum = 0;
    foreach ($entries as $entry) { $sum += $entry; }
    return $sum;
}
$entries = buffer_new<int>($argc);
echo bufferSum($entries, $argc), dynamicSum([1, $argc]);
buffer_free($entries);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let function = optimized_for_target(source, "bufferSum", target);
        let text = crate::ir::print_function(&function);
        let sum = function.locals.iter().find(|local| local.name.as_deref() == Some("sum")).unwrap();
        assert!(!has_companion(&function), "{target}: unexpected lazy rewrite:\n{text}");
        assert!(crate::codegen::stack_numeric::FrameCells::of(&function).contains_local(sum.id),
            "{target}: sum lost its frame cell:\n{text}");
        let dynamic = optimized_for_target(source, "dynamicSum", target);
        assert!(has_companion(&dynamic), "{target}: dynamic sum was not specialized");
    }
}
