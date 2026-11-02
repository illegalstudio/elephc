//! Purpose:
//! Converts validated float-shaped runtime arguments to integers with PHP precision diagnostics.
//!
//! Called from:
//! - `super::conversions::lower_cast()` for `Immediate::ImplicitIntCast`.
//!
//! Key details:
//! - EIR argument guards have already selected int and rejected non-finite or out-of-range values.
//! - Exact integer strings bypass this path before double rounding could change their bounds.
//! - Shared float and quoted-string emitters preserve the converted result across handlers.

use crate::codegen::{abi, context::FunctionContext, CodegenIrError, Result};
use crate::ir::Instruction;
use crate::types::PhpType;

/// Emits tag-dispatched diagnostics and returns the saved integer, without consuming the source.
pub(super) fn lower(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = super::expect_operand(inst, 0)?;
    if ctx.raw_value_php_type(value)?.codegen_repr() != PhpType::Mixed {
        return Err(CodegenIrError::unsupported("implicit int binding requires boxed storage"));
    }
    let float = ctx.next_label("implicit_int_float");
    let string = ctx.next_label("implicit_int_string");
    let invalid = ctx.next_label("implicit_int_invalid");
    let done = ctx.next_label("implicit_int_done");
    super::load_value_to_first_int_arg(ctx, value)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let tag = abi::int_result_reg(ctx.emitter);
    super::enums::emit_mixed_tag_branch(ctx, tag, 2, &float);
    super::enums::emit_mixed_tag_branch(ctx, tag, 1, &string);
    abi::emit_jump(ctx.emitter, &invalid);

    ctx.emitter.label(&float);
    let payload = crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
    super::mixed_narrowing::emit_float_bits_to_float_result(ctx, payload);
    super::builtins::strings::emit_float_result_int_coercion_diagnostics(ctx, &invalid);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&string);
    super::objects::emit_string_into_int_diagnostics(ctx, &invalid);
    abi::emit_jump(ctx.emitter, &done);

    // The guard dominates this operation; keep an explicit refusal for malformed EIR callers.
    ctx.emitter.label(&invalid);
    super::exceptions::emit_type_error(ctx, "Invalid value for implicit integer conversion");
    ctx.emitter.label(&done);
    super::store_if_result(ctx, inst)
}
