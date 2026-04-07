use crate::codegen::emit::Emitter;

/// heap_debug_fail: print a heap-debug fatal error to stderr and terminate.
/// Input: x1 = message pointer, x2 = message length
pub fn emit_heap_debug_fail(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: heap_debug_fail ---");
    emitter.label_global("__rt_heap_debug_fail");
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.syscall(4);
    emitter.instruction("mov x0, #1");                                          // exit code 1 for heap-debug failures
    emitter.syscall(1);
}
