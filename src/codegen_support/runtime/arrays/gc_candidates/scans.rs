//! Purpose:
//! Shares heap and candidate-vector iteration across collector phases.
//!
//! Called from:
//! - The ARM64/x86 collectors and destructor snapshot scans.
//!
//! Key details:
//! - Legacy collection keeps physical heap cursors; application cursors address raw pointer slots.
//! - Each cursor remains stable until the next recount, including during PHP destructors.

use super::{load, op};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Branches over application-only iterator code using an otherwise dead scratch register.
fn legacy(e: &mut Emitter, done: &str) {
    load(e, "x17", "rax", "_gc_candidates_enabled");
    op(e, &format!("cbz x17, {done}"), "test rax, rax", "test whether this phase uses the application candidate vector");
    if e.target.arch == Arch::X86_64 { op(e, "", &format!("jz {done}"), "preserve the physical heap cursor in legacy collection"); }
}

/// Replaces captured heap bounds with the prepared vector's exact pointer-slot bounds.
pub(crate) fn bounds(e: &mut Emitter, arm: (&str, &str), x86: (&str, &str), name: &str) {
    // -- select bounds for the active collector representation --
    let done = format!("__rt_gc_{name}_bounds_ready");
    legacy(e, &done);
    load(e, arm.0, x86.0, "_gc_candidates_nodes");
    load(e, arm.1, x86.1, "_gc_candidates_nodes_count");
    op(e, &format!("add {}, {}, {}, lsl #3", arm.1, arm.0, arm.1), &format!("lea {}, [{} + {} * 8]", x86.1, x86.0, x86.1), "derive the exact end of the prepared pointer-slot range");
    e.label(&done);
}

/// Resolves a vector slot after checking bounds, optionally rejecting stale snapshot identities.
pub(crate) fn header(e: &mut Emitter, arm: &str, x86: &str, name: &str, stale: Option<&str>) {
    // -- resolve and validate an exact candidate identity --
    let done = format!("__rt_gc_{name}_header_ready");
    legacy(e, &done);
    op(e, &format!("ldr {arm}, [{arm}]"), &format!("mov {x86}, QWORD PTR [{x86}]"), "resolve a bounded candidate slot to its stored header");
    if let Some(stale) = stale {
        op(e, "mov x12, x10", &format!("lea rax, [{x86} + 16]"), "preserve the collector scratch while locating the payload");
        if e.target.arch == Arch::AArch64 { op(e, &format!("add x0, {arm}, #16"), "", "provide the payload base required for exact validation"); }
        abi::emit_call_label(e, "__rt_gc_candidate_live");
        op(e, "mov x10, x12", "test rax, rax", "restore the collector scratch after identity validation");
        op(e, &format!("cbz x0, {stale}"), &format!("jz {stale}"), "skip a retired or independently reused allocation");
        op(e, &format!("sub {arm}, x0, #16"), &format!("lea {x86}, [rax - 16]"), "recover the validated header for this scan phase");
        op(e, "ldr x17, [x0, #-8]", "test DWORD PTR [rax - 8], 0x20000", "inspect current snapshot membership before interpreting the node");
        op(e, &format!("tbz x17, #17, {stale}"), &format!("jz {stale}"), "exclude identities that no longer belong to this snapshot");
    }
    e.label(&done);
}

/// Overrides an already computed physical successor with the next candidate pointer slot.
pub(crate) fn next(e: &mut Emitter, arm: &str, x86: &str, arm_cursor: &str, x86_cursor: &str, name: &str) {
    // -- advance the representation-specific cursor --
    let done = format!("__rt_gc_{name}_next_ready");
    legacy(e, &done);
    op(e, &format!("ldr {arm}, {arm_cursor}"), &format!("mov {x86}, QWORD PTR {x86_cursor}"), "recover the stable candidate-slot cursor for this phase");
    op(e, &format!("add {arm}, {arm}, #8"), &format!("add {x86}, 8"), "advance one pointer slot instead of one physical heap block");
    e.label(&done);
}
