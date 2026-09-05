//! Purpose:
//! Resolves descriptor metadata and captured receiver or called-class state.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// Returns true when the EIR module contains the concrete instance-method body.
pub(in crate::codegen) fn class_method_body_exists(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method_key: &str,
) -> bool {
    ctx.module.class_methods.iter().any(|function| {
        !function.flags.is_static
            && function
                .name
                .rsplit_once("::")
                .is_some_and(|(class, method)| {
                    class == class_name && php_symbol_key(method) == method_key
                })
    })
}

/// Allocates a runtime descriptor and stores the receiver in capture slot zero.
pub(in crate::codegen) fn emit_runtime_descriptor_with_receiver_capture(
    ctx: &mut FunctionContext<'_>,
    descriptor_label: &str,
    receiver: ValueId,
    receiver_ty: &PhpType,
) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let descriptor_reg = abi::nested_call_reg(ctx.emitter);
    let total_bytes = callable_descriptor::CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET + 16;
    ctx.load_value_to_result(receiver)?;
    if !ctx.value_can_transfer_ownership_to_consumer(receiver)? {
        abi::emit_incref_if_refcounted(ctx.emitter, receiver_ty);
    }
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, total_bytes as i64);
    abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
    ctx.emitter
        .instruction(&format!("mov {}, {}", descriptor_reg, result_reg)); // keep the runtime callable descriptor while copying its static header
    callable_descriptor::emit_copy_static_descriptor_to_runtime(
        ctx.emitter,
        descriptor_reg,
        descriptor_label,
    );
    abi::emit_pop_reg(ctx.emitter, result_reg);
    callable_descriptor::emit_store_current_result_to_runtime_capture(
        ctx.emitter,
        descriptor_reg,
        0,
        receiver_ty,
    );
    if descriptor_reg != result_reg {
        ctx.emitter
            .instruction(&format!("mov {}, {}", result_reg, descriptor_reg)); // return the receiver-bound callable descriptor
    }
    Ok(())
}

/// Allocates a runtime descriptor and stores the called-class id in capture slot zero.
pub(super) fn emit_runtime_descriptor_with_called_class_capture(
    ctx: &mut FunctionContext<'_>,
    descriptor_label: &str,
    called_class_id: &CalledClassIdArg,
) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let descriptor_reg = abi::nested_call_reg(ctx.emitter);
    let total_bytes = callable_descriptor::CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET + 16;
    materialize_called_class_id(ctx, called_class_id)?;
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, total_bytes as i64);
    abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
    ctx.emitter
        .instruction(&format!("mov {}, {}", descriptor_reg, result_reg)); // keep the runtime callable descriptor while copying its static header
    callable_descriptor::emit_copy_static_descriptor_to_runtime(
        ctx.emitter,
        descriptor_reg,
        descriptor_label,
    );
    abi::emit_pop_reg(ctx.emitter, result_reg);
    callable_descriptor::emit_store_current_result_to_runtime_capture(
        ctx.emitter,
        descriptor_reg,
        0,
        &PhpType::Int,
    );
    if descriptor_reg != result_reg {
        ctx.emitter
            .instruction(&format!("mov {}, {}", result_reg, descriptor_reg)); // return the called-class-bound callable descriptor
    }
    Ok(())
}

/// Descriptor metadata for a compile-time first-class callable target.
pub(super) struct FirstClassCallableDescriptor {
    pub(super) entry_label: Option<String>,
    pub(super) kind: u64,
    pub(super) sig: Option<FunctionSig>,
    pub(super) invocation: callable_descriptor::CallableDescriptorInvocation,
}

/// Returns static descriptor metadata for compile-time callable targets supported by EIR.
pub(super) fn first_class_callable_descriptor(
    ctx: &mut FunctionContext<'_>,
    target: &str,
    strict_php: bool,
) -> Result<Option<FirstClassCallableDescriptor>> {
    if let Some((receiver_label, method_name)) = target.rsplit_once("::") {
        return Ok(first_class_static_method_descriptor(
            ctx,
            receiver_label,
            method_name,
        ));
    }
    if ctx.has_extern_function(target) {
        return Ok(Some(FirstClassCallableDescriptor {
            entry_label: Some(ctx.emitter.target.extern_symbol(target)),
            kind: callable_descriptor::CALLABLE_DESC_KIND_EXTERN,
            sig: None,
            invocation: callable_descriptor::CallableDescriptorInvocation::named(
                callable_descriptor::CallableDescriptorShape::Extern,
                target.to_string(),
            ),
        }));
    }
    if let Some(descriptor) = first_class_builtin_descriptor(ctx, target, strict_php)? {
        return Ok(Some(descriptor));
    }
    if let Some(callee) = ctx.callable_function_by_name(target) {
        return Ok(Some(FirstClassCallableDescriptor {
            entry_label: Some(function_symbol(&callee.name)),
            kind: callable_descriptor::CALLABLE_DESC_KIND_FUNCTION,
            sig: Some(function_signature_from_eir(callee)),
            invocation: callable_descriptor::CallableDescriptorInvocation::named(
                callable_descriptor::CallableDescriptorShape::Function,
                callee.name.clone(),
            ),
        }));
    }
    Ok(None)
}

/// Returns descriptor metadata for builtin first-class callable targets.
pub(super) fn first_class_builtin_descriptor(
    ctx: &mut FunctionContext<'_>,
    target: &str,
    strict_php: bool,
) -> Result<Option<FirstClassCallableDescriptor>> {
    let name = php_symbol_key(target.trim_start_matches('\\'));
    if !crate::types::checker::builtins::is_php_visible_builtin_function_for_profile(
        &name,
        strict_php,
    ) {
        return Ok(None);
    }
    let Some(sig) = first_class_callable_builtin_sig(&name) else {
        return Ok(None);
    };
    if matches!(name.as_str(), "get_class_vars" | "get_class_methods") {
        return Ok(Some(FirstClassCallableDescriptor {
            entry_label: None,
            kind: callable_descriptor::CALLABLE_DESC_KIND_BUILTIN,
            sig: None,
            invocation: callable_descriptor::CallableDescriptorInvocation::named(
                callable_descriptor::CallableDescriptorShape::Builtin,
                name,
            ),
        }));
    }
    let wrapper_sig = runtime_builtin_wrapper_sig(&name, &callable_wrapper_sig(&sig));
    let entry_label =
        emit_runtime_builtin_wrapper_inline(ctx, &name, &wrapper_sig, strict_php)?;
    Ok(Some(FirstClassCallableDescriptor {
        entry_label: Some(entry_label),
        kind: callable_descriptor::CALLABLE_DESC_KIND_BUILTIN,
        sig: Some(wrapper_sig),
        invocation: callable_descriptor::CallableDescriptorInvocation::named(
            callable_descriptor::CallableDescriptorShape::Builtin,
            name,
        ),
    }))
}

/// Returns descriptor metadata for static methods with compile-time class receivers.
pub(super) fn first_class_static_method_descriptor(
    ctx: &mut FunctionContext<'_>,
    receiver_label: &str,
    method_name: &str,
) -> Option<FirstClassCallableDescriptor> {
    if matches!(receiver_label.trim_start_matches('\\'), "static" | "object") {
        return None;
    }
    let receiver = resolve_static_method_receiver(ctx, receiver_label).ok()?;
    let method_key = php_symbol_key(method_name);
    let receiver_info = ctx.module.class_infos.get(receiver.as_str())?;
    let impl_class = receiver_info
        .static_method_impl_classes
        .get(&method_key)
        .map(String::as_str)
        .unwrap_or(receiver.as_str());
    let sig = ctx
        .module
        .class_infos
        .get(impl_class)?
        .static_methods
        .get(&method_key)?
        .clone();
    let wrapper_sig = crate::codegen::callable_dispatch::static_method_runtime_wrapper_sig(&sig);
    let entry_label = emit_static_method_descriptor_entry_wrapper(
        ctx,
        impl_class,
        &method_key,
        &wrapper_sig,
        receiver_info.class_id,
    )
    .ok()?;
    Some(FirstClassCallableDescriptor {
        entry_label: Some(entry_label),
        kind: callable_descriptor::CALLABLE_DESC_KIND_STATIC_METHOD,
        sig: Some(wrapper_sig),
        invocation: callable_descriptor::CallableDescriptorInvocation::method(
            callable_descriptor::CallableDescriptorShape::StaticMethod,
            Some(receiver),
            method_key,
        ),
    })
}

/// Returns the callable-target string attached to `first_class_callable_new`.
pub(super) fn callable_target_data<'a>(ctx: &'a FunctionContext<'_>, inst: &Instruction) -> Result<&'a str> {
    let data = expect_data(inst)?;
    ctx.module
        .data
        .strings
        .get(data.as_raw() as usize)
        .map(String::as_str)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))
}
