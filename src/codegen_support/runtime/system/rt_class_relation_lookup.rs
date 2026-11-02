//! Purpose:
//! Emits `__rt_class_relation_lookup`, the shared runtime dispatcher backing
//! non-literal `class_implements()`/`class_parents()`/`class_uses()`: it tries
//! the class, then interface, then trait relation table (mirroring the literal
//! path's class/interface/trait resolution order) and extracts one payload
//! field from whichever row matches.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::class_relations`, once per
//!   builtin, passing that builtin's fixed payload byte offset within a
//!   relation row (16 = implements, 32 = parents, 48 = uses — see
//!   `crate::codegen_support::runtime::data::class_relation_registry`).
//!
//! Key details:
//! - Input:  x0/rdi=name_ptr, x1/rsi=name_len, x2/rdx=payload_offset.
//! - Output: x0/rax=found (0 or 1); when found, x1/rdi=list_ptr, x2/rsi=list_count
//!   (a `{name_ptr, name_len}` array in PHP declaration order — zero/zero when
//!   the matched target has no such relation, e.g. `parents` on an interface).
//!   Not found leaves x1/rdi=0, x2/rsi=0 too.
//! - Every relation-table row shares the same 8 fixed fields, so `payload_offset`
//!   is valid against whichever of the 3 tables actually matched.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// Emits the target-specific `__rt_class_relation_lookup` dispatcher.
pub fn emit_rt_class_relation_lookup(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 implementation of `__rt_class_relation_lookup`.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: class_relation_lookup ---");
    emitter.label_global("__rt_class_relation_lookup");

    emitter.instruction("stp x29, x30, [sp, #-48]!");                           // save frame pointer/return address, reserve 3 spill slots
    emitter.instruction("mov x29, sp");                                         // establish the new frame pointer
    emitter.instruction("stp x0, x1, [sp, #16]");                               // stash name_ptr/name_len across every probe call
    emitter.instruction("str x2, [sp, #32]");                                   // stash the caller's requested payload byte offset

    // -- try the class relation table first (matches the literal path's class/interface/trait order) --
    emitter.instruction("ldp x0, x1, [sp, #16]");                               // reload name_ptr/name_len
    abi::emit_symbol_address(emitter, "x2", "_class_relation_table");
    abi::emit_load_symbol_to_reg(emitter, "x3", "_class_relation_table_count", 0);
    emitter.instruction("bl __rt_class_relation_probe");                        // x0 = matching class row, or 0
    emitter.instruction("cbnz x0, __rt_class_relation_lookup_found");           // stop searching once a class row matches

    // -- then the interface relation table --
    emitter.instruction("ldp x0, x1, [sp, #16]");                               // reload name_ptr/name_len
    abi::emit_symbol_address(emitter, "x2", "_interface_relation_table");
    abi::emit_load_symbol_to_reg(emitter, "x3", "_interface_relation_table_count", 0);
    emitter.instruction("bl __rt_class_relation_probe");                        // x0 = matching interface row, or 0
    emitter.instruction("cbnz x0, __rt_class_relation_lookup_found");           // stop searching once an interface row matches

    // -- finally the trait relation table --
    emitter.instruction("ldp x0, x1, [sp, #16]");                               // reload name_ptr/name_len
    abi::emit_symbol_address(emitter, "x2", "_trait_relation_table");
    abi::emit_load_symbol_to_reg(emitter, "x3", "_trait_relation_table_count", 0);
    emitter.instruction("bl __rt_class_relation_probe");                        // x0 = matching trait row, or 0
    emitter.instruction("cbz x0, __rt_class_relation_lookup_miss");             // no table has a row for this name

    emitter.label("__rt_class_relation_lookup_found");
    emitter.instruction("ldr x3, [sp, #32]");                                   // reload the requested payload byte offset
    emitter.instruction("ldr x1, [x0, x3]");                                    // list_ptr = *(row + payload_offset)
    emitter.instruction("add x4, x3, #8");                                      // advance to the row's paired list_count field
    emitter.instruction("ldr x2, [x0, x4]");                                    // list_count = *(row + payload_offset + 8)
    emitter.instruction("mov x0, #1");                                          // found = true
    emitter.instruction("b __rt_class_relation_lookup_done");                   // skip the not-found fallback

    emitter.label("__rt_class_relation_lookup_miss");
    emitter.instruction("mov x0, #0");                                          // found = false
    emitter.instruction("mov x1, #0");                                          // no list pointer when the target is unknown
    emitter.instruction("mov x2, #0");                                          // no list count when the target is unknown

    emitter.label("__rt_class_relation_lookup_done");
    emitter.instruction("ldp x29, x30, [sp], #48");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return found/list_ptr/list_count in x0/x1/x2
}

/// Emits the x86_64 implementation of `__rt_class_relation_lookup`.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: class_relation_lookup ---");
    emitter.label_global("__rt_class_relation_lookup");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // keep the nested call 16-byte aligned
    emitter.instruction("sub rsp, 32");                                         // reserve 3 spill slots (16-byte aligned), rsp stays fixed for every call below
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // stash name_ptr across every probe call
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // stash name_len across every probe call
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // stash the caller's requested payload byte offset

    // -- try the class relation table first (matches the literal path's class/interface/trait order) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload name_ptr
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload name_len
    abi::emit_symbol_address(emitter, "rdx", "_class_relation_table");
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_class_relation_table_count", 0);
    emitter.instruction("call __rt_class_relation_probe");                      // rax = matching class row, or 0
    emitter.instruction("test rax, rax");                                       // does a class row match?
    emitter.instruction("jnz __rt_class_relation_lookup_found");                // stop searching once a class row matches

    // -- then the interface relation table --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload name_ptr
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload name_len
    abi::emit_symbol_address(emitter, "rdx", "_interface_relation_table");
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_interface_relation_table_count", 0);
    emitter.instruction("call __rt_class_relation_probe");                      // rax = matching interface row, or 0
    emitter.instruction("test rax, rax");                                       // does an interface row match?
    emitter.instruction("jnz __rt_class_relation_lookup_found");                // stop searching once an interface row matches

    // -- finally the trait relation table --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload name_ptr
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload name_len
    abi::emit_symbol_address(emitter, "rdx", "_trait_relation_table");
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_trait_relation_table_count", 0);
    emitter.instruction("call __rt_class_relation_probe");                      // rax = matching trait row, or 0
    emitter.instruction("test rax, rax");                                       // does a trait row match?
    emitter.instruction("jz __rt_class_relation_lookup_miss");                  // no table has a row for this name

    emitter.label("__rt_class_relation_lookup_found");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the requested payload byte offset
    emitter.instruction("mov rdi, QWORD PTR [rax + rdx]");                      // list_ptr = *(row + payload_offset)
    emitter.instruction("add rdx, 8");                                          // advance to the row's paired list_count field
    emitter.instruction("mov rsi, QWORD PTR [rax + rdx]");                      // list_count = *(row + payload_offset + 8)
    emitter.instruction("mov rax, 1");                                          // found = true
    emitter.instruction("jmp __rt_class_relation_lookup_done");                 // skip the not-found fallback

    emitter.label("__rt_class_relation_lookup_miss");
    emitter.instruction("xor eax, eax");                                        // found = false
    emitter.instruction("xor edi, edi");                                        // no list pointer when the target is unknown
    emitter.instruction("xor esi, esi");                                        // no list count when the target is unknown

    emitter.label("__rt_class_relation_lookup_done");
    emitter.instruction("add rsp, 32");                                         // drop the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return found/list_ptr/list_count in rax/rdi/rsi
}

