//! Purpose:
//! Lowers PHP `is_numeric()` for concrete scalar EIR operands.
//! Keeps the type dispatch separate from the builtin dispatcher; the string grammar
//! itself lives in the shared runtime scanner.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::lower_language_construct_call()`.
//!
//! Key details:
//! - The string case delegates to the length-aware `__rt_str_numeric_ex`, whose numeric flag
//!   implements PHP's `is_numeric_string()` grammar and rejects embedded NUL bytes:
//!   optional leading/trailing PHP whitespace, optional sign, a mantissa with at least
//!   one digit (`12`, `.5`, `5.`), and an exponent only when a digit follows it. Hex,
//!   underscore separators, `INF` and `NAN` are NOT numeric. Numeric classification uses
//!   the whole PHP byte string; casts may instead consume its leading numeric prefix.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::ir::Instruction;
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::super::{expect_operand, store_if_result};
use crate::codegen::{CodegenIrError, Result};

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

/// Emits PHP's numeric-string test for a string in the string-result registers.
///
/// Delegates to `__rt_str_numeric_ex`, which checks the PHP byte length before classification
/// and returns the fully-numeric flag in the integer result register on every target.
fn emit_string_is_numeric(ctx: &mut FunctionContext<'_>) {
    abi::emit_call_label(ctx.emitter, "__rt_str_numeric_ex");
}
