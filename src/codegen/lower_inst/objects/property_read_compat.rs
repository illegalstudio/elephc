//! Purpose:
//! Compatibility helpers for virtual and undefined object-property reads.
//!
//! Called from:
//! - Object property read entry and runtime-dispatch modules.
//!
//! Key details:
//! - Preserves php-src diagnostics, property hooks, and boxed-union dispatch.

use super::*;

/// Lowers an undeclared-property read from one statically known object receiver.
pub(super) fn lower_undefined_property_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
) -> Result<()> {
    ctx.load_value_to_reg(object, abi::int_result_reg(ctx.emitter))?;
    emit_undefined_property_warning_for_loaded_object(ctx, inst, property);
    emit_boxed_null(ctx);
    store_if_result(ctx, inst)
}

/// Lowers an undeclared-property read from a boxed object-plus-scalar union.
pub(super) fn lower_union_undefined_property_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
) -> Result<()> {
    let object_label = ctx.next_label("union_undefined_prop_object");
    let done_label = ctx.next_label("union_undefined_prop_done");
    ctx.load_value_to_reg(object, abi::int_result_reg(ctx.emitter))?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_if_mixed_unboxed_object(ctx, &object_label);
    emit_dynamic_property_miss_result(ctx, inst);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&object_label);
    move_mixed_unboxed_object_payload(ctx, abi::int_result_reg(ctx.emitter));
    emit_undefined_property_warning_for_loaded_object(ctx, inst, property);
    emit_boxed_null(ctx);

    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Emits PHP's undefined-property warning for a runtime property name on a known class.
pub(super) fn emit_dynamic_undefined_property_warning(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
    property_value: ValueId,
) -> Result<()> {
    emit_property_warning_fragment(
        ctx,
        format!("\nWarning: Undefined property: {}::$", class_name).as_bytes(),
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.load_string_value_to_regs(property_value, "x1", "x2")?,
        Arch::X86_64 => ctx.load_string_value_to_regs(property_value, "rdi", "rsi")?,
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
    let source = ctx.module.source_path.as_deref().unwrap_or("Unknown");
    let line = inst.span.map_or(0, |span| span.line);
    emit_property_warning_fragment(ctx, format!(" in {source} on line {line}\n").as_bytes());
    Ok(())
}

/// Resolves the synthetic getter used by a virtual property, when the class exposes one.
pub(super) fn property_hook_get_target(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    property: &str,
) -> Result<Option<MethodCallTarget>> {
    let normalized = class_name.trim_start_matches('\\');
    let accessor = property_hook_get_method(property);
    let accessor_key = php_symbol_key(&accessor);
    let Some(class_info) = ctx.module.class_infos.get(normalized) else {
        return Ok(None);
    };
    if !class_info.methods.contains_key(&accessor_key) {
        return Ok(None);
    }
    resolve_method_call_target(ctx, normalized, &accessor, 1).map(Some)
}

/// Calls a virtual property's synthetic getter on an already-unboxed object receiver.
pub(super) fn emit_property_hook_get_result(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    receiver_reg: &str,
    slot: &PropertySlot,
    target: &MethodCallTarget,
) -> Result<()> {
    if target.by_ref_return {
        return Err(CodegenIrError::unsupported(format!(
            "by-reference property getter {}::${} on a runtime-dispatched receiver",
            slot.class_name, slot.property
        )));
    }
    let receiver_ty = PhpType::Object(slot.class_name.clone());
    let param_types = [receiver_ty.clone()];
    let ref_params = [false];
    let call_args = materialize_method_call_args_with_receiver_reg_and_refs(
        ctx,
        receiver_reg,
        &receiver_ty,
        &[object],
        &param_types,
        &ref_params,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    if let Some(dynamic_slot) = target.dynamic_slot {
        emit_dynamic_instance_method_call(ctx, dynamic_slot);
    } else {
        abi::emit_call_label(
            ctx.emitter,
            &method_symbol(&target.impl_class, &target.method_key),
        );
    }
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    materialize_loaded_property_result(ctx, inst, &slot.php_type)?;
    super::super::emit_call_arg_temp_cleanups(ctx, &call_args, inst.result)
}

/// Returns whether a known object receiver has no visible slot for a property name.
pub(super) fn object_property_is_missing(
    ctx: &FunctionContext<'_>,
    object: ValueId,
    property: &str,
) -> Result<bool> {
    let PhpType::Object(class_name) = ctx.value_php_type(object)? else {
        return Ok(false);
    };
    class_property_is_missing(ctx, &class_name, property)
}

/// Returns whether a known class has no visible property slot for a property name.
pub(super) fn class_property_is_missing(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    property: &str,
) -> Result<bool> {
    let normalized = class_name.trim_start_matches('\\');
    let class_info = ctx
        .module
        .class_infos
        .get(normalized)
        .ok_or_else(|| CodegenIrError::unsupported(format!("unknown class {}", normalized)))?;
    Ok(class_info.visible_property(property).is_none())
}
