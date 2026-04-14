use crate::codegen::{emit::Emitter, platform::Arch};

/// feof: check if EOF has been reached for a file descriptor.
/// Input:  x0=fd
/// Output: x0=1 if EOF, 0 if not
pub fn emit_feof(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_feof_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: feof ---");
    emitter.label_global("__rt_feof");

    // -- load eof flag for this fd from _eof_flags array --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_eof_flags");
    emitter.instruction("ldrb w0, [x9, x0]");                                   // load _eof_flags[fd] into return register
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_feof_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: feof ---");
    emitter.label_global("__rt_feof");

    emitter.instruction("lea r10, [rip + _eof_flags]");                         // materialize the eof-flag table base address for the queried file descriptor
    emitter.instruction("movzx eax, BYTE PTR [r10 + rdi]");                     // load the tracked eof flag byte for the requested file descriptor into the integer result register
    emitter.instruction("ret");                                                 // return the eof flag to the caller using the standard x86_64 integer result register
}
