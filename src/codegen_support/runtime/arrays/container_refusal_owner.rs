//! Purpose:
//! Retires the extra container owner of a mixed-key write that refuses a nullable reference.
//!
//! Called from:
//! - Indexed mixed-key and hash shape-12 refusal paths before raising their catchable Error.
//!
//! Key details:
//! - Mode bit one marks a helper-owned destination distinct from the owner's storage slot.
//! - Both setter frames save mode at sp+56 on AArch64 and rbp-64 on x86_64.
//! - Refusal precedes container consumption; ordinary raw-local owners remain untouched.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Drops only a separately acquired helper owner while the original storage still owns the array.
pub(super) fn emit_release(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [sp, #56]");                           // recover the diagnosed/shared bit and independent helper-owner bit
            emitter.instruction("tbz x9, #1, 986f");                            // raw-local transfers retain their original owner on refusal
            emitter.instruction("ldr x0, [sp]");                                // release the extra owner while the destination storage still keeps its own
        }
        Arch::X86_64 => {
            emitter.instruction("test QWORD PTR [rbp - 64], 2");                // distinguish an extra helper owner from an ordinary transferred local owner
            emitter.instruction("jz 986f");                                     // keep raw-local destinations valid for their surviving catch
            emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                // use the generic native release convention for indexed or hash storage
        }
    }
    abi::emit_call_label(emitter, "__rt_decref_any");
    emitter.label("986");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every supported ABI gates extra-owner retirement independently from the other mode bit.
    #[test]
    fn refusal_owner_release_is_conditional_and_uses_native_container_dispatch() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_release(&mut emitter);
            let assembly = emitter.output();
            let gate = if name == "linux-x86_64" { "test QWORD PTR [rbp - 64], 2" }
                else { "tbz x9, #1, 986f" };
            assert!(assembly.find(gate).unwrap() < assembly.find("__rt_decref_any").unwrap(), "{name}");
            assert!(assembly.find("__rt_decref_any").unwrap() < assembly.find("986:").unwrap(), "{name}");
        }
    }
}
