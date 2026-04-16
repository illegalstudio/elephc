use crate::codegen::{emit::Emitter, platform::Arch};

/// heap_debug_fail: print a heap-debug fatal error to stderr and terminate.
/// Input: x1 = message pointer, x2 = message length
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
