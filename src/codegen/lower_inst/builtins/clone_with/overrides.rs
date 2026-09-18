//! Purpose:
//! Lowers the PHP 8.5 `clone($object, $withProperties)` override application: after the shallow
//! copy and `__clone()`, the clone and the runtime override array are handed to the generated
//! applicator selected by the clone's RUNTIME class and the INVOCATION-SITE scope.
//!
//! Called from:
//! - `super::lower_clone_with()`, inside the window where the clone is an unwind-visible owner.
//!
//! Key details:
//! - No property semantics live here. `crate::ir_lower::clone_overrides` generated one ordinary
//!   PHP function per `(runtime class, scope profile)`, so iteration order, key stringification,
//!   NUL rejection, typed weak coercion, set hooks, `__set`, dynamic-property storage, readonly
//!   reinitialization and every refusal message come from the ordinary lowering pipeline.
//! - Dispatch is two-level. The clone's dense class id selects the class arm; inside it the
//!   transported scope id selects the exact body, falling through to the group that also serves
//!   global scope. A call site whose scope is statically known skips the second level entirely
//!   and names one symbol.
//! - The receiver travels in the callee-saved nested-call register, which is the contract
//!   `materialize_method_call_args_with_receiver_reg_and_refs` enforces, so materializing the
//!   Mixed override argument cannot destroy it.
//! - A runtime class with NO applicator, a reflection or otherwise excluded class reached
//!   through a runtime callable, REPORTS. It runs the same non-empty-array guard the refusal
//!   slice used and raises a catchable `Error`, so an override is never silently dropped.
//! - The by-reference refusal lives HERE too, but it is emitted for the applicator, not for the
//!   clone site: `lower_reference_override_guard()` backs the internal
//!   `__elephc_clone_override_reference_guard` builtin the generated body calls once per entry.
//!   php applies every earlier override before it reaches the referenced one, so a whole-array
//!   pre-scan would throw too early and silently drop writes php had already performed.

use std::collections::BTreeMap;

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::platform::Arch;
use crate::codegen::Result;
use crate::ir::{CloneOverrideApplicator, Instruction, ValueId};
use crate::types::PhpType;

use super::super::super::{
    direct_call_stack_pad_bytes, emit_call_arg_temp_cleanups, emit_ref_arg_writebacks,
    expect_operand, materialize_method_call_args_with_receiver_reg_and_refs, store_if_result,
    RefArgCellLifetime,
};

const REFERENCE_OVERRIDE_MESSAGE: &str =
    "Cannot assign by reference when cloning with updated properties";

/// Applies `$withProperties` to the clone parked in the caller's temporary stack slot.
///
/// `clone_box_offset` is the offset of that boxed clone inside the caller's owner window.
pub(super) fn emit_property_overrides(
    ctx: &mut FunctionContext<'_>,
    properties: ValueId,
    invocation_scope: Option<ValueId>,
    clone_box_offset: usize,
) -> Result<()> {
    if super::value_is_empty_array_literal(ctx, properties)? {
        return Ok(());
    }
    let applicators = applicators_by_class(ctx);
    let done = ctx.next_label("clone_overrides_done");
    let miss = ctx.next_label("clone_overrides_unsupported");
    if applicators.is_empty() {
        // Nothing in this program can take overrides, so every reachable clone reports.
        super::emit_property_override_guard(ctx, properties)?;
        return Ok(());
    }

    let receiver_reg = abi::nested_call_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        clone_box_offset,
    );
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let payload_reg = crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
    abi::emit_reg_move(ctx.emitter, &receiver_reg, payload_reg);

    let class_labels = applicators
        .keys()
        .map(|class_id| (*class_id, ctx.next_label(&format!("clone_overrides_{class_id}"))))
        .collect::<Vec<_>>();
    emit_class_id_dispatch(ctx, &receiver_reg, &class_labels, &miss);

    for ((class_id, label), entries) in class_labels.iter().zip(applicators.values()) {
        let _ = class_id;
        ctx.emitter.label(label);
        emit_class_arm(ctx, &receiver_reg, properties, invocation_scope, entries)?;
        abi::emit_jump(ctx.emitter, &done);
    }

    ctx.emitter.label(&miss);
    super::emit_property_override_guard(ctx, properties)?;
    ctx.emitter.label(&done);
    Ok(())
}

/// Refuses ONE override entry whose value still belongs to a PHP reference set.
///
/// Backs the internal `__elephc_clone_override_reference_guard(overrides, name, value)` builtin
/// the generated applicator body calls after it stringifies the current key and clears the NUL
/// guard, so every earlier entry has already been written when this one refuses.
///
/// The third operand is the applicator's OWN loop value local. A by-value `foreach` retains the
/// boxed Mixed cell of a tag-7 entry (`codegen/lower_inst/iterators.rs`), so the guard discounts
/// that one borrow instead of reading the loop itself as a second owner of the reference cell.
pub(crate) fn lower_reference_override_guard(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "__elephc_clone_override_reference_guard", 3)?;
    ctx.emitter.blank();
    ctx.emitter.comment("__elephc_clone_override_reference_guard()");
    let overrides = expect_operand(inst, 0)?;
    let name = expect_operand(inst, 1)?;
    // The third argument is the override value. The cell-owner predicate no longer needs it:
    // a by-value read retains the boxed value inside the reference cell, not the cell itself,
    // so there is nothing for it to discount from the owner count.
    expect_operand(inst, 2)?;
    let done = ctx.next_label("clone_reference_override_done");
    let probe_done = ctx.next_label("clone_reference_override_probe_done");
    let reject = ctx.next_label("clone_reference_override_reject");
    emit_override_hash_pointer(ctx, overrides, &done)?;
    emit_entry_reference_probe(ctx, name, &probe_done, &reject)?;
    ctx.emitter.label(&probe_done);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&reject);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    super::super::super::exceptions::emit_error(ctx, REFERENCE_OVERRIDE_MESSAGE);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Leaves the override array's hash-table pointer in the integer result register.
///
/// Jumps to `done` for every container shape that cannot carry hash-entry reference metadata,
/// which is exactly the set of arrays whose entries are always ordinary by-value overrides.
fn emit_override_hash_pointer(
    ctx: &mut FunctionContext<'_>,
    properties: ValueId,
    done: &str,
) -> Result<()> {
    let ty = ctx.value_php_type(properties)?.codegen_repr();
    match ty {
        PhpType::AssocArray { .. } => {
            ctx.load_value_to_result(properties)?;
        }
        PhpType::Array(_) => {
            ctx.load_value_to_result(properties)?;
            let result = abi::int_result_reg(ctx.emitter).to_string();
            abi::emit_push_reg(ctx.emitter, &result);
            abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("cmp x0, #3");                      // heap kind 3 identifies a runtime-promoted associative array
                    let hash = ctx.next_label("clone_reference_override_hash");
                    ctx.emitter.instruction(&format!("b.eq {hash}"));           // only hash entries can carry the persistent reference marker
                    abi::emit_pop_reg(ctx.emitter, &result);
                    ctx.emitter.instruction(&format!("b {done}"));              // indexed arrays have no hash-entry reference metadata
                    ctx.emitter.label(&hash);
                    abi::emit_pop_reg(ctx.emitter, &result);
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("cmp rax, 3");                      // heap kind 3 identifies a runtime-promoted associative array
                    let hash = ctx.next_label("clone_reference_override_hash");
                    ctx.emitter.instruction(&format!("je {hash}"));             // only hash entries can carry the persistent reference marker
                    abi::emit_pop_reg(ctx.emitter, &result);
                    ctx.emitter.instruction(&format!("jmp {done}"));            // indexed arrays have no hash-entry reference metadata
                    ctx.emitter.label(&hash);
                    abi::emit_pop_reg(ctx.emitter, &result);
                }
            }
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_result(properties)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("cmp x0, #5");                      // only associative arrays can carry hash-entry reference state
                    ctx.emitter.instruction(&format!("b.ne {done}"));           // other runtime array shapes have no marked hash entries
                    ctx.emitter.instruction("mov x0, x1");                      // select the associative-array payload for the marker scan
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("cmp rax, 5");                      // only associative arrays can carry hash-entry reference state
                    ctx.emitter.instruction(&format!("jne {done}"));            // other runtime array shapes have no marked hash entries
                    ctx.emitter.instruction("mov rax, rdi");                    // select the associative-array payload for the marker scan
                }
            }
        }
        other => {
            return Err(crate::codegen::CodegenIrError::unsupported(format!(
                "clone() reference override guard for {other:?}",
            )))
        }
    }

    Ok(())
}

/// Rejects the single override entry named by `name` when it still carries PHP reference state.
///
/// Runs with the override array's hash pointer in the integer result register. The matching
/// entry's payload is reloaded from the entry ADDRESS `__rt_hash_get` returns, because the
/// lookup's own payload registers are already dereferenced through the reference cell.
///
/// A hash entry belongs to a PHP reference set exactly when its runtime value tag is 11 and its
/// `value_lo` is the managed reference cell that set shares. The refusal predicate is therefore
/// "tag 11 and the cell has more than one owner": the entry itself owns one count, and any live
/// local alias or second entry in the same set owns another. The applicator's own by-value read
/// of the element retains the boxed Mixed value inside the cell rather than the cell, so unlike
/// the previous marker-word scheme there is nothing to discount here.
fn emit_entry_reference_probe(
    ctx: &mut FunctionContext<'_>,
    name: ValueId,
    probe_done: &str,
    reject: &str,
) -> Result<()> {
    abi::emit_reserve_temporary_stack(ctx.emitter, 16);
    let result = abi::int_result_reg(ctx.emitter).to_string();
    abi::emit_store_to_sp(ctx.emitter, &result, 0);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_string_value_to_regs(name, "x1", "x2")?;
            abi::emit_call_label(ctx.emitter, "__rt_hash_normalize_key");
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 0);
            abi::emit_call_label(ctx.emitter, "__rt_hash_get");
            ctx.emitter.instruction(&format!("cbz x4, {probe_done}"));          // a key with no entry of its own carries no reference metadata
            ctx.emitter.instruction("mov x6, x4");                              // hold the matching entry address across the payload loads
            ctx.emitter.instruction("ldr x3, [x6, #24]");                       // x3 = the entry's value_lo, its managed reference cell when tagged 11
            ctx.emitter.instruction("ldr x5, [x6, #40]");                       // x5 = the entry's value tag
            ctx.emitter.instruction("cmp x5, #11");                             // is this entry a member of a PHP reference set?
            ctx.emitter.instruction(&format!("b.ne {probe_done}"));             // ordinary entry values are by-value overrides
            ctx.emitter.instruction("ldr w10, [x3, #-12]");                     // load the shared reference cell's owner count
            ctx.emitter.instruction("cmp w10, #1");                             // does anything besides this entry still own the cell?
            ctx.emitter.instruction(&format!("b.hi {reject}"));                 // shared cell ownership preserves PHP reference identity
        }
        Arch::X86_64 => {
            ctx.load_string_value_to_regs(name, "rax", "rdx")?;
            abi::emit_call_label(ctx.emitter, "__rt_hash_normalize_key");
            ctx.emitter.instruction("mov rsi, rax");                            // move the normalized key low word into the hash lookup ABI register
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 0);
            abi::emit_call_label(ctx.emitter, "__rt_hash_get");
            ctx.emitter.instruction("test r8, r8");                             // a key with no entry of its own carries no reference metadata
            ctx.emitter.instruction(&format!("jz {probe_done}"));               // absent keys reach the ordinary dynamic-property arms
            ctx.emitter.instruction("mov r10, r8");                             // hold the matching entry address across the payload loads
            ctx.emitter.instruction("mov rcx, QWORD PTR [r10 + 24]");           // rcx = the entry's value_lo, its managed reference cell when tagged 11
            ctx.emitter.instruction("mov r9, QWORD PTR [r10 + 40]");            // r9 = the entry's value tag
            ctx.emitter.instruction("cmp r9, 11");                              // is this entry a member of a PHP reference set?
            ctx.emitter.instruction(&format!("jne {probe_done}"));              // ordinary entry values are by-value overrides
            ctx.emitter.instruction("mov r10d, DWORD PTR [rcx - 12]");          // load the shared reference cell's owner count
            ctx.emitter.instruction("cmp r10d, 1");                             // does anything besides this entry still own the cell?
            ctx.emitter.instruction(&format!("ja {reject}"));                   // shared cell ownership preserves PHP reference identity
        }
    }
    Ok(())
}

/// Groups the module's applicators by the runtime class they serve, in class-id order.
fn applicators_by_class(
    ctx: &FunctionContext<'_>,
) -> BTreeMap<u64, Vec<CloneOverrideApplicator>> {
    let mut grouped: BTreeMap<u64, Vec<CloneOverrideApplicator>> = BTreeMap::new();
    for applicator in &ctx.module.clone_override_applicators {
        if !ctx
            .module
            .functions
            .iter()
            .any(|function| function.name == applicator.function_name)
        {
            continue;
        }
        grouped
            .entry(applicator.class_id)
            .or_default()
            .push(applicator.clone());
    }
    grouped
}

/// Branches to the arm whose compile-time class id matches the clone's runtime class id.
fn emit_class_id_dispatch(
    ctx: &mut FunctionContext<'_>,
    receiver_reg: &str,
    class_labels: &[(u64, String)],
    miss: &str,
) {
    let (class_id_reg, compare_reg) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x9", "x10"),
        Arch::X86_64 => ("r11", "r10"),
    };
    abi::emit_load_from_address(ctx.emitter, class_id_reg, receiver_reg, 0);
    for (class_id, label) in class_labels {
        abi::emit_load_int_immediate(ctx.emitter, compare_reg, *class_id as i64);
        ctx.emitter
            .instruction(&format!("cmp {class_id_reg}, {compare_reg}"));
        match ctx.emitter.target.arch {
            Arch::AArch64 => ctx.emitter.instruction(&format!("b.eq {label}")), // select the matching generated clone override applicator
            Arch::X86_64 => ctx.emitter.instruction(&format!("je {label}")),    // select the matching generated clone override applicator
        }
    }
    abi::emit_jump(ctx.emitter, miss);
}

/// Emits one runtime class's arm: pick the scope's body, then call it.
fn emit_class_arm(
    ctx: &mut FunctionContext<'_>,
    receiver_reg: &str,
    properties: ValueId,
    invocation_scope: Option<ValueId>,
    entries: &[CloneOverrideApplicator],
) -> Result<()> {
    let Some(default) = entries
        .iter()
        .find(|entry| entry.is_default_scope)
        .or_else(|| entries.first())
    else {
        return Ok(());
    };
    let Some(invocation_scope) = invocation_scope else {
        // A direct call site knows its own lexical class, so one symbol is the whole answer.
        let scope_id = ctx
            .function
            .lexical_class
            .as_deref()
            .and_then(|name| ctx.module.class_infos.get(name))
            .map(|info| info.class_id);
        let selected = scope_id
            .and_then(|scope_id| {
                entries
                    .iter()
                    .find(|entry| entry.scope_class_ids.contains(&scope_id))
            })
            .unwrap_or(default);
        let selected = selected.clone();
        return emit_applicator_call(ctx, receiver_reg, properties, &selected);
    };
    let scoped = entries
        .iter()
        .filter(|entry| !entry.scope_class_ids.is_empty())
        .cloned()
        .collect::<Vec<_>>();
    let default = default.clone();
    if scoped.is_empty() {
        return emit_applicator_call(ctx, receiver_reg, properties, &default);
    }
    let arm_done = ctx.next_label("clone_overrides_scope_done");
    let default_label = ctx.next_label("clone_overrides_scope_default");
    let scope_labels = scoped
        .iter()
        .map(|_| ctx.next_label("clone_overrides_scope"))
        .collect::<Vec<_>>();
    ctx.load_value_to_result(invocation_scope)?;
    let scope_reg = abi::int_result_reg(ctx.emitter).to_string();
    for (entry, label) in scoped.iter().zip(scope_labels.iter()) {
        for scope_id in &entry.scope_class_ids {
            super::emit_branch_if_reg_equals_immediate(ctx, &scope_reg, *scope_id as i64, label);
        }
    }
    abi::emit_jump(ctx.emitter, &default_label);
    for (entry, label) in scoped.iter().zip(scope_labels.iter()) {
        ctx.emitter.label(label);
        emit_applicator_call(ctx, receiver_reg, properties, entry)?;
        abi::emit_jump(ctx.emitter, &arm_done);
    }
    ctx.emitter.label(&default_label);
    emit_applicator_call(ctx, receiver_reg, properties, &default)?;
    ctx.emitter.label(&arm_done);
    Ok(())
}

/// Calls one applicator with the clone as `$this` and the override array as the second argument.
fn emit_applicator_call(
    ctx: &mut FunctionContext<'_>,
    receiver_reg: &str,
    properties: ValueId,
    applicator: &CloneOverrideApplicator,
) -> Result<()> {
    let receiver_ty = PhpType::Object(applicator.class_name.clone());
    let params = [receiver_ty.clone(), PhpType::Mixed];
    let refs = [false, false];
    // The first operand is never read: the receiver comes from the register instead.
    let operands = [properties, properties];
    let call_args = materialize_method_call_args_with_receiver_reg_and_refs(
        ctx,
        receiver_reg,
        &receiver_ty,
        &operands,
        &params,
        &refs,
        RefArgCellLifetime::CallOnly,
    )?;
    let pad = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, pad);
    abi::emit_call_label(
        ctx.emitter,
        &crate::names::function_symbol(&applicator.function_name),
    );
    abi::emit_release_temporary_stack(ctx.emitter, pad);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    emit_call_arg_temp_cleanups(ctx, &call_args, None)?;
    emit_ref_arg_writebacks(ctx, &call_args)
}
