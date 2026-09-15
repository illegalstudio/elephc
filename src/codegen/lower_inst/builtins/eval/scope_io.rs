//! Purpose:
//! Flushes native values into eval scopes and fetches synchronized entries.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - Scope flags distinguish visibility, ownership, and global aliases.

use super::*;

/// Calls `__elephc_eval_scope_set` for a boxed value identified by a static name.
pub(super) fn emit_eval_scope_set_name(ctx: &mut FunctionContext<'_>, name: &str, flags: i64) {
    let (name_label, name_len) = ctx.data.add_string(name.as_bytes());
    load_eval_scope_to_arg(ctx, 0);
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        EVAL_TEMP_CELL_OFFSET,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        flags,
    );
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_scope_set");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
}

/// Calls `__elephc_eval_scope_set` for one boxed global value.
pub(super) fn emit_eval_global_scope_set(ctx: &mut FunctionContext<'_>, global: &EvalSyncGlobal, flags: i64) {
    let (name_label, name_len) = ctx.data.add_string(global.name.as_bytes());
    load_eval_global_scope_to_arg(ctx, 0);
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        EVAL_TEMP_CELL_OFFSET,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        flags,
    );
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_scope_set");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
}

/// Marks caller-scope global aliases in the materialized eval scope.
pub(super) fn mark_eval_scope_global_aliases(ctx: &mut FunctionContext<'_>, aliases: &[EvalGlobalAlias]) {
    for alias in aliases {
        let (name_label, name_len) = ctx.data.add_string(alias.name.as_bytes());
        let (global_name_label, global_name_len) =
            ctx.data.add_string(alias.global_name.as_bytes());
        load_eval_scope_to_arg(ctx, 0);
        let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
        abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, 2),
            name_len as i64,
        );
        let global_name_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
        abi::emit_symbol_address(ctx.emitter, global_name_arg, &global_name_label);
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, 4),
            global_name_len as i64,
        );
        let symbol = ctx
            .emitter
            .target
            .extern_symbol("__elephc_eval_scope_mark_global_alias");
        abi::emit_call_label(ctx.emitter, &symbol);
        emit_eval_status_check(ctx);
    }
}

/// Calls `__elephc_eval_scope_set` for one boxed local value.
pub(super) fn emit_eval_scope_set(ctx: &mut FunctionContext<'_>, local: &EvalSyncLocal, flags: i64) {
    let (name_label, name_len) = ctx.data.add_string(local.name.as_bytes());
    load_eval_scope_to_arg(ctx, 0);
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        EVAL_TEMP_CELL_OFFSET,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        flags,
    );
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_scope_set");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
}

/// Reloads synchronized locals from the eval scope after the eval interpreter returns.
pub(super) fn reload_eval_scope_locals(
    ctx: &mut FunctionContext<'_>,
    locals: &[EvalSyncLocal],
    pending_throw: Option<usize>,
) -> Result<()> {
    for local in locals {
        emit_eval_scope_get(ctx, local);
        let missing = ctx.next_label("eval_scope_reload_missing");
        let done = ctx.next_label("eval_scope_reload_done");
        emit_branch_if_scope_entry_missing(ctx, &missing);
        let result_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 0);
        store_mixed_scope_cell_to_local(ctx, local, pending_throw)?;
        abi::emit_jump(ctx.emitter, &done);
        ctx.emitter.label(&missing);
        store_missing_scope_entry_to_local(ctx, local, pending_throw)?;
        ctx.emitter.label(&done);
    }
    Ok(())
}

/// Reloads synchronized program globals from the eval global scope after eval.
pub(super) fn reload_eval_global_scope(
    ctx: &mut FunctionContext<'_>,
    globals: &[EvalSyncGlobal],
    pending_throw: Option<usize>,
) -> Result<()> {
    for global in globals {
        emit_eval_global_scope_get(ctx, global);
        let missing = ctx.next_label("eval_global_reload_missing");
        let done = ctx.next_label("eval_global_reload_done");
        emit_branch_if_scope_entry_missing(ctx, &missing);
        let result_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 0);
        store_mixed_scope_cell_to_global(ctx, global, pending_throw)?;
        abi::emit_jump(ctx.emitter, &done);
        ctx.emitter.label(&missing);
        store_missing_scope_entry_to_global(ctx, global, pending_throw)?;
        ctx.emitter.label(&done);
    }
    Ok(())
}

/// Reloads synchronized program globals from the local eval scope after EIR eval AOT.
pub(super) fn reload_eval_globals_from_local_scope(
    ctx: &mut FunctionContext<'_>,
    globals: &[EvalSyncGlobal],
) -> Result<()> {
    for global in globals {
        emit_eval_scope_get_name(ctx, &global.name, 0, 8);
        let missing = ctx.next_label("eval_global_reload_missing");
        let done = ctx.next_label("eval_global_reload_done");
        emit_branch_if_scope_entry_missing(ctx, &missing);
        let result_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 0);
        store_mixed_scope_cell_to_global(ctx, global, None)?;
        abi::emit_jump(ctx.emitter, &done);
        ctx.emitter.label(&missing);
        store_missing_scope_entry_to_global(ctx, global, None)?;
        ctx.emitter.label(&done);
    }
    Ok(())
}

/// Calls `__elephc_eval_scope_get` for a static name and caller-provided scratch slots.
pub(super) fn emit_eval_scope_get_name(
    ctx: &mut FunctionContext<'_>,
    name: &str,
    out_cell_offset: usize,
    out_flags_offset: usize,
) {
    let (name_label, name_len) = ctx.data.add_string(name.as_bytes());
    load_eval_scope_to_arg(ctx, 0);
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    let out_cell_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_temporary_stack_address(ctx.emitter, out_cell_arg, out_cell_offset);
    let out_flags_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    abi::emit_temporary_stack_address(ctx.emitter, out_flags_arg, out_flags_offset);
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_scope_get");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
}

/// Calls `__elephc_eval_scope_get` and stores out cell/flags at the start of eval scratch.
pub(super) fn emit_eval_scope_get(ctx: &mut FunctionContext<'_>, local: &EvalSyncLocal) {
    let (name_label, name_len) = ctx.data.add_string(local.name.as_bytes());
    load_eval_scope_to_arg(ctx, 0);
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    let out_cell_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_temporary_stack_address(ctx.emitter, out_cell_arg, 0);
    let out_flags_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    abi::emit_temporary_stack_address(ctx.emitter, out_flags_arg, 8);
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_scope_get");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
}

/// Calls `__elephc_eval_scope_get` for one program global.
pub(super) fn emit_eval_global_scope_get(ctx: &mut FunctionContext<'_>, global: &EvalSyncGlobal) {
    let (name_label, name_len) = ctx.data.add_string(global.name.as_bytes());
    load_eval_global_scope_to_arg(ctx, 0);
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    let out_cell_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_temporary_stack_address(ctx.emitter, out_cell_arg, 0);
    let out_flags_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    abi::emit_temporary_stack_address(ctx.emitter, out_flags_arg, 8);
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_scope_get");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
}

/// Branches to `label` when the latest scope-get flags do not mark a visible value.
pub(super) fn emit_branch_if_scope_entry_missing(ctx: &mut FunctionContext<'_>, label: &str) {
    emit_branch_if_scope_entry_missing_at(ctx, 8, label);
}

/// Branches to `label` when the scope-get flags at `flags_offset` do not mark a visible value.
pub(super) fn emit_branch_if_scope_entry_missing_at(
    ctx: &mut FunctionContext<'_>,
    flags_offset: usize,
    label: &str,
) {
    let flags_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, flags_reg, flags_offset);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("tst {}, #{}", flags_reg, EVAL_SCOPE_FLAG_PRESENT)); // check whether eval left the local visible
            ctx.emitter.instruction(&format!("b.eq {}", label));                // skip reload when eval unset or omitted the local
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("test {}, {}", flags_reg, EVAL_SCOPE_FLAG_PRESENT)); // check whether eval left the local visible
            ctx.emitter.instruction(&format!("je {}", label));                  // skip reload when eval unset or omitted the local
        }
    }
}
