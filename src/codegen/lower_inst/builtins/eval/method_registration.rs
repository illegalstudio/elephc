//! Purpose:
//! Emits native method signatures, parameter metadata, and defaults for eval.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - Bridge-support flags and parameter ordering retain the existing ABI.
//! - The registered parameter COUNT stays PHYSICAL. Unlike a free function, whose eval calls go
//!   through a descriptor invoker that synthesizes the hidden `func_args` slots from the supplied
//!   arguments, a method is reached through its generated bridge directly: eval has to produce
//!   one argument per physical parameter, so it has to know they exist.
//! - What is registered PHP-VISIBLE is everything a PHP caller can observe or select: names,
//!   declared types and defaults are emitted only for the slots the source declared. A hidden
//!   slot keeps an EMPTY registered name, which is the marker Magician reads to tell the two
//!   apart. A PHP parameter name is never empty, so no eval fragment can name a hidden slot as a
//!   named argument, and Reflection never reports one.

use super::*;

/// Stages the shared leading words of a method metadata registration export.
fn stage_method_metadata_prefix(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    method_key_label: &str,
    method_key_len: usize,
    param_index: Option<usize>,
) {
    stage_eval_native_local_word(ctx, context_offset, PhpType::Pointer(None));
    stage_eval_native_label(ctx, method_key_label);
    stage_eval_native_int(ctx, method_key_len as i64);
    if let Some(param_index) = param_index {
        stage_eval_native_int(ctx, param_index as i64);
    }
}

/// Chooses the instance or static Rust eval-metadata export without target mangling.
fn method_metadata_symbol(is_static: bool, instance: &'static str, static_method: &'static str) -> &'static str {
    if is_static { static_method } else { instance }
}

/// Emits one native method signature registration call into the eval context.
pub(super) fn register_eval_native_method(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    registration: &EvalNativeMethodRegistration,
) {
    let method_key = format!("{}::{}", registration.class_name, registration.method_name);
    let (method_key_label, method_key_len) = ctx.data.add_string(method_key.as_bytes());
    stage_method_metadata_prefix(ctx, context_offset, &method_key_label, method_key_len, None);
    stage_eval_native_int(ctx, registration.signature.params.len() as i64);
    emit_eval_native_c_abi_call(
        ctx,
        method_metadata_symbol(
            registration.is_static,
            "__elephc_eval_register_native_method",
            "__elephc_eval_register_native_static_method",
        ),
        &[PhpType::Pointer(None), PhpType::Pointer(None), PhpType::Int, PhpType::Int],
    );
    register_eval_native_method_bridge_support(
        ctx,
        context_offset,
        &method_key_label,
        method_key_len,
        registration.is_static,
        registration.bridge_supported,
    );
    // The explicit shape is registered BEFORE the per-slot metadata so nothing downstream can
    // observe a signature whose visible/hidden partition is still being inferred.
    register_eval_native_method_shape(
        ctx,
        context_offset,
        &method_key_label,
        method_key_len,
        registration.is_static,
        &eval_native_signature_shape(&registration.signature),
    );
    let param_type_specs = eval_native_callable_param_type_specs(&registration.signature);
    let visible_indexes = source_declared_param_indexes(&registration.signature);
    for (index, (param_name, _)) in registration.signature.params.iter().enumerate() {
        let visible = visible_indexes.contains(&index);
        register_eval_native_method_param(
            ctx,
            context_offset,
            &method_key_label,
            method_key_len,
            registration.is_static,
            index,
            // A hidden slot registers the empty name deliberately, and registering it at all is
            // what sizes the name table to the physical parameter count.
            if visible { param_name.as_str() } else { "" },
        );
        register_eval_native_method_param_flags(
            ctx,
            context_offset,
            &method_key_label,
            method_key_len,
            registration.is_static,
            index,
            registration
                .signature
                .ref_params
                .get(index)
                .copied()
                .unwrap_or(false),
            // The variadic flag stays on its PHYSICAL slot, hidden collector included: that is
            // the slot the bridge expects the collected array in.
            signature_param_is_variadic(&registration.signature, index, param_name),
        );
        if !visible {
            continue;
        }
        if let Some(type_spec) = param_type_specs.get(index).and_then(Option::as_deref) {
            register_eval_native_method_param_type(
                ctx,
                context_offset,
                &method_key_label,
                method_key_len,
                registration.is_static,
                index,
                type_spec,
            );
        }
    }
    let default_context = EvalNativeDefaultContext::for_class(ctx.module, &registration.class_name);
    for (index, default) in registration.signature.defaults.iter().enumerate() {
        // A hidden count slot carries a synthesized `0` default that PHP never declared. Leaving
        // it unregistered is what keeps `required_param_count()` and Reflection honest.
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
            register_eval_native_method_param_default(
                ctx,
                context_offset,
                &method_key_label,
                method_key_len,
                registration.is_static,
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
            registration.is_static,
            &registration.method_name,
            index,
        );
        if ctx.module.functions.iter().any(|function| function.name == helper) {
            register_eval_native_method_compiled_default(
                ctx,
                context_offset,
                &method_key_label,
                method_key_len,
                registration.is_static,
                index,
                &helper,
            );
        }
    }
    if let Some(type_spec) = eval_native_callable_return_type_spec(&registration.signature) {
        register_eval_native_method_return_type(
            ctx,
            context_offset,
            &method_key_label,
            method_key_len,
            registration.is_static,
            &type_spec,
        );
    }
}

/// Registers a compiled Mixed-returning helper for a default outside compact eval metadata.
fn register_eval_native_method_compiled_default(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    method_key_label: &str,
    method_key_len: usize,
    is_static: bool,
    param_index: usize,
    helper_name: &str,
) {
    stage_method_metadata_prefix(
        ctx,
        context_offset,
        method_key_label,
        method_key_len,
        Some(param_index),
    );
    stage_eval_native_int(ctx, NATIVE_DEFAULT_COMPILED);
    let helper_symbol = function_symbol(helper_name);
    stage_eval_native_label(ctx, &helper_symbol);
    emit_eval_native_c_abi_call(
        ctx,
        method_metadata_symbol(
            is_static,
            "__elephc_eval_register_native_method_param_default_scalar",
            "__elephc_eval_register_native_static_method_param_default_scalar",
        ),
        &[
            PhpType::Pointer(None),
            PhpType::Pointer(None),
            PhpType::Int,
            PhpType::Int,
            PhpType::Int,
            PhpType::Pointer(None),
        ],
    );
}

/// Emits one native method bridge-support registration call.
pub(super) fn register_eval_native_method_bridge_support(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    method_key_label: &str,
    method_key_len: usize,
    is_static: bool,
    bridge_supported: bool,
) {
    stage_method_metadata_prefix(ctx, context_offset, method_key_label, method_key_len, None);
    stage_eval_native_int(ctx, i64::from(bridge_supported));
    emit_eval_native_c_abi_call(
        ctx,
        method_metadata_symbol(
            is_static,
            "__elephc_eval_register_native_method_bridge_support",
            "__elephc_eval_register_native_static_method_bridge_support",
        ),
        &[PhpType::Pointer(None), PhpType::Pointer(None), PhpType::Int, PhpType::Int],
    );
}

/// Emits one native method parameter-name registration call.
pub(super) fn register_eval_native_method_param(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    method_key_label: &str,
    method_key_len: usize,
    is_static: bool,
    param_index: usize,
    param_name: &str,
) {
    let (param_name_label, param_name_len) = ctx.data.add_string(param_name.as_bytes());
    stage_method_metadata_prefix(
        ctx,
        context_offset,
        method_key_label,
        method_key_len,
        Some(param_index),
    );
    stage_eval_native_label(ctx, &param_name_label);
    stage_eval_native_int(ctx, param_name_len as i64);
    emit_eval_native_c_abi_call(
        ctx,
        method_metadata_symbol(
            is_static,
            "__elephc_eval_register_native_method_param",
            "__elephc_eval_register_native_static_method_param",
        ),
        &[
            PhpType::Pointer(None), PhpType::Pointer(None), PhpType::Int, PhpType::Int,
            PhpType::Pointer(None), PhpType::Int,
        ],
    );
}

/// Emits one native method parameter-flags registration call.
pub(super) fn register_eval_native_method_param_flags(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    method_key_label: &str,
    method_key_len: usize,
    is_static: bool,
    param_index: usize,
    is_by_ref: bool,
    is_variadic: bool,
) {
    stage_method_metadata_prefix(ctx, context_offset, method_key_label, method_key_len, Some(param_index));
    stage_eval_native_int(ctx, i64::from(is_by_ref));
    stage_eval_native_int(ctx, i64::from(is_variadic));
    emit_eval_native_c_abi_call(
        ctx,
        method_metadata_symbol(
            is_static,
            "__elephc_eval_register_native_method_param_flags",
            "__elephc_eval_register_native_static_method_param_flags",
        ),
        &[
            PhpType::Pointer(None), PhpType::Pointer(None), PhpType::Int, PhpType::Int,
            PhpType::Int, PhpType::Int,
        ],
    );
}

/// Emits one native method parameter-type registration call.
pub(super) fn register_eval_native_method_param_type(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    method_key_label: &str,
    method_key_len: usize,
    is_static: bool,
    param_index: usize,
    type_spec: &str,
) {
    let (type_label, type_len) = ctx.data.add_string(type_spec.as_bytes());
    stage_method_metadata_prefix(ctx, context_offset, method_key_label, method_key_len, Some(param_index));
    stage_eval_native_label(ctx, &type_label);
    stage_eval_native_int(ctx, type_len as i64);
    emit_eval_native_c_abi_call(
        ctx,
        method_metadata_symbol(
            is_static,
            "__elephc_eval_register_native_method_param_type",
            "__elephc_eval_register_native_static_method_param_type",
        ),
        &[
            PhpType::Pointer(None), PhpType::Pointer(None), PhpType::Int, PhpType::Int,
            PhpType::Pointer(None), PhpType::Int,
        ],
    );
}

/// Emits one native method return-type registration call.
pub(super) fn register_eval_native_method_return_type(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    method_key_label: &str,
    method_key_len: usize,
    is_static: bool,
    type_spec: &str,
) {
    let (type_label, type_len) = ctx.data.add_string(type_spec.as_bytes());
    stage_method_metadata_prefix(ctx, context_offset, method_key_label, method_key_len, None);
    stage_eval_native_label(ctx, &type_label);
    stage_eval_native_int(ctx, type_len as i64);
    emit_eval_native_c_abi_call(
        ctx,
        method_metadata_symbol(
            is_static,
            "__elephc_eval_register_native_method_return_type",
            "__elephc_eval_register_native_static_method_return_type",
        ),
        &[
            PhpType::Pointer(None), PhpType::Pointer(None), PhpType::Int,
            PhpType::Pointer(None), PhpType::Int,
        ],
    );
}

/// Emits one native method parameter-default registration call.
pub(super) fn register_eval_native_method_param_default(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    method_key_label: &str,
    method_key_len: usize,
    is_static: bool,
    param_index: usize,
    default: &EvalNativeCallableDefault,
) {
    stage_method_metadata_prefix(ctx, context_offset, method_key_label, method_key_len, Some(param_index));
    let symbol = match default {
        EvalNativeCallableDefault::Scalar { kind, payload } => {
            stage_eval_native_int(ctx, *kind);
            stage_eval_native_int(ctx, *payload);
            if is_static {
                "__elephc_eval_register_native_static_method_param_default_scalar"
            } else {
                "__elephc_eval_register_native_method_param_default_scalar"
            }
        }
        EvalNativeCallableDefault::String(value) => {
            let (default_label, default_len) = ctx.data.add_string(value.as_bytes());
            stage_eval_native_label(ctx, &default_label);
            stage_eval_native_int(ctx, default_len as i64);
            if is_static {
                "__elephc_eval_register_native_static_method_param_default_string"
            } else {
                "__elephc_eval_register_native_method_param_default_string"
            }
        }
        EvalNativeCallableDefault::Object { .. } => {
            let spec = encode_eval_native_object_default(default);
            let (default_label, default_len) = ctx.data.add_string(&spec);
            stage_eval_native_label(ctx, &default_label);
            stage_eval_native_int(ctx, default_len as i64);
            if is_static {
                "__elephc_eval_register_native_static_method_param_default_object"
            } else {
                "__elephc_eval_register_native_method_param_default_object"
            }
        }
        EvalNativeCallableDefault::Array(_) => {
            let spec = encode_eval_native_array_default(default);
            let (default_label, default_len) = ctx.data.add_string(&spec);
            stage_eval_native_label(ctx, &default_label);
            stage_eval_native_int(ctx, default_len as i64);
            if is_static {
                "__elephc_eval_register_native_static_method_param_default_array"
            } else {
                "__elephc_eval_register_native_method_param_default_array"
            }
        }
    };
    emit_eval_native_c_abi_call(
        ctx,
        symbol,
        &[
            PhpType::Pointer(None), PhpType::Pointer(None), PhpType::Int, PhpType::Int,
            PhpType::Int, PhpType::Int,
        ],
    );
}

/// Emits one native method explicit PHP signature shape registration call.
///
/// The instance and static entry points differ only in the symbol, exactly like every other
/// method registration emitter, and both materialize six words through the native C ABI planner.
pub(super) fn register_eval_native_method_shape(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    method_key_label: &str,
    method_key_len: usize,
    is_static: bool,
    shape: &EvalNativeSignatureShape,
) {
    stage_method_metadata_prefix(ctx, context_offset, method_key_label, method_key_len, None);
    stage_eval_native_int(ctx, shape.visible_regular_param_count as i64);
    stage_eval_native_int(ctx, shape.required_param_count as i64);
    stage_eval_native_int(ctx, shape.flags());
    emit_eval_native_c_abi_call(
        ctx,
        method_metadata_symbol(
            is_static,
            "__elephc_eval_register_native_method_shape",
            "__elephc_eval_register_native_static_method_shape",
        ),
        &[
            PhpType::Pointer(None), PhpType::Pointer(None), PhpType::Int,
            PhpType::Int, PhpType::Int, PhpType::Int,
        ],
    );
}
