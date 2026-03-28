use crate::codegen::emit::Emitter;

/// __rt_ptr_check_nonnull: abort with a fatal error on null pointer dereference.
/// Input:  x0 = pointer value
/// Output: x0 unchanged on success; process exits on null
pub(crate) fn emit_ptr_check_nonnull(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for ARM64 instructions
    emitter.comment("--- runtime: ptr_check_nonnull ---");
    emitter.label("__rt_ptr_check_nonnull");

    // -- fast path for valid pointers --
    emitter.instruction("cmp x0, #0");                                          // compare pointer value against null
    emitter.instruction("b.ne __rt_ptr_check_nonnull_ok");                      // continue when pointer is non-null

    // -- fatal error: null pointer dereference --
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.instruction("adrp x1, _ptr_null_err_msg@PAGE");                     // load page of null dereference message
    emitter.instruction("add x1, x1, _ptr_null_err_msg@PAGEOFF");               // resolve message address
    emitter.instruction("mov x2, #38");                                         // message length: "Fatal error: null pointer dereference\n"
    emitter.instruction("mov x16, #4");                                         // syscall 4 = sys_write
    emitter.instruction("svc #0x80");                                           // write error to stderr
    emitter.instruction("mov x0, #1");                                          // exit code 1
    emitter.instruction("mov x16, #1");                                         // syscall 1 = sys_exit
    emitter.instruction("svc #0x80");                                           // terminate process

    // -- success path --
    emitter.label("__rt_ptr_check_nonnull_ok");
    emitter.instruction("ret");                                                 // pointer is valid, return to caller
}
