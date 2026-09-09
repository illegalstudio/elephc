//! Purpose:
//! Lowers pop/shift of boxed PHP arrays through their writable receiver storage.
//!
//! Called from:
//! - The typed ArrayPop and ArrayShift backend paths.
//!
//! Key details:
//! - The outer cell is separated and published before its packed/hash payload is mutated.
//! - The runtime returns an independently owned Mixed value, including null for empty arrays.

use super::*;

/// Separates the caller's boxed cell, publishes it, then removes the selected edge value.
pub(super) fn lower_boxed_array_take(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    shift: bool,
) -> Result<()> {
    require_array_pop_result_type(&inst.result_php_type.codegen_repr())?;
    let name = if shift { "array_shift" } else { "array_pop" };
    let receiver = ReceiverPlace::resolve(ctx, array)?;
    receiver.require_writable(name)?;
    ctx.load_value_to_reg(array, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
    abi::emit_call_label(ctx.emitter, "__rt_array_cell_ensure_unique");
    let valid = ctx.next_label("array_take_valid_cell");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x0, {valid}"));              // reject non-array cells before publishing any receiver change
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // valid empty arrays still have a nonzero boxed cell
            ctx.emitter.instruction(&format!("jnz {valid}"));                   // publish only a validated array cell
        }
    }
    crate::codegen::lower_inst::exceptions::emit_type_error(
        ctx, &format!("{name}(): Argument #1 ($array) must be of type array"),
    );
    ctx.emitter.label(&valid);
    ctx.store_result_value(array)?;
    receiver.store_back_value(ctx, array)?;
    ctx.load_value_to_reg(array, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), i64::from(shift));
    abi::emit_call_label(ctx.emitter, "__rt_array_take_boxed");
    store_if_result(ctx, inst)
}
