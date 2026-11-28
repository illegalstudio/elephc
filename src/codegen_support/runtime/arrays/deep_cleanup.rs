//! Purpose:
//! Preserves compiler imports and tests for Memoir value storage.
//!
//! Called from:
//! - Compiler memory emission and its integration tests.
//!
//! Key details:
//! - Runs only in the compiler and preserves the generated target assembly.

pub use elephc_memoir::values::deep_cleanup::*;

#[cfg(test)]
use crate::codegen_support::{emit::Emitter, platform::Arch, abi};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Each container reserves separate pending/suppression slots and frees itself before rethrowing.
    #[test]
    fn deep_cleanup_preserves_nested_state_and_defers_throw_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            abi::emit_frame_prologue(&mut emitter, 64);
            begin(&mut emitter);
            let payload = abi::int_result_reg(&emitter);
            invoke(&mut emitter, "__rt_decref_any", payload);
            invoke(&mut emitter, "__rt_callable_descriptor_release", payload);
            abi::emit_call_label(&mut emitter, "__rt_heap_free");
            finish(&mut emitter, "__rt_deep_cleanup_test_return");
            assert_ne!(pending_offset(&emitter), suppression_offset(&emitter), "{name}");
            if target.arch == Arch::AArch64 {
                // The 64-byte prologue places x29 at sp+48, while container locals use sp+0..24.
                assert_eq!(48 - pending_offset(&emitter), 32, "{name}");
                assert_eq!(48 - suppression_offset(&emitter), 40, "{name}");
            }
            let asm = emitter.output();
            if target.arch == Arch::AArch64 {
                assert!(asm.contains("stur x10, [x29, #-16]"), "{name}: pending slot");
                assert!(asm.contains("stur x10, [x29, #-8]"), "{name}: suppression slot");
            }
            assert_eq!(asm.matches("__rt_cleanup_invoke").count(), 2, "{name}");
            let first = asm.find("__rt_decref_any").unwrap();
            let second = asm.find("__rt_callable_descriptor_release").unwrap();
            let free = asm.find("__rt_heap_free").unwrap();
            let restore = asm.rfind("_gc_release_suppressed").unwrap();
            let chain = asm.find("__rt_exception_chain").unwrap();
            let throw = asm.find("__rt_throw_current").unwrap();
            assert!(first < second && second < free && free < restore, "{name}");
            assert!(restore < chain && chain < throw, "{name}");
            assert!(!asm.contains("setjmp"), "{name}: child boundaries own their jump buffers");
        }
    }
}
