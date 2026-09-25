//! Purpose:
//! Lowers the three typed internal-array-pointer runtime targets (`ArrayPtrSeek`,
//! `ArrayPtrKey`, `ArrayPtrValue`) that back PHP's `key`/`current`/`next`/`prev`/
//! `reset`/`end`.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays::lower_array_ptr_*()` through the
//!   typed runtime-function dispatch groups.
//!
//! Key details:
//! - Each target marshals the internal SysV/AAPCS helper ABI, independent of the generated
//!   PHP function ABI (which is MS x64 on Windows).
//! - Operand counts are enforced here rather than through a registry runtime signature:
//!   the PHP builtins declare one argument while these calls carry the cursor (and, for
//!   a seek, the seek mode) as extra operands, so a call that reached this code with the
//!   plain PHP arity is a lowering bug and must fail loudly instead of reading garbage.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::Instruction;

use super::super::super::{expect_operand, store_if_result};

/// Lowers `ArrayPtrSeek`: `(container, cursor, mode) -> new cursor`.
///
/// The helper owns all cursor arithmetic and the bounds checks against the live element
/// count, so the backend only marshals three integer-class operands and calls it.
pub(super) fn lower_array_ptr_seek(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    require_operand_count(inst, "array_ptr_seek", 3)?;
    let container = expect_operand(inst, 0)?;
    let cursor = expect_operand(inst, 1)?;
    let mode = expect_operand(inst, 2)?;
    let arg0 = abi::runtime_helper_int_arg_reg(ctx.emitter, 0);
    let arg1 = abi::runtime_helper_int_arg_reg(ctx.emitter, 1);
    let arg2 = abi::runtime_helper_int_arg_reg(ctx.emitter, 2);
    ctx.load_value_to_reg(container, arg0)?;
    ctx.load_value_to_reg(cursor, arg1)?;
    ctx.load_value_to_reg(mode, arg2)?;
    abi::emit_call_label(ctx.emitter, "__rt_array_ptr_seek");
    store_if_result(ctx, inst)
}

/// Lowers `ArrayPtrKey`: `(container, cursor) -> boxed Mixed key`.
///
/// An out-of-range cursor boxes canonical null inside the helper, which is exactly what
/// PHP's `key()` returns once the internal pointer has run off either end.
pub(super) fn lower_array_ptr_key(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_array_ptr_read(ctx, inst, "array_ptr_key", "__rt_array_ptr_key")
}

/// Lowers `ArrayPtrValue`: `(container, cursor) -> boxed Mixed value`.
///
/// An out-of-range cursor boxes `false` inside the helper, matching PHP's return value
/// for `current()` and for a `next`/`prev`/`reset`/`end` that ran out of elements.
pub(super) fn lower_array_ptr_value(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_array_ptr_read(ctx, inst, "array_ptr_value", "__rt_array_ptr_value")
}

/// Marshals `(container, cursor)` and calls one of the two boxing read helpers.
fn lower_array_ptr_read(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    symbol: &str,
) -> Result<()> {
    require_operand_count(inst, name, 2)?;
    let container = expect_operand(inst, 0)?;
    let cursor = expect_operand(inst, 1)?;
    let arg0 = abi::runtime_helper_int_arg_reg(ctx.emitter, 0);
    let arg1 = abi::runtime_helper_int_arg_reg(ctx.emitter, 1);
    ctx.load_value_to_reg(container, arg0)?;
    ctx.load_value_to_reg(cursor, arg1)?;
    abi::emit_call_label(ctx.emitter, symbol);
    store_if_result(ctx, inst)
}

/// Rejects an internal-pointer runtime call that does not carry its cursor operands.
///
/// These targets are only ever emitted by the internal-pointer argument lowering, which
/// always appends the cursor; anything else means the call escaped through the generic
/// registry path with the plain PHP arity and must not be lowered.
fn require_operand_count(inst: &Instruction, name: &str, expected: usize) -> Result<()> {
    if inst.operands.len() != expected {
        return Err(CodegenIrError::unsupported(format!(
            "{} expects {} typed operands but received {}",
            name,
            expected,
            inst.operands.len(),
        )));
    }
    Ok(())
}
