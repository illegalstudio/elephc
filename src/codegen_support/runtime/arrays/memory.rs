//! Purpose:
//! Exposes the allocator's existing live, peak, allocation, and release counters.
//!
//! Called from:
//! - `runtime::emitters::managed` while emitting the common managed heap runtime.
//!
//! Key details:
//! - Live bytes include finalized block sizes and 16-byte headers.
//! - The fixed BSS arena remains reserved even when its blocks are released.
//! - Metric reads and resets never allocate, scan the heap, or change allocation counters.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits leaf helpers shared by direct PHP calls and boxed eval dispatch.
pub fn emit_memory(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    let result = if arm { "x0" } else { "rax" };
    for (name, counter) in [
        ("memory_get_usage", "_gc_live"),
        ("memory_get_peak_usage", "_gc_peak"),
    ] {
        emitter.label_global(&format!("__rt_{name}"));
        // -- choose the fixed reserved arena or the live-byte counter --
        if arm {
            emitter.instruction("mov x11, x0");                                 // preserve the caller's real_usage flag before reading counters
            abi::emit_load_symbol_to_reg(emitter, "x0", counter, 0);
            abi::emit_load_symbol_to_reg(emitter, "x10", "_heap_max", 0);
            emitter.instruction("cmp x11, #0");                                 // select reserved bytes only for a true real_usage flag
            emitter.instruction("csel x0, x10, x0, ne");                        // return arena capacity or current live accounting
        } else {
            abi::emit_load_symbol_to_reg(emitter, "rax", counter, 0);
            abi::emit_load_symbol_to_reg(emitter, "r10", "_heap_max", 0);
            emitter.instruction("test rdi, rdi");                               // inspect the SysV real_usage argument
            emitter.instruction("cmovne rax, r10");                             // include unused arena space when real_usage is true
        }
        emitter.instruction("ret");                                             // return the metric without allocating result storage
    }
    emitter.label_global("__rt_memory_reset_peak_usage");
    // -- reset the peak to current usage without touching cumulative counters --
    abi::emit_load_symbol_to_reg(emitter, result, "_gc_live", 0);
    abi::emit_store_reg_to_symbol(emitter, result, "_gc_peak", 0);
    emitter.instruction("ret");                                                 // return void to the PHP caller
    for (name, counter) in [
        ("memory_get_allocations", "_gc_allocs"),
        ("memory_get_frees", "_gc_frees"),
    ] {
        emitter.label_global(&format!("__rt_{name}"));
        // -- expose a monotonic event counter without constructing a snapshot array --
        abi::emit_load_symbol_to_reg(emitter, result, counter, 0);
        emitter.instruction("ret");                                             // preserve the measured allocation count while returning
    }
}

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
