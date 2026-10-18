//! Purpose:
//! Replaces sensitive backtrace arguments with independently owned value wrappers.
//!
//! Called from:
//! - Native backtrace argument collectors and printers.
//!
//! Key details:
//! - Parameter attributes are already canonicalized by name resolution.
//! - The wrapper owns a detached Mixed cell and never changes the active argument.

use crate::codegen::{abi, emit_box_current_owned_value_as_mixed, emit_box_current_value_as_mixed, Result};
use crate::codegen::context::FunctionContext;
use crate::types::PhpType;

/// Tests one source parameter's retained global SensitiveParameter marker.
pub(super) fn is_sensitive(ctx: &FunctionContext<'_>, name: &str) -> bool {
    let Some(signature) = &ctx.function.signature else { return false };
    let Some(index) = signature.params.iter().position(|(param, _)| param == name) else { return false };
    signature.param_attributes.get(index).is_some_and(|groups| groups.iter()
        .flat_map(|group| &group.attributes)
        .any(|attribute| attribute.name.as_canonical().eq_ignore_ascii_case("SensitiveParameter")))
}

/// Wraps a borrowed boxed argument, returning an owned boxed SensitiveParameterValue.
pub(super) fn wrap(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let class_id = ctx.module.class_infos.get("SensitiveParameterValue")
        .ok_or_else(|| crate::codegen::CodegenIrError::invalid_module("missing SensitiveParameterValue metadata"))?
        .class_id;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_clone");
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result);
    super::super::objects::emit_object_allocation(ctx, class_id, 1, false, &[], &[])?;
    let cell = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_pop_reg(ctx.emitter, cell);
    abi::emit_store_to_address(ctx.emitter, cell, result, 8);
    emit_box_current_owned_value_as_mixed(ctx.emitter, &PhpType::Object("SensitiveParameterValue".into()));
    Ok(())
}

/// Boxes a borrowed typed argument, wrapping it and retiring any temporary input cell.
pub(super) fn wrap_value(ctx: &mut FunctionContext<'_>, ty: &PhpType) -> Result<()> {
    let borrowed = matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_));
    if !borrowed {
        emit_box_current_value_as_mixed(ctx.emitter, &ty.codegen_repr());
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    }
    wrap(ctx)?;
    if !borrowed {
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 16);
        abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
        abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        abi::emit_release_temporary_stack(ctx.emitter, 16);
    }
    Ok(())
}
