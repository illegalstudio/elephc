use crate::codegen::emit::Emitter;

/// heap_debug_fail: print a heap-debug fatal error to stderr and terminate.
/// Input: x1 = message pointer, x2 = message length
pub fn emit_heap_debug_fail(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: heap_debug_fail ---");
    emitter.label("__rt_heap_debug_fail");
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.instruction("mov x16, #4");                                         // syscall 4 = sys_write
    emitter.instruction("svc #0x80");                                           // write the heap-debug error message
    emitter.instruction("mov x0, #1");                                          // exit code 1 for heap-debug failures
    emitter.instruction("mov x16, #1");                                         // syscall 1 = sys_exit
    emitter.instruction("svc #0x80");                                           // terminate the process immediately
}
