//! Purpose:
//! Emits native constructor parameter metadata for eval.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - Names, flags, types, and defaults retain their bridge ABI order.

use super::*;

/// Stages the shared leading words of a constructor parameter metadata export.
fn stage_constructor_metadata_prefix(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    class_name_label: &str,
    class_name_len: usize,
    param_index: Option<usize>,
) {
    stage_eval_native_local_word(ctx, context_offset, PhpType::Pointer(None));
    stage_eval_native_label(ctx, class_name_label);
    stage_eval_native_int(ctx, class_name_len as i64);
    if let Some(param_index) = param_index {
        stage_eval_native_int(ctx, param_index as i64);
    }
}

/// Emits one native constructor parameter-name registration call.
pub(super) fn register_eval_native_constructor_param(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    class_name_label: &str,
    class_name_len: usize,
    param_index: usize,
    param_name: &str,
) {
    let (param_name_label, param_name_len) = ctx.data.add_string(param_name.as_bytes());
    stage_constructor_metadata_prefix(ctx, context_offset, class_name_label, class_name_len, Some(param_index));
    stage_eval_native_label(ctx, &param_name_label);
    stage_eval_native_int(ctx, param_name_len as i64);
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_register_native_constructor_param",
        &[
            PhpType::Pointer(None), PhpType::Pointer(None), PhpType::Int, PhpType::Int,
            PhpType::Pointer(None), PhpType::Int,
        ],
    );
}

/// Emits one native constructor parameter-flags registration call.
pub(super) fn register_eval_native_constructor_param_flags(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    class_name_label: &str,
    class_name_len: usize,
    param_index: usize,
    is_by_ref: bool,
    is_variadic: bool,
) {
    stage_constructor_metadata_prefix(ctx, context_offset, class_name_label, class_name_len, Some(param_index));
    stage_eval_native_int(ctx, i64::from(is_by_ref));
    stage_eval_native_int(ctx, i64::from(is_variadic));
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_register_native_constructor_param_flags",
        &[
            PhpType::Pointer(None), PhpType::Pointer(None), PhpType::Int, PhpType::Int,
            PhpType::Int, PhpType::Int,
        ],
    );
}

/// Emits one native constructor parameter-type registration call.
pub(super) fn register_eval_native_constructor_param_type(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    class_name_label: &str,
    class_name_len: usize,
    param_index: usize,
    type_spec: &str,
) {
    let (type_label, type_len) = ctx.data.add_string(type_spec.as_bytes());
    stage_constructor_metadata_prefix(ctx, context_offset, class_name_label, class_name_len, Some(param_index));
    stage_eval_native_label(ctx, &type_label);
    stage_eval_native_int(ctx, type_len as i64);
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_register_native_constructor_param_type",
        &[
            PhpType::Pointer(None), PhpType::Pointer(None), PhpType::Int, PhpType::Int,
            PhpType::Pointer(None), PhpType::Int,
        ],
    );
}

/// Emits one native constructor parameter-default registration call.
pub(super) fn register_eval_native_constructor_param_default(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    class_name_label: &str,
    class_name_len: usize,
    param_index: usize,
    default: &EvalNativeCallableDefault,
) {
    stage_constructor_metadata_prefix(ctx, context_offset, class_name_label, class_name_len, Some(param_index));
    let symbol = match default {
        EvalNativeCallableDefault::Scalar { kind, payload } => {
            stage_eval_native_int(ctx, *kind);
            stage_eval_native_int(ctx, *payload);
            "__elephc_eval_register_native_constructor_param_default_scalar"
        }
        EvalNativeCallableDefault::String(value) => {
            let (default_label, default_len) = ctx.data.add_string(value.as_bytes());
            stage_eval_native_label(ctx, &default_label);
            stage_eval_native_int(ctx, default_len as i64);
            "__elephc_eval_register_native_constructor_param_default_string"
        }
        EvalNativeCallableDefault::Object { .. } => {
            let spec = encode_eval_native_object_default(default);
            let (default_label, default_len) = ctx.data.add_string(&spec);
            stage_eval_native_label(ctx, &default_label);
            stage_eval_native_int(ctx, default_len as i64);
            "__elephc_eval_register_native_constructor_param_default_object"
        }
        EvalNativeCallableDefault::Array(_) => {
            let spec = encode_eval_native_array_default(default);
            let (default_label, default_len) = ctx.data.add_string(&spec);
            stage_eval_native_label(ctx, &default_label);
            stage_eval_native_int(ctx, default_len as i64);
            "__elephc_eval_register_native_constructor_param_default_array"
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

/// Emits one native constructor explicit PHP signature shape registration call.
pub(super) fn register_eval_native_constructor_shape(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    class_name_label: &str,
    class_name_len: usize,
    shape: &EvalNativeSignatureShape,
) {
    stage_constructor_metadata_prefix(ctx, context_offset, class_name_label, class_name_len, None);
    stage_eval_native_int(ctx, shape.visible_regular_param_count as i64);
    stage_eval_native_int(ctx, shape.required_param_count as i64);
    stage_eval_native_int(ctx, shape.flags());
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_register_native_constructor_shape",
        &[
            PhpType::Pointer(None), PhpType::Pointer(None), PhpType::Int,
            PhpType::Int, PhpType::Int, PhpType::Int,
        ],
    );
}
