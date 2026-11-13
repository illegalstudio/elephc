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

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Unregisters only an owned reference row and restores the enclosing walk's current index.
pub(super) fn emit(emitter: &mut Emitter) {
    let (object_offset, descriptor_offset, index_offset, loop_index) = match emitter.target.arch {
        Arch::AArch64 => (48, 40, 24, "x12"),
        Arch::X86_64 => (8, 16, 32, "r10"),
    };
    let result = abi::int_result_reg(emitter);
    let descriptor = abi::secondary_scratch_reg(emitter);
    let index = abi::tertiary_scratch_reg(emitter);
    let done = "__rt_object_free_deep_source_retired";
    abi::load_at_offset(emitter, descriptor, descriptor_offset);
    abi::load_at_offset(emitter, index, index_offset);
    match emitter.target.arch {
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
    match emitter.target.arch {
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
    let entry = emitter.target.extern_symbol("__elephc_reference_sources_unbind_v1");
    abi::emit_call_label(emitter, &entry);
    super::super::reference_cells::require_source_success(emitter);
    emitter.label(done);
    abi::load_at_offset(emitter, loop_index, index_offset);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::{platform::Target, RuntimeFeatures};

    /// Sources remain present through resurrection and retire before their first property release.
    #[test]
    fn native_reference_sources_retire_only_in_the_final_property_walk() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            super::super::object_free_deep::emit_object_free_deep(&mut emitter,
                RuntimeFeatures { native_reference_validation: true, ..RuntimeFeatures::none() });
            let assembly = emitter.output();
            let loop_at = assembly.find("__rt_object_free_deep_loop:").unwrap();
            let unbind = assembly.find("__elephc_reference_sources_unbind_v1").unwrap();
            let release = assembly.find("__rt_object_free_deep_release_runtime:").unwrap();
            assert!(loop_at < unbind && unbind < release, "{name}: retire one source before its owner");
            assert!(assembly[..loop_at].contains("resurrected"), "{name}: resurrection precedes the final walk");
            assert_eq!(assembly.matches("__elephc_reference_sources_unbind_v1").count(), 1, "{name}");
        }
    }
}
