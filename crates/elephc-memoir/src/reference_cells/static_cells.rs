//! Purpose:
//! Emits raw and managed storage behind static Mixed bindings.
//!
//! Called from:
//! - Compiler static-binding runtime adapters.
//!
//! Key details:
//! - Promotion transfers the existing owner into an allocated cell.
//! - The host unbinds weak type-source metadata after detachment and before managed cell release.
//! - The unbind callback receives the binding address in target C argument zero.

use crate::{abi, Arch, Emitter};

/// Byte offset of the promoted-layout word after value and initialization storage.
pub const LAYOUT_OFFSET: usize = 16;
/// Bytes in a complete raw-or-managed static binding record.
pub const BINDING_BYTES: usize = LAYOUT_OFFSET + 8;

/// Returns a borrowed payload address, accepting a raw binding or its allocated managed cell.
pub fn emit_payload(emitter: &mut impl Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let done = "__rt_reference_static_mixed_payload_done";
    emitter.label_global("__rt_reference_static_mixed_payload");
    abi::emit_reg_move(emitter, result, abi::int_arg_reg_name(emitter, 0));
    abi::emit_load_from_address(emitter, scratch, result, LAYOUT_OFFSET);
    branch_zero(emitter, scratch, done);
    abi::emit_load_from_address(emitter, result, result, 0);
    emitter.label(done);
    abi::emit_return(emitter);
}

/// Gives one initialized Mixed static binding a cell owner without retaining its child twice.
pub fn emit_promote(emitter: &mut impl Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let ready = "__rt_reference_static_mixed_promote_ready";
    let refused = "__rt_reference_static_mixed_promote_refused";
    let done = "__rt_reference_static_mixed_promote_done";
    emitter.label_global("__rt_reference_static_mixed_promote");
    abi::emit_frame_prologue(emitter, 48);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter, 0), 8);
    abi::load_at_offset(emitter, result, 8);
    abi::emit_load_from_address(emitter, scratch, result, LAYOUT_OFFSET);
    branch_nonzero(emitter, scratch, ready);
    abi::emit_load_from_address(emitter, result, result, 8);
    branch_uninitialized(emitter, result, scratch, refused);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter, 0), 7);
    abi::emit_call_label(emitter, "__rt_reference_cell_new");
    abi::store_at_offset(emitter, result, 16);
    abi::load_at_offset(emitter, scratch, 8);
    abi::emit_load_from_address(emitter, scratch, scratch, 0);
    abi::emit_store_to_address(emitter, scratch, result, 0);
    abi::emit_call_label(emitter, "__rt_reference_cell_promote_mixed");
    abi::load_at_offset(emitter, scratch, 8);
    abi::emit_store_to_address(emitter, result, scratch, 0);
    abi::emit_store_zero_to_address(emitter, scratch, 8);
    abi::emit_load_int_immediate(emitter, result, 1);
    abi::emit_store_to_address(emitter, result, scratch, LAYOUT_OFFSET);
    emitter.label(ready);
    abi::load_at_offset(emitter, result, 8);
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::emit_jump(emitter, done);
    emitter.label(refused);
    abi::emit_load_int_immediate(emitter, result, 0);
    emitter.label(done);
    abi::emit_frame_restore(emitter, 48);
    abi::emit_return(emitter);
}

/// Detaches one static owner before releasing either its raw box or its promoted cell.
pub fn emit_release<E: Emitter>(emitter: &mut E, unbind_source: fn(&mut E)) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let raw = "__rt_reference_static_mixed_release_raw";
    let done = "__rt_reference_static_mixed_release_done";
    emitter.label_global("__rt_reference_static_mixed_release");
    abi::emit_frame_prologue(emitter, 48);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter, 0), 8);
    abi::load_at_offset(emitter, result, 8);
    abi::emit_load_from_address(emitter, scratch, result, LAYOUT_OFFSET);
    abi::store_at_offset(emitter, scratch, 24);
    abi::emit_load_from_address(emitter, result, result, 8);
    let initialized = "__rt_reference_static_mixed_release_initialized";
    abi::load_at_offset(emitter, scratch, 24);
    branch_nonzero(emitter, scratch, initialized);
    branch_uninitialized(emitter, result, scratch, done);
    emitter.label(initialized);
    abi::load_at_offset(emitter, scratch, 8);
    abi::emit_load_from_address(emitter, result, scratch, 0);
    abi::store_at_offset(emitter, result, 16);
    abi::emit_store_zero_to_address(emitter, scratch, 0);
    abi::emit_store_zero_to_address(emitter, scratch, LAYOUT_OFFSET);
    abi::emit_load_int_immediate(emitter, result, emitter.uninitialized_property_sentinel());
    abi::emit_store_to_address(emitter, result, scratch, 8);
    abi::load_at_offset(emitter, result, 24);
    abi::emit_branch_if_int_result_zero(emitter, raw);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter, 0), 8);
    unbind_source(emitter);
    abi::load_at_offset(emitter, result, 16);
    abi::emit_call_label(emitter, "__rt_reference_cell_release");
    abi::emit_jump(emitter, done);
    emitter.label(raw);
    abi::load_at_offset(emitter, result, 16);
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    emitter.label(done);
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::emit_frame_restore(emitter, 48);
    abi::emit_return(emitter);
}

/// Selects a zero layout word without borrowing the integer-result register.
fn branch_zero(emitter: &mut impl Emitter, value: &str, label: &str) {
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction(&format!("cbz {value}, {label}")), // raw bindings expose their existing value address
        Arch::X86_64 => {
            emitter.instruction(&format!("test {value}, {value}"));             // inspect the independent promoted-layout word
            emitter.instruction(&format!("jz {label}"));                        // raw bindings expose their existing value address
        }
    }
}

/// Selects an already published managed layout without interpreting the binding as a heap cell.
fn branch_nonzero(emitter: &mut impl Emitter, value: &str, label: &str) {
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction(&format!("cbnz {value}, {label}")), // a promoted binding owns the managed cell in its low word
        Arch::X86_64 => {
            emitter.instruction(&format!("test {value}, {value}"));             // inspect the independent promoted-layout word
            emitter.instruction(&format!("jnz {label}"));                       // a promoted binding owns the managed cell in its low word
        }
    }
}

/// Leaves an uninitialized raw binding unchanged instead of treating its marker as a PHP box.
fn branch_uninitialized(emitter: &mut impl Emitter, value: &str, marker: &str, label: &str) {
    abi::emit_load_int_immediate(emitter, marker, emitter.uninitialized_property_sentinel());
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {value}, {marker}"));             // uninitialized raw statics have no value owner to transfer
            emitter.instruction(&format!("b.eq {label}"));                      // preserve the caller's PHP initialization guard
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {value}, {marker}"));             // uninitialized raw statics have no value owner to transfer
            emitter.instruction(&format!("je {label}"));                        // preserve the caller's PHP initialization guard
        }
    }
}
