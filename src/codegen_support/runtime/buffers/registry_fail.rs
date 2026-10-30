//! Purpose:
//! Emits the fatal diagnostic used when every generation-safe Buffer descriptor
//! is already live or permanently retired.
//!
//! Called from:
//! - `crate::codegen_support::runtime::buffers::buffer_new` on registry exhaustion.
//!
//! Key details:
//! - Registry exhaustion is distinct from allocation-size overflow on both targets.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the target-specific Buffer registry-exhaustion fatal helper.
pub fn emit_buffer_registry_fail(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_buffer_registry_fail_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: buffer_registry_fail ---");
    emitter.label_global("__rt_buffer_registry_exhausted");
    if emitter.phpext_output { abi::emit_cdylib_exit_escape(emitter); }
    abi::emit_symbol_address(emitter, "x1", "_buffer_registry_exhausted_msg");
    emitter.instruction("mov x2, #39");                                         // pass the exact diagnostic byte length
    emitter.instruction("mov x0, #2");                                          // write the diagnostic to standard error
    emitter.syscall(4);
    abi::emit_cdylib_exit_escape(emitter);
    emitter.instruction("mov x0, #70");                                         // report a deterministic software failure status
    emitter.syscall(1);
}

/// Emits the Linux x86_64 Buffer registry-exhaustion fatal helper.
fn emit_buffer_registry_fail_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: buffer_registry_fail ---");
    emitter.label_global("__rt_buffer_registry_exhausted");
    if emitter.phpext_output { abi::emit_cdylib_exit_escape(emitter); }
    abi::emit_symbol_address(emitter, "rsi", "_buffer_registry_exhausted_msg");
    emitter.instruction("mov rdx, 39");                                         // pass the exact diagnostic byte length
    emitter.instruction("mov rdi, 2");                                          // write the diagnostic to standard error
    emitter.instruction("mov rax, 1");                                          // select the Linux x86_64 write syscall
    emitter.instruction("syscall");                                             // emit the registry-exhaustion diagnostic
    abi::emit_cdylib_exit_escape(emitter);
    emitter.instruction("mov rdi, 70");                                         // report a deterministic software failure status
    emitter.instruction("mov rax, 60");                                         // select the Linux x86_64 exit syscall
    emitter.instruction("syscall");                                             // terminate without returning to buffer allocation
}
