use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

pub fn emit_buffer_bounds_fail(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_buffer_bounds_fail_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: buffer_bounds_fail ---");
    emitter.label_global("__rt_buffer_bounds_fail");
    emitter.adrp("x1", "_buffer_bounds_msg");                    // load the error message page
    emitter.add_lo12("x1", "x1", "_buffer_bounds_msg");              // resolve the buffer bounds message address
    emitter.instruction("mov x2, #40");                                         // byte length of the fixed buffer bounds error message
    emitter.instruction("mov x0, #2");                                          // write diagnostics to stderr
    emitter.syscall(4);
    emitter.instruction("mov x0, #70");                                         // use EX_SOFTWARE as the process exit status
    emitter.syscall(1);
}

fn emit_buffer_bounds_fail_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: buffer_bounds_fail ---");
    emitter.label_global("__rt_buffer_bounds_fail");
    emitter.instruction("mov edi, 2");                                          // write diagnostics to the Linux stderr file descriptor
    abi::emit_symbol_address(emitter, "rsi", "_buffer_bounds_msg");
    emitter.instruction("mov edx, 40");                                         // byte length of the fixed buffer-bounds error message
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // emit the fatal buffer-bounds diagnostic to stderr
    emitter.instruction("mov edi, 70");                                         // use EX_SOFTWARE as the process exit status for consistency with the ARM runtime
    emitter.instruction("mov eax, 60");                                         // Linux x86_64 syscall 60 = exit
    emitter.instruction("syscall");                                             // terminate the process immediately after the fatal buffer-bounds diagnostic
}
