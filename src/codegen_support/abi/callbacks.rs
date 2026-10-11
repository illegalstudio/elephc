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
    emit_c_callback_entry_with_arity(emitter, name, 6);
}

/// Emits a C callback entry while forwarding exactly the declared integer arguments.
pub fn emit_c_callback_entry_with_arity(emitter: &mut Emitter, name: &str, arity: usize) {
    let symbol = emitter.target.extern_symbol(name);
    emitter.label_global(&symbol);
    if (emitter.target.platform, emitter.target.arch) == (Platform::Windows, Arch::X86_64) {
        emitter.instruction("mov r10, rdi");                                    // retain the Windows caller's first nonvolatile register
        emitter.instruction("mov r11, rsi");                                    // retain the Windows caller's second nonvolatile register
    }
    emit_c_callback_sysv_register_adapter(emitter, arity);
    let internal_symbol = c_callback_internal_symbol(emitter.target, name);
    if (emitter.target.platform, emitter.target.arch) == (Platform::Windows, Arch::X86_64) {
        let overflow = arity.saturating_sub(6);
        let state_offset = if overflow == 0 { 0 } else { 48 + overflow * 8 };
        let unaligned_size = state_offset + 16 + 160;
        let frame_size = if unaligned_size % 16 == 0 { unaligned_size + 8 } else { unaligned_size };
        emitter.instruction(&format!("sub rsp, {frame_size}"));                 // align the SysV call and reserve exact overflow arguments plus Windows state
        for index in 0..overflow {
            emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", frame_size + 56 + index * 8)); // recover one declared Windows stack argument
            emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", 48 + index * 8)); // retain its reviewed internal-frame offset
        }
        emitter.instruction(&format!("mov QWORD PTR [rsp + {state_offset}], r10")); // save the caller's original rdi across the SysV body
        emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r11", state_offset + 8)); // save the caller's original rsi across the SysV body
        for index in 0..10 {
            emitter.instruction(&format!("movdqu XMMWORD PTR [rsp + {}], xmm{}", state_offset + 16 + index * 16, 6 + index)); // preserve the Windows nonvolatile vector register
        }
        emitter.instruction(&format!("call {internal_symbol}"));                // execute the established SysV-shaped callback body
        for index in 0..10 {
            emitter.instruction(&format!("movdqu xmm{}, XMMWORD PTR [rsp + {}]", 6 + index, state_offset + 16 + index * 16)); // restore the Windows nonvolatile vector register
        }
        emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {state_offset}]")); // restore the Windows caller's first nonvolatile register
        emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", state_offset + 8)); // restore the Windows caller's second nonvolatile register
        emitter.instruction(&format!("add rsp, {frame_size}"));                 // retire the aligned callback preservation frame
        emitter.instruction("ret");                                             // return through the original MSx64 caller frame
        emitter.raw(&format!(".globl {}", internal_symbol));
        emitter.raw(&format!("{}:", internal_symbol));
        return;
    }
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("b {}", internal_symbol)), // enter the adapter-free callback implementation
        Arch::X86_64 => emitter.instruction(&format!("jmp {}", internal_symbol)), // enter the adapter-free callback implementation
    }
    emitter.raw(&format!(".globl {}", internal_symbol));
    emitter.raw(&format!("{}:", internal_symbol));
}

/// Normalizes an incoming Windows x86_64 C callback's first six integer arguments
/// into the register layout consumed by the hand-written SysV-shaped helper body.
fn emit_c_callback_sysv_register_adapter(emitter: &mut Emitter, arity: usize) {
    if (emitter.target.platform, emitter.target.arch) != (Platform::Windows, Arch::X86_64) {
        return;
    }

    emit_windows_c_abi_registers_for_runtime_helper(emitter);
    if arity > 4 {
        emitter.instruction("mov r8, QWORD PTR [rsp + 40]");                    // normalize callback argument 5 from the Windows caller stack
    }
    if arity > 5 {
        emitter.instruction("mov r9, QWORD PTR [rsp + 48]");                    // normalize callback argument 6 from the Windows caller stack
    }
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
