//! Purpose:
//! Emits constructor, class-parent, and property-contract registrations for eval.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - The registration calls preserve target-aware argument materialization.
//! - Constructors follow the method contract exactly: the registered parameter COUNT is the
//!   bridge's PHYSICAL one, because eval calls that bridge directly, while names, declared types
//!   and defaults describe only the PHP-visible signature. A hidden `func_args` slot keeps an
//!   empty registered name so it can be neither named nor reflected.

use super::*;

/// Stages the shared leading context/class-name words of a constructor metadata export.
fn stage_constructor_registration_prefix(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    class_name_label: &str,
    class_name_len: usize,
) {
    stage_eval_native_local_word(ctx, context_offset, PhpType::Pointer(None));
    stage_eval_native_label(ctx, class_name_label);
    stage_eval_native_int(ctx, class_name_len as i64);
}

/// Emits one native constructor signature registration call into the eval context.
pub(super) fn register_eval_native_constructor(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    registration: &EvalNativeConstructorRegistration,
) {
    let (class_name_label, class_name_len) =
        ctx.data.add_string(registration.class_name.as_bytes());
    stage_constructor_registration_prefix(ctx, context_offset, &class_name_label, class_name_len);
    stage_eval_native_int(ctx, registration.signature.params.len() as i64);
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_register_native_constructor",
        &[PhpType::Pointer(None), PhpType::Pointer(None), PhpType::Int, PhpType::Int],
    );
    register_eval_native_constructor_bridge_support(
        ctx,
        context_offset,
        &class_name_label,
        class_name_len,
        registration.bridge_supported,
    );
    // The explicit shape is registered BEFORE the per-slot metadata so nothing downstream can
    // observe a signature whose visible/hidden partition is still being inferred.
    register_eval_native_constructor_shape(
        ctx,
        context_offset,
        &class_name_label,
        class_name_len,
        &eval_native_signature_shape(&registration.signature),
    );
    let param_type_specs = eval_native_callable_param_type_specs(&registration.signature);
    let visible_indexes = source_declared_param_indexes(&registration.signature);
    for (index, (param_name, _)) in registration.signature.params.iter().enumerate() {
        let visible = visible_indexes.contains(&index);
        register_eval_native_constructor_param(
            ctx,
            context_offset,
            &class_name_label,
            class_name_len,
            index,
            // Registering the empty name for a hidden slot is what sizes the name table to the
            // physical parameter count, which is how Magician tells the two kinds apart.
            if visible { param_name.as_str() } else { "" },
        );
        register_eval_native_constructor_param_flags(
            ctx,
            context_offset,
            &class_name_label,
            class_name_len,
            index,
            registration
                .signature
                .ref_params
                .get(index)
                .copied()
                .unwrap_or(false),
            // The variadic flag stays on its PHYSICAL slot so the collected array reaches the
            // parameter the generated constructor bridge actually declares.
            signature_param_is_variadic(&registration.signature, index, param_name),
        );
        if !visible {
            continue;
        }
        if let Some(type_spec) = param_type_specs.get(index).and_then(Option::as_deref) {
            register_eval_native_constructor_param_type(
                ctx,
                context_offset,
                &class_name_label,
                class_name_len,
                index,
                type_spec,
            );
        }
    }
    let default_context = EvalNativeDefaultContext::for_class(ctx.module, &registration.class_name);
    for (index, default) in registration.signature.defaults.iter().enumerate() {
        // The hidden count slot's synthesized `0` is not a PHP default and must not be reported.
        if !visible_indexes.contains(&index) {
            continue;
        }
        // A source-declared variadic has no PHP default: it always collects, so registering one
        // would report an optional the source never wrote. Free-function, method and constructor
        // default registration all skip the variadic slot for that same reason.
        if signature_param_is_variadic(
            &registration.signature,
            index,
            registration
                .signature
                .params
                .get(index)
                .map(|(name, _)| name.as_str())
                .unwrap_or_default(),
        ) {
            continue;
        }
        if let Some(default) = default
            .as_ref()
            .and_then(|expr| eval_native_callable_default(expr, &default_context))
        {
            register_eval_native_constructor_param_default(
                ctx,
                context_offset,
                &class_name_label,
                class_name_len,
                index,
                &default,
            );
            continue;
        }
        let Some(class_info) = ctx.module.class_infos.get(&registration.class_name) else {
            continue;
        };
        let helper = crate::ir_lower::eval_native_default_helper_name(
            class_info.class_id,
            false,
            "__construct",
            index,
        );
        if ctx.module.functions.iter().any(|function| function.name == helper) {
            register_eval_native_constructor_compiled_default(
                ctx,
                context_offset,
                &class_name_label,
                class_name_len,
                index,
                &helper,
            );
        }
    }
}

/// Registers a compiled Mixed-returning helper for a default outside compact eval metadata.
fn register_eval_native_constructor_compiled_default(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    class_name_label: &str,
    class_name_len: usize,
    param_index: usize,
    helper_name: &str,
) {
    stage_constructor_registration_prefix(ctx, context_offset, class_name_label, class_name_len);
    stage_eval_native_int(ctx, param_index as i64);
    stage_eval_native_int(ctx, NATIVE_DEFAULT_COMPILED);
    let helper_symbol = function_symbol(helper_name);
    stage_eval_native_label(ctx, &helper_symbol);
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_register_native_constructor_param_default_scalar",
        &[
            PhpType::Pointer(None), PhpType::Pointer(None), PhpType::Int,
            PhpType::Int, PhpType::Int, PhpType::Pointer(None),
        ],
    );
}

/// Emits one native constructor bridge-support registration call.
pub(super) fn register_eval_native_constructor_bridge_support(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    class_name_label: &str,
    class_name_len: usize,
    bridge_supported: bool,
) {
    stage_constructor_registration_prefix(ctx, context_offset, class_name_label, class_name_len);
    stage_eval_native_int(ctx, i64::from(bridge_supported));
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_register_native_constructor_bridge_support",
        &[PhpType::Pointer(None), PhpType::Pointer(None), PhpType::Int, PhpType::Int],
    );
}

/// Emits one native class-parent metadata registration call into the eval context.
pub(super) fn register_eval_native_class_parent(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    class_name: &str,
    parent_name: &str,
) {
    let (class_name_label, class_name_len) = ctx.data.add_string(class_name.as_bytes());
    let (parent_name_label, parent_name_len) = ctx.data.add_string(parent_name.as_bytes());
    stage_constructor_registration_prefix(ctx, context_offset, &class_name_label, class_name_len);
    stage_eval_native_label(ctx, &parent_name_label);
    stage_eval_native_int(ctx, parent_name_len as i64);
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_register_native_class_parent",
        &[
            PhpType::Pointer(None), PhpType::Pointer(None), PhpType::Int,
            PhpType::Pointer(None), PhpType::Int,
        ],
    );
}

/// Emits one native property-type metadata registration call into the eval context.
pub(super) fn register_eval_native_property_type(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    registration: &EvalNativePropertyTypeRegistration,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    let property_key = format!(
        "{}::{}",
        registration.class_name, registration.property_name
    );
    let (property_key_label, property_key_len) = ctx.data.add_string(property_key.as_bytes());
    let (type_label, type_len) = ctx.data.add_string(registration.type_spec.as_bytes());
    stage_constructor_registration_prefix(ctx, context_offset, &property_key_label, property_key_len);
    stage_eval_native_label(ctx, &type_label);
    stage_eval_native_int(ctx, type_len as i64);
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_register_native_property_type",
        &[
            PhpType::Pointer(None), PhpType::Pointer(None), PhpType::Int,
            PhpType::Pointer(None), PhpType::Int,
        ],
    );
}

/// Emits one native interface-property metadata registration call into the eval context.
pub(super) fn register_eval_native_interface_property(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    registration: &EvalNativeInterfacePropertyRegistration,
) {
    let property_key = format!(
        "{}::{}::{}",
        registration.interface_name,
        registration.declaring_interface_name,
        registration.property_name
    );
    let (property_key_label, property_key_len) = ctx.data.add_string(property_key.as_bytes());
    let (type_label, type_len) = ctx.data.add_string(registration.type_spec.as_bytes());
    let mut flags = 0;
    if registration.requires_get {
        flags |= NATIVE_PROPERTY_REQUIRES_GET;
    }
    if registration.requires_set {
        flags |= NATIVE_PROPERTY_REQUIRES_SET;
    }
    stage_constructor_registration_prefix(ctx, context_offset, &property_key_label, property_key_len);
    stage_eval_native_label(ctx, &type_label);
    stage_eval_native_int(ctx, type_len as i64);
    stage_eval_native_int(ctx, flags);
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_register_native_interface_property",
        &[
            PhpType::Pointer(None), PhpType::Pointer(None), PhpType::Int,
            PhpType::Pointer(None), PhpType::Int, PhpType::Int,
        ],
    );
}

/// Emits one native abstract-property metadata registration call into the eval context.
pub(super) fn register_eval_native_abstract_property(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    registration: &EvalNativeAbstractPropertyRegistration,
) {
    let property_key = format!(
        "{}::{}::{}",
        registration.class_name, registration.declaring_class_name, registration.property_name
    );
    let (property_key_label, property_key_len) = ctx.data.add_string(property_key.as_bytes());
    let (type_label, type_len) = ctx.data.add_string(registration.type_spec.as_bytes());
    let mut flags = 0;
    if registration.requires_get {
        flags |= NATIVE_PROPERTY_REQUIRES_GET;
    }
    if registration.requires_set {
        flags |= NATIVE_PROPERTY_REQUIRES_SET;
    }
    stage_constructor_registration_prefix(ctx, context_offset, &property_key_label, property_key_len);
    stage_eval_native_label(ctx, &type_label);
    stage_eval_native_int(ctx, type_len as i64);
    stage_eval_native_int(ctx, flags);
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_register_native_abstract_property",
        &[
            PhpType::Pointer(None), PhpType::Pointer(None), PhpType::Int,
            PhpType::Pointer(None), PhpType::Int, PhpType::Int,
        ],
    );
}
