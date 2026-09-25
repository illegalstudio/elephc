//! Purpose:
//! Adapts native C callback entry points to the hand-written runtime helper ABI.
//! Keeps Windows MSx64 register and stack placement out of eval wrapper emitters.
//!
//! Called from:
//! - C-visible eval and reflection callback label emitters.
//!
//! Key details:
//! - Hand-written x86_64 callback bodies consume the SysV-shaped integer registers.
//! - Windows overflow offsets include the mandatory 32-byte shadow space.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::{Arch, Platform, Target};

/// Returns the private SysV-shaped entry label used by generated assembly calls
/// that must bypass the native C callback adapter.
pub fn c_callback_internal_symbol(target: Target, name: &str) -> String {
    target.extern_symbol(&format!("{}__internal", name))
}

/// Emits a native C-visible callback label followed by its target adapter and
/// a private label for calls already using the hand-written helper ABI.
pub fn emit_c_callback_entry(emitter: &mut Emitter, name: &str) {
    let symbol = emitter.target.extern_symbol(name);
    emitter.label_global(&symbol);
    emit_c_callback_sysv_register_adapter(emitter);
    let internal_symbol = c_callback_internal_symbol(emitter.target, name);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("b {}", internal_symbol)), // enter the adapter-free callback implementation
        Arch::X86_64 => emitter.instruction(&format!("jmp {}", internal_symbol)), // enter the adapter-free callback implementation
    }
    emitter.raw(&format!(".globl {}", internal_symbol));
    emitter.raw(&format!("{}:", internal_symbol));
}

/// Normalizes an incoming Windows x86_64 C callback's first six integer arguments
/// into the register layout consumed by the hand-written SysV-shaped helper body.
fn emit_c_callback_sysv_register_adapter(emitter: &mut Emitter) {
    if (emitter.target.platform, emitter.target.arch) != (Platform::Windows, Arch::X86_64) {
        return;
    }

    emit_windows_c_abi_registers_for_runtime_helper(emitter);
    emitter.instruction("mov r8, QWORD PTR [rsp + 40]");                        // normalize callback argument 5 from the Windows caller stack
    emitter.instruction("mov r9, QWORD PTR [rsp + 48]");                        // normalize callback argument 6 from the Windows caller stack
}

/// Normalizes four materialized MSx64 integer arguments for a hand-written
/// x86_64 runtime helper that consumes the repository's SysV-shaped ABI.
pub fn emit_windows_c_abi_registers_for_runtime_helper(emitter: &mut Emitter) {
    if (emitter.target.platform, emitter.target.arch) != (Platform::Windows, Arch::X86_64) {
        return;
    }

    emitter.instruction("mov rdi, rcx");                                        // normalize callback argument 1 from MSx64 to the runtime helper ABI
    emitter.instruction("mov rsi, rdx");                                        // normalize callback argument 2 from MSx64 to the runtime helper ABI
    emitter.instruction("mov rdx, r8");                                         // normalize callback argument 3 from MSx64 to the runtime helper ABI
    emitter.instruction("mov rcx, r9");                                         // normalize callback argument 4 from MSx64 to the runtime helper ABI
}

/// Returns the frame-pointer offset for a zero-based integer callback argument
/// that follows the six registers consumed by a SysV-shaped x86_64 helper body.
pub fn c_callback_stack_arg_offset(target: Target, argument_index: usize) -> usize {
    assert!(argument_index >= 6, "callback stack arguments start at index 6");
    match (target.platform, target.arch) {
        (Platform::Windows, Arch::X86_64) => 48 + (argument_index - 4) * 8,
        (_, Arch::X86_64) => 16 + (argument_index - 6) * 8,
        (_, Arch::AArch64) => panic!("x86_64 callback stack offsets are not valid on AArch64"),
    }
}
