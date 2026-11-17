//! Purpose:
//! Connects Memoir string-copy emitters to the runtime inventory.
//!
//! Called from:
//! - The compiler string runtime.
//!
//! Key details:
//! - Compiler consumers retain the existing runtime helper names.

pub use elephc_memoir::strings::strcopy::{emit_str_dup_owned, emit_strcopy};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::{emit::Emitter, platform::Target};

    /// Verifies both targets copy with one libc `memcpy` rather than a byte loop.
    #[test]
    fn strcopy_emits_one_memcpy_on_all_targets() {
        for name in ["macos-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).expect("supported target");
            let mut emitter = Emitter::new(target);
            emit_strcopy(&mut emitter);
            let assembly = emitter.output();
            assert!(assembly.contains("memcpy"), "{name}: {assembly}");
            assert!(!assembly.contains("strcopy_loop"), "{name}: {assembly}");
        }
    }
}
