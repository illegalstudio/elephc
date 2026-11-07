//! Purpose:
//! Lowers the internal `__elephc_reflection_class_build` builtin: an indirect call to the
//! compiled per-class `ReflectionClass` builder whose address a member reflector stored.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions` dispatch, through
//!   `RuntimeFnId::ElephcReflectionClassBuild`.
//!
//! Key details:
//! - The builders are module-level helpers emitted by
//!   `crate::codegen::reflection_class_builders`; each leaves a fresh, OWNED
//!   `ReflectionClass` in the integer result register, which this call stores as its result.
//! - EIR types the call `mixed` (the contract can only say `object`), so the owned result
//!   is boxed into a Mixed cell that takes over the builder's reference.
//! - The address arrives as a boxed `int` slot value, so `resolve_int_operand_to_result`
//!   unboxes it the same way it unboxes any Mixed integer operand.

use crate::codegen::{abi, emit_box_current_owned_value_as_mixed};
use crate::codegen::Result;
use crate::ir::Instruction;
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::super::resolve_int_operand_to_result;
use super::{expect_operand, store_if_result};

/// Calls the stored builder and keeps the fresh reflector it returns.
pub(crate) fn lower_reflection_class_build(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "__elephc_reflection_class_build", 1)?;
    ctx.emitter.blank();
    ctx.emitter.comment("__elephc_reflection_class_build()");
    resolve_int_operand_to_result(ctx, expect_operand(inst, 0)?, "reflection class builder")?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let target_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_reg_move(ctx.emitter, target_reg, result_reg);
    abi::emit_call_reg(ctx.emitter, target_reg);
    // The shared contract can only declare `object`, which EIR carries as `mixed`: hand the
    // builder's owned reference to the Mixed cell instead of storing a raw object pointer.
    if let Some(result) = inst.result {
        if ctx.value_php_type(result)?.codegen_repr() == PhpType::Mixed {
            emit_box_current_owned_value_as_mixed(
                ctx.emitter,
                &PhpType::Object("ReflectionClass".to_string()),
            );
        }
    }
    store_if_result(ctx, inst)
}
