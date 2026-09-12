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

/// Emits one native constructor signature registration call into the eval context.
pub(super) fn register_eval_native_constructor(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    registration: &EvalNativeConstructorRegistration,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    let (class_name_label, class_name_len) =
        ctx.data.add_string(registration.class_name.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        &class_name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        class_name_len as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        registration.signature.params.len() as i64,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_constructor");
    abi::emit_call_label(ctx.emitter, &symbol);
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
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        class_name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        class_name_len as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        param_index as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        NATIVE_DEFAULT_COMPILED,
    );
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 5),
        &function_symbol(helper_name),
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_constructor_param_default_scalar");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Emits one native constructor bridge-support registration call.
pub(super) fn register_eval_native_constructor_bridge_support(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    class_name_label: &str,
    class_name_len: usize,
    bridge_supported: bool,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        class_name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        class_name_len as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        if bridge_supported { 1 } else { 0 },
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_constructor_bridge_support");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Emits one native class-parent metadata registration call into the eval context.
pub(super) fn register_eval_native_class_parent(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    class_name: &str,
    parent_name: &str,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    let (class_name_label, class_name_len) = ctx.data.add_string(class_name.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        &class_name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        class_name_len as i64,
    );
    let (parent_name_label, parent_name_len) = ctx.data.add_string(parent_name.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        &parent_name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        parent_name_len as i64,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_class_parent");
    abi::emit_call_label(ctx.emitter, &symbol);
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
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        &property_key_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        property_key_len as i64,
    );
    let (type_label, type_len) = ctx.data.add_string(registration.type_spec.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        &type_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        type_len as i64,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_property_type");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Emits one native interface-property metadata registration call into the eval context.
pub(super) fn register_eval_native_interface_property(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    registration: &EvalNativeInterfacePropertyRegistration,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    let property_key = format!(
        "{}::{}::{}",
        registration.interface_name,
        registration.declaring_interface_name,
        registration.property_name
    );
    let (property_key_label, property_key_len) = ctx.data.add_string(property_key.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        &property_key_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        property_key_len as i64,
    );
    let (type_label, type_len) = ctx.data.add_string(registration.type_spec.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        &type_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        type_len as i64,
    );
    let mut flags = 0;
    if registration.requires_get {
        flags |= NATIVE_PROPERTY_REQUIRES_GET;
    }
    if registration.requires_set {
        flags |= NATIVE_PROPERTY_REQUIRES_SET;
    }
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 5),
        flags,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_interface_property");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Emits one native abstract-property metadata registration call into the eval context.
pub(super) fn register_eval_native_abstract_property(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    registration: &EvalNativeAbstractPropertyRegistration,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    let property_key = format!(
        "{}::{}::{}",
        registration.class_name, registration.declaring_class_name, registration.property_name
    );
    let (property_key_label, property_key_len) = ctx.data.add_string(property_key.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        &property_key_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        property_key_len as i64,
    );
    let (type_label, type_len) = ctx.data.add_string(registration.type_spec.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        &type_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        type_len as i64,
    );
    let mut flags = 0;
    if registration.requires_get {
        flags |= NATIVE_PROPERTY_REQUIRES_GET;
    }
    if registration.requires_set {
        flags |= NATIVE_PROPERTY_REQUIRES_SET;
    }
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 5),
        flags,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_abstract_property");
    abi::emit_call_label(ctx.emitter, &symbol);
}
