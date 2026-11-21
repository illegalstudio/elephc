//! Purpose:
//! Exposes Memoir's borrowed Throwable storage reader to the compiler runtime.
//!
//! Called from:
//! - Exception initialization and chaining, and Throwable method lowering.
//!
//! Key details:
//! - Runtime slot metadata is generated from compiler class layouts.

pub use elephc_memoir::values::throwable_previous::emit_throwable_previous;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::{emit::Emitter, platform::Target};

    /// Every supported ABI bounds metadata, distinguishes compact storage, and unboxes without retaining.
    #[test]
    fn previous_readers_cover_compact_boxed_and_reference_layouts_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_throwable_previous(&mut emitter);
            let asm = emitter.output();
            assert!(asm.find("_class_previous_slot_count").unwrap() < asm.find("_class_previous_slots").unwrap(), "{name}");
            assert!(asm.contains("__rt_throwable_previous_compact:") && asm.contains("__rt_mixed_unbox"), "{name}");
            assert!(!asm.contains("__rt_incref") && !asm.contains("__rt_heap_alloc"), "{name}");
        }
    }
}
