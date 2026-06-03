//! Purpose:
//! Lowers simple scalar math builtins for the EIR backend.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - Supports concrete integer/boolean and floating-point operands only.
//! - Mixed PHP comparison semantics stay unsupported until the backend can
//!   materialize and compare boxed `Mixed` values.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::ir::Instruction;
use crate::types::PhpType;

use crate::codegen_ir::{CodegenIrError, Result};

use super::super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};

mod binary;

pub(super) use binary::{lower_fdiv, lower_fmod, lower_intdiv, lower_pow};

/// Lowers `abs()` for concrete integer-like and floating operands.
pub(super) fn lower_abs(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "abs", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.load_value_to_result(value)?.codegen_repr() {
        PhpType::Float => emit_float_abs(ctx),
        PhpType::Int | PhpType::Bool => emit_int_abs(ctx),
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "abs for PHP type {:?}",
                other
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `floor()` for concrete integer-like and floating operands.
pub(super) fn lower_floor(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_float_rounding_builtin(ctx, inst, "floor", "frintm", 1)
}

/// Lowers `ceil()` for concrete integer-like and floating operands.
pub(super) fn lower_ceil(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_float_rounding_builtin(ctx, inst, "ceil", "frintp", 2)
}

/// Lowers `sqrt()` for concrete integer-like and floating operands.
pub(super) fn lower_sqrt(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "sqrt", 1)?;
    let value = expect_operand(inst, 0)?;
    load_numeric_as_float(ctx, value, "sqrt")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fsqrt d0, d0");                            // compute the square root in the floating-point result register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sqrtsd xmm0, xmm0");                       // compute the square root in the floating-point result register
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `round()` for concrete integer-like and floating operands.
pub(super) fn lower_round(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.is_empty() || inst.operands.len() > 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "round expected 1 or 2 args, got {}",
            inst.operands.len()
        )));
    }
    let value = expect_operand(inst, 0)?;
    load_numeric_as_float(ctx, value, "round")?;
    if inst.operands.len() == 1 {
        emit_round_loaded_float(ctx);
    } else {
        emit_round_loaded_float_with_precision(ctx, inst)?;
    }
    store_if_result(ctx, inst)
}

/// Lowers numeric `min()` and `max()` over concrete integer-like or float operands.
pub(super) fn lower_min_max(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    want_max: bool,
) -> Result<()> {
    if inst.operands.is_empty() {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expected at least 1 arg, got 0",
            min_max_name(want_max)
        )));
    }
    let result_ty = inst
        .result
        .map(|value| ctx.value_php_type(value))
        .transpose()?
        .unwrap_or(PhpType::Int)
        .codegen_repr();
    match result_ty {
        PhpType::Float => lower_float_min_max(ctx, inst, want_max)?,
        PhpType::Int | PhpType::Bool => lower_int_min_max(ctx, inst, want_max)?,
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "{} for PHP type {:?}",
                min_max_name(want_max),
                other
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers a one-argument float rounding builtin with target-native instructions.
fn lower_float_rounding_builtin(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    aarch64_op: &str,
    x86_round_mode: u8,
) -> Result<()> {
    ensure_arg_count(inst, name, 1)?;
    let value = expect_operand(inst, 0)?;
    load_numeric_as_float(ctx, value, name)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("{} d0, d0", aarch64_op));         // round the floating-point argument with the builtin's direction
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("roundsd xmm0, xmm0, {}", x86_round_mode)); // round the floating-point argument with the builtin's direction
        }
    }
    store_if_result(ctx, inst)
}

/// Rounds the loaded float to the nearest integer using PHP's ties-away behavior.
fn emit_round_loaded_float(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("frinta d0, d0");                           // round to nearest with ties away from zero
        }
        Arch::X86_64 => {
            ctx.emitter.bl_c("round");
        }
    }
}

/// Rounds the loaded float after applying the optional decimal precision.
fn emit_round_loaded_float_with_precision(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str d0, [sp, #-16]!");                     // preserve the round() value while computing the precision multiplier
            let precision = expect_operand(inst, 1)?;
            load_precision_as_int(ctx, precision, "round")?;
            ctx.emitter.instruction("scvtf d1, x0");                            // convert the precision to a floating exponent for pow()
            ctx.emitter.instruction("str d1, [sp, #-16]!");                     // preserve the exponent while materializing the pow() base
            ctx.emitter.instruction("fmov d0, #10.0");                          // materialize 10.0 as the precision multiplier base
            ctx.emitter.instruction("ldr d1, [sp], #16");                       // restore the exponent into the second pow() argument
            ctx.emitter.bl_c("pow");
            ctx.emitter.instruction("ldr d1, [sp], #16");                       // restore the original value after pow() returns the multiplier
            ctx.emitter.instruction("fmul d1, d1, d0");                         // scale the original value by the precision multiplier
            ctx.emitter.instruction("str d0, [sp, #-16]!");                     // preserve the multiplier for the final division
            ctx.emitter.instruction("frinta d0, d1");                           // round the scaled value with ties away from zero
            ctx.emitter.instruction("ldr d1, [sp], #16");                       // restore the precision multiplier for rescaling
            ctx.emitter.instruction("fdiv d0, d0, d1");                         // scale the rounded value back to the requested precision
        }
        Arch::X86_64 => {
            abi::emit_push_float_reg(ctx.emitter, "xmm0");
            let precision = expect_operand(inst, 1)?;
            load_precision_as_int(ctx, precision, "round")?;
            ctx.emitter.instruction("cvtsi2sd xmm1, rax");                      // convert the precision to a floating exponent for pow()
            ctx.emitter.instruction("mov rax, 0x4024000000000000");             // materialize the IEEE-754 payload for 10.0
            ctx.emitter.instruction("movq xmm0, rax");                          // move 10.0 into the first pow() argument
            ctx.emitter.bl_c("pow");
            abi::emit_pop_float_reg(ctx.emitter, "xmm1");
            ctx.emitter.instruction("mulsd xmm1, xmm0");                        // scale the original value by the precision multiplier
            abi::emit_push_float_reg(ctx.emitter, "xmm0");
            ctx.emitter.instruction("movsd xmm0, xmm1");                        // move the scaled value into the round() argument register
            ctx.emitter.bl_c("round");
            abi::emit_pop_float_reg(ctx.emitter, "xmm1");
            ctx.emitter.instruction("divsd xmm0, xmm1");                        // scale the rounded value back to the requested precision
        }
    }
    Ok(())
}

/// Emits absolute value for the loaded floating-point result.
fn emit_float_abs(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fabs d0, d0");                             // clear the floating-point sign bit in place
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("movq r10, xmm0");                          // copy the floating-point payload for sign-bit masking
            ctx.emitter.instruction("mov r11, 0x7fffffffffffffff");             // materialize the IEEE-754 absolute-value mask
            ctx.emitter.instruction("and r10, r11");                            // clear the sign bit in the copied payload
            ctx.emitter.instruction("movq xmm0, r10");                          // restore the absolute floating-point payload to the result register
        }
    }
}

/// Emits absolute value for the loaded integer result.
fn emit_int_abs(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // test whether the integer result is negative
            ctx.emitter.instruction("cneg x0, x0, lt");                         // negate the result only for negative input
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, rax");                            // copy the integer result before deriving its sign mask
            ctx.emitter.instruction("sar r10, 63");                             // expand the sign bit to an all-zero or all-one mask
            ctx.emitter.instruction("xor rax, r10");                            // flip payload bits when the input was negative
            ctx.emitter.instruction("sub rax, r10");                            // finish two's-complement absolute value
        }
    }
}

/// Loads a numeric operand and normalizes values into the integer result register.
/// Loads a `round()` precision operand as an integer in the result register.
fn load_precision_as_int(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    name: &str,
) -> Result<()> {
    match ctx.load_value_to_result(value)?.codegen_repr() {
        PhpType::Int | PhpType::Bool => Ok(()),
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            Ok(())
        }
        PhpType::Float => {
            abi::emit_float_result_to_int_result(ctx.emitter);
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} precision for PHP type {:?}",
            name, other
        ))),
    }
}

/// Lowers integer-only `min()` / `max()`.
fn lower_int_min_max(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    want_max: bool,
) -> Result<()> {
    let first = expect_operand(inst, 0)?;
    require_int_like(ctx.load_value_to_result(first)?, min_max_name(want_max))?;
    for index in 1..inst.operands.len() {
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        let candidate = expect_operand(inst, index)?;
        require_int_like(ctx.load_value_to_result(candidate)?, min_max_name(want_max))?;
        emit_int_select(ctx, want_max);
    }
    Ok(())
}

/// Lowers floating `min()` / `max()`, promoting integer-like operands as needed.
fn lower_float_min_max(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    want_max: bool,
) -> Result<()> {
    let first = expect_operand(inst, 0)?;
    load_numeric_as_float(ctx, first, min_max_name(want_max))?;
    for index in 1..inst.operands.len() {
        abi::emit_push_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));
        let candidate = expect_operand(inst, index)?;
        load_numeric_as_float(ctx, candidate, min_max_name(want_max))?;
        emit_float_select(ctx, want_max);
    }
    Ok(())
}

/// Loads a numeric operand and normalizes integer-like values into the float result register.
fn load_numeric_as_float(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    name: &str,
) -> Result<()> {
    match ctx.load_value_to_result(value)?.codegen_repr() {
        PhpType::Float => Ok(()),
        PhpType::Int | PhpType::Bool => {
            abi::emit_int_result_to_float_result(ctx.emitter);
            Ok(())
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            abi::emit_int_result_to_float_result(ctx.emitter);
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name, other
        ))),
    }
}

/// Selects the lower or greater integer candidate after the previous result is popped.
fn emit_int_select(ctx: &mut FunctionContext<'_>, want_max: bool) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x1, [sp], #16");                       // restore the previous integer candidate from the temporary stack
            ctx.emitter.instruction("cmp x1, x0");                              // compare the previous and current integer candidates
            let cond = if want_max { "gt" } else { "lt" };
            ctx.emitter.instruction(&format!("csel x0, x1, x0, {}", cond));     // keep the selected integer candidate in the result register
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "r9");
            ctx.emitter.instruction("cmp r9, rax");                             // compare the previous and current integer candidates
            let op = if want_max { "cmovg" } else { "cmovl" };
            ctx.emitter.instruction(&format!("{} rax, r9", op));                // keep the selected integer candidate in the result register
        }
    }
}

/// Selects the lower or greater floating candidate after the previous result is popped.
fn emit_float_select(ctx: &mut FunctionContext<'_>, want_max: bool) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_float_reg(ctx.emitter, "d1");
            let op = if want_max { "fmax" } else { "fmin" };
            ctx.emitter.instruction(&format!("{} d0, d1, d0", op));             // keep the selected floating candidate in the result register
        }
        Arch::X86_64 => {
            abi::emit_pop_float_reg(ctx.emitter, "xmm1");
            let op = if want_max { "maxsd" } else { "minsd" };
            ctx.emitter.instruction(&format!("{} xmm1, xmm0", op));             // combine the previous and current floating candidates
            ctx.emitter.instruction("movsd xmm0, xmm1");                        // move the selected floating candidate into the result register
        }
    }
}

/// Verifies that an operand is represented as an integer-like scalar.
fn require_int_like(ty: PhpType, name: &str) -> Result<()> {
    match ty.codegen_repr() {
        PhpType::Int | PhpType::Bool => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name, other
        ))),
    }
}

/// Verifies that the builtin call has the expected number of lowered operands.
fn ensure_arg_count(inst: &Instruction, name: &str, expected: usize) -> Result<()> {
    if inst.operands.len() == expected {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "{} expected {} args, got {}",
        name,
        expected,
        inst.operands.len()
    )))
}

/// Returns the user-facing builtin name for a min/max lowering branch.
fn min_max_name(want_max: bool) -> &'static str {
    if want_max { "max" } else { "min" }
}
