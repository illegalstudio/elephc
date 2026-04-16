use crate::codegen::{abi, emit::Emitter, platform::Arch};

pub fn emit_match_unhandled(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: match_unhandled ---");
    emitter.label_global("__rt_match_unhandled");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.adrp("x1", "_match_unhandled_msg");                          // load the unhandled-match error message page for the AArch64 fatal path
            emitter.add_lo12("x1", "x1", "_match_unhandled_msg");               // resolve the exact unhandled-match error message address for the AArch64 fatal path
            emitter.instruction("mov x2, #34");                                 // byte length of the unhandled-match error message
            emitter.instruction("mov x0, #2");                                  // write diagnostics to stderr on the AArch64 fatal path
            emitter.syscall(4);
            emitter.instruction("mov x0, #70");                                 // use EX_SOFTWARE as the process exit status on the AArch64 fatal path
            emitter.syscall(1);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rsi", "_match_unhandled_msg");   // materialize the unhandled-match error message address for the x86_64 fatal path
            emitter.instruction("mov edx, 34");                                 // byte length of the unhandled-match error message
            emitter.instruction("mov edi, 2");                                  // write diagnostics to stderr on the x86_64 fatal path
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall number 1 = write
            emitter.instruction("syscall");                                     // emit the unhandled-match fatal diagnostic on x86_64
            emitter.instruction("mov edi, 70");                                 // use EX_SOFTWARE as the process exit status on the x86_64 fatal path
            emitter.instruction("mov eax, 60");                                 // Linux x86_64 syscall number 60 = exit
            emitter.instruction("syscall");                                     // terminate the process after reporting the unhandled match case
        }
    }
}
