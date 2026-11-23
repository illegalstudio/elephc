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
use elephc_memoir::values::resource_weak;

/// Publishes retained canonical cells, then retires every orphan under protected cleanup.
pub(super) fn emit(emitter: &mut Emitter) {
    let scope = CleanupScope { arm: 0, x86: 96 };
    emitter.label_global("__rt_web_inventory_prune");
    // -- partition the old chain without invoking user code --
    abi::emit_frame_prologue(emitter, 144);
    scope.begin(emitter);
    resource_weak::emit_prune(emitter, |emitter| {
        emitter.instruction(if emitter.target.arch == Arch::AArch64 { "cmp x0, #1" } else { "cmp rax, 1" }); // stream orphans require full descriptor teardown
        emitter.instruction(if emitter.target.arch == Arch::AArch64 { "b.ne __rt_web_inventory_free" } else { "jne __rt_web_inventory_free" }); // other kinds retain their inventory-owned cleanup
        resource_weak::emit_load_orphan_payload(emitter);
        emitter.instruction(if emitter.target.arch == Arch::AArch64 { "cmp x0, #2" } else { "cmp rax, 2" }); // process stdio remains borrowed by implicit aliases
        emitter.instruction(if emitter.target.arch == Arch::AArch64 { "b.le __rt_web_inventory_free" } else { "jle __rt_web_inventory_free" }); // skip standard descriptors and closed sentinels
        resource_weak::emit_mark_orphan_closed(emitter);
        scope.call(emitter, "__rt_web_close_stream", false);
    });

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
