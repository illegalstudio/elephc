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

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use elephc_builtin_contract::mbstring_abi::invoke::MbQueryStorageV1;

mod storage;
#[cfg(test)]
mod tests;

/// Emits query registration and the native root, append-index, and cursor-release callbacks.
pub(super) fn emit(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    emitter.label_global("__rt_mbstring_query_register");
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
    for (index, callback) in [
        "__rt_mbstring_query_root", "__rt_mbstring_query_next",
        "__rt_mbstring_query_hash_enter", "__rt_mbstring_capture_hash_store",
        "__rt_mbstring_query_hash_remove", "__rt_mbstring_query_release",
    ].into_iter().enumerate() {
        abi::emit_symbol_address(emitter, if arm { "x9" } else { "r10" }, callback);
        let offset = 8 + index * 8;
        let instruction = if arm { format!("str x9, [sp, #{offset}]") }
            else { format!("mov QWORD PTR [rsp + {}], r10", offset + 16) };
        emitter.instruction(&instruction);                                      // populate the required callback inventory in ABI order
    }
    if arm { emitter.instruction("mov x7, sp"); }                               // provide the table in the eighth C argument register
    emitter.bl_c("elephc_mbstring_query_apply_v1");
    if arm {
        emitter.instruction("ldp x29, x30, [sp, #64]");                         // restore linkage after every shared step and cursor release
        emitter.instruction("add sp, sp, #80");                                 // retire the local callback table
    } else {
        emitter.instruction("add rsp, 88");                                     // restore caller alignment after the C8 call
    }
    emitter.instruction("ret");                                                 // preserve the executor's success, fatal, or pending status
    storage::emit(emitter);
}
