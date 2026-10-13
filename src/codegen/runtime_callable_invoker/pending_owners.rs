//! Purpose:
//! Tracks the descriptor invoker's parameter owners across marshalling and callee execution.
//!
//! Called from:
//! - Argument marshalling, invocation handoff, and the parent invoker's exception escape.
//!
//! Key details:
//! - A Stringable conversion can throw before target entry; preceding arguments still belong here.
//! - The native user-call ABI borrows arguments; callee-owned slots acquire independent leases.
//! - Slots are cleared before releases or return-alias transfer, preventing duplicate releases.
//! - Borrowed object receivers/captures and by-reference argument cells are not registered.

use super::*;

/// Locates one parameter's pointer/tag pair beyond the invoker's fixed frame fields.
fn offset(index: usize) -> usize { INVOKER_FRAME_SIZE + index * 16 }

/// Initializes every pending-owner record before marshalling can acquire argument leases.
pub(super) fn clear(emitter: &mut Emitter, ctx: &InvokerEmitContext) {
    for index in 0..ctx.owner_slot_count {
        clear_slot(emitter, index);
    }
}

/// Clears one owner pointer and its release tag without destroying or transferring the value.
pub(super) fn clear_slot(emitter: &mut Emitter, index: usize) {
    abi::emit_store_zero_to_local_slot(emitter, offset(index));
    abi::emit_store_zero_to_local_slot(emitter, offset(index) - 8);
}

/// Selects a storage-aware release tag, using the heap header for generic container carriers.
fn owner_tag(php_type: &PhpType) -> Option<i64> {
    match php_type.codegen_repr() {
        PhpType::Str => Some(1),
        // Generic PHP-array contracts can transport either concrete runtime container shape.
        // The heap header, rather than a specialized callee slot, owns the release decision.
        PhpType::Array(_) | PhpType::AssocArray { .. } => Some(-1),
        PhpType::Mixed | PhpType::Union(_) => Some(7),
        PhpType::Callable => Some(10),
        PhpType::Iterable => Some(-1),
        _ => None,
    }
}

/// Registers a newly acquired owner from the temporary stack for later failure/return cleanup.
pub(super) fn register_pushed(emitter: &mut Emitter, index: usize, php_type: &PhpType) {
    let Some(tag) = owner_tag(php_type) else { return };
    let scratch = abi::symbol_scratch_reg(emitter);
    abi::emit_load_temporary_stack_slot(emitter, scratch, 0);
    abi::store_at_offset(emitter, scratch, offset(index));
    abi::emit_load_int_immediate(emitter, scratch, tag);
    abi::store_at_offset(emitter, scratch, offset(index) - 8);
}

/// Registers an owned current result and its concrete release contract in the parameter record.
pub(super) fn register_result(emitter: &mut Emitter, index: usize, php_type: &PhpType) {
    let Some(tag) = owner_tag(php_type) else { return };
    abi::store_at_offset(emitter, abi::int_result_reg(emitter), offset(index));
    let scratch = abi::symbol_scratch_reg(emitter);
    abi::emit_load_int_immediate(emitter, scratch, tag);
    abi::store_at_offset(emitter, scratch, offset(index) - 8);
}

/// Tracks an owned temporary managed reference cell separately from its contained payload kind.
pub(super) fn register_pushed_ref_cell(emitter: &mut Emitter, index: usize) {
    let scratch = abi::symbol_scratch_reg(emitter);
    abi::emit_load_temporary_stack_slot(emitter, scratch, 0);
    abi::store_at_offset(emitter, scratch, offset(index));
    abi::emit_load_int_immediate(emitter, scratch, 11);
    abi::store_at_offset(emitter, scratch, offset(index) - 8);
}

/// Transfers exactly one matching caller owner into a borrowed identity-Mixed result.
pub(super) fn transfer_mixed_result_alias(emitter: &mut Emitter, ctx: &mut InvokerEmitContext) {
    transfer_result_alias(emitter, ctx, 7, abi::int_result_reg(emitter));
}

/// Gives a borrowed string-argument alias to the existing return persistence/boxing owner path.
pub(super) fn transfer_string_result_alias(emitter: &mut Emitter, ctx: &mut InvokerEmitContext) {
    transfer_result_alias(emitter, ctx, 1, abi::string_result_regs(emitter).0);
}

/// Detaches exactly one matching pending owner when a borrowed result aliases an argument lease.
fn transfer_result_alias(emitter: &mut Emitter, ctx: &mut InvokerEmitContext, expected_tag: i64, result: &str) {
    let done = ctx.next_label("pending_mixed_alias_transferred");
    let tag = abi::secondary_scratch_reg(emitter);
    let pointer = abi::tertiary_scratch_reg(emitter);
    let scratch = abi::symbol_scratch_reg(emitter);
    for index in 0..ctx.owner_slot_count {
        let next = ctx.next_label("pending_mixed_alias_next");
        abi::load_at_offset(emitter, tag, offset(index) - 8);
        abi::emit_load_int_immediate(emitter, scratch, expected_tag);
        emitter.instruction(&format!("cmp {tag}, {scratch}"));                  // only the matching owner kind can satisfy the return transfer contract
        emitter.instruction(&format!("{} {next}", if emitter.target.arch == Arch::AArch64 { "b.ne" } else { "jne" })); // non-Mixed parameters cannot be identity-box aliases
        abi::load_at_offset(emitter, pointer, offset(index));
        emitter.instruction(&format!("cmp {pointer}, {result}"));               // compare the borrowed result against the invoker's pending box owner
        emitter.instruction(&format!("{} {next}", if emitter.target.arch == Arch::AArch64 { "b.ne" } else { "jne" })); // keep unrelated argument owners scheduled for release
        abi::emit_store_zero_to_local_slot(emitter, offset(index));
        abi::emit_store_zero_to_local_slot(emitter, offset(index) - 8);
        abi::emit_jump(emitter, &done);
        emitter.label(&next);
    }
    emitter.label(&done);
}

/// Retires all pending records, including the reserved result owner, in reverse acquisition order.
pub(super) fn release(emitter: &mut Emitter, ctx: &mut InvokerEmitContext) {
    let count = ctx.owner_slot_count;
    release_slots(emitter, ctx, count);
}

/// Releases argument leases while keeping the final reserved result-owner slot intact.
pub(super) fn release_arguments(emitter: &mut Emitter, ctx: &mut InvokerEmitContext) {
    let count = ctx.owner_slot_count.saturating_sub(1);
    release_slots(emitter, ctx, count);
}

/// Clears each record before dispatching its typed release, preventing reentrant duplicate cleanup.
fn release_slots(emitter: &mut Emitter, ctx: &mut InvokerEmitContext, count: usize) {
    for index in (0..count).rev() {
        let done = ctx.next_label("pending_argument_released");
        let result = abi::int_result_reg(emitter);
        let tag = abi::secondary_scratch_reg(emitter);
        abi::load_at_offset(emitter, result, offset(index));
        abi::load_at_offset(emitter, tag, offset(index) - 8);
        abi::emit_store_zero_to_local_slot(emitter, offset(index));
        abi::emit_store_zero_to_local_slot(emitter, offset(index) - 8);
        abi::emit_branch_if_int_result_zero(emitter, &done);
        for (kind, helper) in [(1, "__rt_heap_free_safe"), (4, "__rt_decref_array"), (5, "__rt_decref_hash"), (7, "__rt_decref_mixed"), (10, "__rt_callable_descriptor_release"), (11, "__rt_ref_cell_release"), (-1, "__rt_decref_any")] {
            let next = ctx.next_label("pending_argument_next_kind");
            let scratch = abi::symbol_scratch_reg(emitter);
            abi::emit_load_int_immediate(emitter, scratch, kind);
            emitter.instruction(&format!("cmp {tag}, {scratch}"));              // select the parameter owner's concrete release contract
            emitter.instruction(&format!("{} {next}", if emitter.target.arch == Arch::AArch64 { "b.ne" } else { "jne" })); // skip nonmatching release contracts
            if kind == 11 {
                let value_tag = if emitter.target.arch == Arch::AArch64 { "x1" } else { "rdx" };
                abi::emit_load_from_address(emitter, value_tag, result, 8);
            }
            abi::emit_call_label(emitter, helper);
            abi::emit_jump(emitter, &done);
            emitter.label(&next);
        }
        emitter.label(&done);
    }
}
