//! Purpose:
//! Lowers integer arithmetic, bitwise, shift, and integer-to-float division EIR
//! opcodes for the Phase 04 stack-slot backend.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - The lowering mirrors legacy backend scalar semantics and keeps all target
//!   register choices behind ABI helpers where shared helpers exist.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::ir::{Immediate, Instruction, MixedNumericOp, ValueId};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{expect_operand, require_integer_like, store_if_result};
use crate::codegen::{CodegenIrError, Result};

/// Lowers a two-operand integer arithmetic or bitwise instruction.
pub(super) fn lower_int_binop(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    aarch64_mnemonic: &str,
    x86_64_mnemonic: &str,
) -> Result<()> {
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let rhs_reg = abi::secondary_scratch_reg(ctx.emitter);
    load_integer_operand(ctx, lhs, result_reg, inst)?;
    load_integer_operand(ctx, rhs, rhs_reg, inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("{} {}, {}, {}", aarch64_mnemonic, result_reg, result_reg, rhs_reg)); // compute the integer arithmetic result from both SSA operands
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("{} {}, {}", x86_64_mnemonic, result_reg, rhs_reg)); // update the integer result register with the arithmetic operand
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers a signed integer modulo operation with the legacy backend's zero-divisor guard.
pub(super) fn lower_int_mod(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let rhs_reg = abi::secondary_scratch_reg(ctx.emitter);
    load_integer_operand(ctx, lhs, result_reg, inst)?;
    load_integer_operand(ctx, rhs, rhs_reg, inst)?;
    let zero_label = ctx.next_label("mod_zero");
    let done_label = ctx.next_label("mod_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let quotient_reg = abi::tertiary_scratch_reg(ctx.emitter);
            ctx.emitter.instruction(&format!("cbz {}, {}", rhs_reg, zero_label)); // branch to zero-divisor guard when modulo divisor is zero
            ctx.emitter.instruction(&format!("sdiv {}, {}, {}", quotient_reg, result_reg, rhs_reg)); // compute signed quotient for the modulo operation
            ctx.emitter.instruction(&format!("msub {}, {}, {}, {}", result_reg, quotient_reg, rhs_reg, result_reg)); // compute left - quotient * right as the remainder
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the modulo zero fallback after a normal remainder
            ctx.emitter.label(&zero_label);
            ctx.emitter.instruction(&format!("mov {}, #0", result_reg));        // return zero for modulo by zero to match the legacy backend
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {}, {}", rhs_reg, rhs_reg)); // test whether the modulo divisor is zero
            ctx.emitter.instruction(&format!("je {}", zero_label));             // branch to zero-divisor guard when modulo divisor is zero
            ctx.emitter.instruction("cqo");                                     // sign-extend the dividend before signed division
            ctx.emitter.instruction(&format!("idiv {}", rhs_reg));              // divide signed integers with quotient in rax and remainder in rdx
            ctx.emitter.instruction(&format!("mov {}, rdx", result_reg));       // move the signed remainder into the integer result register
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the modulo zero fallback after a normal remainder
            ctx.emitter.label(&zero_label);
            ctx.emitter.instruction(&format!("mov {}, 0", result_reg));         // return zero for modulo by zero to match the legacy backend
            ctx.emitter.label(&done_label);
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers PHP `/` for integer operands by promoting both sides to floating point.
pub(super) fn lower_int_div_to_float(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    let lhs_reg = abi::secondary_scratch_reg(ctx.emitter);
    let rhs_reg = abi::tertiary_scratch_reg(ctx.emitter);
    load_integer_operand(ctx, lhs, lhs_reg, inst)?;
    load_integer_operand(ctx, rhs, rhs_reg, inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("scvtf d0, {}", lhs_reg));         // promote the integer dividend into the float result register
            ctx.emitter.instruction(&format!("scvtf d1, {}", rhs_reg));         // promote the integer divisor into a float scratch register
            ctx.emitter.instruction("fdiv d0, d0, d1");                         // divide promoted operands as PHP floating-point division
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cvtsi2sd xmm0, {}", lhs_reg));    // promote the integer dividend into the float result register
            ctx.emitter.instruction(&format!("cvtsi2sd xmm1, {}", rhs_reg));    // promote the integer divisor into a float scratch register
            ctx.emitter.instruction("divsd xmm0, xmm1");                        // divide promoted operands as PHP floating-point division
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers a single-operand integer instruction.
pub(super) fn lower_int_unary(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    aarch64_mnemonic: &str,
    x86_64_mnemonic: &str,
) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    load_integer_operand(ctx, value, result_reg, inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("{} {}, {}", aarch64_mnemonic, result_reg, result_reg)); // apply the integer unary operation to the loaded operand
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("{} {}", x86_64_mnemonic, result_reg)); // apply the integer unary operation to the loaded operand
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers a variable-count signed integer shift operation.
pub(super) fn lower_int_shift(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    aarch64_mnemonic: &str,
    x86_64_mnemonic: &str,
) -> Result<()> {
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let rhs_reg = abi::secondary_scratch_reg(ctx.emitter);
    load_integer_operand(ctx, lhs, result_reg, inst)?;
    load_integer_operand(ctx, rhs, rhs_reg, inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("{} {}, {}, {}", aarch64_mnemonic, result_reg, result_reg, rhs_reg)); // shift the integer operand by the EIR count operand
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov rcx, {}", rhs_reg));          // move the variable shift count into x86_64's required cl register
            ctx.emitter.instruction(&format!("{} {}, cl", x86_64_mnemonic, result_reg)); // shift the integer operand by the low count byte
        }
    }
    store_if_result(ctx, inst)
}

/// Loads an integer arithmetic operand, coercing PHP null to integer zero.
fn load_integer_operand(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    reg: &str,
    inst: &Instruction,
) -> Result<()> {
    match ctx.value_php_type(value)? {
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, reg, 0);
            Ok(())
        }
        _ => {
            require_integer_like(ctx.load_value_to_reg(value, reg)?, inst)?;
            Ok(())
        }
    }
}

/// Lowers a dynamic mixed numeric add/sub/mul through the boxed-Mixed runtime helpers.
pub(super) fn lower_mixed_numeric_binop(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    let op = expect_mixed_numeric_op(inst)?;
    let lhs_ty = ctx.value_php_type(lhs)?;
    let rhs_ty = ctx.value_php_type(rhs)?;
    let left_box_temp = !is_mixed_like(&lhs_ty);
    let right_box_temp = !is_mixed_like(&rhs_ty);

    materialize_value_as_mixed(ctx, lhs, &lhs_ty)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    materialize_value_as_mixed(ctx, rhs, &rhs_ty)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", 0);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 0);
        }
    }
    abi::emit_call_label(ctx.emitter, mixed_numeric_helper(op));
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    if left_box_temp {
        decref_mixed_temp_at(ctx, 32);
    }
    if right_box_temp {
        decref_mixed_temp_at(ctx, 16);
    }
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    store_if_result(ctx, inst)
}

/// Returns true when a PHP type is already represented as a boxed Mixed pointer.
fn is_mixed_like(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::Mixed)
}

/// Loads an SSA value as a boxed Mixed pointer in the integer result register.
fn materialize_value_as_mixed(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    ty: &PhpType,
) -> Result<()> {
    let ty = ty.codegen_repr();
    if is_mixed_like(&ty) {
        ctx.load_value_to_result(value)?;
        return Ok(());
    }
    match ty {
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        _ => {
            ctx.load_value_to_result(value)?;
        }
    }
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &ty);
    Ok(())
}

/// Releases a temporary Mixed box saved on the temporary stack.
fn decref_mixed_temp_at(ctx: &mut FunctionContext<'_>, offset: usize) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", offset);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", offset);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
}

/// Returns the mixed numeric operation immediate attached to the EIR instruction.
fn expect_mixed_numeric_op(inst: &Instruction) -> Result<MixedNumericOp> {
    match inst.immediate {
        Some(Immediate::MixedNumericOp(op)) => Ok(op),
        _ => Err(CodegenIrError::invalid_module(format!(
            "{} missing mixed numeric op immediate",
            inst.op.name()
        ))),
    }
}

/// Maps a mixed numeric operation to the target-aware runtime helper label.
fn mixed_numeric_helper(op: MixedNumericOp) -> &'static str {
    match op {
        MixedNumericOp::Add => "__rt_mixed_numeric_add",
        MixedNumericOp::Sub => "__rt_mixed_numeric_sub",
        MixedNumericOp::Mul => "__rt_mixed_numeric_mul",
    }
}
