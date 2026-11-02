//! Purpose:
//! Emits the `__rt_heap_debug_check_live`, `__rt_heap_debug_check_live_done` runtime helper assembly for heap debug check live.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Heap helpers own allocator metadata, debug accounting, and free-list invariants used by all refcounted runtime values.

use crate::codegen_support::{emit::Emitter, platform::Arch};
use crate::codegen_support::sentinels::HEAP_FREE_REFCOUNT_MARK;

/// Bits 24..30 of [`HEAP_FREE_REFCOUNT_MARK`]: what a parked block's refcount keeps there even
/// after a few dangling decrements, with the destruction flag (bit 31) masked off.
const PARKED_TOP_BITS: u32 = (HEAP_FREE_REFCOUNT_MARK >> 24) & 0x7f;

/// Checks whether a heap block appears to have been freed.
///
/// Runs a debug-only integrity check: verifies that the heap block's refcount field
/// (located 12 bytes before the user pointer, in the uniform heap header) is non-zero.
/// A zero refcount indicates the block may have been freed while still referenced, and a
/// refcount whose bits 24..30 still spell [`HEAP_FREE_REFCOUNT_MARK`] means the block is parked
/// on the free list or in a small bin (possibly already decremented by an earlier dangling
/// release). Either way this function jumps to `__rt_heap_debug_fail`, which prints the
/// bad-refcount debug message and terminates. No real refcount reaches 2^24.
///
/// # Inputs
/// - `x0` (ARM64) / `rax` (x86_64): the heap user pointer to check.
///
/// # Outputs
/// - Returns normally when the block's refcount is non-zero (block appears live).
/// - Jumps to `__rt_heap_debug_fail` (never returns) when the refcount is zero or parked.
///
/// # ABI details
/// - The uniform heap header places the refcount 12 bytes before the user pointer.
/// - The refcount field is 4 bytes wide.
/// - Caller-saved registers may be clobbered on the failure path.
pub fn emit_heap_debug_check_live(emitter: &mut Emitter) {
    let msg = "Fatal error: heap debug detected bad refcount\n";

    if emitter.target.arch == Arch::X86_64 {
        emitter.blank();
        emitter.comment("--- runtime: heap_debug_check_live ---");
        emitter.label_global("__rt_heap_debug_check_live");
        emitter.instruction("mov ecx, DWORD PTR [rax - 12]");                   // load the current block refcount from the uniform heap header
        emitter.instruction("test ecx, ecx");                                   // does the heap block still look live?
        emitter.instruction("jz __rt_heap_debug_check_live_fail");              // a zero refcount means the block was released
        emitter.instruction("shr ecx, 24");                                     // keep the refcount's top byte
        emitter.instruction("and ecx, 0x7f");                                   // drop the destruction-in-progress flag (bit 31)
        emitter.instruction(&format!("cmp ecx, {:#x}", PARKED_TOP_BITS));       // does the refcount still spell the parked-block mark?
        emitter.instruction("jne __rt_heap_debug_check_live_done");             // no: an ordinary live refcount
        emitter.label("__rt_heap_debug_check_live_fail");
        crate::codegen_support::abi::emit_symbol_address(emitter, "rsi", "_heap_dbg_bad_refcount_msg");
        emitter.instruction(&format!("mov edx, {}", msg.len()));                // pass the exact bad-refcount message length to the failure helper
        emitter.instruction("jmp __rt_heap_debug_fail");                        // report the bad-refcount heap-debug failure and terminate

        emitter.label("__rt_heap_debug_check_live_done");
        emitter.instruction("ret");                                             // return when the block still looks live
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: heap_debug_check_live ---");
    emitter.label_global("__rt_heap_debug_check_live");
    emitter.instruction("ldr w9, [x0, #-12]");                                  // load the current block refcount from the uniform heap header
    emitter.instruction("cbz w9, __rt_heap_debug_check_live_fail");             // a zero refcount means the block was released
    emitter.instruction("ubfx w9, w9, #24, #7");                                // keep bits 24..30, dropping the destruction-in-progress flag
    emitter.instruction(&format!("cmp w9, #{:#x}", PARKED_TOP_BITS));           // does the refcount still spell the parked-block mark?
    emitter.instruction("b.ne __rt_heap_debug_check_live_done");                // no: an ordinary live refcount
    emitter.label("__rt_heap_debug_check_live_fail");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_heap_dbg_bad_refcount_msg"); // load page of the bad-refcount debug message
    emitter.instruction(&format!("mov x2, #{}", msg.len()));                    // pass the exact bad-refcount message length
    emitter.instruction("b __rt_heap_debug_fail");                              // report the heap-debug failure and exit

    emitter.label("__rt_heap_debug_check_live_done");
    emitter.instruction("ret");                                                 // return when the block still looks live
}
