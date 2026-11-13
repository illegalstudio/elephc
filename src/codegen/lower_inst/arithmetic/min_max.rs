//! Purpose:
//! Lowers `Op::MixedMinMax`, PHP's variadic `min()` / `max()` between two operands that are
//! not both statically raw numbers.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - A result with a proven frame cell is selected inline by `stack_cell::lower_min_max`:
//!   the frame proof guarantees both operands hold an integer or a float.
//! - Every other result calls `__rt_mixed_min_max`, which orders the operands with
//!   `__rt_php_compare` and returns an owned copy of the winner.
//! - Operands are only read. A boxed operand is passed as its cell pointer; a raw integer,
//!   float or bool is written to a temporary stack cell with the Mixed tag/payload layout, so
//!   lending it to the helper allocates nothing.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Immediate, Instruction, MinMaxRule, ValueId};
use crate::types::PhpType;

use super::{expect_operand, stack_cell, store_if_result};

/// Bytes reserved for the two staged operands: each has a 24-byte cell and a pointer word.
const STAGING_BYTES: usize = 64;
/// Offset of the right operand's temporary cell; its pointer word follows the cell.
const RIGHT_BASE: usize = 0;
/// Offset of the left operand's temporary cell; its pointer word follows the cell.
const LEFT_BASE: usize = 32;
/// Distance from an operand's temporary cell to the word holding its cell pointer.
const POINTER_WORD: usize = 24;

/// Lowers one `min()`/`max()` selection into a frame cell or an owned heap cell.
pub(in crate::codegen::lower_inst) fn lower_mixed_min_max(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let rule = expect_rule(inst)?;
    if let Some(offset) = inst.result.and_then(|value| ctx.numeric_cell_offsets.get(&value).copied()) {
        return stack_cell::lower_min_max(ctx, inst, offset, rule);
    }
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, STAGING_BYTES);
    stage_operand(ctx, lhs, LEFT_BASE)?;
    stage_operand(ctx, rhs, RIGHT_BASE)?;
    let (left, right, rule_reg) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x0", "x1", "x2"),
        Arch::X86_64 => ("rdi", "rsi", "rdx"),
    };
    abi::emit_load_temporary_stack_slot(ctx.emitter, left, LEFT_BASE + POINTER_WORD);
    abi::emit_load_temporary_stack_slot(ctx.emitter, right, RIGHT_BASE + POINTER_WORD);
    abi::emit_load_int_immediate(ctx.emitter, rule_reg, rule.immediate());
    abi::emit_call_label(ctx.emitter, "__rt_mixed_min_max");
    abi::emit_release_temporary_stack(ctx.emitter, STAGING_BYTES);
    store_if_result(ctx, inst)
}

/// Stores the pointer to one operand's Mixed-shaped cell in its staging pointer word.
///
/// A boxed operand already is such a cell. A raw scalar gets a temporary cell at `base`
/// (`[tag][payload][0]`), which stays valid until the staging area is released.
fn stage_operand(ctx: &mut FunctionContext<'_>, value: ValueId, base: usize) -> Result<()> {
    let result = abi::int_result_reg(ctx.emitter);
    let scratch = abi::secondary_scratch_reg(ctx.emitter);
    let tag = match ctx.load_value_to_result(value)?.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => None,
        PhpType::Int => Some(0),
        // Lowering boxes a bool operand, but a fold may hand its raw word back.
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
        abi::emit_temporary_stack_address(ctx.emitter, scratch, base);
        abi::emit_store_to_address(ctx.emitter, result, scratch, 8);
        abi::emit_load_int_immediate(ctx.emitter, result, tag);
        abi::emit_store_to_address(ctx.emitter, result, scratch, 0);
        abi::emit_load_int_immediate(ctx.emitter, result, 0);
        abi::emit_store_to_address(ctx.emitter, result, scratch, 16);
        abi::emit_reg_move(ctx.emitter, result, scratch);
    }
    abi::emit_temporary_stack_address(ctx.emitter, scratch, base + POINTER_WORD);
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
