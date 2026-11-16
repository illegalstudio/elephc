//! Purpose:
//! Specializes a boxed `abs()` whose only observation is an `(int)` cast into the raw
//! integer absolute-value reducer.
//!
//! Called from:
//! - The EIR fixed-point pass driver after `array_sum_int_sink`.
//!
//! Key details:
//! - The operand must be a concrete int, bool, or null-capable tagged scalar; a float or a
//!   general Mixed value keeps the boxed `abs()` call.
//! - The result's complete use graph may contain one `Cast` to `I64` producing `int` plus
//!   ordinary `Release` scaffolding.
//! - `INT_MIN` still needs PHP's float promotion and warned float-to-int conversion, which the
//!   raw runtime helper performs; in-range values return their scalar magnitude directly.

use crate::ir::{DataPool, Function, Immediate, InstId, IrType, Op, Ownership, RuntimeCallTarget, RuntimeFnId};
use crate::types::PhpType;

use super::array_sum_int_sink::analyze_uses;
use super::driver::IrPass;
use super::rewrite::{neutralize_to_nop, replace_all_uses};

/// Rewrites provably int-observed `abs()` calls to the raw integer reducer.
pub struct AbsIntSink;

impl IrPass for AbsIntSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "abs_int_sink"
    }

    /// Skips functions with no boxed `abs()` runtime call.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(is_boxed_abs)
    }

    /// Specializes every independently proven call.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| is_boxed_abs(inst))
            .map(|(raw, _)| InstId::from_raw(raw as u32))
            .collect();
        let mut changed = false;
        for candidate in candidates {
            if specialize(function, candidate) {
                changed = true;
            }
        }
        changed
    }
}

/// Returns whether `inst` is a one-operand `abs()` runtime call producing a boxed cell.
fn is_boxed_abs(inst: &crate::ir::Instruction) -> bool {
    inst.op == Op::RuntimeCall
        && matches!(
            inst.immediate,
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::Abs
            )))
        )
        && inst.operands.len() == 1
        && inst.result.is_some()
        && inst.result_php_type.codegen_repr() == PhpType::Mixed
}

/// Specializes one candidate after proving its operand and complete use graph.
fn specialize(function: &mut Function, candidate: InstId) -> bool {
    let Some(call) = function.instruction(candidate) else {
        return false;
    };
    let Some(result) = call.result else {
        return false;
    };
    let Some(&operand) = call.operands.first() else {
        return false;
    };
    if !operand_is_integer_scalar(function, operand) {
        return false;
    }
    let Some((cast, cast_result, releases)) = analyze_uses(function, result) else {
        return false;
    };

    let Some(call) = function.instruction_mut(candidate) else {
        return false;
    };
    call.immediate = Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
        RuntimeFnId::AbsInt,
    )));
    call.result_type = IrType::I64;
    call.result_php_type = PhpType::Int;
    call.result_ownership = Ownership::NonHeap;
    call.effects = RuntimeFnId::AbsInt.intrinsic_effects();
    if let Some(value) = function.value_mut(result) {
        value.ir_type = IrType::I64;
        value.php_type = PhpType::Int;
        value.ownership = Ownership::NonHeap;
    }
    replace_all_uses(
        function,
        &std::collections::HashMap::from([(cast_result, result)]),
    );
    if let Some(inst) = function.instruction_mut(cast) {
        neutralize_to_nop(inst);
    }
    for release in releases {
        if let Some(inst) = function.instruction_mut(release) {
            neutralize_to_nop(inst);
        }
    }
    true
}

/// Returns whether the operand is an int, bool, or null-capable tagged scalar.
fn operand_is_integer_scalar(function: &Function, operand: crate::ir::ValueId) -> bool {
    function.value(operand).is_some_and(|value| {
        matches!(
            value.php_type.codegen_repr(),
            PhpType::Int | PhpType::Bool | PhpType::TaggedScalar
        )
    })
}
