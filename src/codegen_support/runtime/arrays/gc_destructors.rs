//! Purpose:
//! Preserves compiler integration tests and compatibility imports for Memoir emitters.
//!
//! Called from:
//! - The elephc compiler memory subsystem.
//!
//! Key details:
//! - This code runs in the compiler and emits assembly; it is never a target-program library.

pub use elephc_memoir::gc_destructors::*;

#[cfg(test)]
use crate::codegen_support::{emit::Emitter};


#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target pins before callbacks and recounts before unpinning or freeing graph nodes.
    #[test]
    fn gc_destructor_snapshots_cover_every_supported_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_gc_destructors(&mut emitter);
            let assembly = emitter.output();
            for symbol in ["__rt_gc_destructors", "__rt_gc_unpin_reachable", "__rt_gc_drop_pins"] {
                assert!(assembly.contains(&format!("{symbol}:")), "{name}: {symbol}");
            }
            let destructor_loop = assembly.split("__rt_gc_destructors:\n").nth(1).unwrap();
            assert!(destructor_loop.find("__rt_gc_destructors_fill_done:").unwrap()
                < destructor_loop.find("__rt_gc_protected_destructor").unwrap(), "{name}");
            assert!(assembly.contains(&target.extern_symbol("malloc")), "{name}");
            assert!(assembly.contains(&target.extern_symbol("free")), "{name}");
            assert!(assembly.contains("__rt_heap_allocation_failed"), "{name}");
            let dispose = assembly.split("__rt_gc_drop_pins:").nth(1).unwrap();
            assert!(!dispose.contains("__rt_decref"), "{name}");

            let mut emitter = Emitter::new(target);
            super::super::emit_gc_collect_cycles(&mut emitter);
            let assembly = emitter.output();
            let destroy = assembly.find("__rt_gc_destructors").unwrap();
            let after_destroy = &assembly[destroy..];
            assert!(after_destroy.find("__rt_gc_collect_cycles_recount").unwrap()
                < after_destroy.find("__rt_gc_unpin_reachable").unwrap(), "{name}");
            assert!(after_destroy.find("__rt_gc_unpin_reachable").unwrap()
                < after_destroy.find("__rt_gc_collect_cycles_free_loop:").unwrap(), "{name}");
            assert!(assembly.contains("_gc_freeing_unreachable"), "{name}");
            assert!(assembly.rfind("_gc_collecting").unwrap()
                < assembly.find("__rt_gc_rethrow_pending").unwrap(), "{name}");
            assert!(assembly.find("__rt_gc_drop_pins").unwrap()
                < assembly.find("__rt_gc_rethrow_pending").unwrap(), "{name}");
        }
    }
}
