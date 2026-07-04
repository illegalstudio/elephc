//! Purpose:
//! Lowers PHP `is_numeric()` for concrete scalar EIR operands.
//! Keeps the byte scanner separate from the builtin dispatcher.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - The string grammar mirrors the legacy backend: optional leading `-`,
//!   digits, optional `.`, and at least one digit overall.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::ir::Instruction;
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::super::{expect_operand, store_if_result};
use crate::codegen_ir::{CodegenIrError, Result};

/// Lowers `is_numeric()` for concrete scalar values.
pub(crate) fn lower_is_numeric(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "is_numeric", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)? {
        PhpType::Int | PhpType::Float => emit_static_bool(ctx, true),
        PhpType::Str => {
            ctx.load_value_to_result(value)?;
            emit_string_is_numeric(ctx);
        }
        PhpType::Bool | PhpType::Void | PhpType::Never => emit_static_bool(ctx, false),
        PhpType::Mixed | PhpType::Union(_) => emit_mixed_is_numeric(ctx, value)?,
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "is_numeric for PHP type {:?}",
                other
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// Emits runtime `is_numeric()` dispatch for a boxed Mixed value.
fn emit_mixed_is_numeric(ctx: &mut FunctionContext<'_>, value: crate::ir::ValueId) -> Result<()> {
    let string_case = ctx.next_label("isnum_mixed_string");
    let true_case = ctx.next_label("isnum_mixed_true");
    let false_case = ctx.next_label("isnum_mixed_false");
    let done = ctx.next_label("isnum_mixed_done");
    ctx.load_value_to_result(value)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_on_mixed_tag(ctx, 0, &true_case);
    emit_branch_on_mixed_tag(ctx, 2, &true_case);
    emit_branch_on_mixed_tag(ctx, 1, &string_case);
    abi::emit_jump(ctx.emitter, &false_case);

    ctx.emitter.label(&true_case);
    emit_static_bool(ctx, true);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&string_case);
    move_mixed_string_payload_to_string_result(ctx);
    emit_string_is_numeric(ctx);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&false_case);
    emit_static_bool(ctx, false);
    ctx.emitter.label(&done);
    Ok(())
}

/// Branches when the unboxed Mixed tag equals the requested runtime tag.
fn emit_branch_on_mixed_tag(ctx: &mut FunctionContext<'_>, tag: u8, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp x0, #{}", tag));              // compare the unboxed Mixed tag against this is_numeric case
            ctx.emitter.instruction(&format!("b.eq {}", label));                // branch when the Mixed tag matches this is_numeric case
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp rax, {}", tag));              // compare the unboxed Mixed tag against this is_numeric case
            ctx.emitter.instruction(&format!("je {}", label));                  // branch when the Mixed tag matches this is_numeric case
        }
    }
}

/// Moves an unboxed Mixed string payload into the normal string result registers.
fn move_mixed_string_payload_to_string_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {}
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, rdi");                            // move the Mixed string pointer into the string result register
        }
    }
}

/// Emits a boolean immediate into the integer result register.
fn emit_static_bool(ctx: &mut FunctionContext<'_>, value: bool) {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        i64::from(value),
    );
}

/// Emits the legacy ASCII numeric-string scan used by `is_numeric()`.
fn emit_string_is_numeric(ctx: &mut FunctionContext<'_>) {
    let loop_label = ctx.next_label("isnum_loop");
    let dot_label = ctx.next_label("isnum_dot");
    let frac_loop = ctx.next_label("isnum_frac");
    let fail_label = ctx.next_label("isnum_fail");
    let pass_label = ctx.next_label("isnum_pass");
    let end_label = ctx.next_label("isnum_end");
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_string_is_numeric_aarch64(
            ctx,
            &loop_label,
            &dot_label,
            &frac_loop,
            &fail_label,
            &pass_label,
            &end_label,
        ),
        Arch::X86_64 => emit_string_is_numeric_x86_64(
            ctx,
            &loop_label,
            &dot_label,
            &frac_loop,
            &fail_label,
            &pass_label,
            &end_label,
        ),
    }
}

/// Emits the AArch64 string scan for `is_numeric()`.
fn emit_string_is_numeric_aarch64(
    ctx: &mut FunctionContext<'_>,
    loop_label: &str,
    dot_label: &str,
    frac_loop: &str,
    fail_label: &str,
    pass_label: &str,
    end_label: &str,
) {
    ctx.emitter.instruction(&format!("cbz x2, {}", fail_label));                // empty strings are not numeric
    ctx.emitter.instruction("mov x3, #0");                                      // initialize the string scan index
    ctx.emitter.instruction("mov x5, #0");                                      // initialize the consumed digit count
    ctx.emitter.instruction("ldrb w4, [x1]");                                   // load the first string byte for sign handling
    ctx.emitter.instruction("cmp w4, #45");                                     // check whether the string starts with '-'
    ctx.emitter.instruction(&format!("b.ne {}", loop_label));                   // start digit scanning when there is no sign
    ctx.emitter.instruction("add x3, x3, #1");                                  // skip the leading minus sign
    ctx.emitter.instruction("cmp x3, x2");                                      // reject a string that contains only the sign
    ctx.emitter.instruction(&format!("b.ge {}", fail_label));                   // bare '-' is not numeric
    ctx.emitter.label(loop_label);
    ctx.emitter.instruction("cmp x3, x2");                                      // check whether the scan reached the string length
    ctx.emitter.instruction(&format!("b.ge {}", pass_label));                   // finish after scanning the integer part
    ctx.emitter.instruction("ldrb w4, [x1, x3]");                               // load the current integer-part byte
    ctx.emitter.instruction("cmp w4, #46");                                     // check whether the byte is '.'
    ctx.emitter.instruction(&format!("b.eq {}", dot_label));                    // switch to fractional scanning at a dot
    ctx.emitter.instruction("sub w6, w4, #48");                                 // normalize the byte to a candidate decimal digit
    ctx.emitter.instruction("cmp w6, #9");                                      // verify the candidate digit range
    ctx.emitter.instruction(&format!("b.hi {}", fail_label));                   // non-digit bytes make the string non-numeric
    ctx.emitter.instruction("add x5, x5, #1");                                  // record one consumed digit
    ctx.emitter.instruction("add x3, x3, #1");                                  // advance to the next byte
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // continue integer-part scanning
    ctx.emitter.label(dot_label);
    ctx.emitter.instruction("add x3, x3, #1");                                  // skip the decimal point
    ctx.emitter.label(frac_loop);
    ctx.emitter.instruction("cmp x3, x2");                                      // check whether the fractional scan reached the end
    ctx.emitter.instruction(&format!("b.ge {}", pass_label));                   // finish after scanning the fractional part
    ctx.emitter.instruction("ldrb w4, [x1, x3]");                               // load the current fractional byte
    ctx.emitter.instruction("sub w6, w4, #48");                                 // normalize the byte to a candidate decimal digit
    ctx.emitter.instruction("cmp w6, #9");                                      // verify the fractional digit range
    ctx.emitter.instruction(&format!("b.hi {}", fail_label));                   // non-digit fractional bytes make the string non-numeric
    ctx.emitter.instruction("add x5, x5, #1");                                  // record one consumed fractional digit
    ctx.emitter.instruction("add x3, x3, #1");                                  // advance to the next fractional byte
    ctx.emitter.instruction(&format!("b {}", frac_loop));                       // continue fractional scanning
    ctx.emitter.label(pass_label);
    ctx.emitter.instruction("cmp x5, #0");                                      // require at least one digit overall
    ctx.emitter.instruction(&format!("b.eq {}", fail_label));                   // reject strings like '.' or '-.'
    ctx.emitter.instruction("mov x0, #1");                                      // return true for a numeric-looking string
    ctx.emitter.instruction(&format!("b {}", end_label));                       // skip the false result path
    ctx.emitter.label(fail_label);
    ctx.emitter.instruction("mov x0, #0");                                      // return false for a non-numeric string
    ctx.emitter.label(end_label);
}

/// Emits the x86_64 string scan for `is_numeric()`.
fn emit_string_is_numeric_x86_64(
    ctx: &mut FunctionContext<'_>,
    loop_label: &str,
    dot_label: &str,
    frac_loop: &str,
    fail_label: &str,
    pass_label: &str,
    end_label: &str,
) {
    ctx.emitter.instruction("test rdx, rdx");                                   // empty strings are not numeric
    ctx.emitter.instruction(&format!("je {}", fail_label));                     // branch to failure for an empty string
    ctx.emitter.instruction("mov rcx, 0");                                      // initialize the string scan index
    ctx.emitter.instruction("mov r8, 0");                                       // initialize the consumed digit count
    ctx.emitter.instruction("movzx r9d, BYTE PTR [rax]");                       // load the first string byte for sign handling
    ctx.emitter.instruction("cmp r9d, 45");                                     // check whether the string starts with '-'
    ctx.emitter.instruction(&format!("jne {}", loop_label));                    // start digit scanning when there is no sign
    ctx.emitter.instruction("add rcx, 1");                                      // skip the leading minus sign
    ctx.emitter.instruction("cmp rcx, rdx");                                    // reject a string that contains only the sign
    ctx.emitter.instruction(&format!("jae {}", fail_label));                    // bare '-' is not numeric
    ctx.emitter.label(loop_label);
    ctx.emitter.instruction("cmp rcx, rdx");                                    // check whether the scan reached the string length
    ctx.emitter.instruction(&format!("jae {}", pass_label));                    // finish after scanning the integer part
    ctx.emitter.instruction("movzx r9d, BYTE PTR [rax + rcx]");                 // load the current integer-part byte
    ctx.emitter.instruction("cmp r9d, 46");                                     // check whether the byte is '.'
    ctx.emitter.instruction(&format!("je {}", dot_label));                      // switch to fractional scanning at a dot
    ctx.emitter.instruction("sub r9d, 48");                                     // normalize the byte to a candidate decimal digit
    ctx.emitter.instruction("cmp r9d, 9");                                      // verify the candidate digit range
    ctx.emitter.instruction(&format!("ja {}", fail_label));                     // non-digit bytes make the string non-numeric
    ctx.emitter.instruction("add r8, 1");                                       // record one consumed digit
    ctx.emitter.instruction("add rcx, 1");                                      // advance to the next byte
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // continue integer-part scanning
    ctx.emitter.label(dot_label);
    ctx.emitter.instruction("add rcx, 1");                                      // skip the decimal point
    ctx.emitter.label(frac_loop);
    ctx.emitter.instruction("cmp rcx, rdx");                                    // check whether the fractional scan reached the end
    ctx.emitter.instruction(&format!("jae {}", pass_label));                    // finish after scanning the fractional part
    ctx.emitter.instruction("movzx r9d, BYTE PTR [rax + rcx]");                 // load the current fractional byte
    ctx.emitter.instruction("sub r9d, 48");                                     // normalize the byte to a candidate decimal digit
    ctx.emitter.instruction("cmp r9d, 9");                                      // verify the fractional digit range
    ctx.emitter.instruction(&format!("ja {}", fail_label));                     // non-digit fractional bytes make the string non-numeric
    ctx.emitter.instruction("add r8, 1");                                       // record one consumed fractional digit
    ctx.emitter.instruction("add rcx, 1");                                      // advance to the next fractional byte
    ctx.emitter.instruction(&format!("jmp {}", frac_loop));                     // continue fractional scanning
    ctx.emitter.label(pass_label);
    ctx.emitter.instruction("test r8, r8");                                     // require at least one digit overall
    ctx.emitter.instruction(&format!("je {}", fail_label));                     // reject strings like '.' or '-.'
    ctx.emitter.instruction("mov rax, 1");                                      // return true for a numeric-looking string
    ctx.emitter.instruction(&format!("jmp {}", end_label));                     // skip the false result path
    ctx.emitter.label(fail_label);
    ctx.emitter.instruction("mov rax, 0");                                      // return false for a non-numeric string
    ctx.emitter.label(end_label);
}
