use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// iterable_unsupported_kind: emit a fatal error and exit when foreach receives
/// an `iterable` value whose underlying heap kind is not yet supported (currently
/// objects are rejected until PHP's Traversable protocol is modeled).
///
/// Input: x0 = unused (caller does not need to preserve any payload).
/// Behavior: writes a fixed message to stderr and exits with status 70 (EX_SOFTWARE).
pub fn emit_iterable_unsupported_kind(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_iterable_unsupported_kind_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: iterable_unsupported_kind ---");
    emitter.label_global("__rt_iterable_unsupported_kind");
    emitter.adrp("x1", "_iterable_unsupported_kind_msg");                       // load the page that contains the iterable runtime fatal message
    emitter.add_lo12("x1", "x1", "_iterable_unsupported_kind_msg");             // resolve the iterable runtime fatal message address within that page
    emitter.instruction("mov x2, #57");                                         // pass the fixed iterable runtime fatal message length to write()
    emitter.instruction("mov x0, #2");                                          // write diagnostics to stderr
    emitter.syscall(4);
    emitter.instruction("mov x0, #70");                                         // use EX_SOFTWARE as the process exit status
    emitter.syscall(1);
}

fn emit_iterable_unsupported_kind_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: iterable_unsupported_kind ---");
    emitter.label_global("__rt_iterable_unsupported_kind");
    emitter.instruction("mov edi, 2");                                          // write diagnostics to the Linux stderr file descriptor
    abi::emit_symbol_address(emitter, "rsi", "_iterable_unsupported_kind_msg"); // point the Linux write() buffer register at the iterable fatal message
    emitter.instruction("mov edx, 57");                                         // pass the fixed iterable runtime fatal message length to write()
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // emit the iterable runtime fatal message before terminating
    emitter.instruction("mov edi, 70");                                         // use EX_SOFTWARE as the process exit status for consistency with the AArch64 path
    emitter.instruction("mov eax, 60");                                         // Linux x86_64 syscall 60 = exit
    emitter.instruction("syscall");                                             // terminate the process immediately after the iterable runtime fatal diagnostic
}
