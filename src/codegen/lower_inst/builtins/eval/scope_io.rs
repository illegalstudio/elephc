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
    let setter = if flags & NATIVE_BINDING != 0 {
        "__elephc_eval_scope_bind_native"
    } else {
        "__elephc_eval_scope_set"
    };
    let symbol = ctx.emitter.target.extern_symbol(setter);
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

/// One synchronized local that may hold the shared reference cell of a program global.
pub(super) struct EvalGlobalBoundLocal {
    local: EvalSyncLocal,
    /// Globals this function binds a reference cell for (`global $g`, `$x = &$g` after it).
    globals: Vec<String>,
}

/// Splits synchronized locals into plain ones and those that may be bound to a program global.
///
/// Inside a function, `global $g` makes `$g` an ordinary local bound to the global's shared
/// reference cell (`LoweringContext::bind_global_local`), and `$h = &$g` binds `$h` to that same
/// cell. Main lowered `global $g` to a `GlobalAlias` local the bridge marked as a global alias.
/// Flushed as plain values instead, the fragment saw detached copies: `$GLOBALS['g']` missed its
/// writes, and `$r =& $g` turned the entry into an eval reference wrapper that the reload then
/// stored through the cell into the global itself, which every later read saw as NULL. A local
/// still bound when the eval runs is published as a global alias of the global whose cell it
/// holds, checked at run time by [`flush_eval_global_bound_locals`] and
/// [`reload_eval_global_bound_locals`].
pub(super) fn split_eval_global_bound_locals(
    ctx: &mut FunctionContext<'_>,
    locals: Vec<EvalSyncLocal>,
) -> (Vec<EvalSyncLocal>, Vec<EvalGlobalBoundLocal>) {
    if ctx.is_main {
        return (locals, Vec::new());
    }
    let mut bound_globals = BTreeSet::new();
    for inst in &ctx.function.instructions {
        if inst.op != Op::GlobalRefCell {
            continue;
        }
        if let Some(name) = global_instruction_name(ctx, inst) {
            bound_globals.insert(name.to_string());
        }
    }
    let bound_globals = bound_globals
        .into_iter()
        .filter(|name| !crate::superglobals::is_superglobal(name))
        .filter(|name| ctx.shared.uses_shared_ref_cell(ctx.module, name))
        .collect::<Vec<_>>();
    if bound_globals.is_empty() {
        return (locals, Vec::new());
    }
    let mut plain = Vec::new();
    let mut bound = Vec::new();
    for local in locals {
        let candidate = local.ty.codegen_repr() == PhpType::Mixed
            && !crate::superglobals::is_superglobal(&local.name)
            && ctx.local_stores_ref_cell_pointer(local.slot);
        if candidate {
            // The same-named global first: that is what `global $g` binds.
            let mut globals = bound_globals.clone();
            if let Some(index) = globals.iter().position(|name| *name == local.name) {
                globals.swap(0, index);
            }
            bound.push(EvalGlobalBoundLocal { local, globals });
        } else {
            plain.push(local);
        }
    }
    (plain, bound)
}

/// Branches to `label` when the local's slot holds the shared reference cell of `global`.
fn emit_branch_if_local_bound_to_global(
    ctx: &mut FunctionContext<'_>,
    local: &EvalSyncLocal,
    global: &str,
    label: &str,
) -> Result<()> {
    let cell = abi::secondary_scratch_reg(ctx.emitter);
    ctx.materialize_local_storage_address(local.slot, cell)?;
    let symbol = ir_global_symbol(global);
    ctx.data.add_comm(symbol.clone(), 8);
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_load_symbol_to_reg(ctx.emitter, result, &symbol, 0);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("sub {result}, {result}, {cell}")); // zero when the local holds this global's shared cell
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("sub {result}, {cell}"));          // zero when the local holds this global's shared cell
        }
    }
    abi::emit_branch_if_int_result_zero(ctx.emitter, label);
    Ok(())
}

/// Publishes global-bound locals to eval as global aliases, and every other one as a plain value.
///
/// A slot no longer bound to a global (rebound, or never bound on this path) drops an alias an
/// earlier eval in the same frame left on the persistent scope.
pub(super) fn flush_eval_global_bound_locals(
    ctx: &mut FunctionContext<'_>,
    locals: &[EvalGlobalBoundLocal],
) -> Result<()> {
    for bound in locals {
        let local = &bound.local;
        let done = ctx.next_label("eval_scope_flush_global_bound_done");
        let labels = bound
            .globals
            .iter()
            .map(|_| ctx.next_label("eval_scope_flush_global_bound"))
            .collect::<Vec<_>>();
        for (global, label) in bound.globals.iter().zip(&labels) {
            emit_branch_if_local_bound_to_global(ctx, local, global, label)?;
        }
        // An empty global name asks the bridge to drop the alias.
        mark_eval_scope_global_aliases(ctx, &[EvalGlobalAlias {
            name: local.name.clone(),
            global_name: String::new(),
        }]);
        flush_eval_scope_locals(ctx, std::slice::from_ref(local))?;
        for (global, label) in bound.globals.iter().zip(&labels) {
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(label);
            mark_eval_scope_global_aliases(ctx, &[EvalGlobalAlias {
                name: local.name.clone(),
                global_name: global.clone(),
            }]);
        }
        ctx.emitter.label(&done);
    }
    Ok(())
}

/// Reloads global-bound locals that were published as plain values.
///
/// A local still bound to a global needs no reload: the global scope's writeback stores
/// through the shared cell the slot points at. Reloading it as well wrote a stale snapshot
/// through that cell, or released the payload the global scope's borrowed entry still names.
pub(super) fn reload_eval_global_bound_locals(
    ctx: &mut FunctionContext<'_>,
    locals: &[EvalGlobalBoundLocal],
    pending_throw: Option<usize>,
) -> Result<()> {
    for bound in locals {
        let skip = ctx.next_label("eval_scope_reload_global_bound");
        for global in &bound.globals {
            emit_branch_if_local_bound_to_global(ctx, &bound.local, global, &skip)?;
        }
        reload_eval_scope_locals(ctx, std::slice::from_ref(&bound.local), pending_throw)?;
        ctx.emitter.label(&skip);
    }
    Ok(())
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
    reload_eval_scope_locals_with_mask(ctx, locals, pending_throw, None)
}

/// Reloads only dynamic-eval locals selected by the parsed fragment's access mask.
pub(super) fn reload_dynamic_eval_scope_locals(
    ctx: &mut FunctionContext<'_>,
    locals: &[EvalSyncLocal],
    pending_throw: Option<usize>,
) -> Result<()> {
    reload_eval_scope_locals_with_mask(
        ctx,
        locals,
        pending_throw,
        Some(EVAL_SCOPE_ACCESS_MASK_OFFSET),
    )
}

/// Reloads synchronized locals, optionally guarding each name through a runtime mask.
fn reload_eval_scope_locals_with_mask(
    ctx: &mut FunctionContext<'_>,
    locals: &[EvalSyncLocal],
    pending_throw: Option<usize>,
    access_mask_offset: Option<usize>,
) -> Result<()> {
    for local in locals {
        let unaccessed = access_mask_offset.map(|offset| {
            let unaccessed = ctx.next_label("eval_scope_reload_unaccessed");
            emit_branch_if_eval_name_unaccessed(ctx.emitter, &local.name, offset, &unaccessed);
            unaccessed
        });
        emit_eval_scope_get(ctx, local);
        let missing = ctx.next_label("eval_scope_reload_missing");
        let done = ctx.next_label("eval_scope_reload_done");
        branch_scope_binding(ctx, NATIVE_BINDING, &done);
        emit_branch_if_scope_entry_missing(ctx, &missing);
        let result_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 0);
        store_mixed_scope_cell_to_local(ctx, local, pending_throw)?;
        abi::emit_jump(ctx.emitter, &done);
        ctx.emitter.label(&missing);
        store_missing_scope_entry_to_local(ctx, local, pending_throw)?;
        ctx.emitter.label(&done);
        if let Some(unaccessed) = unaccessed {
            ctx.emitter.label(&unaccessed);
        }
    }
    Ok(())
}

/// Reloads synchronized program globals from the eval global scope after eval.
pub(super) fn reload_eval_global_scope(
    ctx: &mut FunctionContext<'_>,
    globals: &[EvalSyncGlobal],
    pending_throw: Option<usize>,
) -> Result<()> {
    reload_eval_global_scope_with_mask(ctx, globals, pending_throw, None)
}

/// Reloads only dynamic-eval globals selected by the parsed fragment's access mask.
pub(super) fn reload_dynamic_eval_global_scope(
    ctx: &mut FunctionContext<'_>,
    globals: &[EvalSyncGlobal],
    pending_throw: Option<usize>,
) -> Result<()> {
    reload_eval_global_scope_with_mask(
        ctx,
        globals,
        pending_throw,
        Some(EVAL_SCOPE_ACCESS_MASK_OFFSET),
    )
}

/// Reloads synchronized globals, optionally guarding each name through a runtime mask.
fn reload_eval_global_scope_with_mask(
    ctx: &mut FunctionContext<'_>,
    globals: &[EvalSyncGlobal],
    pending_throw: Option<usize>,
    access_mask_offset: Option<usize>,
) -> Result<()> {
    for global in globals {
        let unaccessed = access_mask_offset.map(|offset| {
            let unaccessed = ctx.next_label("eval_global_reload_unaccessed");
            emit_branch_if_eval_name_unaccessed(ctx.emitter, &global.name, offset, &unaccessed);
            unaccessed
        });
        emit_eval_global_scope_get(ctx, global);
        let missing = ctx.next_label("eval_global_reload_missing");
        let done = ctx.next_label("eval_global_reload_done");
        emit_branch_if_scope_entry_missing(ctx, &missing);
        // See `emit_clear_eval_global_scope_dirty`: an entry the fragment never wrote is a
        // borrowed snapshot of a cell a native call may already have replaced and released.
        emit_branch_if_scope_entry_clean(ctx, &done);
        let result_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 0);
        store_mixed_scope_cell_to_global(ctx, global, pending_throw)?;
        abi::emit_jump(ctx.emitter, &done);
        ctx.emitter.label(&missing);
        store_missing_scope_entry_to_global(ctx, global, pending_throw)?;
        ctx.emitter.label(&done);
        if let Some(unaccessed) = unaccessed {
            ctx.emitter.label(&unaccessed);
        }
    }
    Ok(())
}

/// Scope-entry flag set by every write to the entry (`crate::abi::SCOPE_FLAG_DIRTY` in magician).
const EVAL_SCOPE_FLAG_DIRTY: i64 = 1 << 2;

/// Marks every entry of the frame's eval global scope clean right after it was flushed.
///
/// Inside a function, a program global held in a shared reference cell can ALSO be reached
/// through a local bound to that cell (a by-reference parameter the caller passed the global
/// to). The global scope holds a BORROWED snapshot of the cell's payload; the local reload
/// stores the fragment's write through the cell and releases that payload, and reloading the
/// untouched global entry afterwards retained freed memory and wrote the stale value back.
/// Only entries the fragment actually wrote stay dirty, and only those are reloaded.
pub(super) fn clear_eval_global_scope_dirty(ctx: &mut FunctionContext<'_>, globals: &[EvalSyncGlobal]) {
    if ctx.is_main
        || !globals
            .iter()
            .any(|global| ctx.shared.uses_shared_ref_cell(ctx.module, &global.name))
    {
        return;
    }
    emit_clear_eval_global_scope_dirty(ctx);
}

/// Marks every entry of the eval global scope clean, without the shared-ref-cell guard.
///
/// The native-to-eval flush publishes a BORROWED snapshot of each native global. Every later
/// native call made from the fragment can replace (and release) that native cell, so replaying
/// an untouched snapshot at the eval-to-native boundary writes freed memory back over the live
/// global. Clearing the flags right after a flush makes the reload skip exactly the entries the
/// fragment never wrote, while entries the interpreter assigns stay dirty and are published.
pub(super) fn emit_clear_eval_global_scope_dirty(ctx: &mut FunctionContext<'_>) {
    load_eval_global_scope_to_arg(ctx, 0);
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_scope_clear_dirty");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
}

/// Branches to `label` when the latest scope-get flags show an entry nothing wrote since flush.
fn emit_branch_if_scope_entry_clean(ctx: &mut FunctionContext<'_>, label: &str) {
    let flags_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, flags_reg, 8);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("tst {}, #{}", flags_reg, EVAL_SCOPE_FLAG_DIRTY)); // did the fragment write this global entry?
            ctx.emitter.instruction(&format!("b.eq {}", label));                // an untouched snapshot is not reloaded
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("test {}, {}", flags_reg, EVAL_SCOPE_FLAG_DIRTY)); // did the fragment write this global entry?
            ctx.emitter.instruction(&format!("je {}", label));                  // an untouched snapshot is not reloaded
        }
    }
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
        branch_scope_binding(ctx, NATIVE_BINDING, &done);
        emit_branch_if_scope_entry_missing(ctx, &missing);
        let result_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 0);
        store_mixed_scope_cell_to_global(ctx, global, None)?;
        abi::emit_jump(ctx.emitter, &done);
        ctx.emitter.label(&missing);
        // Only at program top level is this local scope the global scope. Inside a function the
        // fragment reaches a global through `global $g`, and `unset($g)` there drops that alias,
        // never the global: treating the missing entry as an unset global cleared it.
        if ctx.is_main {
            store_missing_scope_entry_to_global(ctx, global, None)?;
        }
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
