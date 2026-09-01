//! Purpose:
//! Lowers fixed-class allocation and constructor setup.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Builtin special cases and generic property layout keep their existing dispatch order.

use super::*;

/// Allocates a fixed-class object and initializes properties without invoking a constructor.
pub(in crate::codegen::lower_inst) fn lower_object_new_without_constructor(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let class_name = class_name_immediate(ctx, inst)?.to_string();
    if let Some(class_id) = throwable_payload_class_id(ctx, &class_name) {
        let creation_line = inst.span.map_or(0, |span| span.line);
        emit_throwable_allocation(ctx, class_id, creation_line);
        return store_if_result(ctx, inst);
    }
    let (
        class_id,
        property_count,
        allow_dynamic_properties,
        uninitialized_marker_offsets,
        owned_reference_property_offsets,
        property_defaults,
    ) = {
        let class_info = ctx
            .module
            .class_infos
            .get(&class_name)
            .ok_or_else(|| CodegenIrError::unsupported(format!("unknown class {}", class_name)))?;
        if class_interfaces_require_missing_method_symbols(ctx, &class_name, class_info) {
            return Err(CodegenIrError::unsupported(format!(
                "constructorless object allocation requiring interface method symbols not emitted by EIR for {}",
                class_name
            )));
        }
        (
            class_info.class_id,
            class_info.properties.len(),
            class_info.allow_dynamic_properties,
            uninitialized_property_marker_offsets(class_info),
            owned_reference_property_offsets(class_info),
            collect_property_defaults(class_info, inst)?,
        )
    };
    emit_object_allocation(
        ctx,
        class_id,
        property_count,
        allow_dynamic_properties,
        &uninitialized_marker_offsets,
        &owned_reference_property_offsets,
    )?;
    let result = inst.result.ok_or_else(|| {
        CodegenIrError::invalid_module("object_new_without_constructor missing result value")
    })?;
    ctx.store_result_value(result)?;
    emit_property_defaults(ctx, result, &property_defaults)
}

/// Lowers fixed-class object allocation and optional constructor invocation.
pub(in crate::codegen::lower_inst) fn lower_object_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let class_name = class_name_immediate(ctx, inst)?.to_string();
    if is_fiber_class(&class_name) {
        return lower_fiber_new(ctx, inst);
    }
    if reflection::is_reflection_owner_class(&class_name) {
        return reflection::lower_reflection_owner_new(ctx, inst, &class_name);
    }
    if class_name == "CallbackFilterIterator" || class_name == "RecursiveCallbackFilterIterator" {
        return lower_callback_filter_iterator_new(ctx, inst, &class_name);
    }
    if is_builtin_stdclass(&class_name) {
        return lower_stdclass_new(ctx, inst);
    }
    if class_name == "IteratorIterator" {
        return lower_iterator_iterator_new(ctx, inst);
    }
    if is_spl_doubly_linked_list_family(&class_name) {
        return lower_spl_doubly_linked_list_new(ctx, inst, &class_name);
    }
    if class_name == "SplFixedArray" {
        return lower_spl_fixed_array_new(ctx, inst);
    }
    if let Some(class_id) = throwable_payload_class_id(ctx, &class_name) {
        if class_name == "DateObjectError" && ctx.function.name.ends_with("::__construct") {
            emit_date_special_trace_begin(ctx, inst, 2);
        }
        return lower_builtin_throwable_new(ctx, inst, &class_name, class_id);
    }
    let constructor_key = php_symbol_key("__construct");
    let (
        class_id,
        property_count,
        allow_dynamic_properties,
        uninitialized_marker_offsets,
        owned_reference_property_offsets,
        property_defaults,
        constructor_impl,
    ) = {
        let class_info =
            ctx.module.class_infos.get(&class_name).ok_or_else(|| {
                CodegenIrError::unsupported(format!("unknown class {}", class_name))
            })?;
        if class_interfaces_require_missing_method_symbols(ctx, &class_name, class_info) {
            return Err(CodegenIrError::unsupported(format!(
                "object allocation requiring interface method symbols not emitted by EIR for {}",
                class_name
            )));
        }
        let property_defaults = collect_property_defaults(class_info, inst)?;
        let constructor_impl = if let Some(constructor) = class_info.methods.get(&constructor_key) {
            if constructor.params.len() > inst.operands.len() {
                return Err(CodegenIrError::unsupported(format!(
                    "constructor call to {}::__construct with {} args for {} params",
                    class_name,
                    inst.operands.len(),
                    constructor.params.len()
                )));
            }
            let impl_class = class_info
                .method_impl_classes
                .get(&constructor_key)
                .cloned()
                .unwrap_or_else(|| class_name.clone());
            if !class_method_already_emitted(ctx, &impl_class, &constructor_key, false) {
                return Err(CodegenIrError::unsupported(format!(
                    "constructor call to {}::__construct without an emitted EIR method body",
                    impl_class
                )));
            }
            let param_types = constructor
                .params
                .iter()
                .map(|(_, ty)| ty.codegen_repr())
                .collect::<Vec<_>>();
            Some(ConstructorCallTarget {
                impl_class,
                param_types,
                ref_params: constructor.ref_params.clone(),
                sig: constructor.clone(),
                padding_thunk: None,
            })
        } else if !inst.operands.is_empty() && class_info.declaration_span.line == 0 {
            return Err(CodegenIrError::unsupported(format!(
                "constructor arguments for class {} without __construct",
                class_name
            )));
        } else {
            None
        };
        let marker_offsets = uninitialized_property_marker_offsets(class_info);
        let owned_ref_offsets = owned_reference_property_offsets(class_info);
        (
            class_info.class_id,
            class_info.properties.len(),
            class_info.allow_dynamic_properties,
            marker_offsets,
            owned_ref_offsets,
            property_defaults,
            constructor_impl,
        )
    };
    emit_object_allocation(
        ctx,
        class_id,
        property_count,
        allow_dynamic_properties,
        &uninitialized_marker_offsets,
        &owned_reference_property_offsets,
    )?;
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("object_new missing result value"))?;
    ctx.store_result_value(result)?;
    emit_property_defaults(ctx, result, &property_defaults)?;
    if let Some(constructor) = constructor_impl {
        let trace_scratch = abi::temp_int_reg(ctx.emitter.target);
        abi::emit_load_symbol_to_reg(ctx.emitter, trace_scratch, "_date_constructor_trace_line", 0);
        abi::emit_push_reg(ctx.emitter, trace_scratch);
        abi::emit_load_int_immediate(
            ctx.emitter,
            trace_scratch,
            inst.span.map_or(0, |span| i64::from(span.line)),
        );
        abi::emit_store_reg_to_symbol(
            ctx.emitter,
            trace_scratch,
            "_date_constructor_trace_line",
            0,
        );
        emit_constructor_call(
            ctx,
            result,
            &inst.operands[..constructor.param_types.len()],
            &class_name,
            &constructor.impl_class,
            &constructor_key,
            &constructor.param_types,
            &constructor.ref_params,
            None,
        )?;
        abi::emit_pop_reg(ctx.emitter, trace_scratch);
        abi::emit_store_reg_to_symbol(
            ctx.emitter,
            trace_scratch,
            "_date_constructor_trace_line",
            0,
        );
    }
    Ok(())
}
