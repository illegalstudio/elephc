//! Purpose:
//! Preserves compiler imports and tests for Memoir boxed-value ownership.
//!
//! Called from:
//! - Compiler runtime emission and memory integration tests.
//!
//! Key details:
//! - Assembly implementation lives in the Memoir codegen feature.

pub use elephc_memoir::cleanup_preserve::*;

#[cfg(test)]
use crate::codegen_support::{emit::Emitter};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every ABI republishes the pending chain and returns the independent newly-caught flag.
    #[test]
    fn frame_cleanup_preserves_the_pending_chain_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_cleanup_preserve_exception(&mut emitter);
            let arch = emitter.target.arch;
            let asm = emitter.output();
            let invoke = asm.find("__rt_cleanup_invoke").unwrap();
            let publish = asm.rfind("_exc_value").unwrap();
            let (caught_store, caught_reload) = match arch {
                crate::codegen_support::platform::Arch::AArch64 => (
                    "stur x0, [x29, #-32]",
                    "ldur x0, [x29, #-32]",
                ),
                crate::codegen_support::platform::Arch::X86_64 => {
                    (
                        "mov QWORD PTR [rbp - 32], rax",
                        "mov rax, QWORD PTR [rbp - 32]",
                    )
                }
            };
            let save = asm.find(caught_store).unwrap();
            let reload = asm.rfind(caught_reload).unwrap();
            assert!(asm.find("_exc_value").unwrap() < invoke, "{name}");
            assert!(invoke < save && save < publish && publish < reload, "{name}");
            assert!(!asm.contains("__rt_throw_current"), "{name}");
        }
    }
}
