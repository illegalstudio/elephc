//! Purpose:
//! Stages eval bridge Rust FFI calls through the target-native C ABI planner.
//!
//! Called from:
//! - Eval call, scope, dynamic-dispatch, symbol-query, and introspection lowerers.
//!
//! Key details:
//! - MS x64 uses four positional register slots plus mandatory shadow space; additional words
//!   are packed on the caller stack by the shared C-ABI materializer.
//! - The bridge only exposes scalar pointer/integer words here, so staging preserves the
//!   established ABI on SysV and AAPCS targets while fixing Windows overflow placement.

use super::*;

/// Invokes a staged eval bridge Rust FFI target using the native C ABI.
///
/// Callers must stage one result value for every entry in `argument_types`, in declaration order,
/// with [`stage_eval_native_word`]. The shared materializer consumes those temporary slots before
/// the call, leaving just the C ABI overflow area and Windows shadow space live at the call site.
pub(in crate::codegen::lower_inst::builtins) fn emit_eval_native_c_abi_call(
    ctx: &mut FunctionContext<'_>,
    symbol_name: &str,
    argument_types: &[PhpType],
) {
    debug_assert_eq!(
        ctx.eval_native_staged_bytes,
        argument_types.len() * 16,
        "each staged eval C argument occupies one temporary 16-byte slot",
    );
    let assignments = abi::build_c_abi_outgoing_arg_assignments_for_target(
        ctx.emitter.target,
        argument_types,
    );
    let overflow_bytes = abi::materialize_outgoing_c_abi_args(ctx.emitter, &assignments);
    let call_pad = abi::outgoing_call_stack_pad_bytes(ctx.emitter.target, overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, call_pad);
    let symbol = ctx.emitter.target.extern_symbol(symbol_name);
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::emit_release_temporary_stack(ctx.emitter, call_pad);
    abi::emit_release_temporary_stack(ctx.emitter, overflow_bytes);
    ctx.eval_native_staged_bytes = 0;
}

/// Invokes a staged callback that is retained in the caller's current temporary frame.
///
/// Argument materialization is allowed to use every scratch register, including the one that
/// originally held the callback, so reload it only after the outgoing stack area is final.
pub(in crate::codegen::lower_inst::builtins) fn emit_eval_native_c_abi_call_reg_from_stack(
    ctx: &mut FunctionContext<'_>,
    callback_reg: &str,
    callback_offset: usize,
    argument_types: &[PhpType],
) {
    debug_assert_eq!(
        ctx.eval_native_staged_bytes,
        argument_types.len() * 16,
        "each staged eval C argument occupies one temporary 16-byte slot",
    );
    let assignments = abi::build_c_abi_outgoing_arg_assignments_for_target(
        ctx.emitter.target,
        argument_types,
    );
    let overflow_bytes = abi::materialize_outgoing_c_abi_args(ctx.emitter, &assignments);
    let call_pad = abi::outgoing_call_stack_pad_bytes(ctx.emitter.target, overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, call_pad);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        callback_reg,
        call_pad + overflow_bytes + callback_offset,
    );
    abi::emit_call_reg(ctx.emitter, callback_reg);
    abi::emit_release_temporary_stack(ctx.emitter, call_pad);
    abi::emit_release_temporary_stack(ctx.emitter, overflow_bytes);
    ctx.eval_native_staged_bytes = 0;
}

/// Stages the current result-register word for an eval bridge C ABI call.
pub(in crate::codegen::lower_inst::builtins) fn stage_eval_native_word(ctx: &mut FunctionContext<'_>, ty: PhpType) {
    debug_assert!(!matches!(ty.codegen_repr(), PhpType::Void | PhpType::Never));
    abi::emit_push_result_value(ctx.emitter, &ty);
    ctx.eval_native_staged_bytes += 16;
}

/// Stages one pointer-sized local-frame word for an eval bridge C ABI call.
///
/// Setup-time metadata registration holds its persistent context in a function local rather than
/// the transient eval scratch frame, so it cannot use `stage_eval_native_context` below.
pub(super) fn stage_eval_native_local_word(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    ty: PhpType,
) {
    abi::load_at_offset(ctx.emitter, abi::int_result_reg(ctx.emitter), offset);
    stage_eval_native_word(ctx, ty);
}

/// Stages the address of an already interned static-data label for an eval bridge C ABI call.
pub(super) fn stage_eval_native_label(ctx: &mut FunctionContext<'_>, label: &str) {
    abi::emit_symbol_address(ctx.emitter, abi::int_result_reg(ctx.emitter), label);
    stage_eval_native_word(ctx, PhpType::Pointer(None));
}

/// Stages a word that an existing lowering already placed in an integer argument register.
///
/// This is intentionally limited to four words: Windows has only four register slots, and all
/// wider bridge calls must materialize their arguments into the temporary staging area directly.
pub(super) fn emit_loaded_eval_native_c_abi_call(
    ctx: &mut FunctionContext<'_>,
    symbol_name: &str,
    argument_types: &[PhpType],
) {
    debug_assert!(argument_types.len() <= 4);
    for (index, ty) in argument_types.iter().enumerate() {
        stage_eval_native_loaded_word(ctx, index, ty.clone());
    }
    emit_eval_native_c_abi_call(ctx, symbol_name, argument_types);
}

/// Stages an already-materialized single-word integer argument register value.
pub(super) fn stage_eval_native_loaded_word(
    ctx: &mut FunctionContext<'_>,
    index: usize,
    ty: PhpType,
) {
    debug_assert_eq!(ty.register_count(), 1);
    abi::emit_push_reg(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, index));
    ctx.eval_native_staged_bytes += 16;
}

/// Stages an eval scratch-frame pointer argument at `offset`.
pub(in crate::codegen::lower_inst::builtins) fn stage_eval_native_stack_address(ctx: &mut FunctionContext<'_>, offset: usize) {
    abi::emit_temporary_stack_address(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        ctx.eval_native_staged_bytes + offset,
    );
    stage_eval_native_word(ctx, PhpType::Pointer(None));
}

/// Stages one pointer-sized eval scratch-frame word at `offset`.
pub(super) fn stage_eval_native_stack_word(ctx: &mut FunctionContext<'_>, offset: usize) {
    stage_eval_native_stack_word_as(ctx, offset, PhpType::Pointer(None));
}

/// Stages one scalar eval scratch-frame word at `offset` with its native C ABI type.
pub(in crate::codegen::lower_inst::builtins) fn stage_eval_native_stack_word_as(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    ty: PhpType,
) {
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        ctx.eval_native_staged_bytes + offset,
    );
    stage_eval_native_word(ctx, ty);
}

/// Stages one scalar integer word for an eval bridge C ABI call.
pub(super) fn stage_eval_native_int(ctx: &mut FunctionContext<'_>, value: i64) {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), value);
    stage_eval_native_word(ctx, PhpType::Int);
}

/// Stages an interned byte string as its native pointer/length pair.
pub(super) fn stage_eval_native_string(ctx: &mut FunctionContext<'_>, value: &str) {
    let (label, len) = ctx.data.add_string(value.as_bytes());
    stage_eval_native_symbol_address(ctx, &label);
    stage_eval_native_int(ctx, len as i64);
}

/// Stages a native pointer to an already-interned data label.
pub(super) fn stage_eval_native_symbol_address(ctx: &mut FunctionContext<'_>, label: &str) {
    abi::emit_symbol_address(ctx.emitter, abi::int_result_reg(ctx.emitter), label);
    stage_eval_native_word(ctx, PhpType::Pointer(None));
}

/// Stages the persistent eval context handle from the current scratch frame.
pub(super) fn stage_eval_native_context(ctx: &mut FunctionContext<'_>) {
    stage_eval_native_stack_word(ctx, EVAL_CONTEXT_HANDLE_OFFSET);
}

/// Stages the current eval scope handle from the current scratch frame.
pub(super) fn stage_eval_native_scope(ctx: &mut FunctionContext<'_>) {
    stage_eval_native_stack_word(ctx, EVAL_SCOPE_HANDLE_OFFSET);
}

/// Stages the current eval global-scope handle from the current scratch frame.
pub(super) fn stage_eval_native_global_scope(ctx: &mut FunctionContext<'_>) {
    stage_eval_native_stack_word(ctx, EVAL_GLOBAL_SCOPE_HANDLE_OFFSET);
}
