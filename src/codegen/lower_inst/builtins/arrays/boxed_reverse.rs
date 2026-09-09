//! Purpose:
//! Lowers reversal of boxed PHP arrays with runtime key-preservation flags.
//!
//! Called from:
//! - `super::basic::lower_array_reverse()`.
//!
//! Key details:
//! - The runtime helper borrows its source and returns an owned Mixed-value hash.
//! - Result boxing transfers that owner; invalid input raises a catchable TypeError.

use super::*;

/// Calls the boxed reversal helper and transfers its owned hash into the declared boxed result.
pub(super) fn lower_boxed_array_reverse(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
) -> Result<()> {
    if inst.result_php_type.codegen_repr() != PhpType::Mixed {
        return Err(CodegenIrError::unsupported(
            "boxed array_reverse requires a boxed array result".to_string(),
        ));
    }
    let result_reg = abi::int_result_reg(ctx.emitter);
    if let Some(flag) = inst.operands.get(1).copied() {
        ctx.load_value_to_result(flag)?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
    }
    abi::emit_push_reg(ctx.emitter, result_reg);
    ctx.load_value_to_reg(array, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
    abi::emit_pop_reg(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1));
    abi::emit_call_label(ctx.emitter, "__rt_array_reverse_boxed");
    let valid = ctx.next_label("array_reverse_boxed_valid");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x0, {valid}"));              // only a rejected non-array returns no hash owner
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // valid empty arrays still return allocated result hashes
            ctx.emitter.instruction(&format!("jnz {valid}"));                   // preserve the result owner on the valid path
        }
    }
    crate::codegen::lower_inst::exceptions::emit_type_error(
        ctx, "array_reverse(): Argument #1 ($array) must be of type array",
    );
    ctx.emitter.label(&valid);
    box_hash_result_for_mixed_builtin(ctx, inst, &PhpType::Mixed);
    store_if_result(ctx, inst)
}
