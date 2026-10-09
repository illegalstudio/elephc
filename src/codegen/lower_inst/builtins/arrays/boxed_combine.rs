//! Purpose:
//! Lowers `array_combine()` and `array_fill_keys()` through `__rt_array_combine_boxed` for the
//! shapes their string-keyed typed helpers cannot read.
//!
//! Called from:
//! - `super::basic::lower_array_combine()` / `lower_array_fill_keys()` when the checker typed
//!   the call as the boxed PHP array.
//!
//! Key details:
//! - Stack cells borrow the EIR operands; the runtime retains what it keeps.
//! - A rejected operand and `array_combine()`'s length mismatch come back as a zero result with
//!   a position, raised here as php's `TypeError` / `ValueError`.

use super::*;
use crate::codegen_support::runtime::{COMBINE_COUNT_MISMATCH, MODE_COMBINE, MODE_FILL};

const BORROWED_BYTES: usize = 64;

/// Whether the call was typed for the boxed builder rather than a typed helper.
pub(super) fn uses_boxed_builder(inst: &Instruction) -> bool {
    matches!(
        inst.result_php_type.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    )
}

/// Lowers `array_combine($keys, $values)` through the boxed builder.
pub(super) fn lower_boxed_array_combine(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_boxed_builder(
        ctx,
        inst,
        MODE_COMBINE,
        "array_combine(): Argument #1 ($keys) must be of type array",
        "array_combine(): Argument #2 ($values) must be of type array",
    )
}

/// Lowers `array_fill_keys($keys, $value)` through the boxed builder.
pub(super) fn lower_boxed_array_fill_keys(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_boxed_builder(
        ctx,
        inst,
        MODE_FILL,
        "array_fill_keys(): Argument #1 ($keys) must be of type array",
        "",
    )
}

/// Borrows both operands into stack cells, runs the builder, and boxes its owned hash.
fn lower_boxed_builder(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    mode: i64,
    first_error: &str,
    second_error: &str,
) -> Result<()> {
    let keys = expect_operand(inst, 0)?;
    let second = expect_operand(inst, 1)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, BORROWED_BYTES);
    super::boxed_membership::store_borrowed_cell(ctx, keys, 0)?;
    super::boxed_membership::store_borrowed_cell(ctx, second, 32)?;
    for (index, offset) in [(0, 0), (1, 32)] {
        abi::emit_temporary_stack_address(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, index),
            offset,
        );
    }
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), mode);
    abi::emit_call_label(ctx.emitter, "__rt_array_combine_boxed");
    abi::emit_release_temporary_stack(ctx.emitter, BORROWED_BYTES);
    let valid = ctx.next_label("array_combine_boxed_valid");
    let bad_second = ctx.next_label("array_combine_boxed_bad_second");
    let mismatch = ctx.next_label("array_combine_boxed_mismatch");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x0, {valid}"));              // a successful build returns an owned hash even when empty
            ctx.emitter.instruction("cmp x1, #2");                              // a rejected values operand reports position two
            ctx.emitter.instruction(&format!("b.eq {bad_second}"));             // select the values diagnostic
            ctx.emitter.instruction(&format!("cmp x1, #{COMBINE_COUNT_MISMATCH}")); // different lengths report their own position
            ctx.emitter.instruction(&format!("b.eq {mismatch}"));               // select the length diagnostic
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // only a rejected call returns zero
            ctx.emitter.instruction(&format!("jnz {valid}"));                   // keep the owned result hash
            ctx.emitter.instruction("cmp rdx, 2");                              // a rejected values operand reports position two
            ctx.emitter.instruction(&format!("je {bad_second}"));               // select the values diagnostic
            ctx.emitter.instruction(&format!("cmp rdx, {COMBINE_COUNT_MISMATCH}")); // different lengths report their own position
            ctx.emitter.instruction(&format!("je {mismatch}"));                 // select the length diagnostic
        }
    }
    crate::codegen::lower_inst::exceptions::emit_type_error(ctx, first_error);
    ctx.emitter.label(&bad_second);
    if mode == MODE_COMBINE {
        crate::codegen::lower_inst::exceptions::emit_type_error(ctx, second_error);
    }
    ctx.emitter.label(&mismatch);
    if mode == MODE_COMBINE {
        crate::codegen::lower_inst::exceptions::emit_value_error(
            ctx,
            "array_combine(): Argument #1 ($keys) and argument #2 ($values) must have the same number of elements",
        );
    }
    ctx.emitter.label(&valid);
    box_hash_result_for_mixed_builtin(ctx, inst, &PhpType::Mixed);
    store_if_result(ctx, inst)
}
