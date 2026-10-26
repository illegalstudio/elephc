//! Purpose:
//! Regression coverage for logical expressions consumed directly by EIR branches.
//!
//! Called from:
//! - `crate::ir_lower::tests` through Rust's test harness.
//!
//! Key details:
//! - Inspects lowering before EIR optimization, so mem2reg cannot hide boolean slots.
//! - Value-producing logical expressions still need a materialized boolean result.

use crate::ir::{Function, LocalKind, Op, Terminator};
use crate::types::PhpType;
use crate::codegen::platform::Target;
use std::path::Path;

/// Rejects boolean merge slots and logical value joins in a branch-only function.
fn assert_direct_conditions(function: &Function) {
    assert!(
        !function.locals.iter().any(|local| {
            local.kind == LocalKind::HiddenTemp && local.php_type == PhpType::Bool
        }),
        "{} materialized a branch-only logical result: {:?}", function.name, function.locals
    );
    assert!(
        !function.blocks.iter().any(|block| {
            block.name.starts_with("logical.const") || block.name.starts_with("logical.merge")
        }),
        "{} contains a logical value join", function.name
    );
}

/// Every branch consumer handles nested logical operators without boolean merge slots.
#[test]
fn branch_conditions_do_not_materialize_logical_results() {
    let module = super::lower_source(r#"<?php
function choices(int $a, int $b, int $c): int {
    if ($a < 0 || $b < 0 || $c < 0) { return 1; }
    elseif (!($a > 0 && ($b > 0 || $c > 0))) { return 2; }
    return ($a > 1 && $b > 1) ? 3 : 4;
}
function loops(int $a, int $b): int {
    while ($a > 0 && $b > 0) { $a--; if ($a === 1) continue; $b--; }
    do { $a++; } while ($a < 2 || $b < 0);
    for ($i = 0; $i < $a && !($b < 0 || $i > 3); $i++) { $b++; }
    return $b;
}
echo choices($argc, 2, 3), loops($argc, 3);
"#);
    for name in ["choices", "loops"] {
        let function = module.functions.iter().find(|function| function.name == name).unwrap();
        assert_direct_conditions(function);
        assert!(function.blocks.iter().filter(|block| {
            matches!(block.terminator, Some(Terminator::CondBr { .. }))
        }).count() >= 6, "missing operand branches in {name}");
    }
}

/// The collision-grid rejection chain branches once per float comparison without bool stores.
#[test]
fn branch_conditions_collision_chain_has_no_boolean_stores() {
    let source = r#"<?php
function overlaps(float $ax, float $ay, float $bx, float $by): int {
    if ($ax >= $bx || $bx < $ax || $ay >= $by || $by < $ay) { return 0; }
    return 1;
}
echo overlaps((float)$argc, 2.0, 3.0, 4.0);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let mut module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "overlaps").unwrap();
        assert_direct_conditions(function);
        assert_eq!(function.instructions.iter().filter(|inst| inst.op == Op::FCmp).count(), 4);
        assert!(!function.instructions.iter().any(|inst| {
            matches!(inst.op, Op::StoreLocal | Op::ConstBool)
        }), "{target}: collision chain must not store or synthesize booleans");
        for optimized in [false, true] {
            if optimized { crate::ir_passes::optimize_module(&mut module); }
            crate::ir::validate_module(&module).expect("valid condition CFG");
            crate::codegen::generate_user_asm_from_ir(&module, false, false)
                .unwrap_or_else(|error| panic!("{target}, optimized={optimized}: {error:?}"));
        }
    }
}

/// Uniform and mixed logical trees keep every operand continuation in forward block-table order.
#[test]
fn branch_conditions_keep_chain_blocks_in_source_order() {
    let module = super::lower_source(r#"<?php
function or_left(bool $a, bool $b, bool $c, bool $d): int {
    if ($a || $b || $c || $d) return 1;
    return 0;
}
function or_right(bool $a, bool $b, bool $c, bool $d): int {
    if ($a || ($b || ($c || $d))) return 1;
    return 0;
}
function and_left(bool $a, bool $b, bool $c, bool $d): int {
    if ($a && $b && $c && $d) return 1;
    return 0;
}
function and_right(bool $a, bool $b, bool $c, bool $d): int {
    if ($a && ($b && ($c && $d))) return 1;
    return 0;
}
function mixed_or_left(bool $a, bool $b, bool $c): int {
    if (($a && $b) || $c) return 1;
    return 0;
}
function mixed_or_middle(bool $a, bool $b, bool $c, bool $d): int {
    if ($a || ($b && $c) || $d) return 1;
    return 0;
}
function mixed_negated(bool $a, bool $b, bool $c): int {
    if (!($a && $b) || $c) return 1;
    return 0;
}
function mixed_and_left(bool $a, bool $b, bool $c): int {
    if (($a || $b) && $c) return 1;
    return 0;
}
$a = $argc > 0;
echo or_left($a, false, true, false), or_right($a, false, true, false);
echo and_left($a, false, true, false), and_right($a, false, true, false);
echo mixed_or_left($a, false, true), mixed_or_middle($a, false, true, false);
echo mixed_negated($a, false, true), mixed_and_left($a, false, true);
"#);
    for (name, expected_edges) in [
        ("or_left", 3), ("or_right", 3), ("and_left", 3), ("and_right", 3),
        ("mixed_or_left", 3), ("mixed_or_middle", 4), ("mixed_negated", 2),
        ("mixed_and_left", 3),
    ] {
        let function = module.functions.iter().find(|function| function.name == name).unwrap();
        assert_direct_conditions(function);
        let mut continuations = 0;
        for block in &function.blocks {
            let Some(Terminator::CondBr { then_target, else_target, .. }) = &block.terminator
            else { continue; };
            for next in [*then_target, *else_target] {
                if function.block(next).unwrap().name.starts_with("condition.") {
                    assert!(next.as_raw() > block.id.as_raw(),
                        "{name}: operand continuation goes backward: {block:?}");
                    continuations += 1;
                }
            }
        }
        assert_eq!(continuations, expected_edges, "{name}: operand continuation edges");
    }
}

/// Returning a logical value still produces a materialized boolean result.
#[test]
fn branch_conditions_keep_value_producing_logical_expressions() {
    let module = super::lower_source(r#"<?php
function logical_value(int $a, int $b): bool { return $a || $b; }
echo logical_value($argc, 0);
"#);
    let function = module.functions.iter().find(|function| function.name == "logical_value").unwrap();
    assert!(function.locals.iter().any(|local| {
        local.kind == LocalKind::HiddenTemp && local.php_type == PhpType::Bool
    }));
}
