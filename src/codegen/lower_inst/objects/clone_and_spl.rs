//! Purpose:
//! Lowers shallow object clones and runtime-managed SPL constructors.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Dynamic hashes, callbacks, and runtime-managed payloads preserve ownership rules.

use super::*;

/// Lowers PHP object cloning for fixed-class receivers.
pub(in crate::codegen::lower_inst) fn lower_object_clone_shallow(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let source = expect_operand(inst, 0)?;
    let class_name = class_name_immediate(ctx, inst)?.to_string();
    if is_builtin_stdclass(&class_name) {
        return lower_stdclass_clone(ctx, inst, source);
    }
    if is_runtime_managed_object_clone_class(&class_name) {
        return Err(CodegenIrError::unsupported(format!(
            "clone for runtime-managed class {}",
            class_name
        )));
    }
    let (
        class_id,
        property_count,
        allow_dynamic_properties,
        retained_offsets,
        owned_reference_property_offsets,
    ) = {
        let class_info =
            ctx.module.class_infos.get(&class_name).ok_or_else(|| {
                CodegenIrError::unsupported(format!("unknown class {}", class_name))
            })?;
        let retained_offsets = cloned_property_retain_offsets(class_info);
        let owned_reference_property_offsets = owned_reference_property_offsets(class_info);
        (
            class_info.class_id,
            class_info.properties.len(),
            class_info.allow_dynamic_properties,
            retained_offsets,
            owned_reference_property_offsets,
        )
    };
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("object_clone_shallow missing result value"))?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(source, result_reg)?;
    abi::emit_push_reg(ctx.emitter, result_reg);
    emit_named_class_object_allocation(
        ctx,
        &class_name,
        class_id,
        property_count,
        allow_dynamic_properties,
        &[],
        &owned_reference_property_offsets,
    )?;
    ctx.store_result_value(result)?;
    let source_reg = abi::secondary_scratch_reg(ctx.emitter);
    let dest_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_pop_reg(ctx.emitter, source_reg);
    ctx.load_value_to_reg(result, dest_reg)?;
    emit_clone_declared_property_slots(ctx, source_reg, dest_reg, property_count, &retained_offsets);
    if allow_dynamic_properties {
        emit_clone_dynamic_property_hash(
            ctx,
            source_reg,
            dest_reg,
            dynamic_property_hash_offset(
                property_count
                    + crate::internal_extensions::hidden_slot_count_for(
                        &ctx.module.class_infos,
                        &class_name,
                    ),
            ),
        );
    }
    Ok(())
}

/// Lowers `clone` for `stdClass`, whose payload is just class id plus dynamic-property hash.
pub(super) fn lower_stdclass_clone(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    source: ValueId,
) -> Result<()> {
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("stdClass clone missing result value"))?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(source, result_reg)?;
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_new");
    ctx.store_result_value(result)?;
    let source_reg = abi::secondary_scratch_reg(ctx.emitter);
    let dest_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_pop_reg(ctx.emitter, source_reg);
    ctx.load_value_to_reg(result, dest_reg)?;
    emit_clone_dynamic_property_hash(ctx, source_reg, dest_reg, 8);
    Ok(())
}

/// Lowers `new stdClass()` through the runtime helper that seeds its dynamic-property hash.
pub(super) fn lower_stdclass_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if !inst.operands.is_empty() {
        return Err(CodegenIrError::unsupported(format!(
            "stdClass constructor with {} EIR operands",
            inst.operands.len()
        )));
    }
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_new");
    store_if_result(ctx, inst)
}

/// Returns true when the class uses the runtime-managed SPL doubly-linked-list payload.
pub(super) fn is_spl_doubly_linked_list_family(class_name: &str) -> bool {
    matches!(class_name, "SplDoublyLinkedList" | "SplStack" | "SplQueue")
}

/// Returns true for object classes whose payload is not the generic declared-property layout.
pub(super) fn is_runtime_managed_object_clone_class(class_name: &str) -> bool {
    let class_name = class_name.trim_start_matches('\\');
    is_fiber_class(class_name)
        || class_name == "Generator"
        || reflection::is_reflection_owner_class(class_name)
        || class_name == "CallbackFilterIterator"
        || class_name == "RecursiveCallbackFilterIterator"
        || class_name == "IteratorIterator"
        || is_spl_doubly_linked_list_family(class_name)
        || class_name == "SplFixedArray"
}

/// Lowers `new SplDoublyLinkedList`, `new SplStack`, and `new SplQueue`.
pub(super) fn lower_spl_doubly_linked_list_new(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
) -> Result<()> {
    if !inst.operands.is_empty() {
        return Err(CodegenIrError::unsupported(format!(
            "{} constructor with {} EIR operands",
            class_name,
            inst.operands.len()
        )));
    }
    let class_id = ctx
        .module
        .class_infos
        .get(class_name)
        .map(|info| info.class_id)
        .ok_or_else(|| CodegenIrError::unsupported(format!("unknown class {}", class_name)))?;
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        class_id as i64,
    );
    abi::emit_call_label(ctx.emitter, "__rt_spl_dll_new");
    store_if_result(ctx, inst)
}

/// Lowers `new SplFixedArray($size = 0)` through the runtime-backed payload allocator.
pub(super) fn lower_spl_fixed_array_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() > 1 {
        return Err(CodegenIrError::unsupported(format!(
            "SplFixedArray constructor with {} EIR operands",
            inst.operands.len()
        )));
    }
    let class_id = ctx
        .module
        .class_infos
        .get("SplFixedArray")
        .map(|info| info.class_id)
        .ok_or_else(|| CodegenIrError::unsupported("unknown class SplFixedArray"))?;
    if let Some(size) = inst.operands.first().copied() {
        ctx.load_value_to_result(size)?;
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, 0),
            class_id as i64,
        );
        abi::emit_pop_reg(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1));
    } else {
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, 0),
            class_id as i64,
        );
        abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), 0);
    }
    abi::emit_call_label(ctx.emitter, "__rt_spl_fixed_new");
    store_if_result(ctx, inst)
}

/// Lowers `new CallbackFilterIterator($iterator, $callback)` with callable-array capture.
pub(super) fn lower_callback_filter_iterator_new(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
) -> Result<()> {
    if inst.operands.len() != 2 {
        return Err(CodegenIrError::unsupported(format!(
            "{} constructor with {} EIR operands",
            class_name,
            inst.operands.len()
        )));
    }
    let source = expect_operand(inst, 0)?;
    let callback = expect_operand(inst, 1)?;
    let (
        class_id,
        property_count,
        uninitialized_marker_offsets,
        property_defaults,
        callback_env_offset,
    ) = {
        let class_info =
            ctx.module.class_infos.get(class_name).ok_or_else(|| {
                CodegenIrError::unsupported(format!("unknown class {}", class_name))
            })?;
        if class_info.allow_dynamic_properties {
            return Err(CodegenIrError::unsupported(format!(
                "object allocation requiring dynamic properties for {}",
                class_name
            )));
        }
        if class_interfaces_require_missing_method_symbols(ctx, class_name, class_info) {
            return Err(CodegenIrError::unsupported(format!(
                "object allocation requiring interface method symbols not emitted by EIR for {}",
                class_name
            )));
        }
        (
            class_info.class_id,
            class_info.properties.len(),
            uninitialized_property_marker_offsets(class_info),
            collect_property_defaults(class_info, inst)?,
            class_info.property_offsets.get("callbackEnv").copied(),
        )
    };
    let inner_slot = resolve_property_slot_for_class(ctx, class_name, "inner", inst)?;
    let callback_slot = resolve_property_slot_for_class(ctx, class_name, "callback", inst)?;
    emit_object_allocation(
        ctx,
        class_id,
        property_count,
        false,
        &uninitialized_marker_offsets,
        &[],
    )?;
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("object_new missing result value"))?;
    ctx.store_result_value(result)?;
    emit_property_defaults(ctx, result, &property_defaults)?;
    if let Some(offset) = callback_env_offset {
        emit_zero_pointer_property(ctx, result, offset)?;
    }
    emit_callback_filter_source_property(ctx, source, result, &inner_slot, inst)?;
    emit_callback_filter_callback_property(ctx, callback, result, &callback_slot, inst)
}

/// Stores CallbackFilterIterator::$inner from a constructor source operand.
pub(super) fn emit_callback_filter_source_property(
    ctx: &mut FunctionContext<'_>,
    source: ValueId,
    target: ValueId,
    slot: &PropertySlot,
    inst: &Instruction,
) -> Result<()> {
    let value_ty = ctx.value_php_type(source)?;
    ensure_property_value_supported(ctx, slot, source, &value_ty, inst)?;
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(target, base_reg)?;
    emit_property_store(ctx, source, slot, base_reg)
}

/// Stores CallbackFilterIterator::$callback, converting callable arrays to descriptors.
pub(super) fn emit_callback_filter_callback_property(
    ctx: &mut FunctionContext<'_>,
    callback: ValueId,
    target: ValueId,
    slot: &PropertySlot,
    inst: &Instruction,
) -> Result<()> {
    let value_ty = ctx.value_php_type(callback)?;
    match value_ty.codegen_repr() {
        PhpType::Array(elem) if matches!(elem.codegen_repr(), PhpType::Mixed | PhpType::Str) => {
            callables::emit_runtime_callable_array_descriptor_value(
                ctx,
                callback,
                "callback_filter_constructor",
            )?;
            emit_store_result_to_pointer_property(ctx, target, slot.offset)
        }
        _ => {
            ensure_property_value_supported(ctx, slot, callback, &value_ty, inst)?;
            let base_reg = abi::symbol_scratch_reg(ctx.emitter);
            ctx.load_value_to_reg(target, base_reg)?;
            emit_property_store(ctx, callback, slot, base_reg)
        }
    }
}

/// Stores the current single-register result into one pointer-sized object property.
pub(super) fn emit_store_result_to_pointer_property(
    ctx: &mut FunctionContext<'_>,
    target: ValueId,
    offset: usize,
) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    ctx.load_value_to_reg(target, base_reg)?;
    abi::emit_pop_reg(ctx.emitter, result_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, base_reg, offset);
    abi::emit_store_zero_to_address(ctx.emitter, base_reg, offset + 8);
    Ok(())
}

/// Initializes one pointer-sized object property to null.
pub(super) fn emit_zero_pointer_property(
    ctx: &mut FunctionContext<'_>,
    target: ValueId,
    offset: usize,
) -> Result<()> {
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(target, base_reg)?;
    abi::emit_store_zero_to_address(ctx.emitter, base_reg, offset);
    abi::emit_store_zero_to_address(ctx.emitter, base_reg, offset + 8);
    Ok(())
}
