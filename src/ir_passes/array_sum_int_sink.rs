//! Purpose:
//! Specializes a boxed `array_sum()` whose only observation is an `(int)` cast into the
//! allocation-free raw integer reducer.
//!
//! Called from:
//! - The EIR fixed-point pass driver after `checked_int_sink`.
//!
//! Key details:
//! - The receiver must be statically `array<int>`, the shape the boxed aggregate already
//!   lowers through its packed int fast path, so the raw helper reads the same payload.
//! - The result's complete use graph may contain one `Cast` to `I64` producing `int` plus
//!   ordinary `Release` scaffolding. Any other observer keeps the boxed call.
//! - The cast and releases are neutralized, the cast result is replaced by the call result,
//!   and the call target becomes `ArraySumInt` with `I64`/`int`/`NonHeap` metadata.
//! - PHP's float-to-int cast warning on an overflow-promoted sum is reproduced by the
//!   runtime helper through the shared float-key conversion path.

use crate::ir::{
    Function, Immediate, InstId, IrType, Op, Ownership, RuntimeCallTarget, RuntimeFnId,
    ValueId,
};
use crate::ir::DataPool;
use crate::types::PhpType;

use super::driver::IrPass;
use super::rewrite::{neutralize_to_nop, replace_all_uses};

/// Rewrites provably int-observed `array_sum()` calls to the raw reducer.
pub struct ArraySumIntSink;

impl IrPass for ArraySumIntSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "array_sum_int_sink"
    }

    /// Skips functions with no boxed `array_sum()` runtime call.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(is_boxed_array_sum)
    }

    /// Specializes every independently proven call.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| is_boxed_array_sum(inst))
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

/// Returns whether `inst` is a one-operand `array_sum()` runtime call producing a boxed cell.
fn is_boxed_array_sum(inst: &crate::ir::Instruction) -> bool {
    inst.op == Op::RuntimeCall
        && matches!(
            inst.immediate,
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::ArraySum
            )))
        )
        && inst.operands.len() == 1
        && inst.result.is_some()
        && inst.result_php_type.codegen_repr() == PhpType::Mixed
}

/// Specializes one candidate after proving its receiver and complete use graph.
fn specialize(function: &mut Function, candidate: InstId) -> bool {
    let Some(call) = function.instruction(candidate) else {
        return false;
    };
    let Some(result) = call.result else {
        return false;
    };
    let Some(&receiver) = call.operands.first() else {
        return false;
    };
    if !receiver_is_packed_int_array(function, receiver) {
        return false;
    }
    let Some((cast, cast_result, releases)) = analyze_uses(function, result) else {
        return false;
    };

    let Some(call) = function.instruction_mut(candidate) else {
        return false;
    };
    call.immediate = Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
        RuntimeFnId::ArraySumInt,
    )));
    call.result_type = IrType::I64;
    call.result_php_type = PhpType::Int;
    call.result_ownership = Ownership::NonHeap;
    call.effects = RuntimeFnId::ArraySumInt.intrinsic_effects();
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

/// Returns whether the receiver's EIR value is statically a packed `array<int>`.
fn receiver_is_packed_int_array(function: &Function, receiver: ValueId) -> bool {
    function.value(receiver).is_some_and(|value| {
        matches!(
            value.php_type.codegen_repr(),
            PhpType::Array(element) if element.codegen_repr() == PhpType::Int
        )
    })
}

/// Returns the single int cast observing `result`, its result value, and removable releases.
pub(super) fn analyze_uses(
    function: &Function,
    result: ValueId,
) -> Option<(InstId, ValueId, Vec<InstId>)> {
    let mut cast = None;
    let mut releases = Vec::new();
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&result) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        match inst.op {
            Op::Cast
                if inst.operands.as_slice() == [result]
                    && inst.immediate == Some(Immediate::CastTarget(IrType::I64))
                    && inst.result_php_type.codegen_repr() == PhpType::Int =>
            {
                let cast_result = inst.result?;
                if cast.replace((id, cast_result)).is_some() {
                    return None;
                }
            }
            Op::Release if inst.operands.as_slice() == [result] => releases.push(id),
            _ => return None,
        }
    }
    for block in &function.blocks {
        if block
            .terminator
            .as_ref()
            .is_some_and(|term| term.used_values().contains(&result))
        {
            return None;
        }
    }
    let (cast, cast_result) = cast?;
    Some((cast, cast_result, releases))
}
