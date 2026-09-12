//! Purpose:
//! Lowers mutating walks over boxed PHP arrays through a storage-neutral callback runtime.
//!
//! Called from:
//! - `lower_array_walk()` and `lower_array_walk_recursive()`.
//!
//! Key details:
//! - The receiver is separated before callback normalization so element references mutate only
//!   the caller-visible array owner.
//! - The runtime consumes one retained callable descriptor and preserves actual key/value tags.

use super::*;

const CALLBACK_STACK_BYTES: usize = 64;

/// Lowers a boxed `array_walk` variant and returns whether this path handled the operand.
pub(super) fn lower_boxed_array_walk(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    callback: ValueId,
    name: &str,
    recursive: bool,
) -> Result<bool> {
    if ctx.value_php_type(array)?.codegen_repr() != PhpType::Mixed {
        return Ok(false);
    }

    super::boxed_mutation::prepare_boxed_array_receiver(ctx, array, name)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, CALLBACK_STACK_BYTES);
    super::boxed_membership::store_borrowed_cell(ctx, callback, 32)?;
    let callback_error = if recursive {
        "array_walk_recursive(): Argument #2 ($callback) must be a valid callback"
    } else {
        "array_walk(): Argument #2 ($callback) must be a valid callback"
    };
    super::boxed_predicates::acquire_callback_descriptor(
        ctx,
        inst,
        callback,
        name,
        callback_error,
    )?;
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_reg_move(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        result,
    );
    ctx.load_value_to_reg(array, abi::int_arg_reg_name(ctx.emitter.target, 1))?;
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        i64::from(recursive),
    );
    abi::emit_call_label(ctx.emitter, "__rt_array_walk_boxed");
    abi::emit_release_temporary_stack(ctx.emitter, CALLBACK_STACK_BYTES);
    store_void_builtin_result(ctx, inst)?;
    Ok(true)
}
