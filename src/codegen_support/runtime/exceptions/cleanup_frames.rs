//! Purpose:
//! Integrates shared activation cleanup with compiler-owned exception helpers.
//!
//! Called from:
//! - The compiler runtime orchestrator through the exceptions module.
//!
//! Key details:
//! - Guard and protected deep-cleanup helpers retain their original emission order.
//! - Memoir detaches abandoned records; survivor selection and handlers remain compiler-owned.

use crate::codegen_support::emit::Emitter;

/// Emits compiler cleanup prerequisites before the shared activation walk.
pub fn emit_exception_cleanup_frames(emitter: &mut Emitter) {
    super::owned_values::emit(emitter);
    super::deep_cleanup::emit(emitter);
    elephc_memoir::activation_cleanup::emit(emitter, "_exc_call_frame_top");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Arch, Target};

    /// Cleanup detaches the abandoned record before invoking its callback in executable and PIC runtimes.
    #[test]
    fn unwind_detaches_each_frame_before_calling_its_cleanup() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for pic in [false, true] {
                let mut emitter = Emitter::new(Target::parse(name).unwrap());
                emitter.pic_data_refs = pic;
                emit_exception_cleanup_frames(&mut emitter);
                let (detach, call) = if emitter.target.arch == Arch::AArch64 {
                    ("ldr x20, [x20]", "blr x10")
                } else {
                    ("mov r13, QWORD PTR [r13]", "call r10")
                };
                let asm = emitter.output();
                let detach = asm.find(detach).unwrap();
                let call = asm.find(call).unwrap();
                assert!(detach < call, "{name}, PIC={pic}");
                assert!(asm[detach..call].contains("_exc_call_frame_top"), "{name}, PIC={pic}");
            }
        }
    }

    /// Every target/mode uses only the supplied chain symbol, without implicit compiler state.
    #[test]
    fn memoir_activation_cleanup_uses_host_chain_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for mode in 0..6 {
                let mut emitter = Emitter::new(Target::parse(name).unwrap());
                emitter.pic_data_refs = mode == 1 || mode == 2;
                emitter.cdylib_boundary = mode == 2 || mode == 3;
                emitter.phpext_output = mode == 4;
                emitter.dead_strip = mode == 5;
                elephc_memoir::activation_cleanup::emit(&mut emitter, "__memoir_activation_head");
                let asm = emitter.output();
                assert!(asm.contains("__memoir_activation_head"), "{name}, mode={mode}");
                assert!(!asm.contains("_exc_call_frame_top"), "{name}, mode={mode}");
            }
        }
    }
}
