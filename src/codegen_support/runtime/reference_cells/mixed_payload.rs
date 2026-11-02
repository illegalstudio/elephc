//! Purpose:
//! Keeps native Mixed reference cells and persistent PHP value aliases on the same storage.
//!
//! Called from:
//! - Native reference-property capture, local/property stores and eval property replacement.
//!
//! Key details:
//! - Kind-eight cells retain their existing descriptor seven and low-word sentinel.
//! - A requested reference lazily wraps the current box without copying its PHP value.
//! - Writers follow the wrapper's immediate child slot and preserve the wrapper identity.

use super::*;

/// Selects the writable child slot of an existing reference without changing any owner counts.
pub(crate) fn follow_mixed_reference_slot(
    emitter: &mut Emitter, address: &str, value: &str, marker: &str, done: &str,
) {
    debug_assert!(address != value && address != marker && value != marker);
    abi::emit_load_from_address(emitter, value, address, 0);
    abi::emit_load_int_immediate(emitter, marker, UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    match emitter.target.arch {
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
pub(super) fn emit_promote_mixed(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_reference_cell_promote_mixed");
    abi::emit_frame_prologue(emitter, 48);
    abi::store_at_offset(emitter, result, 8);
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::store_at_offset(emitter, result, 16);
    abi::emit_load_int_immediate(emitter, scratch, UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    match emitter.target.arch {
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
    match emitter.target.arch {
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
    abi::emit_frame_restore(emitter, 48);
    abi::emit_return(emitter);
}
