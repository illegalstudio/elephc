//! Purpose:
//! Emits AOT free-function registration and invoker metadata for eval.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - Visibility and descriptor-invoker compatibility remain explicit gates.

use super::*;

/// Returns true when eval can enforce this instance method visibility in the bridge.
pub(super) fn class_method_visibility_bridge_supported(class_info: &ClassInfo, method_name: &str) -> bool {
    class_info
        .method_visibilities
        .get(method_name)
        .is_none_or(|visibility| {
            matches!(
                visibility,
                Visibility::Public | Visibility::Protected | Visibility::Private
            )
        })
}

/// Returns true when eval can enforce this static method visibility in the bridge.
pub(super) fn class_static_method_visibility_bridge_supported(
    class_info: &ClassInfo,
    method_name: &str,
) -> bool {
    class_info
        .static_method_visibilities
        .get(method_name)
        .is_none_or(|visibility| {
            matches!(
                visibility,
                Visibility::Public | Visibility::Protected | Visibility::Private
            )
        })
}

/// Emits one native-function registration call into the just-created eval context.
pub(super) fn register_eval_native_function(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    registration: &EvalNativeFunctionRegistration,
) -> Result<()> {
    let invoker_label = emit_eval_native_function_invoker_inline(ctx, &registration.signature);
    let descriptor_label = callable_descriptor::static_descriptor_with_optional_invoker_meta(
        ctx.data,
        &function_symbol(&registration.name),
        Some(&registration.name),
        callable_descriptor::CALLABLE_DESC_KIND_FUNCTION,
        Some(&registration.signature),
        &[],
        &[],
        callable_descriptor::CallableDescriptorInvocation::named(
            callable_descriptor::CallableDescriptorShape::Function,
            registration.name.clone(),
        ),
        Some(&invoker_label),
    );
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    let (name_label, name_len) = ctx.data.add_string(registration.name.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        &name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        &descriptor_label,
    );
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        &invoker_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 5),
        registration.signature.params.len() as i64,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_function");
    abi::emit_call_label(ctx.emitter, &symbol);
    register_eval_native_function_bridge_support(
        ctx,
        context_offset,
        &name_label,
        name_len,
        registration.bridge_supported,
    );
    let param_type_specs = eval_native_callable_param_type_specs(&registration.signature);
    for (index, (param_name, _)) in registration.signature.params.iter().enumerate() {
        register_eval_native_function_param(
            ctx,
            context_offset,
            &name_label,
            name_len,
            index,
            param_name,
        );
        register_eval_native_function_param_flags(
            ctx,
            context_offset,
            &name_label,
            name_len,
            index,
            registration
                .signature
                .ref_params
                .get(index)
                .copied()
                .unwrap_or(false),
            signature_param_is_variadic(&registration.signature, index, param_name),
        );
        if let Some(type_spec) = param_type_specs.get(index).and_then(Option::as_deref) {
            register_eval_native_function_param_type(
                ctx,
                context_offset,
                &name_label,
                name_len,
                index,
                type_spec,
            );
        }
    }
    let default_context = EvalNativeDefaultContext::global(ctx.module);
    for (index, default) in registration.signature.defaults.iter().enumerate() {
        let Some(default) = default
            .as_ref()
            .and_then(|expr| eval_native_callable_default(expr, &default_context))
        else {
            continue;
        };
        register_eval_native_function_param_default(
            ctx,
            context_offset,
            &name_label,
            name_len,
            index,
            &default,
        );
    }
    if let Some(type_spec) = eval_native_callable_return_type_spec(&registration.signature) {
        register_eval_native_function_return_type(
            ctx,
            context_offset,
            &name_label,
            name_len,
            &type_spec,
        );
    }
    Ok(())
}

/// Emits an eval-safe descriptor invoker for a registered native free function.
pub(super) fn emit_eval_native_function_invoker_inline(
    ctx: &mut FunctionContext<'_>,
    sig: &FunctionSig,
) -> String {
    let label = ctx.next_global_label("eval_callable_invoker");
    let done_label = ctx.next_label("eval_callable_invoker_done");
    let captures: [(String, PhpType, bool); 0] = [];
    let invoker = RuntimeCallableInvoker {
        label: &label,
        sig,
        captures: &captures,
    };
    abi::emit_jump(ctx.emitter, &done_label);
    crate::codegen::runtime_callable_invoker::emit_runtime_callable_invoker_with_exception_boundary(
        ctx.emitter,
        ctx.data,
        &invoker,
    );
    ctx.emitter.label(&done_label);
    label
}
