//! Purpose:
//! Lowers the by-value set operations `array_diff()`, `array_intersect()` and the boxed
//! `array_unique()` through one runtime scan over any array layout.
//!
//! Called from:
//! - `super::reduce_sets::lower_array_diff()` / `lower_array_intersect()`, for the operands
//!   `__rt_hash_value_diff_intersect` cannot compare (see [`needs_rendering_scan`]).
//! - `super::basic::lower_array_unique()` for elements the typed dedup helpers cannot compare.
//!
//! Key details:
//! - php compares these elements by their string rendering, so a boxed element is compared by
//!   value rather than by the box pointer the typed helpers would have compared.
//! - Survivors keep their original keys: the result is always a fresh hash, boxed when the
//!   checker typed the call as the declared `array`.
//! - Stack cells borrow the EIR operands; the runtime never consumes them.

use super::*;
use crate::codegen_support::runtime::{MODE_DIFF, MODE_INTERSECT, MODE_UNIQUE};

const BORROWED_BYTES: usize = 64;

/// Whether `array_diff()` / `array_intersect()` must take the rendering scan: a boxed or declared
/// `array` operand or result, or an operand whose elements are not the scalars and strings
/// `__rt_hash_value_diff_intersect` compares.
pub(super) fn needs_rendering_scan(ctx: &FunctionContext<'_>, inst: &Instruction) -> Result<bool> {
    if matches!(
        inst.result_php_type.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return Ok(true);
    }
    for operand in inst.operands.iter().take(2) {
        let elements = match ctx.value_php_type(*operand)?.codegen_repr() {
            PhpType::Array(elem) => elem.codegen_repr(),
            PhpType::AssocArray { value, .. } => value.codegen_repr(),
            _ => return Ok(true),
        };
        if !super::reduce_sets::value_set_op_element_converts(&elements) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Lowers `array_diff()`: keeps first-array elements whose rendering is absent from the second.
pub(super) fn lower_array_diff_values(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_value_set_op(ctx, inst, "array_diff", MODE_DIFF)
}

/// Lowers `array_intersect()`: keeps first-array elements whose rendering the second holds.
pub(super) fn lower_array_intersect_values(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_value_set_op(ctx, inst, "array_intersect", MODE_INTERSECT)
}

/// Lowers `array_unique()`: keeps the first element of each rendering.
pub(super) fn lower_array_unique_values(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_value_set_op(ctx, inst, "array_unique", MODE_UNIQUE)
}

/// Borrows the operands into stack cells, runs the scan, and stores the owned result hash.
fn lower_value_set_op(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    mode: i64,
) -> Result<()> {
    let arity = if mode == MODE_UNIQUE { 1 } else { 2 };
    super::super::ensure_arg_count(inst, name, arity)?;
    let result_ty = inst.result_php_type.codegen_repr();
    if !matches!(
        result_ty,
        PhpType::AssocArray { .. } | PhpType::Mixed | PhpType::Union(_)
    ) {
        return Err(CodegenIrError::unsupported(format!(
            "{name} result PHP type {:?}",
            inst.result_php_type
        )));
    }
    let first = expect_operand(inst, 0)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, BORROWED_BYTES);
    super::boxed_membership::store_borrowed_cell(ctx, first, 0)?;
    // array_unique has no second operand; the helper never reads it in that mode.
    let second_offset = if mode == MODE_UNIQUE {
        0
    } else {
        super::boxed_membership::store_borrowed_cell(ctx, expect_operand(inst, 1)?, 32)?;
        32
    };
    for (index, offset) in [(0, 0), (1, second_offset)] {
        abi::emit_temporary_stack_address(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, index),
            offset,
        );
    }
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), mode);
    abi::emit_call_label(ctx.emitter, "__rt_array_set_op_boxed");
    abi::emit_release_temporary_stack(ctx.emitter, BORROWED_BYTES);
    let valid = ctx.next_label("array_set_op_valid");
    let bad_second = ctx.next_label("array_set_op_bad_second");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x0, {valid}"));              // a successful scan returns an owned hash even when empty
            ctx.emitter.instruction("cmp x1, #2");                              // a rejected operand reports its one-based position
            ctx.emitter.instruction(&format!("b.eq {bad_second}"));             // select the second argument diagnostic
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // only a rejected operand returns zero
            ctx.emitter.instruction(&format!("jnz {valid}"));                   // keep the owned result hash
            ctx.emitter.instruction("cmp rdx, 2");                              // a rejected operand reports its one-based position
            ctx.emitter.instruction(&format!("je {bad_second}"));               // select the second argument diagnostic
        }
    }
    crate::codegen::lower_inst::exceptions::emit_type_error(
        ctx,
        &format!("{name}(): Argument #1 ($array) must be of type array"),
    );
    ctx.emitter.label(&bad_second);
    crate::codegen::lower_inst::exceptions::emit_type_error(
        ctx,
        &format!("{name}(): Argument #2 must be of type array"),
    );
    ctx.emitter.label(&valid);
    box_hash_result_for_mixed_builtin(ctx, inst, &PhpType::Mixed);
    store_if_result(ctx, inst)
}
