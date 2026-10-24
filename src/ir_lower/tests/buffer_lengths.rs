//! Purpose:
//! Regression coverage for the EIR lowering of `buffer_new<T>()` lengths.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - An `int` length reaches `buffer_new` directly, with no conversion.
//! - A `mixed` length from checked int arithmetic dispatches on its runtime tag (int, float,
//!   other), checks a non-int payload for a whole-number round trip, and reaches `buffer_new` as
//!   the parameter of the block that selects the count.

use crate::ir::{Function, Module, Op, ValueDef};

/// Returns the lowered function named `name`, panicking with the name when it is missing.
fn function<'a>(module: &'a Module, name: &str) -> &'a Function {
    module
        .functions
        .iter()
        .find(|function| function.name == name)
        .unwrap_or_else(|| panic!("missing lowered function {name}"))
}

/// Returns the definition of the length operand of the single `buffer_new` in `function`.
fn length_definition(function: &Function) -> ValueDef {
    let allocations: Vec<_> = function
        .instructions
        .iter()
        .filter(|instruction| instruction.op == Op::BufferNew)
        .collect();
    assert_eq!(allocations.len(), 1, "expected one buffer_new in {}", function.name);
    let length = allocations[0].operands[0];
    function.value(length).expect("buffer_new length value").def
}

/// Returns whether `function` contains an instruction with opcode `op`.
fn has_op(function: &Function, op: Op) -> bool {
    function.instructions.iter().any(|instruction| instruction.op == op)
}

/// Regression for task f9ea1d1e: `$n + 1` infers `mixed`, so its length goes through the
/// whole-number conversion, while a plain `int` length keeps the direct operand.
#[test]
fn buffer_new_converts_only_non_int_lengths() {
    let module = super::lower_source(
        r#"<?php
function direct(int $n): int { return buffer_len(buffer_new<int>($n)); }
function computed(int $n): int { return buffer_len(buffer_new<int>($n + 1)); }
echo direct(2), computed(2);
"#,
    );

    let direct = function(&module, "direct");
    assert!(
        matches!(length_definition(direct), ValueDef::Instruction { .. }),
        "an int length must reach buffer_new directly"
    );
    for op in [Op::TypePredicate, Op::Cast, Op::IToF, Op::FCmp] {
        assert!(!has_op(direct, op), "an int length must not be converted, found {op:?}");
    }

    let computed = function(&module, "computed");
    assert!(
        matches!(length_definition(computed), ValueDef::BlockParam { .. }),
        "a mixed length must reach buffer_new through the whole-number selection"
    );
    for op in [Op::TypePredicate, Op::Cast, Op::FToI, Op::IToF, Op::FCmp, Op::ICmp] {
        assert!(has_op(computed, op), "the mixed length conversion is missing {op:?}");
    }
}
