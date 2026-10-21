//! Purpose:
//! Publishes completed decoded arrays for later references in the same wire stream.
//!
//! Called from:
//! - The target-specific recursive unserialize decoders.
//!
//! Key details:
//! - A context-owned retain keeps registry boxes valid even if hydration discards their data.
//! - Only in-bounds slots publish owners; parser completion retires them after restoring context.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Registers a boxed array from C argument zero at the zero-based index in C argument one.
/// Returns the original caller-owned box, with a separate lease held until context completion.
pub(super) fn emit_register_array(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_unserialize_register_array");
    abi::emit_frame_prologue(emitter, 32);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), 16);
    abi::load_at_offset(emitter, abi::secondary_scratch_reg(emitter), 16);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x11, #65536");                             // bound publication to the physical registry capacity
            emitter.instruction("cmp x10, x11");                                // test the reserved index before computing an address
            emitter.instruction("b.hs __rt_unserialize_register_array_done");   // overflow values have no registry lease
            abi::emit_symbol_address(emitter, "x11", "_unser_values");
            abi::load_at_offset(emitter, result, 8);
            emitter.instruction("str x0, [x11, x10, lsl #3]");                  // publish the stable Mixed box for subsequent references
        }
        Arch::X86_64 => {
            emitter.instruction("cmp r10, 65536");                              // test the reserved index before computing an address
            emitter.instruction("jae __rt_unserialize_register_array_done");    // overflow values have no registry lease
            abi::emit_symbol_address(emitter, "r11", "_unser_values");
            abi::load_at_offset(emitter, result, 8);
            emitter.instruction("mov QWORD PTR [r11 + r10*8], rax");            // publish the stable Mixed box for subsequent references
        }
    }
    abi::emit_call_label(emitter, "__rt_incref");
    abi::load_at_offset(emitter, result, 8);
    abi::emit_call_label(emitter, "__rt_unserialize_defer_data");
    emitter.label("__rt_unserialize_register_array_done");
    abi::load_at_offset(emitter, result, 8);
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);
}

/// Promotes a published Mixed box in place so every uppercase-R owner shares writable identity.
pub(super) fn emit_reference_alias(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let base = abi::secondary_scratch_reg(emitter);
    let marker = abi::symbol_scratch_reg(emitter);
    let (low, high) = match emitter.target.arch {
        Arch::AArch64 => ("x1", "x2"),
        Arch::X86_64 => ("rdi", "rsi"),
    };
    emitter.blank();
    emitter.label_global("__rt_unserialize_reference_alias");
    abi::emit_frame_prologue(emitter, 32);
    abi::store_at_offset(emitter, result, 8);
    abi::emit_call_label(emitter, "__rt_reference_is");
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_unserialize_reference_retain");
    abi::load_at_offset(emitter, base, 8);
    abi::emit_load_from_address(emitter, low, base, 8);
    abi::emit_load_from_address(emitter, high, base, 16);
    abi::emit_load_from_address(emitter, result, base, 0);
    // Transfer the old payload owner into a child box; a retain here would leak its payload.
    abi::emit_call_label(emitter, match emitter.target.arch {
        Arch::AArch64 => "__rt_hash_to_mixed_box_owned",
        Arch::X86_64 => "__rt_hash_to_mixed_x86_box_owned",
    });
    abi::load_at_offset(emitter, base, 8);
    abi::emit_store_to_address(emitter, result, base, 8);
    abi::emit_load_int_immediate(emitter, marker, 7);
    abi::emit_store_to_address(emitter, marker, base, 0);
    abi::emit_load_int_immediate(emitter, marker, 1);
    abi::emit_store_to_address(emitter, marker, base, 16);
    emitter.label("__rt_unserialize_reference_retain");
    abi::load_at_offset(emitter, result, 8);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::load_at_offset(emitter, result, 8);
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Runtime alias helpers must not clobber callee-saved registers owned by EIR callers.
    #[test]
    fn reference_promotion_preserves_callee_saved_registers_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_reference_alias(&mut emitter);
            let asm = emitter.output();
            assert!(!asm.contains("mov x19") && !asm.contains("ldur x19"), "{name}: {asm}");
            assert!(!asm.contains("mov r12"), "{name}: {asm}");
            assert!(asm.contains(if name == "linux-x86_64" {
                "__rt_hash_to_mixed_x86_box_owned"
            } else { "__rt_hash_to_mixed_box_owned" }), "{name}: {asm}");
        }
    }

    /// Recursive decoders publish completed arrays and use the shared payload-owning clone path.
    #[test]
    fn array_references_publish_and_retain_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            match emitter.target.arch {
                Arch::AArch64 => super::super::decoder_aarch64::emit_parser(&mut emitter),
                Arch::X86_64 => super::super::decoder_x86_64::emit_parser(&mut emitter),
            }
            let asm = emitter.output();
            let array_close = asm.split_once("__rt_unser_at_array_close:\n").unwrap().1
                .split_once("__rt_unser_at_array_fail:\n").unwrap().0;
            assert!(array_close.contains("__rt_unserialize_register_array"), "{name}");
            let reference = asm.split_once("__rt_unser_at_ref:\n").unwrap().1;
            assert!(reference.contains("__rt_mixed_clone"), "{name}");
            assert!(reference.contains("114"), "{name}: lowercase references stay object-only");
            assert!(!reference.contains("__rt_heap_alloc"), "{name}: no unretained shallow payload copies");
        }
    }

    /// Every ABI bounds publication and retains the registry owner before deferring its release.
    #[test]
    fn completed_array_registry_owners_are_bounded_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_register_array(&mut emitter);
            let asm = emitter.output();
            assert!(asm.find("65536").unwrap() < asm.find("_unser_values").unwrap(), "{name}");
            assert!(asm.find("_unser_values").unwrap() < asm.find("__rt_incref").unwrap(), "{name}");
            assert!(asm.find("__rt_incref").unwrap() < asm.find("__rt_unserialize_defer_data").unwrap(), "{name}");
        }
    }
}
