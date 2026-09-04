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

/// Loads a `?int` argument and materialises what NULL means at this call site.
///
/// The two steps belong together. `require_optional_int` ACCEPTS a literal null — correctly, php
/// spells these parameters `?int $length = null` — but a literal null LOADS AS ZERO, and zero is a
/// real length to every consumer here. Validating and loading separately is therefore a shape the
/// validator permits and the code below mishandles, silently: `stream_get_contents($h, null)`
/// answered `""` and `stream_copy_to_stream($a, $b, null)` copied nothing, where `php -n` 8.5.6
/// answers the whole stream for both.
///
/// `null_answer` is the word the CALLER's consumer reads as "no bound", and it is not the same
/// everywhere: `stream_get_contents` compares against `NULL_SENTINEL` (a large POSITIVE, tested by
/// equality), while `stream_copy_to_stream`'s copier reads `-1`. Passing the site's own sentinel
/// keeps that difference where it is true instead of picking one and hoping.
pub(super) fn load_optional_int_to_result(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    name: &str,
    null_answer: i64,
) -> Result<()> {
    // The DECLARED type, not the loaded one. `load_value_to_result` answers the representation it
    // put in the register — a `const_null` is an `I64`, so it reports `Int` — and asking it about
    // nullness is why `require_optional_int`'s `Void` arm was dead for a literal null while the
    // checker was happily accepting one.
    let declared = ctx.value_php_type(value)?.codegen_repr();
    let loaded = ctx.load_value_to_result(value)?.codegen_repr();
    let is_tagged = loaded == PhpType::TaggedScalar;
    require_optional_int(loaded, name)?;
    if matches!(declared, PhpType::Void | PhpType::Never) {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), null_answer);
    } else if is_tagged {
        // A forwarded `?int $length = null` parameter, which arrives as a (value, tag) pair.
        crate::codegen::lower_inst::emit_tagged_scalar_to_int_or(ctx, null_answer);
    }
    Ok(())
}

/// Verifies that an optional integer argument is either `int` or literal `null`.
///
/// Private on purpose: a caller that validates without going through
/// `load_optional_int_to_result` reintroduces the zero-for-null bug above, and this is what stops
/// that spelling from existing.
fn require_optional_int(ty: PhpType, name: &str) -> Result<()> {
    if matches!(
        ty,
        PhpType::Int | PhpType::Void | PhpType::Never | PhpType::TaggedScalar
    ) {
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


