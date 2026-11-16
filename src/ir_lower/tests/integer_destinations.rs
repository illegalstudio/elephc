//! Purpose:
//! Pins the scalar EIR boundary for integer arithmetic destinations.
//!
//! Called from:
//! - The library's AST-to-EIR test harness.
//!
//! Key details:
//! - Unknown Mixed values, union destinations and reference places keep their representations.
//! - Only complete integer-leaf arithmetic trees gain a final scalar conversion.

use crate::ir::{Immediate, IrType, Op};
use crate::types::PhpType;

/// Finds a user function before EIR optimization can inline its signature boundary.
fn function<'a>(module: &'a crate::ir::Module, name: &str) -> &'a crate::ir::Function {
    module.functions.iter().find(|function| function.name == name).unwrap()
}

/// Calls receive integer arithmetic as raw scalar values, before operand owners are introduced.
#[test]
fn integer_destinations_calls_receive_scalar_arithmetic() {
    let module = super::lower_source(r#"<?php
function sink(int $value): int { return $value; }
function arithmetic(int $a, int $b): int { return sink(($a * $b) + $a); }
echo arithmetic($argc, 3);
"#);
    let f = function(&module, "arithmetic");
    let call = f.instructions.iter().find(|inst| inst.op == Op::Call).unwrap();
    assert_eq!(f.value(call.operands[0]).unwrap().ir_type, IrType::I64);
    assert!(!f.instructions.iter().any(|inst| inst.op == Op::PushCallOperandOwner));
    assert!(f.instructions.iter().any(|inst| inst.op == Op::Cast
        && inst.immediate == Some(Immediate::CastTarget(IrType::I64))));
}

/// An inferred local and an explicitly Mixed or union destination retain their PHP numeric tags.
#[test]
fn integer_destinations_do_not_narrow_dynamic_values_or_reference_places() {
    let module = super::lower_source(r#"<?php
function sink(int $value): int { return $value; }
function unknown(mixed $value): int { return sink($value + 1); }
function observe(int $value): mixed { $value = $value + 1; return $value; }
function wide(int $value): mixed { int|float $result = $value + 1; return $result; }
function returned(int $value): int { return $value + 1; }
function refsink(int &$value): void { $value = 3; }
function reference(int $value): int { refsink($value); return $value; }
echo unknown($argc), observe($argc), wide($argc), returned($argc), reference($argc);
"#);
    for name in ["unknown", "observe", "wide", "returned", "reference"] {
        let f = function(&module, name);
        assert!(!f.instructions.iter().any(|inst| inst.op == Op::Cast
            && inst.result_php_type == PhpType::Int
            && inst.immediate == Some(Immediate::CastTarget(IrType::I64))), "{name}");
    }
    let f = function(&module, "observe");
    assert!(f.instructions.iter().any(|inst| inst.op == Op::ICheckedAdd));
    let f = function(&module, "returned");
    assert!(f.instructions.iter().any(|inst| inst.op == Op::ReturnBoundaryMixedToInt));
}

/// A new declaration replaces the prior integer contract instead of narrowing later writes.
#[test]
fn integer_destinations_redeclaration_replaces_int_contract() {
    for declaration in ["mixed", "int|float"] {
        let source = format!(r#"<?php
function redeclared(int $value): mixed {{
    int $result = 0;
    {declaration} $result = 0;
    $result = PHP_INT_MAX + $value;
    return $result;
}}
echo gettype(redeclared($argc));
"#);
        let module = super::lower_source(&source);
        let f = function(&module, "redeclared");
        assert!(f.instructions.iter().any(|inst| inst.op == Op::ICheckedAdd));
        assert!(!f.instructions.iter().any(|inst| inst.op == Op::Cast
            && inst.result_php_type == PhpType::Int
            && inst.immediate == Some(Immediate::CastTarget(IrType::I64))), "{declaration}");
    }
}

/// Conditional declarations cannot publish integer facts across joins or loop backedges.
#[test]
fn integer_destinations_conditional_declarations_do_not_narrow_writes() {
    for control in [
        "if ($one > 100) { int $value = 0; }",
        "if ($one < 0) {} elseif ($one > 100) { int $value = 0; }",
        "while ($one > 100) { int $value = 0; break; }",
        "do { int $value = 0; } while ($one > 100);",
        "for ($i = 0; $i > $one; $i++) { int $value = 0; }",
        "foreach ([$one] as $item) { int $value = 0; }",
        "switch ($one) { case 100: int $value = 0; break; }",
        "try { if ($one > 100) { throw new Exception(); } } catch (Exception $e) { int $value = 0; }",
        "try { if ($one > 100) { throw new Exception(); } int $value = 0; } finally { echo $one; }",
    ] {
        let source = format!(r#"<?php
function conditional(int $one): mixed {{
    mixed $value = 0;
    {control}
    return ($value = PHP_INT_MAX + $one);
}}
echo gettype(conditional($argc));
"#);
        let module = super::lower_source(&source);
        let f = function(&module, "conditional");
        assert!(f.instructions.iter().any(|inst| inst.op == Op::ICheckedAdd), "{control}");
        assert!(!f.instructions.iter().any(|inst| inst.op == Op::Cast
            && inst.result_php_type == PhpType::Int
            && inst.immediate == Some(Immediate::CastTarget(IrType::I64))), "{control}");
    }
}
