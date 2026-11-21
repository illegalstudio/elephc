//! Purpose:
//! Publishes a retained Mixed cell into bounded indexed storage.
//!
//! Called from:
//! - The native deserializer through its existing deferred-owner chain.
//!
//! Key details:
//! - The caller supplies the reserved ordinal and the storage and cleanup symbols.
//! - Out-of-range ordinals publish nothing. The incoming owner remains with the caller.

use crate::{Arch, Emitter};

/// Number of reference slots shared by native unserialize scopes and their snapshots.
pub const UNSERIALIZE_CAPACITY: usize = 65_536;

/// Declares the count and backing slots of the native unserialize reference registry.
pub fn append_unserialize_storage(out: &mut String, target: &impl crate::data::DataTarget) {
    out.push_str(&target.common("_unser_count", 8));
    out.push_str(&target.common("_unser_values", UNSERIALIZE_CAPACITY * 8));
}

/// Emits a two-argument C entry that registers a borrowed cell and gives its scope a lease.
/// The fixed table capacity must fit a single immediate on both native architectures.
pub fn emit_register_cell(
    emitter: &mut impl Emitter,
    entry: &str,
    table: &str,
    capacity: usize,
    defer_owner: &str,
) {
    assert!((1..=65_536).contains(&capacity));
    let done = format!("{entry}_done");
    let result = emitter.int_result_reg();
    emitter.blank();
    emitter.label_global(entry);
    emitter.emit_frame_prologue(32);
    emitter.store_at_offset(emitter.int_arg_reg_name(0), 8);
    emitter.store_at_offset(emitter.int_arg_reg_name(1), 16);
    emitter.load_at_offset(emitter.secondary_scratch_reg(), 16);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x11, #{capacity}"));              // bound publication to the physical registry capacity
            emitter.instruction("cmp x10, x11");                                // test the reserved index before computing an address
            emitter.instruction(&format!("b.hs {done}"));                       // overflow values have no registry lease
            emitter.emit_symbol_address("x11", table);
            emitter.load_at_offset(result, 8);
            emitter.instruction("str x0, [x11, x10, lsl #3]");                  // publish the stable Mixed box for subsequent references
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp r10, {capacity}"));               // test the reserved index before computing an address
            emitter.instruction(&format!("jae {done}"));                        // overflow values have no registry lease
            emitter.emit_symbol_address("r11", table);
            emitter.load_at_offset(result, 8);
            emitter.instruction("mov QWORD PTR [r11 + r10*8], rax");            // publish the stable Mixed box for subsequent references
        }
    }
    emitter.emit_call_label("__rt_incref");
    emitter.load_at_offset(result, 8);
    emitter.emit_call_label(defer_owner);
    emitter.label(&done);
    emitter.load_at_offset(result, 8);
    emitter.emit_frame_restore(32);
    emitter.emit_return();
}
