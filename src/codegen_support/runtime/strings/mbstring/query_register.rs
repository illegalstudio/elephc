//! Purpose:
//! Connects the native V5 query writer to the shared Rust step executor.
//!
//! Called from:
//! - The mbstring runtime emitter and native query registration tests.
//!
//! Key details:
//! - The C7 callback forwards its borrowed inputs to a C8 entry with a V1 storage table.
//! - The shared executor owns cursor ordering; native callbacks own COW and protected cleanup.
//! - The x86_64 adapter forwards both stack arguments without disturbing six register arguments.

use crate::codegen_support::{abi, emit::Emitter, platform::{Arch, Platform}};
use elephc_builtin_contract::mbstring_abi::invoke::MbQueryStorageV1;

mod storage;
#[cfg(test)]
mod tests;

/// Emits query registration and the native root, append-index, and cursor-release callbacks.
pub(super) fn emit(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    let callbacks = [
        "__rt_mbstring_query_root", "__rt_mbstring_query_next",
        "__rt_mbstring_query_hash_enter", "__rt_mbstring_capture_hash_store",
        "__rt_mbstring_query_hash_remove", "__rt_mbstring_query_release",
    ];
    for callback in callbacks {
        let wrapper = format!("{callback}_cabi");
        abi::emit_c_callback_entry(emitter, &wrapper);
        emitter.instruction(&format!("{} {callback}", if arm { "b" } else { "jmp" })); // enter the established internal query callback
    }
    emit_query_register_entry(emitter);
    if arm {
        emitter.instruction("sub sp, sp, #80");                                 // reserve the callback table and caller linkage
        emitter.instruction("stp x29, x30, [sp, #64]");                         // preserve linkage across shared Rust execution
        emitter.instruction("add x29, sp, #64");                                // establish the native registration frame
    } else {
        emitter.instruction("sub rsp, 88");                                     // align the C8 call and reserve two outgoing stack arguments plus the table
        emitter.instruction("mov r10, QWORD PTR [rsp + 96]");                   // recover the incoming seventh argument above the caller's return address
        emitter.instruction("mov QWORD PTR [rsp], r10");                        // forward writable registration metadata as the seventh C argument
        emitter.instruction("lea r10, [rsp + 16]");                             // locate the local storage callback table
        emitter.instruction("mov QWORD PTR [rsp + 8], r10");                    // provide the table as the eighth C argument
    }
    let header = (std::mem::size_of::<MbQueryStorageV1>() as i64) << 32 | 1;
    abi::emit_load_int_immediate(emitter, if arm { "x9" } else { "r10" }, header);
    emitter.instruction(if arm { "str x9, [sp]" } else { "mov QWORD PTR [rsp + 16], r10" }); // publish version one and the complete table size
    for (index, callback) in callbacks.into_iter().enumerate() {
        let wrapper = emitter.target.extern_symbol(&format!("{callback}_cabi"));
        abi::emit_symbol_address(emitter, if arm { "x9" } else { "r10" }, &wrapper);
        let offset = 8 + index * 8;
        let instruction = if arm { format!("str x9, [sp, #{offset}]") }
            else { format!("mov QWORD PTR [rsp + {}], r10", offset + 16) };
        emitter.instruction(&instruction);                                      // populate the required callback inventory in ABI order
    }
    if arm { emitter.instruction("mov x7, sp"); }                               // provide the table in the eighth C argument register
    emit_query_apply_call(emitter);
    if arm {
        emitter.instruction("ldp x29, x30, [sp, #64]");                         // restore linkage after every shared step and cursor release
        emitter.instruction("add sp, sp, #80");                                 // retire the local callback table
    } else {
        emitter.instruction("add rsp, 88");                                     // restore caller alignment after the C8 call
    }
    emitter.instruction("ret");                                                 // preserve the executor's success, fatal, or pending status
    storage::emit(emitter);
}

/// Calls the native eight-argument query executor through the target C ABI.
fn emit_query_apply_call(emitter: &mut Emitter) {
    if (emitter.target.platform, emitter.target.arch) != (Platform::Windows, Arch::X86_64) {
        emitter.emit_native_bridge_symbol_call("elephc_mbstring_query_apply_v1", 8);
        return;
    }
    let symbol = emitter.target.extern_symbol("elephc_mbstring_query_apply_v1");
    abi::emit_symbol_address(emitter, "r11", &symbol);
    emitter.instruction("mov rax, QWORD PTR [rsp]");                            // retain SysV argument seven before reserving Windows shadow space
    emitter.instruction("mov r10, QWORD PTR [rsp + 8]");                        // retain SysV argument eight before reserving Windows shadow space
    emitter.instruction("sub rsp, 64");                                         // reserve shadow space plus four Windows stack arguments
    emitter.instruction("mov QWORD PTR [rsp + 32], r8");                        // publish Windows argument five
    emitter.instruction("mov QWORD PTR [rsp + 40], r9");                        // publish Windows argument six
    emitter.instruction("mov QWORD PTR [rsp + 48], rax");                       // publish Windows argument seven
    emitter.instruction("mov QWORD PTR [rsp + 56], r10");                       // publish Windows argument eight
    emitter.remap_sysv_args_to_platform_for_callback(4);
    emitter.instruction("call r11");                                            // invoke the Rust query executor through MSx64
    emitter.instruction("add rsp, 64");                                         // release Windows shadow and stack arguments
}

/// Adapts all seven Windows C arguments, including the three MSx64 stack words,
/// into the SysV-shaped layout consumed by the established implementation.
fn emit_query_register_entry(emitter: &mut Emitter) {
    let name = "__rt_mbstring_query_register";
    if (emitter.target.platform, emitter.target.arch) != (Platform::Windows, Arch::X86_64) {
        abi::emit_c_callback_entry(emitter, name);
        return;
    }
    let public = emitter.target.extern_symbol(name);
    let internal = abi::c_callback_internal_symbol(emitter.target, name);
    emitter.label_global(&public);
    emitter.instruction("mov r10, QWORD PTR [rsp + 56]");                       // retain callback argument seven before allocating the SysV stack slot
    emitter.instruction("mov r11, rdi");                                        // retain the Windows caller's first nonvolatile register
    emitter.instruction("mov rax, rsi");                                        // retain the Windows caller's second nonvolatile register
    abi::emit_windows_c_abi_registers_for_runtime_helper(emitter);
    emitter.instruction("mov r8, QWORD PTR [rsp + 40]");                        // normalize callback argument five from Windows shadow space
    emitter.instruction("mov r9, QWORD PTR [rsp + 48]");                        // normalize callback argument six from Windows shadow space
    emitter.instruction("sub rsp, 200");                                        // align the internal call and reserve its stack argument and Windows state
    emitter.instruction("mov QWORD PTR [rsp], r10");                            // publish SysV callback argument seven
    emitter.instruction("mov QWORD PTR [rsp + 8], r11");                        // save the caller's original rdi across the SysV body
    emitter.instruction("mov QWORD PTR [rsp + 16], rax");                       // save the caller's original rsi across the SysV body
    for index in 0..10 {
        emitter.instruction(&format!("movdqu XMMWORD PTR [rsp + {}], xmm{}", 24 + index * 16, 6 + index)); // preserve the Windows nonvolatile vector register
    }
    emitter.instruction(&format!("call {internal}"));                           // run the existing ownership implementation
    for index in 0..10 {
        emitter.instruction(&format!("movdqu xmm{}, XMMWORD PTR [rsp + {}]", 6 + index, 24 + index * 16)); // restore the Windows nonvolatile vector register
    }
    emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");                        // restore the Windows caller's first nonvolatile register
    emitter.instruction("mov rsi, QWORD PTR [rsp + 16]");                       // restore the Windows caller's second nonvolatile register
    emitter.instruction("add rsp, 200");                                        // release staged arguments and Windows state without changing the result
    emitter.instruction("ret");                                                 // return through the original MSx64 caller frame
    emitter.raw(&format!(".globl {internal}"));
    emitter.raw(&format!("{internal}:"));
}
