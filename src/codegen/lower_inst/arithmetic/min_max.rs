//! Purpose:
//! Lowers `Op::MixedMinMax`, PHP's variadic `min()` / `max()` over operands that are not all
//! statically `int` or all statically `float`.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - A two-argument selection with a proven frame cell is emitted inline by
//!   `stack_cell::lower_min_max`: the frame proof guarantees both operands hold an integer
//!   or a float.
//! - Every other selection calls `__rt_mixed_min_max_select`, which follows php-src's
//!   comparison modes across all operands and returns the winning operand's borrowed cell.
//!   A result with a frame cell copies that cell's tag and payload; any other result is an
//!   owned `__rt_mixed_clone` of it.
//! - Operands are only read. A boxed operand is passed as its cell pointer; a raw integer,
//!   float or bool is written to a temporary stack cell with the Mixed tag/payload layout, so
//!   lending it to the helper allocates nothing.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Immediate, Instruction, MinMaxRule, ValueId};
use crate::types::PhpType;

use super::{stack_cell, store_if_result};

/// Bytes of the helper's pointer-array entry for one operand.
const POINTER_BYTES: usize = 8;
/// Bytes of the temporary Mixed-shaped cell reserved for one raw scalar operand.
const CELL_BYTES: usize = 24;

/// Lowers one `min()`/`max()` selection into a frame cell or an owned heap cell.
pub(in crate::codegen::lower_inst) fn lower_mixed_min_max(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let rule = expect_rule(inst)?;
    let cell = inst.result.and_then(|value| ctx.numeric_cell_offsets.get(&value).copied());
    if let (true, Some(offset)) = (rule.is_pair(), cell) {
        return stack_cell::lower_min_max(ctx, inst, offset, rule);
    }
    // Staging area: the pointer array the helper walks, then one temporary cell per operand.
    let count = inst.operands.len();
    let cells_base = count * POINTER_BYTES;
    let staging = (cells_base + count * CELL_BYTES + 15) & !15;
    abi::emit_reserve_temporary_stack(ctx.emitter, staging);
    for (index, &operand) in inst.operands.iter().enumerate() {
        stage_operand(ctx, operand, index * POINTER_BYTES, cells_base + index * CELL_BYTES)?;
    }
    let (cells, count_reg, rule_reg) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x0", "x1", "x2"),
        Arch::X86_64 => ("rdi", "rsi", "rdx"),
    };
    abi::emit_temporary_stack_address(ctx.emitter, cells, 0);
    abi::emit_load_int_immediate(ctx.emitter, count_reg, count as i64);
    abi::emit_load_int_immediate(ctx.emitter, rule_reg, rule.immediate());
    abi::emit_call_label(ctx.emitter, "__rt_mixed_min_max_select");
    if let Some(offset) = cell {
        // The winner may be a temporary cell, so it is read before the staging area goes.
        return stack_cell::store_selected(ctx, inst, offset, staging);
    }
    // The selected cell pointer is already in the clone helper's argument register.
    abi::emit_call_label(ctx.emitter, "__rt_mixed_clone");
    abi::emit_release_temporary_stack(ctx.emitter, staging);
    store_if_result(ctx, inst)
}

/// Stores the pointer to one operand's Mixed-shaped cell in its pointer-array entry.
///
/// A boxed operand already is such a cell. A raw scalar gets the temporary cell at
/// `cell_base` (`[tag][payload][0]`), which stays valid until the staging area is released.
fn stage_operand(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    pointer_slot: usize,
    cell_base: usize,
) -> Result<()> {
    let result = abi::int_result_reg(ctx.emitter);
    let scratch = abi::secondary_scratch_reg(ctx.emitter);
    let tag = match ctx.load_value_to_result(value)?.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => None,
        PhpType::Int => Some(0),
        // Lowering boxes a bool operand; a raw one is still lent as a bool-tagged cell.
        PhpType::Bool | PhpType::False => Some(3),
        PhpType::Float => {
            abi::emit_reg_move(ctx.emitter, result, abi::float_result_reg(ctx.emitter));
            Some(2)
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "mixed_min_max operand of PHP type {:?}",
                other
            )))
        }
    };
    if let Some(tag) = tag {
        // -- lend the raw scalar to the helper as a temporary Mixed-shaped stack cell --
        abi::emit_temporary_stack_address(ctx.emitter, scratch, cell_base);
        abi::emit_store_to_address(ctx.emitter, result, scratch, 8);
        abi::emit_load_int_immediate(ctx.emitter, result, tag);
        abi::emit_store_to_address(ctx.emitter, result, scratch, 0);
        abi::emit_load_int_immediate(ctx.emitter, result, 0);
        abi::emit_store_to_address(ctx.emitter, result, scratch, 16);
        abi::emit_reg_move(ctx.emitter, result, scratch);
    }
    abi::emit_temporary_stack_address(ctx.emitter, scratch, pointer_slot);
    abi::emit_store_to_address(ctx.emitter, result, scratch, 0);
    Ok(())
}

/// Returns the selection rule attached to the EIR instruction.
fn expect_rule(inst: &Instruction) -> Result<MinMaxRule> {
    match inst.immediate {
        Some(Immediate::I64(rule)) => MinMaxRule::from_immediate(rule),
        _ => None,
    }
    .ok_or_else(|| {
        CodegenIrError::invalid_module(format!("{} missing min/max rule immediate", inst.op.name()))
    })
}
