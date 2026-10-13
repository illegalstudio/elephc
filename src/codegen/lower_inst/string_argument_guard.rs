//! Purpose:
//! Checks declared string arguments before their scalar storage conversion.
//!
//! Called from:
//! - EIR StringArgumentGuard and StrictStringArgumentGuard instruction lowering.
//!
//! Key details:
//! - Reuses descriptor preflight; validation borrows the input and never stringifies it.
//! - Strict binding accepts only strings, while weak binding also permits scalar/Stringable values.
//! - Target-specific tag/metadata/TypeError mechanics stay in the shared validator.

use super::*;

/// Validates one named parameter with the shared strict/weak string rule without conversion.
pub(super) fn lower(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let data = expect_data(inst)?;
    let parameter = ctx.module.data.strings.get(data.as_raw() as usize)
        .ok_or_else(|| CodegenIrError::missing_entry("parameter name", data.as_raw()))?.clone();
    let source = ctx.load_value_to_result(value)?;
    let label = ctx.next_label("string_argument_guard");
    crate::codegen::runtime_callable_invoker::emit_string_argument_validation(
        ctx.emitter, ctx.data, &label, &source, &parameter,
        inst.op == Op::StrictStringArgumentGuard,
    );
    Ok(())
}

/// Returns the same shared policy decision without allocating or throwing.
pub(super) fn lower_predicate(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let source = ctx.load_value_to_result(expect_operand(inst, 0)?)?;
    let label = ctx.next_label("string_binding_accepted");
    let strict = matches!(inst.immediate, Some(Immediate::Bool(true)));
    crate::codegen::runtime_callable_invoker::emit_string_binding_predicate(
        ctx.emitter, ctx.data, &label, &source, strict,
    );
    store_if_result(ctx, inst)
}
