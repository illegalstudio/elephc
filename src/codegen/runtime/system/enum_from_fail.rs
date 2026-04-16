use crate::codegen::{abi, emit::Emitter, platform::Arch};

pub fn emit_enum_from_fail(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: enum_from_fail ---");
    emitter.label_global("__rt_enum_from_fail");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.adrp("x1", "_enum_from_msg");                    // load the enum-from error message page
            emitter.add_lo12("x1", "x1", "_enum_from_msg");         // resolve the enum-from error message address
            emitter.instruction("mov x2, #33");                                 // byte length of the enum-from error message
            emitter.instruction("mov x0, #2");                                  // write diagnostics to stderr
            emitter.syscall(4);
            emitter.instruction("mov x0, #70");                                 // use EX_SOFTWARE as the process exit status
            emitter.syscall(1);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rsi", "_enum_from_msg"); // materialize the enum-from error message address for x86_64
            emitter.instruction("mov edx, 33");                                 // byte length of the enum-from error message
            emitter.instruction("mov edi, 2");                                  // write diagnostics to stderr
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall number 1 = write
            emitter.instruction("syscall");                                     // emit the enum-from error message
            emitter.instruction("mov edi, 70");                                 // use EX_SOFTWARE as the process exit status
            emitter.instruction("mov eax, 60");                                 // Linux x86_64 syscall number 60 = exit
            emitter.instruction("syscall");                                     // terminate the process after the fatal enum conversion failure
        }
    }
}
