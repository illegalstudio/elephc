//! Purpose:
//! Retains Mixed reference storage and selects stable writable wrapper children.
//!
//! Called from:
//! - Compiler reference runtime and inline storage adapters.
//!
//! Key details:
//! - Promotion preserves native cell identity and the wrapper child owner.
//! - The host supplies the uninitialized sentinel and optional post-promotion type-source association.

use crate::{abi, Arch, Emitter};

/// Retains an existing managed cell or gives a borrowed Mixed slot an owned wrapper-backed cell.
pub fn emit_retain_mixed(emitter: &mut impl Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_reference_cell_retain_mixed");
    abi::emit_frame_prologue(emitter, 48);
    abi::store_at_offset(emitter, result, 8);
    abi::emit_call_label(emitter, "__rt_reference_cell_owner");
    abi::emit_branch_if_int_result_zero(emitter, "__rt_reference_cell_retain_mixed_borrowed");
    abi::emit_call_label(emitter, "__rt_incref");
    abi::emit_jump(emitter, "__rt_reference_cell_retain_mixed_done");
    emitter.label("__rt_reference_cell_retain_mixed_borrowed");
    abi::load_at_offset(emitter, result, 8);
    abi::emit_call_label(emitter, "__rt_reference_cell_promote_mixed");
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::store_at_offset(emitter, result, 16);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter, 0), 7);
    abi::emit_call_label(emitter, "__rt_reference_cell_new");
    abi::load_at_offset(emitter, scratch, 16);
    abi::emit_store_to_address(emitter, scratch, result, 0);
    emitter.label("__rt_reference_cell_retain_mixed_done");
    abi::emit_frame_restore(emitter, 48);
    abi::emit_return(emitter);
}

/// Selects the writable child slot of an existing reference without changing any owner counts.
pub fn follow_mixed_reference_slot(
    emitter: &mut impl Emitter, address: &str, value: &str, marker: &str, done: &str,
) {
    debug_assert!(address != value && address != marker && value != marker);
    abi::emit_load_from_address(emitter, value, address, 0);
    abi::emit_load_int_immediate(emitter, marker, emitter.uninitialized_property_sentinel());
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz {value}, {done}"));               // empty Mixed storage has no reference wrapper
            emitter.instruction(&format!("cmp {value}, {marker}"));             // preserve the low-word uninitialized marker
            emitter.instruction(&format!("b.eq {done}"));                       // never dereference the property sentinel
            emitter.instruction(&format!("ldr {marker}, [{value}]"));           // inspect the stored box without borrowing its child
            emitter.instruction(&format!("cmp {marker}, #7"));                  // persistent references use the nested Mixed tag
            emitter.instruction(&format!("b.ne {done}"));                       // ordinary boxes remain directly writable
            emitter.instruction(&format!("ldr {marker}, [{value}, #16]"));      // inspect the persistent reference marker
            emitter.instruction(&format!("cmp {marker}, #1"));                  // ordinary nested boxes are values rather than aliases
            emitter.instruction(&format!("b.ne {done}"));                       // retain the original native address for ordinary values
            emitter.instruction(&format!("add {address}, {value}, #8"));        // publish assignments into the stable wrapper's child slot
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {value}, {value}"));             // empty Mixed storage has no reference wrapper
            emitter.instruction(&format!("jz {done}"));                         // preserve the native null storage address
            emitter.instruction(&format!("cmp {value}, {marker}"));             // preserve the low-word uninitialized marker
            emitter.instruction(&format!("je {done}"));                         // never dereference the property sentinel
            emitter.instruction(&format!("cmp QWORD PTR [{value}], 7"));        // persistent references use the nested Mixed tag
            emitter.instruction(&format!("jne {done}"));                        // ordinary boxes remain directly writable
            emitter.instruction(&format!("cmp QWORD PTR [{value} + 16], 1"));   // distinguish a PHP reference from an ordinary nested box
            emitter.instruction(&format!("jne {done}"));                        // retain the native address for an ordinary value
            emitter.instruction(&format!("lea {address}, [{value} + 8]"));      // publish assignments into the stable wrapper's child slot
        }
    }
    emitter.label(done);
}

/// Installs one persistent PHP wrapper while leaving the native cell's address and ownership intact.
pub fn emit_promote_mixed<E: Emitter>(emitter: &mut E, associate: Option<fn(&mut E)>) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_reference_cell_promote_mixed");
    abi::emit_frame_prologue(emitter, 48);
    abi::store_at_offset(emitter, result, 8);
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::store_at_offset(emitter, result, 16);
    abi::emit_load_int_immediate(emitter, scratch, emitter.uninitialized_property_sentinel());
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {result}, {scratch}"));           // do not allocate over an uninitialized property
            emitter.instruction("b.eq __rt_reference_cell_promote_mixed_done"); // leave the existing initialization guard authoritative
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {result}, {scratch}"));           // do not allocate over an uninitialized property
            emitter.instruction("je __rt_reference_cell_promote_mixed_done");   // leave the existing initialization guard authoritative
        }
    }
    abi::emit_call_label(emitter, "__rt_reference_is");
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_reference_cell_promote_mixed_done");
    match emitter.arch() {
        Arch::AArch64 => {
            abi::load_at_offset(emitter, "x1", 16);
            abi::emit_load_int_immediate(emitter, "x0", 7);
            abi::emit_load_int_immediate(emitter, "x2", 1);
        }
        Arch::X86_64 => {
            abi::load_at_offset(emitter, "rdi", 16);
            abi::emit_load_int_immediate(emitter, "rax", 7);
            abi::emit_load_int_immediate(emitter, "rsi", 1);
        }
    }
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::load_at_offset(emitter, scratch, 8);
    abi::emit_store_to_address(emitter, result, scratch, 0);
    abi::load_at_offset(emitter, result, 16);
    abi::emit_call_label(emitter, "__rt_decref_any");
    emitter.label("__rt_reference_cell_promote_mixed_done");
    abi::load_at_offset(emitter, result, 8);
    if let Some(associate) = associate {
        associate(emitter);
    }
    abi::emit_frame_restore(emitter, 48);
    abi::emit_return(emitter);
}
