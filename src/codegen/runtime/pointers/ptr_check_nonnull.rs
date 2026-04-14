use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// __rt_ptr_check_nonnull: abort with a fatal error on null pointer dereference.
/// Input:  x0 = pointer value
/// Output: x0 unchanged on success; process exits on null
pub(crate) fn emit_ptr_check_nonnull(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ptr_check_nonnull_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for ARM64 instructions
    emitter.comment("--- runtime: ptr_check_nonnull ---");
    emitter.label_global("__rt_ptr_check_nonnull");

    // -- fast path for valid pointers --
    emitter.instruction("cmp x0, #0");                                          // compare pointer value against null
    emitter.instruction("b.ne __rt_ptr_check_nonnull_ok");                      // continue when pointer is non-null

    // -- fatal error: null pointer dereference --
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.adrp("x1", "_ptr_null_err_msg");                     // load page of null dereference message
    emitter.add_lo12("x1", "x1", "_ptr_null_err_msg");               // resolve message address
    emitter.instruction("mov x2, #38");                                         // message length: "Fatal error: null pointer dereference\n"
    emitter.syscall(4);
    emitter.instruction("mov x0, #1");                                          // exit code 1
    emitter.syscall(1);

    // -- success path --
    emitter.label("__rt_ptr_check_nonnull_ok");
    emitter.instruction("ret");                                                 // pointer is valid, return to caller
}

fn emit_ptr_check_nonnull_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ptr_check_nonnull ---");
    emitter.label_global("__rt_ptr_check_nonnull");

    // -- fast path for valid pointers --
    emitter.instruction("test rax, rax");                                       // compare the incoming pointer value against null
    emitter.instruction("jnz __rt_ptr_check_nonnull_ok");                       // return immediately when the pointer is non-null

    // -- fatal error: null pointer dereference --
    emitter.instruction("mov edi, 2");                                          // target the Linux stderr file descriptor for the fatal error message
    abi::emit_symbol_address(emitter, "rsi", "_ptr_null_err_msg");
    emitter.instruction("mov edx, 38");                                         // describe the full fatal null-dereference message byte length to write(2)
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall number 1 = write
    emitter.instruction("syscall");                                             // emit the fatal null-dereference message before terminating the process
    emitter.instruction("mov edi, 1");                                          // return process exit code 1 for the fatal null-dereference abort path
    emitter.instruction("mov eax, 60");                                         // Linux x86_64 syscall number 60 = exit
    emitter.instruction("syscall");                                             // terminate the process after reporting the fatal null-dereference

    // -- success path --
    emitter.label("__rt_ptr_check_nonnull_ok");
    emitter.instruction("ret");                                                 // pointer is valid, return to the caller unchanged in rax
}
