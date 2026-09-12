//! Purpose:
//! Emits native free-function parameter and return metadata for eval.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - Bridge-support and default registration remain signature-driven.

use super::*;

/// Emits one native-function parameter-name registration call.
pub(super) fn register_eval_native_function_param(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    function_name_label: &str,
    function_name_len: usize,
    param_index: usize,
    param_name: &str,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        function_name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        function_name_len as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        param_index as i64,
    );
    let (param_name_label, param_name_len) = ctx.data.add_string(param_name.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        &param_name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 5),
        param_name_len as i64,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_function_param");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Emits one native-function bridge-support registration call.
pub(super) fn register_eval_native_function_bridge_support(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    function_name_label: &str,
    function_name_len: usize,
    bridge_supported: bool,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        function_name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        function_name_len as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        if bridge_supported { 1 } else { 0 },
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_function_bridge_support");
    abi::emit_call_label(ctx.emitter, &symbol);
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
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        function_name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        function_name_len as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        param_index as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        if is_by_ref { 1 } else { 0 },
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 5),
        if is_variadic { 1 } else { 0 },
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_function_param_flags");
    abi::emit_call_label(ctx.emitter, &symbol);
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
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        function_name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        function_name_len as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        param_index as i64,
    );
    let (type_label, type_len) = ctx.data.add_string(type_spec.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        &type_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 5),
        type_len as i64,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_function_param_type");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Emits one native-function return-type registration call.
pub(super) fn register_eval_native_function_return_type(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    function_name_label: &str,
    function_name_len: usize,
    type_spec: &str,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        function_name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        function_name_len as i64,
    );
    let (type_label, type_len) = ctx.data.add_string(type_spec.as_bytes());
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
        .extern_symbol("__elephc_eval_register_native_function_return_type");
    abi::emit_call_label(ctx.emitter, &symbol);
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
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        function_name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        function_name_len as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        param_index as i64,
    );
    let symbol = match default {
        EvalNativeCallableDefault::Scalar { kind, payload } => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 4),
                *kind,
            );
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 5),
                *payload,
            );
            ctx.emitter
                .target
                .extern_symbol("__elephc_eval_register_native_function_param_default_scalar")
        }
        EvalNativeCallableDefault::String(value) => {
            let (default_label, default_len) = ctx.data.add_string(value.as_bytes());
            abi::emit_symbol_address(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 4),
                &default_label,
            );
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 5),
                default_len as i64,
            );
            ctx.emitter
                .target
                .extern_symbol("__elephc_eval_register_native_function_param_default_string")
        }
        EvalNativeCallableDefault::Object { .. } => {
            let spec = encode_eval_native_object_default(default);
            let (default_label, default_len) = ctx.data.add_string(&spec);
            abi::emit_symbol_address(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 4),
                &default_label,
            );
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 5),
                default_len as i64,
            );
            ctx.emitter
                .target
                .extern_symbol("__elephc_eval_register_native_function_param_default_object")
        }
        EvalNativeCallableDefault::Array(_) => {
            let spec = encode_eval_native_array_default(default);
            let (default_label, default_len) = ctx.data.add_string(&spec);
            abi::emit_symbol_address(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 4),
                &default_label,
            );
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 5),
                default_len as i64,
            );
            ctx.emitter
                .target
                .extern_symbol("__elephc_eval_register_native_function_param_default_array")
        }
    };
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Emits one native-function explicit PHP signature shape registration call.
///
/// Target-aware like every sibling emitter: the six ABI words go through
/// `abi::int_arg_reg_name`, which is the integer-argument register sequence of whichever
/// supported target is being emitted. Six is the limit on every supported target (x86_64 SysV
/// passes only six integers in registers), which is why the two shape booleans travel packed in
/// one flags word rather than as separate arguments.
pub(super) fn register_eval_native_function_shape(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    function_name_label: &str,
    function_name_len: usize,
    shape: &EvalNativeSignatureShape,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        function_name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        function_name_len as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        shape.visible_regular_param_count as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        shape.required_param_count as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 5),
        shape.flags(),
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_function_shape");
    abi::emit_call_label(ctx.emitter, &symbol);
}
