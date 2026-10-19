//! Purpose:
//! Emits `__rt_property_hash_reference`, the runtime half of a PHP reference to a property that
//! lives in an object's per-instance property hash: a `stdClass` property, or a dynamic property
//! of a class with hash storage (`$o->{$k} = &$v`, `$o->{$k}['x'] = &$v`).
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::managed` (always emitted, dead-stripped).
//! - `crate::codegen::lower_inst::objects::property_hash_references` (the `DynamicPropRefCell` and
//!   `BindDynamicPropRefCell` EIR ops).
//!
//! Key details:
//! - The property hash holds boxed Mixed entries (value tag 7), exactly like a Mixed-entry
//!   associative array, so a property joins a reference set the same way an array entry does:
//!   the entry is stamped with runtime tag 11 over a managed reference cell
//!   (`__rt_hash_entry_make_reference` / `__rt_hash_entry_bind_reference`). Every property-hash
//!   reader already dereferences tag 11 through `__rt_hash_get`, and `__rt_hash_set` writes
//!   through it, so reads and plain writes of the property see the shared value.
//! - The hash is allocated when the slot is still empty and split (`__rt_hash_ensure_unique`)
//!   when another owner shares it, then republished into its slot BEFORE the entry address is
//!   taken. A missing name is inserted as boxed null first, which is PHP's fetch for writing.
//! - Inputs: argument 0 = address of the hash slot, argument 1 = name pointer, argument 2 = name
//!   length, argument 3 = cell to bind, or 0 to promote the entry and return its own cell.
//! - Output: the entry's (borrowed) reference cell when argument 3 is 0, otherwise 0.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_property_hash_reference` for the current target.
pub fn emit_property_hash_reference(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: property_hash_reference ---");
    emitter.label_global("__rt_property_hash_reference");
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

fn emit_aarch64(emitter: &mut Emitter) {
    // Frame: [sp+16] slot, [sp+24] name_ptr, [sp+32] name_len, [sp+40] cell.
    emitter.instruction("stp x29, x30, [sp, #-48]!");                           // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame
    emitter.instruction("stp x0, x1, [sp, #16]");                               // save the hash slot address and the name pointer
    emitter.instruction("stp x2, x3, [sp, #32]");                               // save the name length and the cell to bind
    emitter.instruction("ldr x9, [x0]");                                        // load the property hash from its slot
    emitter.instruction("cbnz x9, __rt_property_hash_reference_have_hash");     // an allocated hash needs no lazy creation
    emitter.instruction("mov x0, #8");                                          // initial capacity = 8 entries
    emitter.instruction("mov x1, #7");                                          // value type 7 = boxed Mixed entries
    emitter.instruction("bl __rt_hash_new");                                    // allocate the property hash
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the hash slot address
    emitter.instruction("str x0, [x10]");                                       // publish the new hash into its slot
    emitter.label("__rt_property_hash_reference_have_hash");
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the hash slot address
    emitter.instruction("ldr x0, [x10]");                                       // load the hash to split if shared
    emitter.instruction("bl __rt_hash_ensure_unique");                          // a reference must not leak into another owner's copy
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the hash slot address
    emitter.instruction("str x0, [x10]");                                       // publish the unique hash before taking an entry address
    emitter.instruction("ldp x1, x2, [sp, #24]");                               // key = property name pointer and length
    emitter.instruction("bl __rt_hash_get");                                    // x4 = matching entry address, 0 on a miss
    emitter.instruction("cbnz x4, __rt_property_hash_reference_found");         // an existing property keeps its position
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 = null
    emitter.instruction("mov x1, #0");                                          // null carries no low payload
    emitter.instruction("mov x2, #0");                                          // null carries no high payload
    emitter.instruction("bl __rt_mixed_from_value");                            // box the null the fetch for writing creates
    emitter.instruction("mov x3, x0");                                          // value_lo = the boxed null (consumed by the insert)
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the hash slot address
    emitter.instruction("ldr x0, [x10]");                                       // hash to insert into
    emitter.instruction("ldp x1, x2, [sp, #24]");                               // key = property name pointer and length
    emitter.instruction("mov x4, #0");                                          // boxed Mixed has no high payload word
    emitter.instruction("mov x5, #7");                                          // value tag 7 = boxed Mixed
    emitter.instruction("bl __rt_hash_set");                                    // insert the new null property, x0 = possibly grown hash
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the hash slot address
    emitter.instruction("str x0, [x10]");                                       // publish the grown hash before taking an entry address
    emitter.instruction("ldp x1, x2, [sp, #24]");                               // key = property name pointer and length
    emitter.instruction("bl __rt_hash_get");                                    // x4 = the inserted entry
    emitter.label("__rt_property_hash_reference_found");
    emitter.instruction("add x0, x4, #24");                                     // address the entry's low value word
    emitter.instruction("ldr x1, [sp, #40]");                                   // the cell to bind, or 0
    emitter.instruction("cbz x1, __rt_property_hash_reference_make");           // no cell: promote the entry and return its cell
    emitter.instruction("bl __rt_hash_entry_bind_reference");                   // the entry joins the given reference set
    emitter.instruction("mov x0, #0");                                          // a bind returns no cell
    emitter.instruction("b __rt_property_hash_reference_done");                 // finish
    emitter.label("__rt_property_hash_reference_make");
    emitter.instruction("bl __rt_hash_entry_make_reference");                   // x0 = the entry's reference cell
    emitter.label("__rt_property_hash_reference_done");
    emitter.instruction("ldp x29, x30, [sp], #48");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return the cell or 0
}

fn emit_x86_64(emitter: &mut Emitter) {
    // Frame: [rbp-8] slot, [rbp-16] name_ptr, [rbp-24] name_len, [rbp-32] cell.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("sub rsp, 32");                                         // reserve aligned slots for the four inputs
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the hash slot address
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the name pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the name length
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the cell to bind
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the property hash from its slot
    emitter.instruction("test r10, r10");                                       // is the hash allocated?
    emitter.instruction("jne __rt_property_hash_reference_have_hash");          // an allocated hash needs no lazy creation
    emitter.instruction("mov rdi, 8");                                          // initial capacity = 8 entries
    emitter.instruction("mov rsi, 7");                                          // value type 7 = boxed Mixed entries
    emitter.instruction("call __rt_hash_new");                                  // allocate the property hash
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the hash slot address
    emitter.instruction("mov QWORD PTR [r10], rax");                            // publish the new hash into its slot
    emitter.label("__rt_property_hash_reference_have_hash");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the hash slot address
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // load the hash to split if shared
    emitter.instruction("call __rt_hash_ensure_unique");                        // a reference must not leak into another owner's copy
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the hash slot address
    emitter.instruction("mov QWORD PTR [r10], rax");                            // publish the unique hash before taking an entry address
    emitter.instruction("mov rdi, rax");                                        // hash to probe
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // key_lo = property name pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // key_hi = property name length
    emitter.instruction("call __rt_hash_get");                                  // r8 = matching entry address, 0 on a miss
    emitter.instruction("test r8, r8");                                         // does the property exist?
    emitter.instruction("jne __rt_property_hash_reference_found");              // an existing property keeps its position
    emitter.instruction("mov rax, 8");                                          // runtime tag 8 = null
    emitter.instruction("xor edi, edi");                                        // null carries no low payload
    emitter.instruction("xor esi, esi");                                        // null carries no high payload
    emitter.instruction("call __rt_mixed_from_value");                          // box the null the fetch for writing creates
    emitter.instruction("mov rcx, rax");                                        // value_lo = the boxed null (consumed by the insert)
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the hash slot address
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // hash to insert into
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // key_lo = property name pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // key_hi = property name length
    emitter.instruction("xor r8d, r8d");                                        // boxed Mixed has no high payload word
    emitter.instruction("mov r9, 7");                                           // value tag 7 = boxed Mixed
    emitter.instruction("call __rt_hash_set");                                  // insert the new null property, rax = possibly grown hash
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the hash slot address
    emitter.instruction("mov QWORD PTR [r10], rax");                            // publish the grown hash before taking an entry address
    emitter.instruction("mov rdi, rax");                                        // hash to probe again
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // key_lo = property name pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // key_hi = property name length
    emitter.instruction("call __rt_hash_get");                                  // r8 = the inserted entry
    emitter.label("__rt_property_hash_reference_found");
    emitter.instruction("lea rdi, [r8 + 24]");                                  // address the entry's low value word
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // the cell to bind, or 0
    emitter.instruction("test rsi, rsi");                                       // was a cell given?
    emitter.instruction("je __rt_property_hash_reference_make");                // no cell: promote the entry and return its cell
    emitter.instruction("call __rt_hash_entry_bind_reference");                 // the entry joins the given reference set
    emitter.instruction("xor eax, eax");                                        // a bind returns no cell
    emitter.instruction("jmp __rt_property_hash_reference_done");               // finish
    emitter.label("__rt_property_hash_reference_make");
    emitter.instruction("call __rt_hash_entry_make_reference");                 // rax = the entry's reference cell
    emitter.label("__rt_property_hash_reference_done");
    emitter.instruction("mov rsp, rbp");                                        // release the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the cell or 0
}
