//! Purpose:
//! String ABI loading and IO argument type validation.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Loads a string SSA value into the target string result registers, coercing
/// any scalar to its PHP string form. Shared with `system::lower_header`.
pub(in crate::codegen::lower_inst::builtins) fn load_string_to_result(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    context: &str,
) -> Result<()> {
    match ctx.value_php_type(value)?.codegen_repr() {
        PhpType::Str => {
            ctx.load_value_to_result(value)?;
            Ok(())
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_ftoa");
            Ok(())
        }
        PhpType::Int => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_itoa");
            Ok(())
        }
        PhpType::Bool => {
            ctx.load_value_to_result(value)?;
            lower_loaded_bool_to_string(ctx);
            Ok(())
        }
        PhpType::TaggedScalar => {
            ctx.load_value_to_result(value)?;
            lower_loaded_tagged_scalar_to_string(ctx);
            Ok(())
        }
        PhpType::Void | PhpType::Never => {
            emit_empty_string_result(ctx);
            Ok(())
        }
        PhpType::Resource(_) => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_resource_to_string");
            Ok(())
        }
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            context,
            other
        ))),
    }
}

/// Converts the currently loaded boolean result to PHP string result registers.
pub(super) fn lower_loaded_bool_to_string(ctx: &mut FunctionContext<'_>) {
    let false_label = ctx.next_label("io_bool_to_str_false");
    let done_label = ctx.next_label("io_bool_to_str_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {}", false_label));       // false stringifies to an empty string
            abi::emit_call_label(ctx.emitter, "__rt_itoa");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the empty-string fallback after true conversion
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // test whether the boolean payload is false
            ctx.emitter.instruction(&format!("je {}", false_label));            // false stringifies to an empty string
            abi::emit_call_label(ctx.emitter, "__rt_itoa");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the empty-string fallback after true conversion
        }
    }
    ctx.emitter.label(&false_label);
    emit_empty_string_result(ctx);
    ctx.emitter.label(&done_label);
}

/// Converts the currently loaded tagged scalar result to PHP string result registers.
pub(super) fn lower_loaded_tagged_scalar_to_string(ctx: &mut FunctionContext<'_>) {
    let null_label = ctx.next_label("io_tagged_to_str_null");
    let done_label = ctx.next_label("io_tagged_to_str_done");
    crate::codegen::sentinels::emit_branch_if_tagged_scalar_null(ctx.emitter, &null_label);
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&null_label);
    emit_empty_string_result(ctx);
    ctx.emitter.label(&done_label);
}

/// Publishes PHP's empty-string result in the target string ABI registers.
pub(super) fn emit_empty_string_result(ctx: &mut FunctionContext<'_>) {
    let len_reg = abi::string_result_regs(ctx.emitter).1;
    abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
}

/// Verifies that a path builtin scalar argument has the supported integer representation.
pub(super) fn require_int(ty: PhpType, name: &str) -> Result<()> {
    if ty == PhpType::Int {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for PHP type {:?}",
        name,
        ty
    )))
}

/// Verifies that an optional integer argument is either `int` or literal `null`.
pub(super) fn require_optional_int(ty: PhpType, name: &str) -> Result<()> {
    if matches!(ty, PhpType::Int | PhpType::Void | PhpType::Never) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for PHP type {:?}",
        name,
        ty
    )))
}

/// Verifies that a scalar flag argument has an integer-like representation.
pub(super) fn require_int_or_bool(ty: PhpType, name: &str) -> Result<()> {
    if matches!(ty, PhpType::Int | PhpType::Bool) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for PHP type {:?}",
        name,
        ty
    )))
}


