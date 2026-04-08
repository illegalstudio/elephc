use crate::codegen::{emit::Emitter, platform::Arch};

use super::{
    emit_store_reg_to_symbol,
    process_argc_reg,
    process_argv_reg,
    temp_int_reg,
};

pub fn emit_store_process_args_to_globals(emitter: &mut Emitter) {
    emit_store_reg_to_symbol(emitter, process_argc_reg(emitter.target), "_global_argc", 0);
    emit_store_reg_to_symbol(emitter, process_argv_reg(emitter.target), "_global_argv", 0);
}

pub fn emit_enable_heap_debug_flag(emitter: &mut Emitter) {
    let scratch = temp_int_reg(emitter.target);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov {}, #1", scratch));                       // materialize the enabled heap-debug flag in the temporary integer register
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {}, 1", scratch));                        // materialize the enabled heap-debug flag in the temporary integer register
        }
    }
    emit_store_reg_to_symbol(emitter, scratch, "_heap_debug_enabled", 0);
}

pub fn emit_copy_frame_pointer(emitter: &mut Emitter, dest: &str) {
    emitter.instruction(&format!("mov {}, {}", dest, super::registers::frame_pointer_reg(emitter))); // copy the current frame pointer into the requested scratch register
}

pub fn emit_exit(emitter: &mut Emitter, code: u32) {
    match (emitter.target.platform, emitter.target.arch) {
        (super::super::platform::Platform::MacOS, Arch::AArch64)
        | (super::super::platform::Platform::Linux, Arch::AArch64) => {
            emitter.instruction(&format!("mov x0, #{}", code));                        // load the requested process exit code into the ABI return register
            emitter.syscall(1);
        }
        (super::super::platform::Platform::Linux, Arch::X86_64) => {
            emitter.instruction(&format!("mov edi, {}", code));                        // load the requested process exit code into the SysV first-argument register
            emitter.instruction("mov eax, 60");                                        // Linux x86_64 syscall 60 = exit
            emitter.instruction("syscall");                                            // terminate the process through the Linux x86_64 syscall ABI
        }
        (super::super::platform::Platform::MacOS, Arch::X86_64) => {
            panic!("process exit emission is not implemented yet for target macos-x86_64");
        }
    }
}
