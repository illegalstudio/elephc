//! Purpose:
//! Exposes the native storage emitter to the compiler runtime.
//!
//! Called from:
//! - The compiler's managed runtime emitter inventory.
//!
//! Key details:
//! - Assembly and symbol selection enter through the established compiler adapter.

pub use elephc_memoir::values::array_push_int::emit_array_push_int;

#[cfg(test)]
mod tests {
    use crate::codegen_support::{emit::Emitter, platform::{Arch, Target}};

    use super::*;

    /// Every target appends into a unique, pre-shaped array without a helper call or frame.
    #[test]
    fn array_push_int_has_a_call_free_unique_path_on_every_target() {
        for name in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_array_push_int(&mut emitter);
            let asm = emitter.output();
            let slow = if target.arch == Arch::X86_64 {
                "__rt_array_push_int_x86_slow:"
            } else {
                "__rt_array_push_int_slow:"
            };
            let start = asm.find("__rt_array_push_int:\n").expect("push helper");
            let end = asm.find(slow).expect("slow-path label");
            let leaf = &asm[start..end];
            assert!(!leaf.contains("__rt_array_ensure_unique"), "{name}: {leaf}");
            assert!(!leaf.contains("__rt_array_grow"), "{name}: {leaf}");
            assert!(!leaf.contains("sub sp"), "{name}: {leaf}");
            assert!(!leaf.contains("push rbp"), "{name}: {leaf}");
            assert!(leaf.contains("ret"), "{name}: {leaf}");
        }
    }

    /// The slow path keeps copy-on-write, first-append shape, and growth handling.
    #[test]
    fn array_push_int_slow_path_keeps_cow_shape_and_growth_on_every_target() {
        for name in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_array_push_int(&mut emitter);
            let asm = emitter.output();
            let slow = if target.arch == Arch::X86_64 {
                "__rt_array_push_int_x86_slow:"
            } else {
                "__rt_array_push_int_slow:"
            };
            let start = asm.find(slow).expect("slow-path label");
            let tail = &asm[start..];
            assert!(tail.contains("__rt_array_ensure_unique"), "{name}: {tail}");
            assert!(tail.contains("__rt_array_push_int_shape_ready"), "{name}: {tail}");
            assert!(tail.contains("__rt_array_push_int_grow"), "{name}: {tail}");
        }
    }
}
