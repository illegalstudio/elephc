//! Purpose:
//! Exposes persistent reference storage and detached value copies through the eval C ABI.
//!
//! Called from:
//! - Eval bridge emission for the active target architecture.
//!
//! Key details:
//! - References own GC-traced Mixed values independently of interpreter scopes.
//! - Replacement returns the previous owner so Rust controls when cleanup occurs.

use super::*;

/// Emits narrow C adapters over the target-aware shared reference runtime helpers.
pub(super) fn emit(emitter: &mut Emitter) {
    for (name, helper) in [
        ("__elephc_eval_value_reference_new", "__rt_reference_new"),
        ("__elephc_eval_value_is_reference", "__rt_reference_is"),
        ("__elephc_eval_value_copy", "__rt_mixed_clone"),
    ] {
        label_c_global(emitter, name);
        if emitter.target.arch == Arch::AArch64 {
            emitter.instruction(&format!("b {helper}"));                        // tail-call with the existing AArch64 boxed argument
        } else {
            emitter.instruction("mov rax, rdi");                                // adapt the borrowed C argument to the native boxed-value convention
            emitter.instruction(&format!("jmp {helper}"));                      // transfer directly to the shared runtime implementation
        }
    }
    label_c_global(emitter, "__elephc_eval_value_reference_replace");
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("b __rt_reference_replace");                        // return the previous owned value using the unchanged two-argument convention
    } else {
        emitter.instruction("mov rax, rdi");                                    // pass the persistent reference identity through the internal value register
        emitter.instruction("mov rdi, rsi");                                    // pass the borrowed replacement value through the internal second argument
        emitter.instruction("jmp __rt_reference_replace");                      // transfer the previous owner back to Rust for explicit cleanup
    }
}
