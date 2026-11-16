//! Purpose:
//! Preserves compiler integration tests and compatibility imports for Memoir emitters.
//!
//! Called from:
//! - The elephc compiler memory subsystem.
//!
//! Key details:
//! - This code runs in the compiler and emits assembly; it is never a target-program library.


#[cfg(test)]
use crate::codegen_support::{emit::Emitter, platform::Arch};


#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// All targets gate the callback on a Mixed allocation and preserve the allocator input.
    #[test]
    fn heap_free_retires_eval_array_reference_cells_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            super::super::heap_free::emit_heap_free(&mut emitter, true, false);
            let output = emitter.output();
            assert_eq!(output.matches("__rt_heap_free_eval_references_done:").count(), 1, "{name}");
            assert!(output.contains("_elephc_eval_array_reference_retire_fn"), "{name}");
            let required: &[&str] = match target.arch {
                Arch::AArch64 => &["ldrb w9, [x0, #-8]", "cmp w9, #5", "blr x10", "ldp x0, x30, [sp], #16"],
                Arch::X86_64 => &["cmp BYTE PTR [rax - 8], 5", "push rax", "mov rdi, rax", "call r10", "pop rax"],
            };
            for instruction in required { assert!(output.contains(instruction), "{name}: {instruction}"); }
            let retirement = output.find("__rt_heap_free_eval_references_done:").unwrap();
            let clear_kind = match target.arch {
                Arch::AArch64 => "str xzr, [x9, #8]",
                Arch::X86_64 => "mov QWORD PTR [r9 + 8], 0",
            };
            assert!(retirement < output.find(clear_kind).unwrap(), "{name}");
        }
    }

    /// Plain AOT runtimes do not pay the callback probe on every heap release.
    #[test]
    fn heap_free_omits_eval_array_reference_retirement_without_eval_bridge() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            super::super::heap_free::emit_heap_free(&mut emitter, false, false);
            let output = emitter.output();
            assert!(!output.contains("__rt_heap_free_eval_references_done"), "{name}");
            assert!(!output.contains("_elephc_eval_array_reference_retire_fn"), "{name}");
        }
    }
}
