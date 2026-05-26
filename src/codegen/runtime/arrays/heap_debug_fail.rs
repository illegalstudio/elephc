//! Purpose:
//! Emits the `__rt_heap_debug_fail` runtime helper assembly for heap debug fail.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Heap helpers own allocator metadata, debug accounting, and free-list invariants used by all refcounted runtime values.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Emits the `__rt_heap_debug_fail` runtime helper that prints a heap-debug fatal
/// error to stderr and terminates the process.
///
/// # ABI contract
///
/// - **ARM64**: expects fd=2 (stderr) in `x0`; exit code=1 in `x0` after write.
/// - **x86_64**: expects fd=2 (stderr) in `edi`; exit code=1 in `edi` after write.
/// - Syscall 4 (write) outputs the message; syscall 1 (exit) terminates.
pub fn emit_heap_debug_fail(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.blank();
        emitter.comment("--- runtime: heap_debug_fail ---");
        emitter.label_global("__rt_heap_debug_fail");
        emitter.instruction("mov edi, 2");                                      // fd = stderr for the heap-debug fatal error
        emitter.instruction("mov eax, 1");                                      // Linux x86_64 syscall 1 = write
        emitter.instruction("syscall");                                         // print the heap-debug failure message to stderr
        emitter.instruction("mov edi, 1");                                      // exit code 1 marks the heap-debug process failure
        emitter.instruction("mov eax, 60");                                     // Linux x86_64 syscall 60 = exit
        emitter.instruction("syscall");                                         // terminate immediately after reporting the heap-debug failure
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: heap_debug_fail ---");
    emitter.label_global("__rt_heap_debug_fail");
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.syscall(4);
    emitter.instruction("mov x0, #1");                                          // exit code 1 for heap-debug failures
    emitter.syscall(1);
}
