//! Native bindings for compiled eval scopes.
//! Scope entries own a borrowed-storage marker or carry a global-name binding;
//! reads/writes operate on original storage instead of a detached snapshot.

use super::*;

pub(super) const NATIVE_REF: i64 = 1 << 5;
pub(super) const NATIVE_GLOBAL: i64 = 1 << 6;
pub(super) const NATIVE_BINDING: i64 = NATIVE_REF | NATIVE_GLOBAL;

/// Branches on binding metadata returned by the scope getter at scratch offset 8.
pub(super) fn branch_scope_binding(ctx: &mut FunctionContext<'_>, mask: i64, label: &str) {
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 8);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("and {result}, {result}, #{mask}")),
        Arch::X86_64 => ctx.emitter.instruction(&format!("and {result}, {mask}")),
    }
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, label);
}

/// Replaces snapshot entries with live bindings only on paths holding a ref-cell.
pub(super) fn prepare_native_scope_bindings(
    ctx: &mut FunctionContext<'_>,
    locals: &[EvalSyncLocal],
    globals: &[EvalSyncGlobal],
) -> Result<()> {
    for local in locals {
        if local.ty.codegen_repr() != PhpType::Mixed {
            continue;
        }
        let definite = ctx.local_ref_cell_representation_is_definite(local.slot);
        if !definite && !ctx.local_ref_cell_representation_is_dynamic(local.slot) {
            continue;
        }
        let done = ctx.next_label("eval_native_binding_done");
        if !definite {
            let offset = ctx.ref_cell_state_offset(local.slot).ok_or_else(|| {
                CodegenIrError::invalid_module("native scope binding has no representation flag")
            })?;
            abi::load_at_offset(ctx.emitter, abi::int_result_reg(ctx.emitter), offset);
            abi::emit_branch_if_int_result_zero(ctx.emitter, &done);
        }
        let cell = abi::secondary_scratch_reg(ctx.emitter);
        ctx.materialize_local_storage_address(local.slot, cell)?;
        let tag = abi::tertiary_scratch_reg(ctx.emitter);
        let source_tag = abi::symbol_scratch_reg(ctx.emitter);
        abi::emit_load_int_immediate(ctx.emitter, tag,
            crate::codegen::callable_invoker_args::INVOKER_ARG_REF_CELL_TAG);
        abi::emit_load_int_immediate(ctx.emitter, source_tag, 7);
        crate::codegen::emit_box_runtime_payload_as_mixed(ctx.emitter, tag, cell, source_tag);
        abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), EVAL_TEMP_CELL_OFFSET);
        emit_eval_scope_set_name(ctx, &local.name, EVAL_SCOPE_FLAG_OWNED | NATIVE_REF);
        ctx.emitter.label(&done);
    }
    for global in globals {
        if global.ty.codegen_repr() != PhpType::Mixed {
            continue;
        }
        // This binding is resolved by name. Do not retain a borrowed snapshot
        // pointer that could become dangling after an in-eval global write.
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), EVAL_TEMP_CELL_OFFSET);
        emit_eval_scope_set_name(ctx, &global.name, NATIVE_GLOBAL);
    }
    Ok(())
}

/// Loads the boxed value behind the borrowed-storage marker at scratch offset 0.
pub(super) fn load_native_scope_reference(ctx: &mut FunctionContext<'_>) {
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 0);
    abi::emit_load_from_address(ctx.emitter, result, result, 8);
    abi::emit_load_from_address(ctx.emitter, result, result, 0);
}

/// Transfers the new boxed owner at offset 40 before releasing the old payload.
pub(super) fn store_native_scope_reference(ctx: &mut FunctionContext<'_>) {
    let cell = abi::secondary_scratch_reg(ctx.emitter);
    let value = abi::tertiary_scratch_reg(ctx.emitter);
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, cell, 0);
    abi::emit_load_from_address(ctx.emitter, cell, cell, 8);
    abi::emit_load_from_address(ctx.emitter, result, cell, 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, value, EVAL_TEMP_CELL_OFFSET);
    abi::emit_store_to_address(ctx.emitter, value, cell, 0);
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
}

/// Replaces call-bounded markers with owned value views before PHP resumes.
pub(super) fn clear_native_scope_bindings(
    ctx: &mut FunctionContext<'_>,
    locals: &[EvalSyncLocal],
    globals: &[EvalSyncGlobal],
) {
    for name in locals.iter().map(|local| &local.name).chain(globals.iter().map(|global| &global.name)) {
        emit_eval_scope_get_name(ctx, name, 0, 8);
        let reference = ctx.next_label("eval_native_binding_reference_view");
        let global = ctx.next_label("eval_native_binding_global_view");
        let store = ctx.next_label("eval_native_binding_store_view");
        let done = ctx.next_label("eval_native_binding_keep");
        branch_scope_binding(ctx, NATIVE_REF, &reference);
        branch_scope_binding(ctx, NATIVE_GLOBAL, &global);
        abi::emit_jump(ctx.emitter, &done);
        ctx.emitter.label(&reference);
        load_native_scope_reference(ctx);
        abi::emit_jump(ctx.emitter, &store);
        ctx.emitter.label(&global);
        load_global_to_result(ctx, &EvalSyncGlobal { name: name.clone(), ty: PhpType::Mixed });
        ctx.emitter.label(&store);
        abi::emit_call_label(ctx.emitter, "__rt_incref");
        abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), EVAL_TEMP_CELL_OFFSET);
        emit_eval_scope_set_name(ctx, name, EVAL_SCOPE_FLAG_OWNED);
        ctx.emitter.label(&done);
    }
}
