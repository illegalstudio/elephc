//! Purpose:
//! Specializes a boxed fused map+sum kernel whose only observation is an `(int)` cast
//! into the allocation-free raw integer entry.
//!
//! Called from:
//! - The EIR fixed-point pass driver after `array_sum_int_sink`.
//!
//! Key details:
//! - The frozen `array_sum(array_map(fn($x) => $x + C, $packedInts))` lowering emits
//!   `ArrayMapSumAdd` with a boxed result; the cast-only sink replaces it with
//!   `ArrayMapSumAddInt` and retires the cast/release scaffolding.
//! - The same complete use graph as `array_sum_int_sink` is required: one int cast and
//!   ordinary releases, never a second observer or terminator use.
//! - A promoted map+sum still terminates in PHP's warned float-to-int conversion.

use crate::ir::{
    DataPool, Function, Immediate, InstId, IrType, Op, Ownership, RuntimeCallTarget, RuntimeFnId,
};
use crate::types::PhpType;

use super::driver::IrPass;
use super::rewrite::{neutralize_to_nop, replace_all_uses};

/// Rewrites provably int-observed fused map+sum calls to the raw reducer.
pub struct ArrayMapSumAddIntSink;

impl IrPass for ArrayMapSumAddIntSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "array_map_sum_add_int_sink"
    }

    /// Skips functions with no boxed fused map+sum call.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(is_boxed_map_sum)
    }

    /// Specializes every independently proven call.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| is_boxed_map_sum(inst))
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

/// Returns whether `inst` is a boxed fused map+sum runtime call.
fn is_boxed_map_sum(inst: &crate::ir::Instruction) -> bool {
    inst.op == Op::RuntimeCall
        && matches!(
            inst.immediate,
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::ArrayMapSumAdd
            )))
        )
        && inst.result.is_some()
        && inst.result_php_type.codegen_repr() == PhpType::Mixed
}

/// Specializes one candidate after proving its complete use graph.
fn specialize(function: &mut Function, candidate: InstId) -> bool {
    let Some(call) = function.instruction(candidate) else {
        return false;
    };
    let Some(result) = call.result else {
        return false;
    };
    let Some((cast, cast_result, releases)) = super::array_sum_int_sink::analyze_uses(function, result) else {
        return false;
    };

    let Some(call) = function.instruction_mut(candidate) else {
        return false;
    };
    call.immediate = Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
        RuntimeFnId::ArrayMapSumAddInt,
    )));
    call.result_type = IrType::I64;
    call.result_php_type = PhpType::Int;
    call.result_ownership = Ownership::NonHeap;
    call.effects = RuntimeFnId::ArrayMapSumAddInt.intrinsic_effects();
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
