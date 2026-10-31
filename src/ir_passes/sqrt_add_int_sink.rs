//! Purpose:
//! Fuses a boxed checked integer addition consumed only by a cast-observed `sqrt()` into
//! the raw integer square-root reducer.
//!
//! Called from:
//! - The EIR fixed-point pass driver after the integer cast sinks.
//!
//! Key details:
//! - Both addends must be concrete PHP ints; the runtime helper reproduces PHP's checked-add
//!   overflow promotion before the square root.
//! - The add result may be observed only by the selected `sqrt()` call and release scaffolding.
//! - The `sqrt()` result keeps its cast-only observation rule: one `(int)` cast plus releases.
//! - A negative sum still yields NAN, PHP's cast warning and zero through the helper.

use crate::ir::{
    DataPool, Function, Immediate, InstId, IrType, Op, Ownership, RuntimeCallTarget, RuntimeFnId,
    ValueDef, ValueId,
};
use crate::types::PhpType;

use super::array_sum_int_sink::analyze_uses;
use super::driver::IrPass;
use super::rewrite::{neutralize_to_nop, replace_all_uses};

/// Rewrites provably int-observed `sqrt(int + int)` calls to the fused raw reducer.
pub struct SqrtAddIntSink;

impl IrPass for SqrtAddIntSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "sqrt_add_int_sink"
    }

    /// Skips functions with no boxed `sqrt()` runtime call.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(is_boxed_sqrt)
    }

    /// Specializes every independently proven call.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| is_boxed_sqrt(inst))
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

/// Returns whether `inst` is a one-operand `sqrt()` runtime call producing a float.
fn is_boxed_sqrt(inst: &crate::ir::Instruction) -> bool {
    inst.op == Op::RuntimeCall
        && matches!(
            inst.immediate,
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::Sqrt
            )))
        )
        && inst.operands.len() == 1
        && inst.result.is_some()
        && inst.result_php_type.codegen_repr() == PhpType::Float
}

/// Returns the checked-add instruction whose boxed result feeds the candidate.
fn checked_add_source(
    function: &Function,
    value: ValueId,
) -> Option<(InstId, ValueId, ValueId)> {
    let ValueDef::Instruction { inst, .. } = function.value(value)?.def else {
        return None;
    };
    let add = function.instruction(inst)?;
    if add.op != Op::ICheckedAdd
        || add.operands.len() != 2
        || add.result.is_none()
        || add.result_php_type.codegen_repr() != PhpType::Mixed
    {
        return None;
    }
    let lhs = *add.operands.first()?;
    let rhs = *add.operands.get(1)?;
    for operand in [lhs, rhs] {
        if function
            .value(operand)
            .is_none_or(|value| value.php_type.codegen_repr() != PhpType::Int)
        {
            return None;
        }
    }
    Some((inst, lhs, rhs))
}

/// Returns the removable releases of the add result, or `None` on any other observer.
fn analyze_add_uses(
    function: &Function,
    add_result: ValueId,
    sqrt_call: InstId,
) -> Option<Vec<InstId>> {
    let mut releases = Vec::new();
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&add_result) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        if id == sqrt_call {
            continue;
        }
        if inst.op == Op::Release && inst.operands.as_slice() == [add_result] {
            releases.push(id);
        } else {
            return None;
        }
    }
    for block in &function.blocks {
        if block
            .terminator
            .as_ref()
            .is_some_and(|term| term.used_values().contains(&add_result))
        {
            return None;
        }
    }
    Some(releases)
}

/// Specializes one candidate after proving the add, its operands and complete use graph.
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
    let Some((add, lhs, rhs)) = checked_add_source(function, operand) else {
        return false;
    };
    let Some((cast, cast_result, releases)) = analyze_uses(function, result) else {
        return false;
    };
    let Some(add_releases) = analyze_add_uses(function, operand, candidate) else {
        return false;
    };

    let Some(call) = function.instruction_mut(candidate) else {
        return false;
    };
    call.operands = vec![lhs, rhs];
    call.immediate = Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
        RuntimeFnId::SqrtAddInt,
    )));
    call.result_type = IrType::I64;
    call.result_php_type = PhpType::Int;
    call.result_ownership = Ownership::NonHeap;
    call.effects = RuntimeFnId::SqrtAddInt.intrinsic_effects();
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
    for release in releases.into_iter().chain(add_releases) {
        if let Some(inst) = function.instruction_mut(release) {
            neutralize_to_nop(inst);
        }
    }
    if let Some(inst) = function.instruction_mut(add) {
        neutralize_to_nop(inst);
    }
    true
}
