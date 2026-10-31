//! Purpose:
//! Checks boxed eval replacements before writing through native heap reference cells.
//!
//! Called from:
//! - The eval local and global scope reload paths.
//!
//! Key details:
//! - Rejected values leave the original native storage intact.
//! - Guarded reload accumulates a TypeError and continues the remaining scope writeback.

use super::*;

/// Accepts only the native container tag and the object's declared nominal class.
pub(super) fn guard_heap_replacement(
    ctx: &mut FunctionContext<'_>, ty: &PhpType, name: &str,
    pending_throw: Option<usize>, rejected: &str,
) {
    let (tag, class) = match ty.codegen_repr() {
        PhpType::Object(class) => (6, Some(class)),
        PhpType::Array(_) => (4, None),
        PhpType::AssocArray { .. } => (5, None),
        _ => return,
    };
    let invalid = ctx.next_label("eval_reload_invalid_heap_value");
    let accepted = ctx.next_label("eval_reload_checked_heap_value");
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {result}, #{tag}"));          // require the declared native heap payload tag
            ctx.emitter.instruction(&format!("b.ne {invalid}"));                // reject an incompatible box before touching the destination
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {result}, {tag}"));           // require the declared native heap payload tag
            ctx.emitter.instruction(&format!("jne {invalid}"));                 // reject an incompatible box before touching the destination
        }
    }
    if let Some(class) = class.filter(|class| !class.is_empty() && !class.eq_ignore_ascii_case("object")) {
        let info = ctx.module.class_infos.iter().find(|(name, _)| name.eq_ignore_ascii_case(&class))
            .map(|(_, info)| (info.class_id, 0))
            .or_else(|| ctx.module.interface_infos.iter().find(|(name, _)| name.eq_ignore_ascii_case(&class))
                .map(|(_, info)| (info.interface_id, 1)));
        if let Some((id, kind)) = info {
            abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), 0);
            abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), id as i64);
            abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), kind);
            abi::emit_call_label(ctx.emitter, "__rt_mixed_instanceof");
            abi::emit_branch_if_int_result_zero(ctx.emitter, &invalid);
        } else {
            abi::emit_jump(ctx.emitter, &invalid);
        }
    }
    abi::emit_jump(ctx.emitter, &accepted);
    ctx.emitter.label(&invalid);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    let expected = crate::types::diagnostic_names::php_type_name(ty);
    let message = format!("Unsupported assignment through native reference ${name}: value must be of type {expected}");
    if let Some(offset) = pending_throw {
        super::super::super::exceptions::emit_type_error_owner(ctx, &message);
        abi::emit_push_reg(ctx.emitter, result);
        abi::emit_load_temporary_stack_slot(ctx.emitter, result, offset + 16);
        let previous = abi::secondary_scratch_reg(ctx.emitter);
        abi::emit_reg_move(ctx.emitter, previous, result);
        abi::emit_load_temporary_stack_slot(ctx.emitter, result, 0);
        abi::emit_store_to_sp(ctx.emitter, result, offset + 16);
        abi::emit_reg_move(ctx.emitter, result, previous);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        scope_reload::retire_eval_replacement_owner(ctx.emitter, &PhpType::Object("Throwable".into()), Some(offset));
    } else {
        super::super::super::exceptions::emit_type_error(ctx, &message);
    }
    abi::emit_jump(ctx.emitter, rejected);
    ctx.emitter.label(&accepted);
    abi::emit_pop_reg(ctx.emitter, result);
}

/// Detaches a removed PHP name before writing its raw missing-value fallback.
/// Other aliases keep their cell and payload, while this activation retires its own leases.
pub(super) fn detach_missing_local(
    ctx: &mut FunctionContext<'_>, local: &EvalSyncLocal, pending_throw: Option<usize>,
) -> Result<()> {
    let Some(state) = ctx.ref_cell_state_offset(local.slot) else { return Ok(()); };
    let offset = ctx.local_offset(local.slot)?;
    let result = abi::int_result_reg(ctx.emitter);
    let raw = ctx.next_label("eval_unset_raw_local");
    let owner = ctx.next_label("eval_unset_hidden_owner");
    abi::load_at_offset(ctx.emitter, result, state);
    abi::emit_store_zero_to_local_slot(ctx.emitter, state);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &raw);
    abi::emit_store_zero_to_local_slot(ctx.emitter, offset);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {result}, #1"));              // ordinary borrowed cells have no counted state-word lease
            ctx.emitter.instruction(&format!("b.ls {owner}"));                  // leave the shared cell's payload intact
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {result}, 1"));               // ordinary borrowed cells have no counted state-word lease
            ctx.emitter.instruction(&format!("jbe {owner}"));                   // leave the shared cell's payload intact
        }
    }
    scope_reload::retire_eval_owner_with_entry(ctx.emitter, "__rt_decref_any", pending_throw);
    abi::emit_jump(ctx.emitter, &owner);
    ctx.emitter.label(&raw);
    abi::load_at_offset(ctx.emitter, result, offset);
    abi::emit_store_zero_to_local_slot(ctx.emitter, offset);
    scope_reload::retire_eval_replacement_owner(ctx.emitter, &local.ty, pending_throw);
    ctx.emitter.label(&owner);
    if local.ty.stack_size() > 8 {
        abi::emit_store_zero_to_local_slot(ctx.emitter, offset - 8);
    }
    if let Some(owner) = ctx.function.locals[local.slot.as_raw() as usize].ref_cell_owner {
        let offset = ctx.local_offset(owner)?;
        abi::load_at_offset(ctx.emitter, result, offset);
        abi::emit_store_zero_to_local_slot(ctx.emitter, offset);
        scope_reload::retire_eval_owner_with_entry(ctx.emitter, "__rt_reference_cell_release", pending_throw);
    }
    Ok(())
}

/// A present value after unset belongs to the newly bound PHP name, not its former property cell.
pub(super) fn detach_rebound_local(
    ctx: &mut FunctionContext<'_>, local: &EvalSyncLocal, pending_throw: Option<usize>,
) -> Result<()> {
    if ctx.ref_cell_state_offset(local.slot).is_none() { return Ok(()); }
    let attached = ctx.next_label("eval_reload_still_attached");
    let flags = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, flags, 8);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("tst {flags}, #{EVAL_SCOPE_FLAG_NATIVE_DETACHED}")); // test the persistent native-name detachment marker
            ctx.emitter.instruction(&format!("b.eq {attached}"));               // preserve the original alias unless unset detached this name
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {flags}, {EVAL_SCOPE_FLAG_NATIVE_DETACHED}")); // test the persistent native-name detachment marker
            ctx.emitter.instruction(&format!("je {attached}"));                 // preserve the original alias unless unset detached this name
        }
    }
    detach_missing_local(ctx, local, pending_throw)?;
    ctx.emitter.label(&attached);
    Ok(())
}
