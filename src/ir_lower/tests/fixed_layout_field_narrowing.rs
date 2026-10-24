//! Purpose:
//! Regression coverage for the strict narrowing a boxed value gets before it is stored into a
//! fixed-layout scalar field.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Float arithmetic over a loop counter is `float` to the checker but stays boxed in EIR, and a
//!   `mixed` call result is boxed by definition; the packed store must receive the narrowed raw
//!   payload, never the box.

use crate::ir::{IrType, Op};
use crate::types::PhpType;

/// Verifies boxed values bound for packed `float` and `bool` fields pass through their strict
/// narrowing op, and that the field store receives the op's raw result.
#[test]
fn boxed_values_reach_packed_scalar_fields_through_strict_narrowing() {
    let module = super::lower_source(
        r#"<?php
packed class Vec2 { public float $x; public bool $live; }
function load(mixed $v) { return $v; }
$pts = buffer_new<Vec2>(3);
for ($i = 0; $i < 3; $i++) { $pts[$i]->x = 1.5 * ($i + 1); }
$pts[0]->live = load(true);
"#,
    );
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main function");
    for (op, ir_type, php_type) in [
        (Op::PackedFieldMixedToFloat, IrType::F64, PhpType::Float),
        (Op::PackedFieldMixedToBool, IrType::I64, PhpType::Bool),
    ] {
        let narrowing = main
            .instructions
            .iter()
            .find(|instruction| instruction.op == op)
            .unwrap_or_else(|| panic!("{op:?} is missing"));
        assert_eq!(narrowing.result_type, ir_type, "{op:?}");
        assert_eq!(narrowing.result_php_type, php_type, "{op:?}");
        let narrowed = narrowing.result.expect("narrowing result");
        assert!(
            main.instructions.iter().any(|instruction| instruction.op == Op::PropSet
                && instruction.operands.get(1) == Some(&narrowed)),
            "the {op:?} result is not the stored value"
        );
    }
}
