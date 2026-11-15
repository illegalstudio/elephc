//! Purpose:
//! Selects borrowed raw static addresses or validated managed boxed static bindings.
//!
//! Called from:
//! - Named and late-bound static reference selection in the EIR backend.
//!
//! Key details:
//! - Logical declarations choose the immutable validator, including inherited physical storage.
//! - The process-lifetime binding owns the returned cell; callers receive a borrowed pointer.

use super::*;

/// Loads a static reference while preserving existing raw ABI paths outside checked boxed storage.
pub(super) fn load(ctx: &mut FunctionContext<'_>, slot: &StaticPropertySlot) -> Result<()> {
    let original = ctx.module.class_infos.get(&slot.declaring_class).and_then(|info|
        info.static_properties.iter().enumerate().find(|(_, (name, _))| name == &slot.property)
            .map(|(index, (_, ty))| (info.class_id, index as u32, ty)));
    let selected = ctx.module.semantic_profile.contains(elephc_semantics::StrictFeature::Casts)
        && slot.is_declared && original.is_some_and(|(_, _, ty)|
            crate::types::canonical_property_reference_payload(ty)
                == Some(crate::types::CanonicalReferencePayload::Boxed));
    if !selected {
        abi::emit_symbol_address(ctx.emitter, abi::int_result_reg(ctx.emitter), &slot.symbol);
        return Ok(());
    }
    if !ctx.module.required_runtime_features.native_reference_validation {
        return Err(CodegenIrError::invalid_module("managed static reference has no validation capability"));
    }
    let (class_id, index, _) = original.expect("selected static reference declaration");
    let descriptor = crate::codegen::runtime_metadata::native_reference_descriptor_symbol(class_id, index, true);
    abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), &slot.symbol);
    abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), &descriptor);
    abi::emit_call_label(ctx.emitter, "__rt_reference_static_mixed_bind");
    Ok(())
}
