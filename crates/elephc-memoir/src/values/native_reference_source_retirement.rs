//! Purpose:
//! Retires one physical property's weak type source immediately before its cell owner.
//!
//! Called from:
//! - The final object-property walk after destructor resurrection checks have completed.
//!
//! Key details:
//! - Other properties retain their constraints until their own teardown iteration.
//! - The registry callback runs no PHP and adds no runtime value owner.
//! - Every value needed after the foreign call is reloaded from the deep-cleanup frame.

use crate::{abi, Emitter, Arch};

/// Unregisters only an owned reference row and restores the enclosing walk's current index.
pub fn emit(emitter: &mut impl Emitter) {
    let (object_offset, descriptor_offset, index_offset, loop_index) = match emitter.arch() {
        Arch::AArch64 => (48, 40, 24, "x12"),
        Arch::X86_64 => (8, 16, 32, "r10"),
    };
    let result = abi::int_result_reg(emitter);
    let descriptor = abi::secondary_scratch_reg(emitter);
    let index = abi::tertiary_scratch_reg(emitter);
    let done = "__rt_object_free_deep_source_retired";
    abi::load_at_offset(emitter, descriptor, descriptor_offset);
    abi::load_at_offset(emitter, index, index_offset);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldrb w0, [{descriptor}, {index}]"));  // inspect the current physical row without retaining its payload
            emitter.instruction("cmp x0, #11");                                 // only owned reference cells can have a native property source
            emitter.instruction(&format!("b.ne {done}"));                       // leave ordinary values and borrowed references unchanged
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("movzx eax, BYTE PTR [{descriptor} + {index}]")); // inspect the physical row without retaining its payload
            emitter.instruction("cmp rax, 11");                                 // only owned reference cells can have a native property source
            emitter.instruction(&format!("jne {done}"));                        // leave ordinary values and borrowed references unchanged
        }
    }
    abi::load_at_offset(emitter, result, object_offset);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("add x0, x0, {index}, lsl #4"));       // locate this physical property in the final object layout
            emitter.instruction("add x0, x0, #8");                              // skip the object's leading class identifier
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("shl {index}, 4"));                    // scale the physical property index by its sixteen-byte slot
            emitter.instruction(&format!("add rax, {index}"));                  // locate this physical binding rather than its shared cell
            emitter.instruction("lea rdi, [rax + 8]");                          // pass the exact binding to the registry C ABI
        }
    }
    let entry = emitter.extern_symbol("__elephc_reference_sources_unbind_v1");
    abi::emit_call_label(emitter, &entry);
    emitter.emit_reference_source_success();
    emitter.label(done);
    abi::load_at_offset(emitter, loop_index, index_offset);
}
