//! Purpose:
//! Selects immediate forms for integer arithmetic, masks, and comparisons.
//!
//! Called from:
//! - Integer arithmetic and scalar comparison lowering.
//!
//! Key details:
//! - Shares legality with the allocator's use-cost analysis.
//! - Swapping comparison operands reverses the predicate, preserving signed ordering.
//! - Compares read register-allocated operands in place instead of copying them first.

use crate::codegen::{abi, platform::Arch, Result};
use crate::ir::{CmpPredicate, Instruction, ValueId};
use crate::ir_passes::rematerialize::immediate_use;

use super::super::context::FunctionContext;
use super::{expect_cmp_predicate, expect_operand, require_integer_like, store_if_result};

/// Emits an immediate arithmetic operation when one operand has a legal encoding.
pub(super) fn try_int_binop(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<bool> {
    let Some(usage) = immediate_use(ctx.function, inst, ctx.emitter.target) else { return Ok(false); };
    let reg = abi::int_result_reg(ctx.emitter);
    let encoding = usage.encoding;
    let assembly = match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let source = super::arithmetic::integer_operand_register(ctx, usage.other, reg, inst)?;
            format!("{} {reg}, {source}, {}", encoding.mnemonic, encoding.operand)
        }
        Arch::X86_64 => {
            super::arithmetic::load_integer_operand(ctx, usage.other, reg, inst)?;
            format!("{} {reg}, {}", encoding.mnemonic, encoding.operand)
        }
    };
    // -- incorporate the constant directly in the consuming instruction --
    ctx.emitter.instruction(&assembly);                                         // compute without a constant register or spill load
    store_if_result(ctx, inst)?;
    Ok(true)
}

/// Emits the comparison flags and returns the predicate after any operand exchange.
pub(super) fn emit_int_compare(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<CmpPredicate> {
    let predicate = expect_cmp_predicate(inst)?;
    let result = abi::int_result_reg(ctx.emitter);
    if let Some(usage) = immediate_use(ctx.function, inst, ctx.emitter.target) {
        let reg = compare_operand(ctx, usage.other, result, inst)?;
        let encoding = usage.encoding;
        // -- compare with an encoded bound or scalar constant --
        ctx.emitter.instruction(&format!("{} {reg}, {}", encoding.mnemonic, encoding.operand)); // preserve flags for the selected comparison
        return Ok(if usage.constant_index == 0 { reversed(predicate) } else { predicate });
    }
    let scratch = abi::secondary_scratch_reg(ctx.emitter);
    let lhs = compare_operand(ctx, expect_operand(inst, 0)?, result, inst)?;
    let rhs = compare_operand(ctx, expect_operand(inst, 1)?, scratch, inst)?;
    // -- use full-width registers when neither constant has a legal encoding --
    ctx.emitter.instruction(&format!("cmp {lhs}, {rhs}"));                      // compare without truncating either operand
    Ok(predicate)
}

/// Returns the register holding an integer compare operand.
///
/// A compare only reads its operands, so a value with an allocated register home is used in
/// place; anything else (spilled or rematerialized) is loaded into `fallback` first.
fn compare_operand(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    fallback: &str,
    inst: &Instruction,
) -> Result<String> {
    if let Some(home) = ctx.allocation.register_of(value) {
        require_integer_like(ctx.value_php_type(value)?, inst)?;
        return Ok(home.to_string());
    }
    require_integer_like(ctx.load_value_to_reg(value, fallback)?, inst)?;
    Ok(fallback.to_string())
}

/// Reverses ordered predicates when a constant moves from the left to the right.
fn reversed(predicate: CmpPredicate) -> CmpPredicate {
    use CmpPredicate::*;
    match predicate {
        Eq => Eq, Ne => Ne,
        Slt => Sgt, Sle => Sge, Sgt => Slt, Sge => Sle,
        Olt => Ogt, Ole => Oge, Ogt => Olt, Oge => Ole,
    }
}
