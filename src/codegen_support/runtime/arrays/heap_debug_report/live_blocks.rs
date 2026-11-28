//! Purpose:
//! Preserves compiler integration tests and compatibility imports for Memoir emitters.
//!
//! Called from:
//! - The elephc compiler memory subsystem.
//!
//! Key details:
//! - This code runs in the compiler and emits assembly; it is never a target-program library.

pub use elephc_memoir::heap_debug_report::live_blocks::*;

#[cfg(test)]
use crate::codegen_support::{emit::Emitter};


#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every supported target bounds the scan and log size without allocating while inspecting leaks.
    #[test]
    fn live_block_reports_are_bounded_and_non_allocating_on_all_targets() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            emit_live_blocks(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("_heap_off") && asm.contains("__rt_heap_debug_live_blocks_next:"), "{target}");
            assert!(asm.contains("cmp x0, #64") || asm.contains("cmp rax, 64"), "{target}");
            assert_eq!(asm.matches("__rt_itoa").count(), 9, "{target}");
            assert!(asm.contains("_heap_dbg_block_class") && asm.contains("_heap_dbg_block_tag"), "{target}");
            assert!(asm.contains("_heap_dbg_block_length"), "{target}");
            assert!(asm.contains("_heap_dbg_block_fingerprint"), "{target}");
            assert!(asm.contains("__rt_heap_debug_live_integer_loop:"), "{target}");
            assert!(asm.contains("cmp x0, #40") || asm.contains("cmp rax, 40"), "{target}");
            assert!(asm.contains("mov x11, #8") || asm.contains("mov edx, 8"), "{target}");
            assert!(asm.contains("_class_name_count") && asm.contains("_class_name_entries"), "{target}");
            assert!(!asm.contains("__rt_heap_alloc") && !asm.contains("__rt_incref"), "{target}");
            assert_eq!("HEAP DEBUG: live offset=".len(), 24);
        }
    }
}
