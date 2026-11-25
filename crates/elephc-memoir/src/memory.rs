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

use crate::abi;
use crate::Emitter;
use crate::Arch;

/// Branches when an unsigned allocation size exceeds the configured arena capacity.
pub fn emit_branch_if_size_exceeds_heap(
    emitter: &mut impl Emitter,
    size: &str,
    scratch: &str,
    overflow: &str,
) {
    abi::emit_load_symbol_to_reg(emitter, scratch, "_heap_max", 0);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {size}, {scratch}"));             // compare the requested byte count with the managed arena
            emitter.instruction(&format!("b.hi {overflow}"));                   // reject requests that cannot fit in the arena
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {size}, {scratch}"));             // compare the requested byte count with the managed arena
            emitter.instruction(&format!("ja {overflow}"));                     // reject requests that cannot fit in the arena
        }
    }
}

/// Emits leaf helpers shared by direct PHP calls and boxed eval dispatch.
pub fn emit_memory(emitter: &mut impl Emitter) {
    let arm = emitter.arch() == Arch::AArch64;
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

/// Installs a ceiling from the integer result register and returns a refusal flag there.
///
/// The unlimited sentinel selects the arena capacity. A finite request outside that capacity
/// leaves the current ceiling intact. The caller provides fresh labels for inline emission.
pub fn emit_set_limit(
    emitter: &mut impl Emitter, unlimited: &str, publish: &str, rejected: &str, done: &str,
) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("cmn x0, #1");                                  // is the request exactly -1 (PHP's unlimited spelling)?
            emitter.instruction(&format!("b.eq {unlimited}"));                  // -1 selects the reserved arena capacity
            abi::emit_load_symbol_to_reg(emitter, "x9", "_heap_max", 0);
            emitter.instruction("cmp x0, x9");                                  // compare the request against the reserved capacity
            emitter.instruction(&format!("b.hi {rejected}"));                   // a finite request above the arena cannot be honored
            emitter.instruction(&format!("b {publish}"));                       // publish an in-arena ceiling
            emitter.label(unlimited);
            abi::emit_load_symbol_to_reg(emitter, "x0", "_heap_max", 0);   // unlimited is the whole reserved arena
            emitter.label(publish);
            abi::emit_store_reg_to_symbol(emitter, "x0", "_heap_limit", 0);
            emitter.instruction("mov x0, #0");                                  // report a successful installation
            emitter.instruction(&format!("b {done}"));                          // skip the rejection status
            emitter.label(rejected);
            emitter.instruction("mov x0, #1");                                  // report a request the arena cannot grow to
            emitter.label(done);
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, -1");                                 // is the request exactly -1 (PHP's unlimited spelling)?
            emitter.instruction(&format!("je {unlimited}"));                    // -1 selects the reserved arena capacity
            abi::emit_load_symbol_to_reg(emitter, "r10", "_heap_max", 0);
            emitter.instruction("cmp rax, r10");                                // compare the request against the reserved capacity
            emitter.instruction(&format!("ja {rejected}"));                     // a finite request above the arena cannot be honored
            emitter.instruction(&format!("jmp {publish}"));                     // publish an in-arena ceiling
            emitter.label(unlimited);
            abi::emit_load_symbol_to_reg(emitter, "rax", "_heap_max", 0);  // unlimited is the whole reserved arena
            emitter.label(publish);
            abi::emit_store_reg_to_symbol(emitter, "rax", "_heap_limit", 0);
            emitter.instruction("xor eax, eax");                                // report a successful installation
            emitter.instruction(&format!("jmp {done}"));                        // skip the rejection status
            emitter.label(rejected);
            emitter.instruction("mov eax, 1");                                  // report a request the arena cannot grow to
            emitter.label(done);
        }
    }
}
