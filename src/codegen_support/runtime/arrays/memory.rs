//! Purpose:
//! Preserves compiler integration tests and compatibility imports for Memoir emitters.
//!
//! Called from:
//! - The elephc compiler memory subsystem.
//!
//! Key details:
//! - This code runs in the compiler and emits assembly; it is never a target-program library.

pub use elephc_memoir::memory::*;

#[cfg(test)]
use crate::codegen_support::{emit::Emitter};


#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every supported target emits allocation-free helpers for live bytes and event counts.
    #[test]
    fn memory_metrics_helpers_cover_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_memory(&mut emitter);
            let asm = emitter.output();
            for symbol in ["__rt_memory_get_usage", "__rt_memory_get_peak_usage",
                "__rt_memory_reset_peak_usage", "__rt_memory_get_allocations", "__rt_memory_get_frees",
                "_gc_live", "_gc_peak", "_gc_allocs", "_gc_frees", "_heap_max"] {
                assert!(asm.contains(symbol), "{name}: missing {symbol}");
            }
            assert!(!asm.contains("__rt_heap_alloc"), "{name}");
        }
    }
}
