//! Purpose:
//! Transfers Throwable ownership between boxed eval values and raw native exceptions.
//!
//! Called from:
//! - Eval status lowering and the generated native pending-exception bridge.
//!
//! Key details:
//! - Both directions consume one input owner and return one output owner.
//! - The new owner is acquired before releasing the old representation, so no destructor can run.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits consuming conversions between validated Throwable boxes and native object owners.
pub fn emit_throwable_boxed_owners(emitter: &mut Emitter) {
    // -- detach a raw exception owner from an owned eval box --
    emitter.label_global("__rt_throwable_take_boxed");
    abi::emit_frame_prologue(emitter, 32);
    let result = abi::int_result_reg(emitter);
    abi::store_at_offset(emitter, result, 8);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("mov x0, x1"),                     // retain the raw object payload before consuming its box
        Arch::X86_64 => emitter.instruction("mov rax, rdi"),                    // retain the raw object payload before consuming its box
    }
    abi::emit_call_label(emitter, "__rt_incref");
    abi::store_at_offset(emitter, result, 16);
    abi::load_at_offset(emitter, result, 8);
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    abi::load_at_offset(emitter, result, 16);
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);

    // -- box a transferred native exception without retaining a second raw owner --
    emitter.label_global("__rt_throwable_box_owned");
    abi::emit_frame_prologue(emitter, 32);
    abi::store_at_offset(emitter, result, 8);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, x0");                                  // move the owned raw exception into the object payload
            emitter.instruction("mov x0, #6");                                  // box the validated object with the object tag
            emitter.instruction("mov x2, #0");                                  // objects have no second payload word
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // move the owned raw exception into the object payload
            emitter.instruction("mov eax, 6");                                  // box the validated object with the object tag
            emitter.instruction("xor esi, esi");                                // objects have no second payload word
        }
    }
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::store_at_offset(emitter, result, 16);
    abi::load_at_offset(emitter, result, 8);
    abi::emit_call_label(emitter, "__rt_decref_any");
    abi::load_at_offset(emitter, result, 16);
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Both conversions acquire the destination owner before releasing the source on all targets.
    #[test]
    fn throwable_box_transfers_balance_both_representations_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_throwable_boxed_owners(&mut emitter);
            let asm = emitter.output();
            let (take, give) = asm.split_once("__rt_throwable_box_owned:").unwrap();
            assert!(take.find("__rt_incref").unwrap() < take.find("__rt_decref_mixed").unwrap(), "{name}");
            assert_eq!(take.matches("__rt_decref_mixed").count(), 1, "{name}");
            assert!(give.find("__rt_mixed_from_value").unwrap() < give.find("__rt_decref_any").unwrap(), "{name}");
            assert_eq!(give.matches("__rt_decref_any").count(), 1, "{name}");
            assert!(!asm.contains("__rt_throw_current"), "{name}");
        }
    }
}
