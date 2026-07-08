//! Purpose:
//! Lowers libm-backed floating-point PHP builtins for the EIR backend.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::math`.
//!
//! Key details:
//! - Integer-like operands are normalized to the target floating-point result
//!   register before C ABI calls.
//! - Source-order operand evaluation already happened during AST-to-EIR lowering;
//!   this module only rematerializes stored SSA values for ABI calls.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::Instruction;

use super::super::super::super::context::FunctionContext;
use super::super::{expect_operand, store_if_result};

/// Lowers a one-argument libm builtin such as `sin()`, `cos()`, or `exp()`.
pub(crate) fn lower_unary_libm(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 1)?;
    let value = expect_operand(inst, 0)?;
    super::load_numeric_as_float(ctx, value, name)?;
    ctx.emitter.bl_c(name);
    store_if_result(ctx, inst)
}

/// Lowers `atan2()` using the C ABI argument order `y, x`.
pub(crate) fn lower_atan2(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_binary_libm(ctx, inst, "atan2")
}

/// Lowers `hypot()` using the C ABI argument order `x, y`.
pub(crate) fn lower_hypot(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_binary_libm(ctx, inst, "hypot")
}

/// Lowers `log()` in one-argument and base-changing two-argument forms.
pub(crate) fn lower_log(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.is_empty() || inst.operands.len() > 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "log expected 1 or 2 args, got {}",
            inst.operands.len()
        )));
    }
    let value = expect_operand(inst, 0)?;
    super::load_numeric_as_float(ctx, value, "log")?;
    ctx.emitter.bl_c("log");
    if inst.operands.len() == 2 {
        abi::emit_push_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));
        let base = expect_operand(inst, 1)?;
        super::load_numeric_as_float(ctx, base, "log")?;
        ctx.emitter.bl_c("log");
        emit_change_of_base_divide(ctx);
    }
    store_if_result(ctx, inst)
}

/// Lowers `deg2rad()` by multiplying with `PI / 180`.
pub(crate) fn lower_deg2rad(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_angle_conversion(ctx, inst, "deg2rad", std::f64::consts::PI / 180.0)
}

/// Lowers `rad2deg()` by multiplying with `180 / PI`.
pub(crate) fn lower_rad2deg(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_angle_conversion(ctx, inst, "rad2deg", 180.0 / std::f64::consts::PI)
}

/// Lowers a two-argument libm builtin with both operands passed as doubles.
fn lower_binary_libm(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 2)?;
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    super::load_numeric_as_float(ctx, lhs, name)?;
    abi::emit_push_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));
    super::load_numeric_as_float(ctx, rhs, name)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fmov d1, d0");                             // move the second operand into the second libm argument register
            abi::emit_pop_float_reg(ctx.emitter, "d0");
            ctx.emitter.bl_c(name);
        }
        Arch::X86_64 => {
            abi::emit_pop_float_reg(ctx.emitter, "xmm1");
            ctx.emitter.instruction("movapd xmm2, xmm0");                       // preserve the second operand while ordering SysV libm arguments
            ctx.emitter.instruction("movapd xmm0, xmm1");                       // move the first operand into the first libm argument register
            ctx.emitter.instruction("movapd xmm1, xmm2");                       // move the second operand into the second libm argument register
            ctx.emitter.bl_c(name);
        }
    }
    store_if_result(ctx, inst)
}

/// Divides preserved `log(value)` by the active `log(base)` result.
fn emit_change_of_base_divide(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fmov d1, d0");                             // preserve log(base) as the divisor
            abi::emit_pop_float_reg(ctx.emitter, "d0");
            ctx.emitter.instruction("fdiv d0, d0, d1");                         // compute log(value) divided by log(base)
        }
        Arch::X86_64 => {
            abi::emit_pop_float_reg(ctx.emitter, "xmm1");
            ctx.emitter.instruction("divsd xmm1, xmm0");                        // compute log(value) divided by log(base)
            ctx.emitter.instruction("movsd xmm0, xmm1");                        // move the change-of-base result into the result register
        }
    }
}

/// Lowers a one-argument angle conversion as a floating multiply by a data constant.
fn lower_angle_conversion(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    factor: f64,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 1)?;
    let value = expect_operand(inst, 0)?;
    super::load_numeric_as_float(ctx, value, name)?;
    let label = ctx.data.add_float(factor);
    let scratch = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, scratch, &label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("ldr d1, [{}]", scratch));         // load the angle conversion multiplier from the data section
            ctx.emitter.instruction("fmul d0, d0, d1");                         // apply the angle conversion multiplier
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("movsd xmm1, QWORD PTR [{}]", scratch)); // load the angle conversion multiplier from the data section
            ctx.emitter.instruction("mulsd xmm0, xmm1");                        // apply the angle conversion multiplier
        }
    }
    store_if_result(ctx, inst)
}
