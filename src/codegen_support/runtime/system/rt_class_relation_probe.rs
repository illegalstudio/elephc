//! Purpose:
//! Emits `__rt_class_relation_probe`, the shared low-level search backing every
//! non-literal `class_implements()`/`class_parents()`/`class_uses()` lookup: it
//! lowercases a query name and binary-searches one 64-byte-row relation table,
//! returning the matching row pointer (or null).
//!
//! Called from:
//! - `crate::codegen_support::runtime::system::rt_class_relation_lookup::emit_rt_class_relation_lookup()`,
//!   once per candidate table (`_class_relation_table`, `_interface_relation_table`,
//!   `_trait_relation_table`).
//!
//! Key details:
//! - Input:  x0/rdi=name_ptr, x1/rsi=name_len, x2/rdx=table_base, x3/rcx=row_count.
//! - Output: x0/rax = matching row pointer, or 0 when the table has no row for
//!   this name.
//! - Every relation table shares the same uniform 64-byte row layout (see
//!   `crate::codegen_support::runtime::data::class_relation_registry`), so the search
//!   always passes entry_size = 64 to `__rt_sorted_name_search` regardless of
//!   which table (class/interface/trait) is being probed.
//! - Mirrors `__rt_class_exists`'s lowercase-then-search shape, but returns the
//!   row pointer itself (so the caller can read the row's `implements`/
//!   `parents`/`uses` payload fields) instead of collapsing the result to a
//!   boolean.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Emits the target-specific `__rt_class_relation_probe` search helper.
pub fn emit_rt_class_relation_probe(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 implementation of `__rt_class_relation_probe`.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: class_relation_probe ---");
    emitter.label_global("__rt_class_relation_probe");

    emitter.instruction("stp x29, x30, [sp, #-48]!");                           // save frame pointer/return address, reserve 4 spill slots
    emitter.instruction("mov x29, sp");                                         // establish the new frame pointer
    emitter.instruction("stp x2, x3, [sp, #16]");                               // stash table_base/row_count across the strtolower call

    // -- lowercase the query name so the lookup matches PHP's case-insensitive names --
    emitter.instruction("mov x2, x1");                                          // stash the query length before strtolower's args overwrite x1
    emitter.instruction("mov x1, x0");                                          // move the query pointer into strtolower's pointer argument
    emitter.instruction("bl __rt_strtolower");                                  // x1=lowercased query copy, x2=length unchanged
    emitter.instruction("str x1, [sp, #32]");                                   // keep the owned lowered copy for release after the search

    // -- search the requested relation table --
    emitter.instruction("mov x0, x1");                                          // lowercased query pointer -> search arg0
    emitter.instruction("mov x1, x2");                                          // query length -> search arg1
    emitter.instruction("ldp x2, x3, [sp, #16]");                               // reload table_base/row_count
    emitter.instruction("mov x4, #64");                                         // relation rows are 64 bytes wide (uniform across all 3 tables)
    emitter.instruction("bl __rt_sorted_name_search");                          // x0 = matching row pointer, or 0

    // -- strtolower returns an OWNED copy; give it back before answering --
    emitter.instruction("str x0, [sp, #40]");                                   // preserve the row pointer across the copy release
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the owned lowered query copy
    emitter.instruction("bl __rt_decref_any");                                  // release the lowered copy
    emitter.instruction("ldr x0, [sp, #40]");                                   // restore the row pointer
    emitter.instruction("ldp x29, x30, [sp], #48");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return the row pointer (or null) in x0
}

/// Emits the x86_64 implementation of `__rt_class_relation_probe`.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: class_relation_probe ---");
    emitter.label_global("__rt_class_relation_probe");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 32");                                         // reserve table, row-count, lowered-copy and answer slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdx");                        // stash table_base across the strtolower call
    emitter.instruction("mov QWORD PTR [rbp - 16], rcx");                       // stash row_count across the strtolower call

    // -- lowercase the query name so the lookup matches PHP's case-insensitive names --
    emitter.instruction("mov rdx, rsi");                                        // stash the query length before strtolower's args overwrite it
    emitter.instruction("mov rax, rdi");                                        // move the query pointer into strtolower's pointer argument
    emitter.instruction("call __rt_strtolower");                                // rax=lowercased query copy, rdx=length unchanged
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // keep the owned lowered copy for release after the search

    // -- search the requested relation table --
    emitter.instruction("mov rdi, rax");                                        // lowercased query pointer -> search arg0
    emitter.instruction("mov rsi, rdx");                                        // query length -> search arg1
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // restore row_count -> search arg3
    emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");                        // restore table_base -> search arg2
    emitter.instruction("mov r8, 64");                                          // relation rows are 64 bytes wide (uniform across all 3 tables)
    emitter.instruction("call __rt_sorted_name_search");                        // rax = matching row pointer, or 0

    // -- strtolower returns an OWNED copy; give it back before answering --
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the row pointer across the copy release
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the owned lowered query copy
    emitter.instruction("call __rt_decref_any");                                // release the lowered copy
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // restore the row pointer
    emitter.instruction("leave");                                               // release the slots and restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the row pointer (or null) in rax
}
