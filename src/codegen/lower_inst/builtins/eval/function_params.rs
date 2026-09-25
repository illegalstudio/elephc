//! Purpose:
//! Emits native free-function parameter and return metadata for eval.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - Bridge-support and default registration remain signature-driven.

use super::*;

/// Stages the common leading words of a free-function metadata registration export.
fn stage_function_metadata_prefix(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    function_name_label: &str,
    function_name_len: usize,
    param_index: Option<usize>,
) {
    stage_eval_native_local_word(ctx, context_offset, PhpType::Pointer(None));
    stage_eval_native_label(ctx, function_name_label);
    stage_eval_native_int(ctx, function_name_len as i64);
    if let Some(param_index) = param_index {
        stage_eval_native_int(ctx, param_index as i64);
    }
}

/// Emits one native-function parameter-name registration call.
pub(super) fn register_eval_native_function_param(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    function_name_label: &str,
    function_name_len: usize,
    param_index: usize,
    param_name: &str,
) {
    let (param_name_label, param_name_len) = ctx.data.add_string(param_name.as_bytes());
    stage_function_metadata_prefix(
        ctx,
        context_offset,
        function_name_label,
        function_name_len,
        Some(param_index),
    );
    stage_eval_native_label(ctx, &param_name_label);
    stage_eval_native_int(ctx, param_name_len as i64);
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_register_native_function_param",
        &[
            PhpType::Pointer(None),
            PhpType::Pointer(None),
            PhpType::Int,
            PhpType::Int,
            PhpType::Pointer(None),
            PhpType::Int,
        ],
    );
}

/// Emits one native-function bridge-support registration call.
pub(super) fn register_eval_native_function_bridge_support(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    function_name_label: &str,
    function_name_len: usize,
    bridge_supported: bool,
) {
    stage_function_metadata_prefix(
        ctx,
        context_offset,
        function_name_label,
        function_name_len,
        None,
    );
    stage_eval_native_int(ctx, i64::from(bridge_supported));
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_register_native_function_bridge_support",
        &[
            PhpType::Pointer(None),
            PhpType::Pointer(None),
            PhpType::Int,
            PhpType::Int,
        ],
    );
}

/// Emits one native-function parameter-flags registration call.
pub(super) fn register_eval_native_function_param_flags(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    function_name_label: &str,
    function_name_len: usize,
    param_index: usize,
    is_by_ref: bool,
    is_variadic: bool,
) {
    stage_function_metadata_prefix(
        ctx,
        context_offset,
        function_name_label,
        function_name_len,
        Some(param_index),
    );
    stage_eval_native_int(ctx, i64::from(is_by_ref));
    stage_eval_native_int(ctx, i64::from(is_variadic));
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_register_native_function_param_flags",
        &[
            PhpType::Pointer(None),
            PhpType::Pointer(None),
            PhpType::Int,
            PhpType::Int,
            PhpType::Int,
            PhpType::Int,
        ],
    );
}

/// Emits one native-function parameter-type registration call.
pub(super) fn register_eval_native_function_param_type(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    function_name_label: &str,
    function_name_len: usize,
    param_index: usize,
    type_spec: &str,
) {
    let (type_label, type_len) = ctx.data.add_string(type_spec.as_bytes());
    stage_function_metadata_prefix(
        ctx,
        context_offset,
        function_name_label,
        function_name_len,
        Some(param_index),
    );
    stage_eval_native_label(ctx, &type_label);
    stage_eval_native_int(ctx, type_len as i64);
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_register_native_function_param_type",
        &[
            PhpType::Pointer(None),
            PhpType::Pointer(None),
            PhpType::Int,
            PhpType::Int,
            PhpType::Pointer(None),
            PhpType::Int,
        ],
    );
}

/// Emits one native-function return-type registration call.
pub(super) fn register_eval_native_function_return_type(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    function_name_label: &str,
    function_name_len: usize,
    type_spec: &str,
) {
    let (type_label, type_len) = ctx.data.add_string(type_spec.as_bytes());
    stage_function_metadata_prefix(
        ctx,
        context_offset,
        function_name_label,
        function_name_len,
        None,
    );
    stage_eval_native_label(ctx, &type_label);
    stage_eval_native_int(ctx, type_len as i64);
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_register_native_function_return_type",
        &[
            PhpType::Pointer(None),
            PhpType::Pointer(None),
            PhpType::Int,
            PhpType::Pointer(None),
            PhpType::Int,
        ],
    );
}

/// Emits one native function parameter-default registration call.
pub(super) fn register_eval_native_function_param_default(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    function_name_label: &str,
    function_name_len: usize,
    param_index: usize,
    default: &EvalNativeCallableDefault,
) {
    match default {
        EvalNativeCallableDefault::Scalar { kind, payload } => {
            stage_function_metadata_prefix(
                ctx,
                context_offset,
                function_name_label,
                function_name_len,
                Some(param_index),
            );
            stage_eval_native_int(ctx, *kind);
            stage_eval_native_int(ctx, *payload);
            emit_eval_native_c_abi_call(
                ctx,
                "__elephc_eval_register_native_function_param_default_scalar",
                &[
                    PhpType::Pointer(None),
                    PhpType::Pointer(None),
                    PhpType::Int,
                    PhpType::Int,
                    PhpType::Int,
                    PhpType::Int,
                ],
            );
        }
        EvalNativeCallableDefault::String(value) => {
            let (default_label, default_len) = ctx.data.add_string(value.as_bytes());
            stage_function_metadata_prefix(
                ctx,
                context_offset,
                function_name_label,
                function_name_len,
                Some(param_index),
            );
            stage_eval_native_label(ctx, &default_label);
            stage_eval_native_int(ctx, default_len as i64);
            emit_eval_native_c_abi_call(
                ctx,
                "__elephc_eval_register_native_function_param_default_string",
                &[
                    PhpType::Pointer(None),
                    PhpType::Pointer(None),
                    PhpType::Int,
                    PhpType::Int,
                    PhpType::Pointer(None),
                    PhpType::Int,
                ],
            );
        }
        EvalNativeCallableDefault::Object { .. } => {
            let spec = encode_eval_native_object_default(default);
            let (default_label, default_len) = ctx.data.add_string(&spec);
            stage_function_metadata_prefix(
                ctx,
                context_offset,
                function_name_label,
                function_name_len,
                Some(param_index),
            );
            stage_eval_native_label(ctx, &default_label);
            stage_eval_native_int(ctx, default_len as i64);
            emit_eval_native_c_abi_call(
                ctx,
                "__elephc_eval_register_native_function_param_default_object",
                &[
                    PhpType::Pointer(None),
                    PhpType::Pointer(None),
                    PhpType::Int,
                    PhpType::Int,
                    PhpType::Pointer(None),
                    PhpType::Int,
                ],
            );
        }
        EvalNativeCallableDefault::Array(_) => {
            let spec = encode_eval_native_array_default(default);
            let (default_label, default_len) = ctx.data.add_string(&spec);
            stage_function_metadata_prefix(
                ctx,
                context_offset,
                function_name_label,
                function_name_len,
                Some(param_index),
            );
            stage_eval_native_label(ctx, &default_label);
            stage_eval_native_int(ctx, default_len as i64);
            emit_eval_native_c_abi_call(
                ctx,
                "__elephc_eval_register_native_function_param_default_array",
                &[
                    PhpType::Pointer(None),
                    PhpType::Pointer(None),
                    PhpType::Int,
                    PhpType::Int,
                    PhpType::Pointer(None),
                    PhpType::Int,
                ],
            );
        }
    }
}

/// Emits one native-function explicit PHP signature shape registration call.
///
/// The six ABI words are materialized through the shared native C ABI planner: Windows x64
/// spills words five and six after its shadow space, while SysV and AAPCS keep their register
/// placement. The two shape booleans stay packed in one flags word.
pub(super) fn register_eval_native_function_shape(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    function_name_label: &str,
    function_name_len: usize,
    shape: &EvalNativeSignatureShape,
) {
    stage_function_metadata_prefix(
        ctx,
        context_offset,
        function_name_label,
        function_name_len,
        None,
    );
    stage_eval_native_int(ctx, shape.visible_regular_param_count as i64);
    stage_eval_native_int(ctx, shape.required_param_count as i64);
    stage_eval_native_int(ctx, shape.flags());
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_register_native_function_shape",
        &[
            PhpType::Pointer(None),
            PhpType::Pointer(None),
            PhpType::Int,
            PhpType::Int,
            PhpType::Int,
            PhpType::Int,
        ],
    );
}
