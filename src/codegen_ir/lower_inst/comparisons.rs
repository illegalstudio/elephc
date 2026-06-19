//! Purpose:
//! Lowers scalar equality EIR opcodes for the Phase 04 backend.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - Strict equality respects static PHP type identity before comparing payloads.
//! - Mixed strict equality boxes concrete operands and delegates tag/payload comparison
//!   to the shared runtime helper.
//! - Loose equality mirrors the legacy scalar paths for int/bool/null/string
//!   combinations and delegates string numeric parsing to shared runtime helpers.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{expect_operand, secondary_float_reg, store_if_result};
use crate::codegen_ir::{CodegenIrError, Result};

/// Lowers ordered string comparison (`<`, `<=`, `>`, `>=`) via lexicographic `__rt_strcmp`.
///
/// Loads both operands into the runtime comparator's string registers, calls `__rt_strcmp`
/// (result `< 0` / `0` / `> 0`), then reduces it to a PHP boolean by applying the EIR
/// comparison predicate against zero — mirroring the integer-compare path. Comparison is by
/// byte sequence and length; PHP's numeric-string ordering rule is not applied here (the
/// synthetic date/time methods compare single format characters, for which the two agree).
pub(super) fn lower_str_cmp(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    let predicate = super::expect_cmp_predicate(inst)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_string_value_to_regs(lhs, "x1", "x2")?;
            ctx.load_string_value_to_regs(rhs, "x3", "x4")?;
            abi::emit_call_label(ctx.emitter, "__rt_strcmp");
            ctx.emitter
                .instruction(&format!("cmp {}, #0", result_reg));               // compare the lexicographic result against zero
            ctx.emitter.instruction(&format!(
                "cset {}, {}",
                result_reg,
                super::aarch64_condition(predicate)?
            )); // materialize the ordered predicate as 0 or 1
        }
        Arch::X86_64 => {
            ctx.load_string_value_to_regs(lhs, "rdi", "rsi")?;
            ctx.load_string_value_to_regs(rhs, "rdx", "rcx")?;
            abi::emit_call_label(ctx.emitter, "__rt_strcmp");
            ctx.emitter
                .instruction(&format!("cmp {}, 0", result_reg));                // compare the lexicographic result against zero
            ctx.emitter
                .instruction(&format!("set{} al", super::x86_64_condition(predicate)?)); // materialize the ordered predicate in the low byte
            ctx.emitter
                .instruction(&format!("movzx {}, al", result_reg));             // widen the predicate byte into the integer result register
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers strict equality or inequality for scalar values.
pub(super) fn lower_strict_eq(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    is_equal: bool,
) -> Result<()> {
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    let lhs_ty = ctx.value_php_type(lhs)?.codegen_repr();
    let rhs_ty = ctx.value_php_type(rhs)?.codegen_repr();
    if needs_mixed_strict_compare(&lhs_ty) || needs_mixed_strict_compare(&rhs_ty) {
        emit_mixed_strict_compare(ctx, lhs, &lhs_ty, rhs, &rhs_ty, is_equal)?;
        return store_if_result(ctx, inst);
    }
    if matches!(
        (&lhs_ty, &rhs_ty),
        (PhpType::Object(_), PhpType::Object(_))
            | (PhpType::Iterable, PhpType::Iterable)
            | (PhpType::Pointer(_), PhpType::Pointer(_))
    ) {
        emit_pointer_compare(ctx, lhs, rhs, is_equal)?;
        return store_if_result(ctx, inst);
    }
    if lhs_ty != rhs_ty {
        emit_bool_literal(ctx, !is_equal);
        return store_if_result(ctx, inst);
    }
    match lhs_ty {
        PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Never => {
            emit_intish_compare(ctx, lhs, rhs, is_equal, false)?;
        }
        PhpType::Float => {
            emit_float_compare(ctx, lhs, rhs, is_equal)?;
        }
        PhpType::Str => {
            emit_string_eq_call(ctx, lhs, rhs, is_equal, "__rt_str_eq")?;
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "{} for PHP type {:?}",
                inst.op.name(),
                other
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// Emits a pointer-like identity comparison for strict equality.
fn emit_pointer_compare(
    ctx: &mut FunctionContext<'_>,
    lhs: ValueId,
    rhs: ValueId,
    is_equal: bool,
) -> Result<()> {
    let lhs_reg = abi::secondary_scratch_reg(ctx.emitter);
    let rhs_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(lhs, lhs_reg)?;
    ctx.load_value_to_reg(rhs, rhs_reg)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", lhs_reg, rhs_reg));  // compare pointer-like payloads for PHP strict identity
            ctx.emitter.instruction(&format!("cset x0, {}", equality_cond(is_equal, ctx.emitter.target.arch))); // materialize pointer identity as a boolean
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", lhs_reg, rhs_reg));  // compare pointer-like payloads for PHP strict identity
            ctx.emitter.instruction(&format!("set{} al", equality_cond(is_equal, ctx.emitter.target.arch))); // materialize pointer identity in the low byte
            ctx.emitter.instruction("movzx rax, al");                           // widen the pointer identity byte into the integer result register
        }
    }
    Ok(())
}

/// Returns true for boxed runtime payloads that need mixed-aware comparison.
fn is_mixed_like(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::Mixed)
}

/// Returns true when strict equality needs runtime tag comparison through Mixed boxing.
fn needs_mixed_strict_compare(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::TaggedScalar)
}

/// Compares a mixed operand against another mixed or concrete operand using runtime tags.
fn emit_mixed_strict_compare(
    ctx: &mut FunctionContext<'_>,
    lhs: ValueId,
    lhs_ty: &PhpType,
    rhs: ValueId,
    rhs_ty: &PhpType,
    is_equal: bool,
) -> Result<()> {
    let left_box_temp = !is_mixed_like(lhs_ty);
    let right_box_temp = !is_mixed_like(rhs_ty);
    materialize_value_as_mixed(ctx, lhs, lhs_ty)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    materialize_value_as_mixed(ctx, rhs, rhs_ty)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", 0);
            abi::emit_call_label(ctx.emitter, "__rt_mixed_strict_eq");
            if !is_equal {
                ctx.emitter.instruction("eor x0, x0, #1");                      // invert the mixed strict-equality result for !==
            }
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", 0);
            abi::emit_call_label(ctx.emitter, "__rt_mixed_strict_eq");
            if !is_equal {
                ctx.emitter.instruction("xor rax, 1");                          // invert the mixed strict-equality result for !==
            }
        }
    }
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    if left_box_temp {
        decref_mixed_temp_at(ctx, 32);
    }
    if right_box_temp {
        decref_mixed_temp_at(ctx, 16);
    }
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    Ok(())
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
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                crate::codegen::sentinels::NULL_SENTINEL,
            );
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

/// Lowers loose equality or inequality for scalar int/bool/null and string pairs.
pub(super) fn lower_loose_eq(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    is_equal: bool,
) -> Result<()> {
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    let lhs_ty = ctx.value_php_type(lhs)?;
    let rhs_ty = ctx.value_php_type(rhs)?;
    if lhs_ty == PhpType::Str && rhs_ty == PhpType::Str {
        emit_string_eq_call(ctx, lhs, rhs, is_equal, "__rt_str_loose_eq")?;
    } else if string_bool_comparable(&lhs_ty, &rhs_ty) {
        emit_bool_string_loose_eq(ctx, lhs, &lhs_ty, rhs, &rhs_ty, is_equal)?;
    } else if string_null_comparable(&lhs_ty, &rhs_ty) {
        emit_null_string_loose_eq(ctx, lhs, &lhs_ty, rhs, &rhs_ty, is_equal)?;
    } else if string_numeric_comparable(&lhs_ty, &rhs_ty) {
        emit_numeric_string_loose_eq(ctx, lhs, &lhs_ty, rhs, &rhs_ty, is_equal)?;
    } else if lhs_ty == PhpType::Float && rhs_ty == PhpType::Float {
        emit_float_compare(ctx, lhs, rhs, is_equal)?;
    } else if loose_intish_comparable(&lhs_ty, &rhs_ty) {
        let compare_truthiness = lhs_ty == PhpType::Bool || rhs_ty == PhpType::Bool;
        emit_intish_compare(ctx, lhs, rhs, is_equal, compare_truthiness)?;
    } else {
        return Err(CodegenIrError::unsupported(format!(
            "{} for PHP types {:?} and {:?}",
            inst.op.name(),
            lhs_ty,
            rhs_ty
        )));
    }
    store_if_result(ctx, inst)
}

/// Returns true when loose equality must compare a bool with a string by PHP truthiness.
fn string_bool_comparable(lhs_ty: &PhpType, rhs_ty: &PhpType) -> bool {
    matches!((lhs_ty, rhs_ty), (PhpType::Bool, PhpType::Str) | (PhpType::Str, PhpType::Bool))
}

/// Returns true when loose equality must compare null with the empty string.
fn string_null_comparable(lhs_ty: &PhpType, rhs_ty: &PhpType) -> bool {
    matches!(
        (lhs_ty, rhs_ty),
        (PhpType::Void | PhpType::Never, PhpType::Str)
            | (PhpType::Str, PhpType::Void | PhpType::Never)
    )
}

/// Returns true when loose equality must parse a string as a PHP numeric string.
fn string_numeric_comparable(lhs_ty: &PhpType, rhs_ty: &PhpType) -> bool {
    matches!((lhs_ty, rhs_ty), (PhpType::Int | PhpType::Float, PhpType::Str))
        || matches!((lhs_ty, rhs_ty), (PhpType::Str, PhpType::Int | PhpType::Float))
}

/// Emits loose equality for bool/string operands using PHP truthiness rules.
fn emit_bool_string_loose_eq(
    ctx: &mut FunctionContext<'_>,
    lhs: ValueId,
    lhs_ty: &PhpType,
    rhs: ValueId,
    rhs_ty: &PhpType,
    is_equal: bool,
) -> Result<()> {
    let (bool_value, string_value) = if *lhs_ty == PhpType::Bool {
        (lhs, rhs)
    } else {
        debug_assert_eq!(*rhs_ty, PhpType::Bool);
        (rhs, lhs)
    };
    let bool_reg = abi::tertiary_scratch_reg(ctx.emitter);
    load_intish_value(ctx, bool_value, bool_reg, true)?;
    ctx.load_value_to_result(string_value)?;
    emit_string_truthiness_to_result(ctx);
    emit_compare_reg_with_result(ctx, bool_reg, is_equal);
    Ok(())
}

/// Emits loose equality for null/string operands using PHP's empty-string rule.
fn emit_null_string_loose_eq(
    ctx: &mut FunctionContext<'_>,
    lhs: ValueId,
    lhs_ty: &PhpType,
    rhs: ValueId,
    _rhs_ty: &PhpType,
    is_equal: bool,
) -> Result<()> {
    let string_value = if *lhs_ty == PhpType::Str { lhs } else { rhs };
    ctx.load_value_to_result(string_value)?;
    emit_compare_current_string_length_to_zero(ctx, is_equal);
    Ok(())
}

/// Emits loose equality for number/string operands using PHP numeric-string parsing.
fn emit_numeric_string_loose_eq(
    ctx: &mut FunctionContext<'_>,
    lhs: ValueId,
    lhs_ty: &PhpType,
    rhs: ValueId,
    rhs_ty: &PhpType,
    is_equal: bool,
) -> Result<()> {
    let (numeric_value, numeric_ty, string_value) = if *lhs_ty == PhpType::Str {
        (rhs, rhs_ty, lhs)
    } else {
        (lhs, lhs_ty, rhs)
    };
    load_numeric_to_float_reg(ctx, numeric_value, numeric_ty, abi::float_result_reg(ctx.emitter))?;
    abi::emit_push_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));
    ctx.load_value_to_result(string_value)?;
    abi::emit_call_label(ctx.emitter, "__rt_str_to_number");
    let false_label = ctx.next_label("loose_numeric_string_false");
    let done_label = ctx.next_label("loose_numeric_string_done");
    let saved_float_reg = secondary_float_reg(ctx.emitter.target.arch);
    abi::emit_pop_float_reg(ctx.emitter, saved_float_reg);
    emit_branch_if_current_flag_false(ctx, &false_label);
    emit_compare_saved_float_with_parsed_string(ctx);
    emit_float_bool_from_flags(ctx, is_equal);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&false_label);
    emit_bool_literal(ctx, !is_equal);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Converts the current string result register pair to PHP truthiness in the integer result register.
fn emit_string_truthiness_to_result(ctx: &mut FunctionContext<'_>) {
    let falsy_label = ctx.next_label("str_falsy");
    let truthy_label = ctx.next_label("str_truthy");
    let done_label = ctx.next_label("str_truth_done");
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz {}, {}", len_reg, falsy_label)); // empty strings are falsy
            ctx.emitter.instruction(&format!("cmp {}, #1", len_reg));           // check whether this can be the special string "0"
            ctx.emitter.instruction(&format!("b.ne {}", truthy_label));         // non-empty strings longer than one byte are truthy
            ctx.emitter.instruction(&format!("ldrb w9, [{}]", ptr_reg));        // load the only string byte for the PHP "0" exception
            ctx.emitter.instruction("cmp w9, #48");                             // compare the byte with ASCII '0'
            ctx.emitter.instruction(&format!("b.eq {}", falsy_label));          // the exact string "0" is falsy
            ctx.emitter.label(&truthy_label);
            ctx.emitter.instruction("mov x0, #1");                              // materialize string truthiness as true
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the falsy path
            ctx.emitter.label(&falsy_label);
            ctx.emitter.instruction("mov x0, #0");                              // materialize string truthiness as false
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            let scratch = abi::temp_int_reg(ctx.emitter.target);
            ctx.emitter.instruction(&format!("test {}, {}", len_reg, len_reg)); // empty strings are falsy
            ctx.emitter.instruction(&format!("je {}", falsy_label));            // branch to the falsy path for empty strings
            ctx.emitter.instruction(&format!("cmp {}, 1", len_reg));            // check whether this can be the special string "0"
            ctx.emitter.instruction(&format!("jne {}", truthy_label));          // non-empty strings longer than one byte are truthy
            ctx.emitter.instruction(&format!("movzx {}d, BYTE PTR [{}]", scratch, ptr_reg)); // load the only string byte for the PHP "0" exception
            ctx.emitter.instruction(&format!("cmp {}d, 48", scratch));          // compare the byte with ASCII '0'
            ctx.emitter.instruction(&format!("je {}", falsy_label));            // the exact string "0" is falsy
            ctx.emitter.label(&truthy_label);
            ctx.emitter.instruction("mov rax, 1");                              // materialize string truthiness as true
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the falsy path
            ctx.emitter.label(&falsy_label);
            ctx.emitter.instruction("mov rax, 0");                              // materialize string truthiness as false
            ctx.emitter.label(&done_label);
        }
    }
}

/// Compares the current string length register against zero and materializes equality.
fn emit_compare_current_string_length_to_zero(ctx: &mut FunctionContext<'_>, is_equal: bool) {
    let (_, len_reg) = abi::string_result_regs(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #0", len_reg));           // compare the string length with the empty string for null loose equality
            ctx.emitter.instruction(&format!("cset x0, {}", equality_cond(is_equal, ctx.emitter.target.arch))); // materialize the null/string loose comparison result
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, 0", len_reg));            // compare the string length with the empty string for null loose equality
            ctx.emitter.instruction(&format!("set{} al", equality_cond(is_equal, ctx.emitter.target.arch))); // materialize the null/string loose comparison byte
            ctx.emitter.instruction("movzx rax, al");                           // widen the loose comparison byte into the integer result register
        }
    }
}

/// Branches to `label` when `__rt_str_to_number` reported a non-numeric string.
fn emit_branch_if_current_flag_false(ctx: &mut FunctionContext<'_>, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // test whether string-to-number parsing failed
            ctx.emitter.instruction(&format!("b.eq {}", label));                // branch when the string was not numeric
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // test whether string-to-number parsing failed
            ctx.emitter.instruction(&format!("je {}", label));                  // branch when the string was not numeric
        }
    }
}

/// Compares the saved numeric operand with the parsed string number.
fn emit_compare_saved_float_with_parsed_string(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fcmp d1, d0");                             // compare the numeric operand against the parsed numeric string
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("ucomisd xmm1, xmm0");                      // compare the numeric operand against the parsed numeric string
        }
    }
}

/// Materializes the current floating-point equality flags as a PHP boolean.
fn emit_float_bool_from_flags(ctx: &mut FunctionContext<'_>, is_equal: bool) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cset x0, {}", equality_cond(is_equal, ctx.emitter.target.arch))); // materialize numeric-string equality as boolean
        }
        Arch::X86_64 => {
            emit_x86_64_float_equality_result(ctx, is_equal);
            ctx.emitter.instruction("movzx rax, al");                           // widen the numeric-string equality byte into the integer result register
        }
    }
}

/// Materializes float equality from x86_64 flags, treating unordered as unequal.
fn emit_x86_64_float_equality_result(ctx: &mut FunctionContext<'_>, is_equal: bool) {
    if is_equal {
        ctx.emitter.instruction("sete al");                                     // materialize ordered float equality in the low byte
        ctx.emitter.instruction("setnp r10b");                                  // materialize whether the comparison was ordered
        ctx.emitter.instruction("and al, r10b");                                // clear equality for unordered NaN comparisons
    } else {
        ctx.emitter.instruction("setne al");                                    // materialize ordered float inequality in the low byte
        ctx.emitter.instruction("setp r10b");                                   // materialize unordered NaN comparison as true for !=
        ctx.emitter.instruction("or al, r10b");                                 // merge ordered inequality with unordered inequality
    }
}

/// Compares an integer register with the canonical integer result and materializes equality.
fn emit_compare_reg_with_result(ctx: &mut FunctionContext<'_>, lhs_reg: &str, is_equal: bool) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", lhs_reg, result_reg)); // compare scalar truthiness operands
            ctx.emitter.instruction(&format!("cset x0, {}", equality_cond(is_equal, ctx.emitter.target.arch))); // materialize truthiness equality as boolean
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", lhs_reg, result_reg)); // compare scalar truthiness operands
            ctx.emitter.instruction(&format!("set{} al", equality_cond(is_equal, ctx.emitter.target.arch))); // materialize truthiness equality in the low byte
            ctx.emitter.instruction("movzx rax, al");                           // widen the truthiness equality byte into the integer result register
        }
    }
}

/// Lowers the scalar spaceship operator, returning -1, 0, or 1.
pub(super) fn lower_spaceship(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    let lhs_ty = ctx.value_php_type(lhs)?;
    let rhs_ty = ctx.value_php_type(rhs)?;
    let uses_float_compare = lhs_ty == PhpType::Float || rhs_ty == PhpType::Float;
    if uses_float_compare {
        emit_numeric_float_compare(ctx, lhs, &lhs_ty, rhs, &rhs_ty)?;
    } else if intish_or_null(&lhs_ty) && intish_or_null(&rhs_ty) {
        emit_numeric_int_compare(ctx, lhs, rhs)?;
    } else {
        return Err(CodegenIrError::unsupported(format!(
            "spaceship for PHP types {:?} and {:?}",
            lhs_ty,
            rhs_ty
        )));
    }
    emit_spaceship_result(ctx, uses_float_compare);
    store_if_result(ctx, inst)
}

/// Returns true for scalar values that can participate in the current loose integer path.
fn intish_or_null(ty: &PhpType) -> bool {
    matches!(
        ty,
        PhpType::Int
            | PhpType::Bool
            | PhpType::Void
            | PhpType::Never
            | PhpType::Mixed
            | PhpType::TaggedScalar
    )
}

/// Returns true for the scalar loose-equality subset that can normalize through integer slots.
fn loose_intish_comparable(lhs_ty: &PhpType, rhs_ty: &PhpType) -> bool {
    if intish_or_null(lhs_ty) && intish_or_null(rhs_ty) {
        return true;
    }
    matches!(lhs_ty, PhpType::Mixed) && intish_or_null(rhs_ty)
        || matches!(rhs_ty, PhpType::Mixed) && intish_or_null(lhs_ty)
}

/// Emits the target compare instruction for integer-like spaceship operands.
fn emit_numeric_int_compare(
    ctx: &mut FunctionContext<'_>,
    lhs: ValueId,
    rhs: ValueId,
) -> Result<()> {
    let lhs_reg = match ctx.emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "r10",
    };
    let rhs_reg = abi::int_result_reg(ctx.emitter);
    load_intish_value(ctx, lhs, lhs_reg, false)?;
    abi::emit_push_reg(ctx.emitter, lhs_reg);
    load_intish_value(ctx, rhs, rhs_reg, false)?;
    abi::emit_pop_reg(ctx.emitter, lhs_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x1, x0");                              // compare left and right integer operands for spaceship ordering
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", lhs_reg, rhs_reg));  // compare left and right integer operands for spaceship ordering
        }
    }
    Ok(())
}

/// Emits the target compare instruction for float-capable spaceship operands.
fn emit_numeric_float_compare(
    ctx: &mut FunctionContext<'_>,
    lhs: ValueId,
    lhs_ty: &PhpType,
    rhs: ValueId,
    rhs_ty: &PhpType,
) -> Result<()> {
    load_numeric_to_float_reg(ctx, lhs, lhs_ty, secondary_float_reg(ctx.emitter.target.arch))?;
    load_numeric_to_float_reg(ctx, rhs, rhs_ty, abi::float_result_reg(ctx.emitter))?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fcmp d1, d0");                             // compare left and right float operands for spaceship ordering
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("ucomisd xmm1, xmm0");                      // compare left and right float operands for spaceship ordering
        }
    }
    Ok(())
}

/// Loads a numeric scalar into a selected floating-point register.
fn load_numeric_to_float_reg(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    ty: &PhpType,
    float_reg: &str,
) -> Result<()> {
    match ty {
        PhpType::Float => {
            ctx.load_value_to_reg(value, float_reg)?;
        }
        PhpType::Int | PhpType::Bool => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            ctx.load_value_to_reg(value, int_reg)?;
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction(&format!("scvtf {}, {}", float_reg, int_reg)); // promote integer spaceship operand to float
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction(&format!("cvtsi2sd {}, {}", float_reg, int_reg)); // promote integer spaceship operand to float
                }
            }
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction(&format!("scvtf {}, x0", float_reg)); // promote null spaceship operand to 0.0
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction(&format!("cvtsi2sd {}, rax", float_reg)); // promote null spaceship operand to 0.0
                }
            }
        }
        PhpType::Mixed => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float");
            move_float_result_to_reg(ctx, float_reg);
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "float spaceship for PHP type {:?}",
                other
            )))
        }
    }
    Ok(())
}

/// Moves the current float result into the requested comparison register.
fn move_float_result_to_reg(ctx: &mut FunctionContext<'_>, reg: &str) {
    let result_reg = abi::float_result_reg(ctx.emitter);
    if reg == result_reg {
        return;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("fmov {}, {}", reg, result_reg));  // preserve the normalized mixed float comparison operand
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("movapd {}, {}", reg, result_reg)); // preserve the normalized mixed float comparison operand
        }
    }
}

/// Materializes the result of the most recent compare as a spaceship integer.
fn emit_spaceship_result(ctx: &mut FunctionContext<'_>, uses_float_compare: bool) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            if uses_float_compare {
                let unordered_label = ctx.next_label("spaceship_unordered");
                let done_label = ctx.next_label("spaceship_done");
                ctx.emitter.instruction(&format!("b.vs {}", unordered_label));  // PHP treats any NaN spaceship comparison as greater
                ctx.emitter.instruction("cset x0, gt");                         // set result to 1 when left is greater than right
                ctx.emitter.instruction("csinv x0, x0, xzr, ge");               // keep 1/0 for greater/equal, or produce -1 for less
                ctx.emitter.instruction(&format!("b {}", done_label));          // skip the unordered NaN result
                ctx.emitter.label(&unordered_label);
                ctx.emitter.instruction("mov x0, #1");                          // unordered float comparisons produce spaceship result 1
                ctx.emitter.label(&done_label);
                return;
            }
            ctx.emitter.instruction("cset x0, gt");                             // set result to 1 when left is greater than right
            ctx.emitter.instruction("csinv x0, x0, xzr, ge");                   // keep 1/0 for greater/equal, or produce -1 for less
        }
        Arch::X86_64 => {
            let greater_label = ctx.next_label("spaceship_gt");
            let less_label = ctx.next_label("spaceship_lt");
            let done_label = ctx.next_label("spaceship_done");
            let greater_jump = if uses_float_compare { "ja" } else { "jg" };
            let less_jump = if uses_float_compare { "jb" } else { "jl" };
            if uses_float_compare {
                ctx.emitter.instruction(&format!("jp {}", greater_label));      // PHP treats any NaN spaceship comparison as greater
            }
            ctx.emitter.instruction(&format!("{} {}", greater_jump, greater_label)); // branch when the left operand is greater
            ctx.emitter.instruction(&format!("{} {}", less_jump, less_label));  // branch when the left operand is less
            ctx.emitter.instruction("mov rax, 0");                              // equal operands produce spaceship result 0
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the greater and less result branches
            ctx.emitter.label(&greater_label);
            ctx.emitter.instruction("mov rax, 1");                              // greater operands produce spaceship result 1
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the less result branch
            ctx.emitter.label(&less_label);
            ctx.emitter.instruction("mov rax, -1");                             // lesser operands produce spaceship result -1
            ctx.emitter.label(&done_label);
        }
    }
}

/// Emits an integer-like equality comparison into the integer result register.
fn emit_intish_compare(
    ctx: &mut FunctionContext<'_>,
    lhs: ValueId,
    rhs: ValueId,
    is_equal: bool,
    compare_truthiness: bool,
) -> Result<()> {
    let lhs_reg = abi::secondary_scratch_reg(ctx.emitter);
    let rhs_reg = abi::tertiary_scratch_reg(ctx.emitter);
    // Loading a Mixed operand unboxes it through `__rt_mixed_cast_int`/`__rt_mixed_cast_bool`,
    // whose call clobbers the caller-saved scratch registers. Without saving the already-loaded
    // left operand across the right-operand load, an `Int == Mixed` comparison lost its left value
    // (while `Mixed == Int` happened to work, because the call ran before the int was loaded).
    load_intish_value(ctx, lhs, lhs_reg, compare_truthiness)?;
    abi::emit_push_reg(ctx.emitter, lhs_reg);
    load_intish_value(ctx, rhs, rhs_reg, compare_truthiness)?;
    abi::emit_pop_reg(ctx.emitter, lhs_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", lhs_reg, rhs_reg));  // compare scalar equality operands
            ctx.emitter.instruction(&format!("cset x0, {}", equality_cond(is_equal, ctx.emitter.target.arch))); // materialize scalar equality as boolean
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", lhs_reg, rhs_reg));  // compare scalar equality operands
            ctx.emitter.instruction(&format!("set{} al", equality_cond(is_equal, ctx.emitter.target.arch))); // materialize scalar equality in the low byte
            ctx.emitter.instruction("movzx rax, al");                           // widen the equality byte into the integer result register
        }
    }
    Ok(())
}

/// Loads an int/bool/null value into `reg`, optionally coercing to PHP truthiness.
fn load_intish_value(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    reg: &str,
    truthy: bool,
) -> Result<()> {
    match ctx.value_php_type(value)? {
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, reg, 0);
        }
        PhpType::Int | PhpType::Bool => {
            ctx.load_value_to_reg(value, reg)?;
            if truthy {
                emit_reg_nonzero_bool(ctx, reg);
            }
        }
        PhpType::TaggedScalar => {
            // A miss-capable read materializes as a TaggedScalar; narrow it to a plain int
            // (null sentinel → 0) exactly once so loose equality compares the int payload,
            // matching PHP's `null == 0` / `null == <nonzero>` semantics.
            ctx.load_value_to_result(value)?;
            crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(ctx.emitter);
            move_int_result_to_reg(ctx, reg);
            if truthy {
                emit_reg_nonzero_bool(ctx, reg);
            }
        }
        PhpType::Mixed => {
            ctx.load_value_to_result(value)?;
            let helper = if truthy {
                "__rt_mixed_cast_bool"
            } else {
                "__rt_mixed_cast_int"
            };
            abi::emit_call_label(ctx.emitter, helper);
            move_int_result_to_reg(ctx, reg);
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "integer equality for PHP type {:?}",
                other
            )))
        }
    }
    Ok(())
}

/// Moves the current integer result into the requested comparison register.
fn move_int_result_to_reg(ctx: &mut FunctionContext<'_>, reg: &str) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    if reg == result_reg {
        return;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov {}, {}", reg, result_reg));   // preserve the normalized mixed comparison operand
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov {}, {}", reg, result_reg));   // preserve the normalized mixed comparison operand
        }
    }
}

/// Rewrites `reg` to 1 when nonzero and 0 otherwise.
fn emit_reg_nonzero_bool(ctx: &mut FunctionContext<'_>, reg: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #0", reg));               // compare scalar value against zero for truthiness
            ctx.emitter.instruction(&format!("cset {}, ne", reg));              // materialize nonzero truthiness in the same register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {}, {}", reg, reg));         // compare scalar value against zero for truthiness
            ctx.emitter.instruction("setne al");                                // materialize nonzero truthiness in the low byte
            ctx.emitter.instruction(&format!("movzx {}, al", reg));             // widen truthiness into the requested register
        }
    }
}

/// Emits a floating-point equality comparison into the integer result register.
fn emit_float_compare(
    ctx: &mut FunctionContext<'_>,
    lhs: ValueId,
    rhs: ValueId,
    is_equal: bool,
) -> Result<()> {
    let lhs_reg = match ctx.emitter.target.arch {
        Arch::AArch64 => "d1",
        Arch::X86_64 => "xmm1",
    };
    let rhs_reg = abi::float_result_reg(ctx.emitter);
    ctx.load_value_to_reg(lhs, lhs_reg)?;
    ctx.load_value_to_reg(rhs, rhs_reg)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fcmp d1, d0");                             // compare strict float equality operands
            ctx.emitter.instruction(&format!("cset x0, {}", equality_cond(is_equal, ctx.emitter.target.arch))); // materialize float equality as boolean
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("ucomisd xmm1, xmm0");                      // compare strict float equality operands
            emit_x86_64_float_equality_result(ctx, is_equal);
            ctx.emitter.instruction("movzx rax, al");                           // widen the equality byte into the integer result register
        }
    }
    Ok(())
}

/// Calls the selected runtime string equality helper and optionally inverts its boolean result.
fn emit_string_eq_call(
    ctx: &mut FunctionContext<'_>,
    lhs: ValueId,
    rhs: ValueId,
    is_equal: bool,
    helper: &str,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_string_value_to_regs(lhs, "x1", "x2")?;
            ctx.load_string_value_to_regs(rhs, "x3", "x4")?;
            abi::emit_call_label(ctx.emitter, helper);
            if !is_equal {
                ctx.emitter.instruction("eor x0, x0, #1");                      // invert string equality for inequality
            }
        }
        Arch::X86_64 => {
            ctx.load_string_value_to_regs(lhs, "rdi", "rsi")?;
            ctx.load_string_value_to_regs(rhs, "rdx", "rcx")?;
            abi::emit_call_label(ctx.emitter, helper);
            if !is_equal {
                ctx.emitter.instruction("xor rax, 1");                          // invert string equality for inequality
            }
        }
    }
    Ok(())
}

/// Emits a concrete boolean value into the integer result register.
fn emit_bool_literal(ctx: &mut FunctionContext<'_>, value: bool) {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), i64::from(value));
}

/// Returns the target condition-code suffix for equality or inequality.
fn equality_cond(is_equal: bool, arch: Arch) -> &'static str {
    match (is_equal, arch) {
        (true, Arch::AArch64) => "eq",
        (false, Arch::AArch64) => "ne",
        (true, Arch::X86_64) => "e",
        (false, Arch::X86_64) => "ne",
    }
}
