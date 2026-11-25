//! Purpose:
//! Initializes Error/Exception constructor fields on compact and ordinary native objects.
//!
//! Called from:
//! - EIR's typed Throwable initialization operation inside builtin constructor methods.
//!
//! Key details:
//! - The five native arguments are borrowed: receiver, message pointer/length, code, previous box.
//! - New field owners are installed before releasing displaced owners, including self aliases.
//! - Previous-slot metadata selects a raw object or a nullable Mixed cell without changing layout.

use crate::codegen_support::emit::Emitter;

/// Emits a layout-aware initializer for a checked Throwable receiver and normalized parameters.
pub fn emit_throwable_initialize(emitter: &mut Emitter) {
    elephc_memoir::values::throwable_storage::emit_initialize(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Arch, Target};

    /// Every target stages the five ABI words and publishes replacement owners before old cleanup.
    #[test]
    fn throwable_initializer_preserves_layout_and_owner_order_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_throwable_initialize(&mut emitter);
            let asm = emitter.output();
            let previous_argument = match target.arch {
                Arch::AArch64 => "stur x4, [x29, #-40]",
                Arch::X86_64 => "mov QWORD PTR [rbp - 40], r8",
            };
            let publish_previous = match target.arch {
                Arch::AArch64 => "str x0, [x9]",
                Arch::X86_64 => "mov QWORD PTR [r11], rax",
            };
            assert!(asm.contains(previous_argument), "{name}: {asm}");
            assert!(asm.contains("__rt_throwable_previous_slot"), "{name}");
            let retain = asm.find("__rt_incref").unwrap();
            let publish = asm.find(publish_previous).unwrap();
            let release_message = asm.find("__rt_heap_free_safe").unwrap();
            let release_previous = asm.find("__rt_decref_any").unwrap();
            assert!(retain < publish && publish < release_message && release_message < release_previous, "{name}");
        }
    }
}
