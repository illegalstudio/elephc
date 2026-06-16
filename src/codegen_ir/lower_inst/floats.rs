//! Purpose:
//! Lowers floating-point constants, arithmetic, comparisons, and scalar
//! conversions for the Phase 04 EIR backend.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - Integer `/` lowers elsewhere; this module owns EIR operations whose
//!   storage/result representation is already floating-point.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::ir::{CmpPredicate, Instruction};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{
    expect_f64, expect_operand, expect_cmp_predicate, require_float, secondary_float_reg,
    store_if_result, x86_64_float_condition,
};
use crate::codegen_ir::{CodegenIrError, Result};

/// Lowers a floating-point constant into the canonical float result register and slot.
pub(super) fn lower_const_f64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_f64(inst)?;
    let label = ctx.data.add_float(value);
    let scratch = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, scratch, &label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("ldr {}, [{}]", abi::float_result_reg(ctx.emitter), scratch)); // load the 64-bit float literal through the symbol scratch register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("movsd {}, QWORD PTR [{}]", abi::float_result_reg(ctx.emitter), scratch)); // load the 64-bit float literal through the symbol scratch register
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers a floating-point comparison into a boolean result value.
pub(super) fn lower_float_compare(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    let predicate = expect_cmp_predicate(inst)?;
    let lhs_reg = secondary_float_reg(ctx.emitter.target.arch);
    let rhs_reg = abi::float_result_reg(ctx.emitter);
    require_float(ctx.load_value_to_reg(lhs, lhs_reg)?, inst)?;
    require_float(ctx.load_value_to_reg(rhs, rhs_reg)?, inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fcmp d1, d0");                             // compare float operands for the EIR predicate
            ctx.emitter.instruction(&format!("cset x0, {}", aarch64_float_condition(predicate)?)); // materialize the ordered float predicate result as 0 or 1
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("ucomisd xmm1, xmm0");                      // compare float operands for the EIR predicate
            emit_x86_64_float_predicate_result(ctx, predicate)?;
        }
    }
    store_if_result(ctx, inst)
}

/// Returns the AArch64 condition for PHP float comparisons, excluding unordered NaN cases.
fn aarch64_float_condition(predicate: CmpPredicate) -> Result<&'static str> {
    match predicate {
        CmpPredicate::Eq => Ok("eq"),
        CmpPredicate::Ne => Ok("ne"),
        CmpPredicate::Slt | CmpPredicate::Olt => Ok("mi"),
        CmpPredicate::Sle | CmpPredicate::Ole => Ok("ls"),
        CmpPredicate::Sgt | CmpPredicate::Ogt => Ok("gt"),
        CmpPredicate::Sge | CmpPredicate::Oge => Ok("ge"),
    }
}

/// Materializes a PHP float comparison from x86_64 flags, treating unordered as false.
fn emit_x86_64_float_predicate_result(
    ctx: &mut FunctionContext<'_>,
    predicate: CmpPredicate,
) -> Result<()> {
    match predicate {
        CmpPredicate::Ne => {
            ctx.emitter.instruction("setne al");                                // materialize ordered float inequality in the low byte
            ctx.emitter.instruction("setp r10b");                               // materialize unordered NaN comparison as true for !=
            ctx.emitter.instruction("or al, r10b");                             // merge ordered inequality with unordered inequality
        }
        predicate => {
            ctx.emitter.instruction(&format!("set{} al", x86_64_float_condition(predicate)?)); // materialize the ordered float predicate in the low byte
            ctx.emitter.instruction("setnp r10b");                              // materialize whether the comparison was ordered
            ctx.emitter.instruction("and al, r10b");                            // clear ordered predicates for unordered NaN comparisons
        }
    }
    ctx.emitter.instruction("movzx rax, al");                                   // widen the predicate byte into the integer result register
    Ok(())
}

/// Lowers a two-operand floating-point arithmetic instruction.
pub(super) fn lower_float_binop(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    aarch64_mnemonic: &str,
    x86_64_mnemonic: &str,
) -> Result<()> {
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    let lhs_reg = secondary_float_reg(ctx.emitter.target.arch);
    let rhs_reg = abi::float_result_reg(ctx.emitter);
    require_float(ctx.load_value_to_reg(lhs, lhs_reg)?, inst)?;
    require_float(ctx.load_value_to_reg(rhs, rhs_reg)?, inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("{} {}, {}, {}", aarch64_mnemonic, rhs_reg, lhs_reg, rhs_reg)); // compute the floating-point arithmetic result
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("{} {}, {}", x86_64_mnemonic, lhs_reg, rhs_reg)); // update the left float scratch with the arithmetic result
            ctx.emitter.instruction(&format!("movsd {}, {}", rhs_reg, lhs_reg)); // move the float arithmetic result back to the result register
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers floating-point exponentiation through libc `pow`.
pub(super) fn lower_float_pow(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            require_float(ctx.load_value_to_reg(lhs, "d0")?, inst)?;
            require_float(ctx.load_value_to_reg(rhs, "d1")?, inst)?;
            ctx.emitter.bl_c("pow");
        }
        Arch::X86_64 => {
            require_float(ctx.load_value_to_reg(lhs, "xmm0")?, inst)?;
            require_float(ctx.load_value_to_reg(rhs, "xmm1")?, inst)?;
            ctx.emitter.instruction("call pow");                                // compute floating-point exponentiation through libc pow()
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers floating-point negation.
pub(super) fn lower_float_neg(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    require_float(ctx.load_value_to_result(value)?, inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fneg d0, d0");                             // negate the loaded floating-point operand
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("xorpd xmm1, xmm1");                        // materialize a zero float register for subtraction
            ctx.emitter.instruction("subsd xmm1, xmm0");                        // compute 0.0 - operand as floating-point negation
            ctx.emitter.instruction("movsd xmm0, xmm1");                        // move the negated float back to the result register
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers a float-to-int conversion using the target ABI conversion helper.
pub(super) fn lower_float_to_int(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    require_float(ctx.load_value_to_result(value)?, inst)?;
    abi::emit_float_result_to_int_result(ctx.emitter);
    store_if_result(ctx, inst)
}

/// Lowers an integer-like-to-float conversion, treating PHP null as numeric zero.
pub(super) fn lower_int_to_float(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    match ctx.load_value_to_result(value)? {
        PhpType::Int | PhpType::Bool => {}
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "{} for PHP type {:?}",
                inst.op.name(),
                other
            )))
        }
    }
    abi::emit_int_result_to_float_result(ctx.emitter);
    store_if_result(ctx, inst)
}
