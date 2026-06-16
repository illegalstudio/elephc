//! Purpose:
//! Lowers simple scalar math builtins for the EIR backend.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - Supports concrete integer/boolean, floating-point, and boxed Mixed numeric operands.
//! - Mixed PHP comparison semantics stay unsupported until the backend can
//!   materialize and compare boxed `Mixed` values.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::ir::Instruction;
use crate::types::PhpType;

use crate::codegen_ir::{CodegenIrError, Result};

use super::super::super::context::FunctionContext;
use super::super::load_value_to_first_int_arg;
use super::{expect_operand, store_if_result};

mod binary;
mod libm;
mod random;

pub(super) use binary::{lower_fdiv, lower_fmod, lower_intdiv, lower_pow};
pub(super) use libm::{
    lower_atan2, lower_deg2rad, lower_hypot, lower_log, lower_rad2deg, lower_unary_libm,
};
pub(super) use random::{lower_rand, lower_random_int};

const CLAMP_MIN_NAN_MESSAGE: &str = "clamp(): Argument #2 ($min) must not be NAN";
const CLAMP_MAX_NAN_MESSAGE: &str = "clamp(): Argument #3 ($max) must not be NAN";
const CLAMP_BOUNDS_MESSAGE: &str =
    "clamp(): Argument #2 ($min) must be smaller than or equal to argument #3 ($max)";
const CLAMP_MAX_SLOT: usize = 0;
const CLAMP_MIN_SLOT: usize = 16;
const CLAMP_VALUE_SLOT: usize = 32;
const CLAMP_STACK_BYTES: usize = 48;

/// Lowers `abs()` for concrete integer-like and floating operands.
pub(super) fn lower_abs(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "abs", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.load_value_to_result(value)?.codegen_repr() {
        PhpType::Float => emit_float_abs(ctx),
        PhpType::Int | PhpType::Bool => emit_int_abs(ctx),
        PhpType::Mixed | PhpType::Union(_) => {
            abi::emit_call_label(ctx.emitter, "__rt_abs_mixed");
        }
        PhpType::TaggedScalar => {
            crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(ctx.emitter);
            emit_int_abs(ctx);
        }
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

/// Lowers numeric `clamp(value, min, max)` calls with PHP-compatible bound checks.
pub(super) fn lower_clamp(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "clamp", 3)?;
    match inst.result_php_type.codegen_repr() {
        PhpType::Int => lower_int_clamp(ctx, inst)?,
        PhpType::Float => lower_float_clamp(ctx, inst)?,
        PhpType::Str => lower_string_clamp(ctx, inst)?,
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "clamp for PHP type {:?}",
                other
            )))
        }
    }
    store_if_result(ctx, inst)
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

/// Lowers `is_nan()` by checking whether the normalized float is unordered with itself.
pub(super) fn lower_is_nan(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "is_nan", 1)?;
    let value = expect_operand(inst, 0)?;
    load_numeric_as_float(ctx, value, "is_nan")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fcmp d0, d0");                             // compare the value against itself so NaN sets the unordered flag
            ctx.emitter.instruction("cset x0, vs");                             // materialize true only for unordered NaN comparisons
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("ucomisd xmm0, xmm0");                      // compare the value against itself so NaN sets the parity flag
            ctx.emitter.instruction("setp al");                                 // materialize true only for unordered NaN comparisons
            ctx.emitter.instruction("movzx rax, al");                           // widen the predicate byte into the integer result register
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `is_infinite()` by comparing the normalized float against +/- infinity.
pub(super) fn lower_is_infinite(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count(inst, "is_infinite", 1)?;
    let value = expect_operand(inst, 0)?;
    load_numeric_as_float(ctx, value, "is_infinite")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fabs d0, d0");                             // make +INF and -INF compare against the same positive infinity constant
            load_float_literal_to_reg(ctx, "d1", f64::INFINITY);
            ctx.emitter.instruction("fcmp d0, d1");                             // compare the absolute value against positive infinity
            ctx.emitter.instruction("cset x0, eq");                             // materialize true only when the value is infinite
        }
        Arch::X86_64 => {
            let not_inf_label = ctx.next_label("is_infinite_false");
            let done_label = ctx.next_label("is_infinite_done");
            ctx.emitter.instruction("ucomisd xmm0, xmm0");                      // compare the value against itself so NaN sets the parity flag
            ctx.emitter.instruction(&format!("jp {}", not_inf_label));          // NaN is unordered against everything, so it is not infinite
            load_float_literal_to_reg(ctx, "xmm1", f64::INFINITY);
            ctx.emitter.instruction("ucomisd xmm0, xmm1");                      // compare the value against positive infinity
            ctx.emitter.instruction("sete al");                                 // remember whether the value equals positive infinity
            load_float_literal_to_reg(ctx, "xmm1", f64::NEG_INFINITY);
            ctx.emitter.instruction("ucomisd xmm0, xmm1");                      // compare the value against negative infinity
            ctx.emitter.instruction("sete cl");                                 // remember whether the value equals negative infinity
            ctx.emitter.instruction("or al, cl");                               // combine the positive and negative infinity checks
            ctx.emitter.instruction("movzx rax, al");                           // widen the infinity boolean into the integer result register
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the NaN false path after checking real infinities
            ctx.emitter.label(&not_inf_label);
            ctx.emitter.instruction("mov rax, 0");                              // NaN is not infinite
            ctx.emitter.label(&done_label);
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `is_finite()` by rejecting NaN and both infinities.
pub(super) fn lower_is_finite(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "is_finite", 1)?;
    let value = expect_operand(inst, 0)?;
    load_numeric_as_float(ctx, value, "is_finite")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fabs d0, d0");                             // make +INF and -INF compare against the same positive infinity constant
            load_float_literal_to_reg(ctx, "d1", f64::INFINITY);
            ctx.emitter.instruction("fcmp d0, d1");                             // finite values compare strictly below positive infinity
            ctx.emitter.instruction("cset x0, mi");                             // materialize true only for non-NaN values below infinity
        }
        Arch::X86_64 => emit_x86_64_is_finite(ctx),
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
        PhpType::Mixed | PhpType::Union(_) => {
            lower_int_min_max(ctx, inst, want_max)?;
            crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
        }
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

/// Emits the x86_64 finite check, including explicit NaN and +/- infinity branches.
fn emit_x86_64_is_finite(ctx: &mut FunctionContext<'_>) {
    let not_finite_label = ctx.next_label("is_finite_false");
    let done_label = ctx.next_label("is_finite_done");
    ctx.emitter.instruction("ucomisd xmm0, xmm0");                              // compare the value against itself so NaN sets the parity flag
    ctx.emitter.instruction(&format!("jp {}", not_finite_label));               // NaN values are not finite
    load_float_literal_to_reg(ctx, "xmm1", f64::INFINITY);
    ctx.emitter.instruction("ucomisd xmm0, xmm1");                              // compare the value against positive infinity
    ctx.emitter.instruction(&format!("je {}", not_finite_label));               // positive infinity is not finite
    load_float_literal_to_reg(ctx, "xmm1", f64::NEG_INFINITY);
    ctx.emitter.instruction("ucomisd xmm0, xmm1");                              // compare the value against negative infinity
    ctx.emitter.instruction(&format!("je {}", not_finite_label));               // negative infinity is not finite
    ctx.emitter.instruction("mov rax, 1");                                      // any remaining non-NaN and non-infinite value is finite
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the false materialization path after confirming finiteness
    ctx.emitter.label(&not_finite_label);
    ctx.emitter.instruction("mov rax, 0");                                      // NaN and infinities return false for is_finite()
    ctx.emitter.label(&done_label);
}

/// Lowers integer `clamp()` by saving arguments and selecting max, min, or value.
fn lower_int_clamp(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    push_int_clamp_args(ctx, inst)?;
    let throw_label = ctx.next_label("clamp_int_invalid_bounds");
    let use_max_label = ctx.next_label("clamp_int_use_max");
    let use_min_label = ctx.next_label("clamp_int_use_min");
    let selected_label = ctx.next_label("clamp_int_selected");
    let finish_label = ctx.next_label("clamp_int_finish");
    let (message_label, message_len) = ctx.data.add_string(CLAMP_BOUNDS_MESSAGE.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            emit_int_clamp_aarch64(ctx, &throw_label, &use_max_label, &use_min_label, &selected_label);
        }
        Arch::X86_64 => {
            emit_int_clamp_x86_64(ctx, &throw_label, &use_max_label, &use_min_label, &selected_label);
        }
    }
    ctx.emitter.label(&selected_label);
    abi::emit_release_temporary_stack(ctx.emitter, CLAMP_STACK_BYTES);
    abi::emit_jump(ctx.emitter, &finish_label);
    ctx.emitter.label(&throw_label);
    abi::emit_release_temporary_stack(ctx.emitter, CLAMP_STACK_BYTES);
    emit_throw_value_error(ctx, &message_label, message_len);
    ctx.emitter.label(&finish_label);
    Ok(())
}

/// Lowers floating `clamp()` by normalizing operands and validating NaN/bound rules.
fn lower_float_clamp(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    push_float_clamp_args(ctx, inst)?;
    let throw_min_nan_label = ctx.next_label("clamp_float_min_nan");
    let throw_max_nan_label = ctx.next_label("clamp_float_max_nan");
    let throw_bounds_label = ctx.next_label("clamp_float_invalid_bounds");
    let use_max_label = ctx.next_label("clamp_float_use_max");
    let use_min_label = ctx.next_label("clamp_float_use_min");
    let in_range_label = ctx.next_label("clamp_float_in_range");
    let selected_label = ctx.next_label("clamp_float_selected");
    let finish_label = ctx.next_label("clamp_float_finish");
    let (min_nan_label, min_nan_len) = ctx.data.add_string(CLAMP_MIN_NAN_MESSAGE.as_bytes());
    let (max_nan_label, max_nan_len) = ctx.data.add_string(CLAMP_MAX_NAN_MESSAGE.as_bytes());
    let (bounds_label, bounds_len) = ctx.data.add_string(CLAMP_BOUNDS_MESSAGE.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_float_clamp_aarch64(
            ctx,
            &throw_min_nan_label,
            &throw_max_nan_label,
            &throw_bounds_label,
            &use_max_label,
            &use_min_label,
            &in_range_label,
            &selected_label,
        ),
        Arch::X86_64 => emit_float_clamp_x86_64(
            ctx,
            &throw_min_nan_label,
            &throw_max_nan_label,
            &throw_bounds_label,
            &use_max_label,
            &use_min_label,
            &in_range_label,
            &selected_label,
        ),
    }
    ctx.emitter.label(&in_range_label);
    abi::emit_jump(ctx.emitter, &selected_label);
    ctx.emitter.label(&selected_label);
    abi::emit_release_temporary_stack(ctx.emitter, CLAMP_STACK_BYTES);
    abi::emit_jump(ctx.emitter, &finish_label);
    ctx.emitter.label(&throw_min_nan_label);
    abi::emit_release_temporary_stack(ctx.emitter, CLAMP_STACK_BYTES);
    emit_throw_value_error(ctx, &min_nan_label, min_nan_len);
    ctx.emitter.label(&throw_max_nan_label);
    abi::emit_release_temporary_stack(ctx.emitter, CLAMP_STACK_BYTES);
    emit_throw_value_error(ctx, &max_nan_label, max_nan_len);
    ctx.emitter.label(&throw_bounds_label);
    abi::emit_release_temporary_stack(ctx.emitter, CLAMP_STACK_BYTES);
    emit_throw_value_error(ctx, &bounds_label, bounds_len);
    ctx.emitter.label(&finish_label);
    Ok(())
}

/// Lowers string `clamp()` with PHP lexicographic ordering and bound validation.
fn lower_string_clamp(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    push_string_clamp_args(ctx, inst)?;
    let throw_label = ctx.next_label("clamp_string_invalid_bounds");
    let use_max_label = ctx.next_label("clamp_string_use_max");
    let use_min_label = ctx.next_label("clamp_string_use_min");
    let selected_label = ctx.next_label("clamp_string_selected");
    let finish_label = ctx.next_label("clamp_string_finish");
    let (message_label, message_len) = ctx.data.add_string(CLAMP_BOUNDS_MESSAGE.as_bytes());
    emit_compare_string_slots(ctx, CLAMP_MIN_SLOT, CLAMP_MAX_SLOT);
    emit_branch_if_string_compare_gt(ctx, &throw_label);
    emit_compare_string_slots(ctx, CLAMP_VALUE_SLOT, CLAMP_MAX_SLOT);
    emit_branch_if_string_compare_gt(ctx, &use_max_label);
    emit_compare_string_slots(ctx, CLAMP_VALUE_SLOT, CLAMP_MIN_SLOT);
    emit_branch_if_string_compare_lt(ctx, &use_min_label);
    emit_load_string_slot_to_result(ctx, CLAMP_VALUE_SLOT);
    abi::emit_jump(ctx.emitter, &selected_label);
    ctx.emitter.label(&use_max_label);
    emit_load_string_slot_to_result(ctx, CLAMP_MAX_SLOT);
    abi::emit_jump(ctx.emitter, &selected_label);
    ctx.emitter.label(&use_min_label);
    emit_load_string_slot_to_result(ctx, CLAMP_MIN_SLOT);
    ctx.emitter.label(&selected_label);
    abi::emit_release_temporary_stack(ctx.emitter, CLAMP_STACK_BYTES);
    abi::emit_jump(ctx.emitter, &finish_label);
    ctx.emitter.label(&throw_label);
    abi::emit_release_temporary_stack(ctx.emitter, CLAMP_STACK_BYTES);
    emit_throw_value_error(ctx, &message_label, message_len);
    ctx.emitter.label(&finish_label);
    Ok(())
}

/// Evaluates and saves integer clamp operands in value, min, max order.
fn push_int_clamp_args(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    for index in 0..3 {
        let value = expect_operand(inst, index)?;
        require_int_like(ctx.load_value_to_result(value)?, "clamp")?;
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    }
    Ok(())
}

/// Evaluates and saves string clamp operands in value, min, max order.
fn push_string_clamp_args(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    for index in 0..3 {
        let value = expect_operand(inst, index)?;
        match ctx.load_value_to_result(value)?.codegen_repr() {
            PhpType::Str => {
                let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
                abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
            }
            other => {
                return Err(CodegenIrError::unsupported(format!(
                    "clamp string operand PHP type {:?}",
                    other
                )))
            }
        }
    }
    Ok(())
}

/// Evaluates and saves floating clamp operands in value, min, max order.
fn push_float_clamp_args(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    for index in 0..3 {
        let value = expect_operand(inst, index)?;
        load_numeric_as_float(ctx, value, "clamp")?;
        abi::emit_push_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));
    }
    Ok(())
}

/// Emits the AArch64 integer clamp selection and bound validation.
fn emit_int_clamp_aarch64(
    ctx: &mut FunctionContext<'_>,
    throw_label: &str,
    use_max_label: &str,
    use_min_label: &str,
    selected_label: &str,
) {
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", CLAMP_MIN_SLOT);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x10", CLAMP_MAX_SLOT);
    ctx.emitter.instruction("cmp x9, x10");                                     // validate that clamp's lower integer bound does not exceed the upper bound
    ctx.emitter.instruction(&format!("b.gt {}", throw_label));                  // throw ValueError when clamp min is greater than max
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", CLAMP_VALUE_SLOT);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x10", CLAMP_MAX_SLOT);
    ctx.emitter.instruction("cmp x9, x10");                                     // compare the candidate integer against the upper bound first
    ctx.emitter.instruction(&format!("b.gt {}", use_max_label));                // choose the upper bound when the candidate is too large
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x10", CLAMP_MIN_SLOT);
    ctx.emitter.instruction("cmp x9, x10");                                     // compare the candidate integer against the lower bound second
    ctx.emitter.instruction(&format!("b.lt {}", use_min_label));                // choose the lower bound when the candidate is too small
    ctx.emitter.instruction("mov x0, x9");                                      // keep the original integer candidate when it is in range
    abi::emit_jump(ctx.emitter, selected_label);
    ctx.emitter.label(use_max_label);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", CLAMP_MAX_SLOT);
    abi::emit_jump(ctx.emitter, selected_label);
    ctx.emitter.label(use_min_label);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", CLAMP_MIN_SLOT);
}

/// Emits the x86_64 integer clamp selection and bound validation.
fn emit_int_clamp_x86_64(
    ctx: &mut FunctionContext<'_>,
    throw_label: &str,
    use_max_label: &str,
    use_min_label: &str,
    selected_label: &str,
) {
    abi::emit_load_temporary_stack_slot(ctx.emitter, "r9", CLAMP_MIN_SLOT);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", CLAMP_MAX_SLOT);
    ctx.emitter.instruction("cmp r9, r10");                                     // validate that clamp's lower integer bound does not exceed the upper bound
    ctx.emitter.instruction(&format!("jg {}", throw_label));                    // throw ValueError when clamp min is greater than max
    abi::emit_load_temporary_stack_slot(ctx.emitter, "r9", CLAMP_VALUE_SLOT);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", CLAMP_MAX_SLOT);
    ctx.emitter.instruction("cmp r9, r10");                                     // compare the candidate integer against the upper bound first
    ctx.emitter.instruction(&format!("jg {}", use_max_label));                  // choose the upper bound when the candidate is too large
    abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", CLAMP_MIN_SLOT);
    ctx.emitter.instruction("cmp r9, r10");                                     // compare the candidate integer against the lower bound second
    ctx.emitter.instruction(&format!("jl {}", use_min_label));                  // choose the lower bound when the candidate is too small
    ctx.emitter.instruction("mov rax, r9");                                     // keep the original integer candidate when it is in range
    abi::emit_jump(ctx.emitter, selected_label);
    ctx.emitter.label(use_max_label);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", CLAMP_MAX_SLOT);
    abi::emit_jump(ctx.emitter, selected_label);
    ctx.emitter.label(use_min_label);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", CLAMP_MIN_SLOT);
}

/// Emits the AArch64 floating clamp selection and bound validation.
fn emit_float_clamp_aarch64(
    ctx: &mut FunctionContext<'_>,
    throw_min_nan_label: &str,
    throw_max_nan_label: &str,
    throw_bounds_label: &str,
    use_max_label: &str,
    use_min_label: &str,
    in_range_label: &str,
    selected_label: &str,
) {
    abi::emit_load_temporary_stack_slot(ctx.emitter, "d1", CLAMP_MIN_SLOT);
    ctx.emitter.instruction("fcmp d1, d1");                                     // detect NaN in clamp's lower floating bound before range comparisons
    ctx.emitter.instruction(&format!("b.vs {}", throw_min_nan_label));          // throw ValueError for a NaN lower bound
    abi::emit_load_temporary_stack_slot(ctx.emitter, "d2", CLAMP_MAX_SLOT);
    ctx.emitter.instruction("fcmp d2, d2");                                     // detect NaN in clamp's upper floating bound before range comparisons
    ctx.emitter.instruction(&format!("b.vs {}", throw_max_nan_label));          // throw ValueError for a NaN upper bound
    ctx.emitter.instruction("fcmp d1, d2");                                     // validate that the lower floating bound does not exceed the upper bound
    ctx.emitter.instruction(&format!("b.gt {}", throw_bounds_label));           // throw ValueError when clamp min is greater than max
    abi::emit_load_temporary_stack_slot(ctx.emitter, "d0", CLAMP_VALUE_SLOT);
    ctx.emitter.instruction("fcmp d0, d2");                                     // compare the candidate float against the upper bound first
    ctx.emitter.instruction(&format!("b.vs {}", in_range_label));               // leave a NaN candidate unclamped because only bounds reject NaN
    ctx.emitter.instruction(&format!("b.gt {}", use_max_label));                // choose the upper bound when the candidate is too large
    ctx.emitter.instruction("fcmp d0, d1");                                     // compare the candidate float against the lower bound second
    ctx.emitter.instruction(&format!("b.vs {}", in_range_label));               // leave a NaN candidate unclamped after the lower-bound comparison too
    ctx.emitter.instruction(&format!("b.lt {}", use_min_label));                // choose the lower bound when the candidate is too small
    abi::emit_jump(ctx.emitter, in_range_label);
    ctx.emitter.label(use_max_label);
    ctx.emitter.instruction("fmov d0, d2");                                     // return the upper floating bound when the candidate is too large
    abi::emit_jump(ctx.emitter, selected_label);
    ctx.emitter.label(use_min_label);
    ctx.emitter.instruction("fmov d0, d1");                                     // return the lower floating bound when the candidate is too small
}

/// Emits the x86_64 floating clamp selection and bound validation.
fn emit_float_clamp_x86_64(
    ctx: &mut FunctionContext<'_>,
    throw_min_nan_label: &str,
    throw_max_nan_label: &str,
    throw_bounds_label: &str,
    use_max_label: &str,
    use_min_label: &str,
    in_range_label: &str,
    selected_label: &str,
) {
    abi::emit_load_temporary_stack_slot(ctx.emitter, "xmm1", CLAMP_MIN_SLOT);
    ctx.emitter.instruction("ucomisd xmm1, xmm1");                              // detect NaN in clamp's lower floating bound before range comparisons
    ctx.emitter.instruction(&format!("jp {}", throw_min_nan_label));            // throw ValueError for a NaN lower bound
    abi::emit_load_temporary_stack_slot(ctx.emitter, "xmm2", CLAMP_MAX_SLOT);
    ctx.emitter.instruction("ucomisd xmm2, xmm2");                              // detect NaN in clamp's upper floating bound before range comparisons
    ctx.emitter.instruction(&format!("jp {}", throw_max_nan_label));            // throw ValueError for a NaN upper bound
    ctx.emitter.instruction("ucomisd xmm1, xmm2");                              // validate that the lower floating bound does not exceed the upper bound
    ctx.emitter.instruction(&format!("ja {}", throw_bounds_label));             // throw ValueError when clamp min is greater than max
    abi::emit_load_temporary_stack_slot(ctx.emitter, "xmm0", CLAMP_VALUE_SLOT);
    ctx.emitter.instruction("ucomisd xmm0, xmm2");                              // compare the candidate float against the upper bound first
    ctx.emitter.instruction(&format!("jp {}", in_range_label));                 // leave a NaN candidate unclamped because only bounds reject NaN
    ctx.emitter.instruction(&format!("ja {}", use_max_label));                  // choose the upper bound when the candidate is too large
    ctx.emitter.instruction("ucomisd xmm0, xmm1");                              // compare the candidate float against the lower bound second
    ctx.emitter.instruction(&format!("jp {}", in_range_label));                 // leave a NaN candidate unclamped after the lower-bound comparison too
    ctx.emitter.instruction(&format!("jb {}", use_min_label));                  // choose the lower bound when the candidate is too small
    abi::emit_jump(ctx.emitter, in_range_label);
    ctx.emitter.label(use_max_label);
    ctx.emitter.instruction("movsd xmm0, xmm2");                                // return the upper floating bound when the candidate is too large
    abi::emit_jump(ctx.emitter, selected_label);
    ctx.emitter.label(use_min_label);
    ctx.emitter.instruction("movsd xmm0, xmm1");                                // return the lower floating bound when the candidate is too small
}

/// Compares two saved string slots and leaves the runtime comparison integer active.
fn emit_compare_string_slots(
    ctx: &mut FunctionContext<'_>,
    left_offset: usize,
    right_offset: usize,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", left_offset);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", left_offset + 8);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x3", right_offset);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x4", right_offset + 8);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", left_offset);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", left_offset + 8);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdx", right_offset);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rcx", right_offset + 8);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_strcmp");
}

/// Branches to `label` when the latest string comparison result is greater than zero.
fn emit_branch_if_string_compare_gt(ctx: &mut FunctionContext<'_>, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // test whether the left clamp string sorts after the right string
            ctx.emitter.instruction(&format!("b.gt {}", label));                // branch when the string comparison result is positive
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 0");                              // test whether the left clamp string sorts after the right string
            ctx.emitter.instruction(&format!("jg {}", label));                  // branch when the string comparison result is positive
        }
    }
}

/// Branches to `label` when the latest string comparison result is less than zero.
fn emit_branch_if_string_compare_lt(ctx: &mut FunctionContext<'_>, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // test whether the left clamp string sorts before the right string
            ctx.emitter.instruction(&format!("b.lt {}", label));                // branch when the string comparison result is negative
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 0");                              // test whether the left clamp string sorts before the right string
            ctx.emitter.instruction(&format!("jl {}", label));                  // branch when the string comparison result is negative
        }
    }
}

/// Loads a saved string slot into the target string result registers.
fn emit_load_string_slot_to_result(ctx: &mut FunctionContext<'_>, offset: usize) {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, ptr_reg, offset);
    abi::emit_load_temporary_stack_slot(ctx.emitter, len_reg, offset + 8);
}

/// Emits a catchable `ValueError` using a static message string.
fn emit_throw_value_error(
    ctx: &mut FunctionContext<'_>,
    message_symbol: &str,
    message_len: usize,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_throw_value_error_aarch64(ctx, message_symbol, message_len),
        Arch::X86_64 => emit_throw_value_error_x86_64(ctx, message_symbol, message_len),
    }
}

/// Emits the AArch64 allocation and unwinder handoff for a `ValueError`.
fn emit_throw_value_error_aarch64(
    ctx: &mut FunctionContext<'_>,
    message_symbol: &str,
    message_len: usize,
) {
    ctx.emitter.instruction("mov x0, #32");                                     // request Throwable payload storage for the clamp ValueError
    ctx.emitter.instruction("bl __rt_heap_alloc");                              // allocate the ValueError object payload
    ctx.emitter.instruction("mov x9, #6");                                      // heap kind 6 marks an object instance allocation
    ctx.emitter.instruction("str x9, [x0, #-8]");                               // stamp the allocation header as a runtime object
    abi::emit_symbol_address(ctx.emitter, "x9", "_spl_value_error_class_id");
    ctx.emitter.instruction("ldr x9, [x9]");                                    // load ValueError's runtime class id for this program
    ctx.emitter.instruction("str x9, [x0]");                                    // store the ValueError class id in the Throwable header
    abi::emit_symbol_address(ctx.emitter, "x9", message_symbol);
    ctx.emitter.instruction("str x9, [x0, #8]");                                // store the static ValueError message pointer
    ctx.emitter.instruction(&format!("mov x9, #{}", message_len));              // materialize the static ValueError message length
    ctx.emitter.instruction("str x9, [x0, #16]");                               // store the exception message length
    ctx.emitter.instruction("str xzr, [x0, #24]");                              // store the default zero exception code
    abi::emit_symbol_address(ctx.emitter, "x9", "_exc_value");
    ctx.emitter.instruction("str x0, [x9]");                                    // publish the active ValueError object
    ctx.emitter.instruction("b __rt_throw_current");                            // enter the standard exception unwinder
}

/// Emits the x86_64 allocation and unwinder handoff for a `ValueError`.
fn emit_throw_value_error_x86_64(
    ctx: &mut FunctionContext<'_>,
    message_symbol: &str,
    message_len: usize,
) {
    ctx.emitter.instruction("push rbp");                                        // preserve caller frame pointer for exception allocation
    ctx.emitter.instruction("mov rbp, rsp");                                    // establish an aligned helper frame for heap allocation
    ctx.emitter.instruction("sub rsp, 16");                                     // keep the nested heap allocation call 16-byte aligned
    ctx.emitter.instruction("mov rax, 32");                                     // request Throwable payload storage for the clamp ValueError
    ctx.emitter.instruction("call __rt_heap_alloc");                            // allocate the ValueError object payload
    ctx.emitter.instruction("mov r10, 0x4548504c00000006");                     // materialize the x86_64 object heap-kind header
    ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");                    // stamp the allocation header as a runtime object
    ctx.emitter.instruction("mov r10, QWORD PTR [rip + _spl_value_error_class_id]"); // load ValueError's runtime class id for this program
    ctx.emitter.instruction("mov QWORD PTR [rax], r10");                        // store the ValueError class id in the Throwable header
    ctx.emitter.instruction(&format!("lea r10, [rip + {}]", message_symbol));   // materialize the static ValueError message pointer
    ctx.emitter.instruction("mov QWORD PTR [rax + 8], r10");                    // store the static ValueError message pointer
    ctx.emitter.instruction(&format!("mov QWORD PTR [rax + 16], {}", message_len)); // store the exception message length
    ctx.emitter.instruction("mov QWORD PTR [rax + 24], 0");                     // store the default zero exception code
    ctx.emitter.instruction("mov QWORD PTR [rip + _exc_value], rax");           // publish the active ValueError object
    ctx.emitter.instruction("mov rsp, rbp");                                    // release the helper frame before throwing
    ctx.emitter.instruction("pop rbp");                                         // restore caller frame pointer before throwing
    ctx.emitter.instruction("jmp __rt_throw_current");                          // enter the standard exception unwinder
}

/// Loads a floating-point literal into `reg` through the data section.
fn load_float_literal_to_reg(ctx: &mut FunctionContext<'_>, reg: &str, value: f64) {
    let label = ctx.data.add_float(value);
    let scratch = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, scratch, &label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("ldr {}, [{}]", reg, scratch));    // load the floating-point comparison constant through the symbol scratch register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("movsd {}, QWORD PTR [{}]", reg, scratch)); // load the floating-point comparison constant through the symbol scratch register
        }
    }
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
    load_numeric_as_int(ctx, first, min_max_name(want_max))?;
    for index in 1..inst.operands.len() {
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        let candidate = expect_operand(inst, index)?;
        load_numeric_as_int(ctx, candidate, min_max_name(want_max))?;
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
        PhpType::TaggedScalar => {
            crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(ctx.emitter);
            abi::emit_int_result_to_float_result(ctx.emitter);
            Ok(())
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            abi::emit_int_result_to_float_result(ctx.emitter);
            Ok(())
        }
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float");
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

/// Loads a numeric operand and normalizes integer-like values into the integer result register.
fn load_numeric_as_int(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    name: &str,
) -> Result<()> {
    match ctx.load_value_to_result(value)?.codegen_repr() {
        PhpType::Int | PhpType::Bool => Ok(()),
        PhpType::TaggedScalar => {
            crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(ctx.emitter);
            Ok(())
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            Ok(())
        }
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name, other
        ))),
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
