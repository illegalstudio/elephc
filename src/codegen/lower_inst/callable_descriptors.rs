//! Purpose:
//! Builds first-class callable descriptors for functions, methods, and captured receivers.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// Materializes a first-class callable value as a static descriptor pointer when possible.
pub(super) fn lower_first_class_callable_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let target = callable_target_data(ctx, inst)?.to_string();
    let strict_php = instruction_strict_php_profile(inst);
    if emit_static_late_bound_first_class_callable(ctx, &target)? {
        return store_if_result(ctx, inst);
    }
    if emit_instance_method_first_class_callable(ctx, inst, &target)? {
        return store_if_result(ctx, inst);
    }
    if let Some(descriptor) = first_class_callable_descriptor(ctx, &target, strict_php)? {
        let invoker_label = descriptor
            .sig
            .as_ref()
            .map(|sig| emit_runtime_callable_invoker_inline(ctx, sig, &[]));
        let descriptor_label = match descriptor.entry_label.as_deref() {
            Some(entry_label) => {
                callable_descriptor::static_descriptor_with_optional_invoker_meta(
                    ctx.data,
                    entry_label,
                    Some(&target),
                    descriptor.kind,
                    descriptor.sig.as_ref(),
                    &[],
                    &[],
                    descriptor.invocation,
                    invoker_label.as_deref(),
                )
            }
            None => callable_descriptor::static_only_descriptor(
                ctx.data,
                &target,
                descriptor.kind,
                descriptor.invocation,
            ),
        };
        // `f(...)` produces a Closure in PHP and therefore consumes an object
        // handle, exactly like `function () {}` does. Give it the same runtime
        // descriptor storage so the handle can be bound at creation and returned
        // when the descriptor is released — see `lower_closure_new`.
        emit_runtime_closure_descriptor_with_captures(ctx, &descriptor_label, &[], &[])?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    }
    store_if_result(ctx, inst)
}

/// Emits a runtime descriptor for `static::method(...)` first-class callables.
pub(super) fn emit_static_late_bound_first_class_callable(
    ctx: &mut FunctionContext<'_>,
    target: &str,
) -> Result<bool> {
    let Some((receiver_label, method_name)) = target.rsplit_once("::") else {
        return Ok(false);
    };
    if receiver_label.trim_start_matches('\\') != "static" {
        return Ok(false);
    }

    let receiver = resolve_static_method_receiver(ctx, receiver_label)?;
    let called_class_id = resolve_static_called_class_arg(ctx, receiver_label, &receiver)?;
    let receiver_info = ctx
        .module
        .class_infos
        .get(receiver.as_str())
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "late-bound first-class callable '{}' on unknown class '{}'",
                target, receiver
            ))
        })?;
    let method_key = php_symbol_key(method_name);
    let impl_class = receiver_info
        .static_method_impl_classes
        .get(&method_key)
        .cloned()
        .unwrap_or_else(|| receiver.clone());
    let dynamic_slot = receiver_info.static_vtable_slots.get(&method_key).copied();
    let sig = ctx
        .module
        .class_infos
        .get(impl_class.as_str())
        .and_then(|class_info| class_info.static_methods.get(&method_key))
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "late-bound first-class callable '{}' with unknown implementation",
                target
            ))
        })?
        .clone();
    let wrapper_sig = crate::codegen::callable_dispatch::static_method_runtime_wrapper_sig(&sig);
    let captures = vec![("called_class_id".to_string(), PhpType::Int, false)];
    let entry_label = emit_static_late_bound_descriptor_entry_wrapper(
        ctx,
        impl_class.as_str(),
        &method_key,
        &wrapper_sig,
        dynamic_slot,
    )?;
    let invoker_label = emit_runtime_callable_invoker_inline(ctx, &wrapper_sig, &captures);
    let descriptor_label = callable_descriptor::static_descriptor_with_optional_invoker_meta(
        ctx.data,
        &entry_label,
        Some(target),
        callable_descriptor::CALLABLE_DESC_KIND_STATIC_METHOD,
        Some(&wrapper_sig),
        &captures,
        &[],
        callable_descriptor::CallableDescriptorInvocation::method(
            callable_descriptor::CallableDescriptorShape::StaticMethod,
            Some("static".to_string()),
            method_key.clone(),
        ),
        Some(&invoker_label),
    );
    emit_runtime_descriptor_with_called_class_capture(ctx, &descriptor_label, &called_class_id)?;
    crate::codegen_support::runtime::emit_acquire_object_handle(ctx.emitter); // `static::m(...)` is a Closure and consumes an object handle
    Ok(true)
}

/// Emits a runtime descriptor for receiver-bound `object::method` first-class callables.
pub(super) fn emit_instance_method_first_class_callable(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: &str,
) -> Result<bool> {
    let Some((receiver_label, method_name)) = target.rsplit_once("::") else {
        return Ok(false);
    };
    if receiver_label.trim_start_matches('\\') != "object" {
        return Ok(false);
    }
    let receiver = inst.operands.first().copied().ok_or_else(|| {
        CodegenIrError::invalid_module(format!(
            "instance first-class callable '{}' has no receiver operand",
            target
        ))
    })?;
    let receiver_ty = ctx.value_php_type(receiver)?;
    let PhpType::Object(class_name) = receiver_ty.codegen_repr() else {
        return Err(CodegenIrError::unsupported(format!(
            "instance first-class callable '{}' with receiver PHP type {:?}",
            target, receiver_ty
        )));
    };
    let normalized_class = class_name.trim_start_matches('\\').to_string();
    let method_key = php_symbol_key(method_name);
    let class_info = ctx
        .module
        .class_infos
        .get(normalized_class.as_str())
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "instance first-class callable '{}' with unknown receiver class '{}'",
                target, normalized_class
            ))
        })?;
    let sig = class_info
        .methods
        .get(&method_key)
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "instance first-class callable '{}' with unknown method",
                target
            ))
        })?
        .clone();
    let impl_class = class_info
        .method_impl_classes
        .get(&method_key)
        .cloned()
        .unwrap_or_else(|| normalized_class.clone());
    if !class_method_body_exists(ctx, &impl_class, &method_key) {
        return Err(CodegenIrError::unsupported(format!(
            "instance first-class callable '{}' without emitted method body",
            target
        )));
    }
    let receiver_ty = PhpType::Object(normalized_class.clone());
    let captures = vec![("receiver".to_string(), receiver_ty.clone(), false)];
    let entry_label =
        emit_instance_method_descriptor_entry_wrapper(ctx, &impl_class, &method_key, &sig)?;
    let invoker_label = emit_runtime_callable_invoker_inline(ctx, &sig, &captures);
    let descriptor_label = callable_descriptor::static_descriptor_with_optional_invoker_meta(
        ctx.data,
        &entry_label,
        Some(target),
        callable_descriptor::CALLABLE_DESC_KIND_FIRST_CLASS,
        Some(&sig),
        &captures,
        &[],
        callable_descriptor::CallableDescriptorInvocation::method(
            callable_descriptor::CallableDescriptorShape::InstanceMethod,
            Some(normalized_class),
            method_name,
        ),
        Some(&invoker_label),
    );
    emit_runtime_descriptor_with_receiver_capture(ctx, &descriptor_label, receiver, &receiver_ty)?;
    // `$o->m(...)` is a Closure in PHP and consumes an object handle. The acquire
    // sits here rather than inside the shared descriptor helper because that helper
    // also builds the internal adapter for calling an `__invoke`-able object, and
    // `$obj()` creates no Closure in PHP.
    crate::codegen_support::runtime::emit_acquire_object_handle(ctx.emitter);
    Ok(true)
}
