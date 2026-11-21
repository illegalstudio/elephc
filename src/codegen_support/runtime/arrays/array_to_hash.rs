//! Purpose:
//! Exposes Memoir's container emitter through the compiler runtime.
//!
//! Called from:
//! - Runtime assembly generation.
//!
//! Key details:
//! - The compiler controls emitter selection and validates the generated code.

pub use elephc_memoir::values::array_to_hash::emit_array_to_hash;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::{emit::Emitter, platform::Target};

    /// Promotion passes the child pointer, not the loop index, to the retain helper on every ABI.
    #[test]
    fn array_to_hash_retains_heap_children_using_the_internal_abi() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_array_to_hash(&mut emitter);
            let asm = emitter.output();
            let prefix = asm.split_once("__rt_incref").expect("child retain").0;
            let load = if name == "linux-x86_64" {
                "mov rax, QWORD PTR [rbp - 56]"
            } else {
                "ldr x0, [sp, #48]"
            };
            assert!(prefix.lines().rev().take(2).any(|line| line.contains(load)), "{name}");
        }
    }
}
