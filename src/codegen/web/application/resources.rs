//! Purpose:
//! Prunes dead resource nodes without closing application-owned handles.
//!
//! Called from:
//! - The application request-end boundary after cycle collection.
//!
//! Key details:
//! - Publish live cells before callbacks, which may create new resources.
//! - Retire closed entries without closing a reused descriptor or resetting IDs.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch, runtime::CleanupScope};

/// Publishes retained canonical cells, then retires every orphan under protected cleanup.
pub(super) fn emit(emitter: &mut Emitter) {
    let value = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let scope = CleanupScope { arm: 0, x86: 96 };
    emitter.label_global("__rt_web_inventory_prune");
    // -- partition the old chain without invoking user code --
    abi::emit_frame_prologue(emitter, 144);
    scope.begin(emitter);
    abi::emit_load_symbol_to_reg(emitter, value, "_resource_inventory_head", 0);
    abi::store_at_offset(emitter, value, 8);
    for offset in [16, 24, 32, 40] { abi::emit_store_zero_to_local_slot(emitter, offset); }
    emitter.label("__rt_web_inventory_scan");
    abi::load_at_offset(emitter, value, 8);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_web_inventory_publish");
    abi::store_at_offset(emitter, value, 48);
    abi::emit_load_from_address(emitter, scratch, value, 0);
    abi::store_at_offset(emitter, scratch, 8);
    abi::emit_store_zero_to_address(emitter, value, 0);
    abi::emit_load_from_address(emitter, value, value, 32);
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_web_inventory_orphan");
    abi::load_at_offset(emitter, value, 48);
    abi::emit_load_from_address(emitter, value, value, 40);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_web_inventory_orphan");
    append(emitter, 24, 32, "live");
    abi::emit_jump(emitter, "__rt_web_inventory_scan");
    emitter.label("__rt_web_inventory_orphan");
    append(emitter, 16, 40, "dead");
    abi::emit_jump(emitter, "__rt_web_inventory_scan");

    // -- make retained resources visible before close hooks can inspect the inventory --
    emitter.label("__rt_web_inventory_publish");
    for (offset, symbol) in [(24, "_resource_inventory_head"), (32, "_resource_inventory_tail")] {
        abi::load_at_offset(emitter, value, offset);
        abi::emit_store_reg_to_symbol(emitter, value, symbol, 0);
    }
    abi::load_at_offset(emitter, value, 16);
    abi::store_at_offset(emitter, value, 8);
    emitter.label("__rt_web_inventory_retire");
    abi::load_at_offset(emitter, value, 8);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_web_inventory_done");
    abi::store_at_offset(emitter, value, 48);
    abi::emit_load_from_address(emitter, scratch, value, 0);
    abi::store_at_offset(emitter, scratch, 8);
    abi::emit_store_zero_to_address(emitter, value, 0);
    abi::emit_load_from_address(emitter, value, value, 32);
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_web_inventory_free");
    abi::load_at_offset(emitter, value, 48);
    abi::emit_load_from_address(emitter, value, value, 24);
    emitter.instruction(if emitter.target.arch == Arch::AArch64 { "cmp x0, #1" } else { "cmp rax, 1" }); // stream orphans require full descriptor teardown
    emitter.instruction(if emitter.target.arch == Arch::AArch64 { "b.ne __rt_web_inventory_free" } else { "jne __rt_web_inventory_free" }); // other kinds retain their inventory-owned cleanup
    abi::load_at_offset(emitter, value, 48);
    abi::emit_load_from_address(emitter, value, value, 16);
    emitter.instruction(if emitter.target.arch == Arch::AArch64 { "cmp x0, #2" } else { "cmp rax, 2" }); // process stdio remains borrowed by implicit aliases
    emitter.instruction(if emitter.target.arch == Arch::AArch64 { "b.le __rt_web_inventory_free" } else { "jle __rt_web_inventory_free" }); // skip standard descriptors and closed sentinels
    abi::load_at_offset(emitter, scratch, 48);
    let flag = if emitter.target.arch == Arch::AArch64 { "x12" } else { "r11" };
    abi::emit_load_int_immediate(emitter, flag, 1);
    abi::emit_store_to_address(emitter, flag, scratch, 32);
    scope.call(emitter, "__rt_web_close_stream", false);

    // -- isolate the legacy node free without losing callback-created resources or IDs --
    emitter.label("__rt_web_inventory_free");
    for (offset, symbol) in [(56, "_resource_inventory_head"), (64, "_resource_inventory_tail"), (72, "_resource_id_next")] {
        abi::emit_load_symbol_to_reg(emitter, value, symbol, 0);
        abi::store_at_offset(emitter, value, offset);
    }
    abi::load_at_offset(emitter, value, 48);
    abi::emit_store_reg_to_symbol(emitter, value, "_resource_inventory_head", 0);
    abi::emit_store_reg_to_symbol(emitter, value, "_resource_inventory_tail", 0);
    abi::emit_call_label(emitter, "__rt_resource_inventory_reset");
    for (offset, symbol) in [(56, "_resource_inventory_head"), (64, "_resource_inventory_tail"), (72, "_resource_id_next")] {
        abi::load_at_offset(emitter, value, offset);
        abi::emit_store_reg_to_symbol(emitter, value, symbol, 0);
    }
    abi::emit_jump(emitter, "__rt_web_inventory_retire");

    // -- propagate a close-hook error only after every orphan has retired --
    emitter.label("__rt_web_inventory_done");
    scope.finish(emitter);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_web_inventory_clean");
    abi::emit_frame_restore(emitter, 144);
    abi::emit_jump(emitter, "__rt_throw_current");
    emitter.label("__rt_web_inventory_clean");
    abi::emit_frame_restore(emitter, 144);
    abi::emit_return(emitter);
}

/// Appends the node in slot 48 to a local chain without allocating or invoking PHP.
fn append(emitter: &mut Emitter, head: usize, tail: usize, suffix: &str) {
    let value = abi::int_result_reg(emitter);
    let node = abi::secondary_scratch_reg(emitter);
    let first = format!("__rt_web_inventory_{suffix}_first");
    let done = format!("__rt_web_inventory_{suffix}_appended");
    abi::load_at_offset(emitter, value, tail);
    abi::emit_branch_if_int_result_zero(emitter, &first);
    abi::load_at_offset(emitter, node, 48);
    abi::emit_store_to_address(emitter, node, value, 0);
    abi::emit_jump(emitter, &done);
    emitter.label(&first);
    abi::load_at_offset(emitter, node, 48);
    abi::store_at_offset(emitter, node, head);
    emitter.label(&done);
    abi::store_at_offset(emitter, node, tail);
}
