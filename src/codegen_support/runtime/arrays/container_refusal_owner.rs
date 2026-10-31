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
//! - The pending Error is rooted before either rejected owner can run a throwing destructor.

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
    abi::emit_unary_cleanup_preserving_exception(
        emitter, "__rt_decref_any", abi::int_result_reg(emitter),
    );
    emitter.label("986");
}

/// Consumes the boxed rejected value and optional extra destination owner before propagating Error.
pub(super) fn emit_throw(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    crate::codegen_support::callable_invoker_args::emit_tagged_scalar_ref_assignment_pending_error(emitter, result);
    emit_release(emitter);
    abi::emit_call_label(emitter, "__rt_throw_current");
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

    /// Destructor throws cannot bypass either owner retirement or erase the initial refusal.
    #[test]
    fn nullable_refusal_roots_error_before_protected_owner_cleanup_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_throw(&mut emitter);
            let assembly = emitter.output();
            let error = assembly.find("__rt_tagged_reference_assignment_error_new").unwrap();
            let cleanup = assembly.find("__rt_cleanup_preserve_exception").unwrap();
            let thrown = assembly.find("__rt_throw_current").unwrap();
            assert!(error < cleanup && cleanup < thrown, "{name}: {assembly}");
            assert!(assembly.contains("__rt_decref_mixed") && assembly.contains("__rt_decref_any"), "{name}");
        }
    }
}
