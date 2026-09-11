//! Purpose:
//! Emits AOT free-function registration and invoker metadata for eval.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - Visibility and descriptor-invoker compatibility remain explicit gates.
//! - Inline invokers restore the caller's ELF section before registration resumes.
//! - Only PHP-VISIBLE parameters are registered. The hidden slots `crate::func_args` appends
//!   (`__elephc_func_argc`, the `__elephc_func_args` collector) are compiler-internal and must
//!   never become eval-bindable parameters: the invoker synthesizes them from the container.
//!   Registering them would also make eval's own arity, named-argument and reflection metadata
//!   describe a signature the PHP source never declared.

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
    let invoker_label = emit_eval_native_function_invoker_inline(ctx, &registration.signature, &registration.name);
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
    let visible_indexes = source_declared_param_indexes(&registration.signature);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 5),
        visible_indexes.len() as i64,
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
    // The explicit shape is registered BEFORE the per-slot metadata so nothing downstream can
    // observe a signature whose arity is still being inferred from the registered defaults.
    register_eval_native_function_shape(
        ctx,
        context_offset,
        &name_label,
        name_len,
        &eval_native_signature_shape(&registration.signature),
    );
    let param_type_specs = eval_native_callable_param_type_specs(&registration.signature);
    let default_context = EvalNativeDefaultContext::global(ctx.module);
    for (registered_index, physical_index) in visible_indexes.into_iter().enumerate() {
        // `source_declared_param_indexes` only yields in-range indexes, so this loop registers exactly
        // the parameter count emitted above even for a malformed signature.
        let Some((param_name, _)) = registration.signature.params.get(physical_index) else {
            debug_assert!(false, "visible parameter indexes must address a declared parameter");
            continue;
        };
        register_eval_native_function_param(
            ctx,
            context_offset,
            &name_label,
            name_len,
            registered_index,
            param_name,
        );
        let is_variadic =
            signature_param_is_variadic(&registration.signature, physical_index, param_name);
        register_eval_native_function_param_flags(
            ctx,
            context_offset,
            &name_label,
            name_len,
            registered_index,
            registration
                .signature
                .ref_params
                .get(physical_index)
                .copied()
                .unwrap_or(false),
            is_variadic,
        );
        if let Some(type_spec) = param_type_specs
            .get(physical_index)
            .and_then(Option::as_deref)
        {
            register_eval_native_function_param_type(
                ctx,
                context_offset,
                &name_label,
                name_len,
                registered_index,
                type_spec,
            );
        }
        // A variadic parameter has no default in PHP; only regular slots carry one.
        if is_variadic {
            continue;
        }
        let Some(default) = registration
            .signature
            .defaults
            .get(physical_index)
            .and_then(Option::as_ref)
            .and_then(|expr| eval_native_callable_default(expr, &default_context))
        else {
            continue;
        };
        register_eval_native_function_param_default(
            ctx,
            context_offset,
            &name_label,
            name_len,
            registered_index,
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

/// Returns the PHYSICAL parameter indexes the PHP source itself declared, in declaration order.
///
/// This is the compiler-side selection every eval registration emitter walks: a free function
/// registers only these slots (the descriptor invoker synthesizes the rest), while a method or
/// constructor registers every physical slot and uses this list only to decide which of them get
/// a PHP name, declared type and default. It is deliberately NOT the same question as
/// `NativeCallableSignature::visible_param_indexes` in Magician, which answers physical-slot
/// visibility for an already registered signature; that one reads the shape this module emits.
///
/// The prefix is `regular_param_count`, which already excludes the hidden `__elephc_func_argc`
/// slot sitting right after the visible regulars. A SOURCE-declared variadic keeps its own
/// trailing slot; the hidden `__elephc_func_args` collector, which occupies the same physical
/// position when the source declares no variadic, is deliberately dropped.
///
/// Every returned index addresses a declared parameter. `regular_param_count` never exceeds
/// `params.len()`, and the variadic slot is only appended when a slot past the visible prefix
/// actually exists, so a degenerate signature (a `variadic` name with no parameter behind it)
/// yields a shorter list rather than an index the caller has to skip. That is what keeps the
/// registered parameter COUNT and the registration loop in agreement: the count is this list's
/// length, and the loop walks this same list.
pub(super) fn source_declared_param_indexes(signature: &FunctionSig) -> Vec<usize> {
    // `regular_param_count` subtracts the variadic and hidden slots from `params.len()`, so it
    // is never larger than it and needs no clamp.
    let visible_regular = crate::types::call_args::regular_param_count(signature);
    let mut indexes: Vec<usize> = (0..visible_regular).collect();
    let source_variadic = signature
        .variadic
        .as_deref()
        .is_some_and(|variadic| variadic != crate::func_args::HIDDEN_ARGS_PARAM);
    // The variadic always owns the last physical slot, after any hidden argc parameter.
    let variadic_slot = signature.params.len().checked_sub(1);
    if let (true, Some(variadic_slot)) = (source_variadic, variadic_slot) {
        if variadic_slot >= visible_regular {
            indexes.push(variadic_slot);
        }
    }
    indexes
}

/// Emits an eval-safe descriptor invoker for a registered native free function.
pub(super) fn emit_eval_native_function_invoker_inline(
    ctx: &mut FunctionContext<'_>,
    sig: &FunctionSig,
    name: &str,
) -> String {
    ctx.shared.callable_argument_normalizer |=
        crate::codegen::runtime_callable_invoker::needs_callable_argument_normalizer(sig);
    let label = ctx.next_global_label("eval_callable_invoker");
    let done_label = ctx.next_label("eval_callable_invoker_done");
    let captures: [(String, PhpType, bool); 0] = [];
    let invoker = RuntimeCallableInvoker {
        label: &label,
        sig,
        captures: &captures,
        owns_string_return: ctx.module.functions.iter().find(|function| function.name == name)
            .is_some_and(crate::codegen::runtime_callable_invoker::function_returns_owned_string),
    };
    let enclosing = ctx.emitter.current_text_section();
    abi::emit_jump(ctx.emitter, &done_label);
    crate::codegen::runtime_callable_invoker::emit_runtime_callable_invoker_with_exception_boundary(
        ctx.emitter,
        ctx.data,
        &invoker,
    );
    ctx.emitter.reopen_text_section(enclosing);
    ctx.emitter.label(&done_label);
    label
}
