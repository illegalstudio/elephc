//! Purpose:
//! Emits `__rt_hash_from_name_list`, the runtime loop that materializes a PHP
//! `array<string,string>` (key === value) assoc hash from a resolved
//! `{name_ptr, name_len}` list — the runtime counterpart of the literal
//! `class_implements()`/`class_parents()`/`class_uses()` fold's compile-time
//! unrolled hash construction, driven by a count only known at runtime.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::class_relations`, after
//!   `__rt_class_relation_lookup` resolves a `(list_ptr, list_count)` pair.
//!
//! Key details:
//! - Input:  x0/rdi=list_ptr, x1/rsi=list_count.
//! - Output: x0/rax = the built hash pointer (empty hash when `list_count == 0`).
//! - Per-entry sequence mirrors
//!   `crate::codegen::lower_inst::builtins::class_relations::emit_string_hash_entries_*`
//!   exactly (`__rt_hash_normalize_key` for the key, `__rt_str_persist` for an
//!   owned value copy, `__rt_hash_set` to insert), just driven by a runtime
//!   loop instead of unrolled per compile-time-known name.
//! - None of the called runtime helpers preserve caller registers beyond the
//!   frame pointer/return address, so every value that must survive a nested
//!   call is spilled to a fixed stack slot before the call and reloaded after.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Runtime tag for string hash values (matches `MIXED_TAG_STRING` in `const_registry.rs`).
const STR_TAG: i64 = 1;

/// Emits the target-specific `__rt_hash_from_name_list` helper.
pub fn emit_rt_hash_from_name_list(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 implementation of `__rt_hash_from_name_list`.
///
/// Spill layout (sp-relative, 80-byte frame): #16 cursor_ptr, #24 remaining_count,
/// #32 hash_ptr, #40 raw_name_ptr, #48 raw_name_len, #56 norm_key_ptr, #64 norm_key_len.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_from_name_list ---");
    emitter.label_global("__rt_hash_from_name_list");

    emitter.instruction("stp x29, x30, [sp, #-80]!");                           // save frame pointer/return address, reserve 7 spill slots
    emitter.instruction("mov x29, sp");                                         // establish the new frame pointer

    // -- allocate the result hash: capacity = max(list_count * 2, 16) --
    emitter.instruction("lsl x9, x1, #1");                                      // x9 = list_count * 2
    emitter.instruction("mov x10, #16");                                        // x10 = the minimum hash capacity
    emitter.instruction("cmp x9, x10");                                         // is list_count * 2 below the minimum capacity?
    emitter.instruction("csel x9, x10, x9, lo");                                // x9 = max(list_count * 2, 16)
    emitter.instruction("str x0, [sp, #16]");                                   // cursor_ptr = list_ptr
    emitter.instruction("str x1, [sp, #24]");                                   // remaining_count = list_count
    emitter.instruction("mov x0, x9");                                          // capacity argument for __rt_hash_new
    emitter.instruction(&format!("mov x1, #{}", STR_TAG));                      // value_tag = string, for __rt_hash_new
    emitter.instruction("bl __rt_hash_new");                                    // x0 = new empty hash
    emitter.instruction("str x0, [sp, #32]");                                   // hash_ptr = the new empty hash

    emitter.instruction("b __rt_hash_from_name_list_check");                    // enter the loop at the remaining-count test

    emitter.label("__rt_hash_from_name_list_top");
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload cursor_ptr
    emitter.instruction("ldr x3, [x1]");                                        // raw name_ptr for this entry
    emitter.instruction("ldr x4, [x1, #8]");                                    // raw name_len for this entry
    emitter.instruction("add x1, x1, #16");                                     // advance the cursor to the next entry
    emitter.instruction("str x1, [sp, #16]");                                   // save the advanced cursor
    emitter.instruction("ldr x2, [sp, #24]");                                   // reload remaining_count
    emitter.instruction("sub x2, x2, #1");                                      // one fewer entry left to insert
    emitter.instruction("str x2, [sp, #24]");                                   // save the decremented remaining count
    emitter.instruction("str x3, [sp, #40]");                                   // stash raw_name_ptr across the normalize call
    emitter.instruction("str x4, [sp, #48]");                                   // stash raw_name_len across the normalize call

    emitter.instruction("mov x1, x3");                                          // key ptr argument for __rt_hash_normalize_key
    emitter.instruction("mov x2, x4");                                          // key len argument for __rt_hash_normalize_key
    emitter.instruction("bl __rt_hash_normalize_key");                          // x1/x2 = normalized key ptr/len
    emitter.instruction("str x1, [sp, #56]");                                   // stash the normalized key ptr across the persist call
    emitter.instruction("str x2, [sp, #64]");                                   // stash the normalized key len across the persist call

    emitter.instruction("ldr x1, [sp, #40]");                                   // reload the raw name ptr (the value source)
    emitter.instruction("ldr x2, [sp, #48]");                                   // reload the raw name len
    emitter.instruction("bl __rt_str_persist");                                 // x1/x2 = owned persisted value ptr/len
    emitter.instruction("mov x3, x1");                                          // value_lo = the owned value pointer
    emitter.instruction("mov x4, x2");                                          // value_hi = the owned value length
    emitter.instruction("ldr x1, [sp, #56]");                                   // reload the normalized key ptr
    emitter.instruction("ldr x2, [sp, #64]");                                   // reload the normalized key len
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the current hash pointer
    emitter.instruction(&format!("mov x5, #{}", STR_TAG));                      // value_tag = string
    emitter.instruction("bl __rt_hash_set");                                    // x0 = the (possibly grown) updated hash
    emitter.instruction("str x0, [sp, #32]");                                   // save the updated hash pointer

    emitter.label("__rt_hash_from_name_list_check");
    emitter.instruction("ldr x2, [sp, #24]");                                   // reload remaining_count
    emitter.instruction("cbnz x2, __rt_hash_from_name_list_top");               // insert the next entry while entries remain

    emitter.instruction("ldr x0, [sp, #32]");                                   // the finished hash pointer is the return value
    emitter.instruction("ldp x29, x30, [sp], #80");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return the built hash in x0
}

/// Emits the x86_64 implementation of `__rt_hash_from_name_list`.
///
/// Spill layout (rbp-relative, 80-byte frame): -8 cursor_ptr, -16 remaining_count,
/// -24 hash_ptr, -32 raw_name_ptr, -40 raw_name_len, -48 norm_key_ptr, -56 norm_key_len.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_from_name_list ---");
    emitter.label_global("__rt_hash_from_name_list");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the new frame pointer
    emitter.instruction("sub rsp, 80");                                         // reserve 7 spill slots (16-byte aligned)

    // -- allocate the result hash: capacity = max(list_count * 2, 16) --
    emitter.instruction("mov rax, rsi");                                        // rax = list_count
    emitter.instruction("shl rax, 1");                                          // rax = list_count * 2
    emitter.instruction("cmp rax, 16");                                         // is list_count * 2 below the minimum capacity?
    emitter.instruction("jae __rt_hash_from_name_list_cap_ok");                 // keep list_count * 2 when it already meets the minimum
    emitter.instruction("mov rax, 16");                                         // otherwise clamp the capacity to the minimum

    emitter.label("__rt_hash_from_name_list_cap_ok");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // cursor_ptr = list_ptr
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // remaining_count = list_count
    emitter.instruction("mov rdi, rax");                                        // capacity argument for __rt_hash_new
    emitter.instruction(&format!("mov rsi, {}", STR_TAG));                      // value_tag = string, for __rt_hash_new
    emitter.instruction("call __rt_hash_new");                                  // rax = new empty hash
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // hash_ptr = the new empty hash

    emitter.instruction("jmp __rt_hash_from_name_list_check");                  // enter the loop at the remaining-count test

    emitter.label("__rt_hash_from_name_list_top");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");                        // reload cursor_ptr
    emitter.instruction("mov rax, QWORD PTR [rdx]");                            // raw name_ptr for this entry
    emitter.instruction("mov r8, QWORD PTR [rdx + 8]");                         // raw name_len for this entry
    emitter.instruction("add rdx, 16");                                         // advance the cursor to the next entry
    emitter.instruction("mov QWORD PTR [rbp - 8], rdx");                        // save the advanced cursor
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload remaining_count
    emitter.instruction("dec rcx");                                             // one fewer entry left to insert
    emitter.instruction("mov QWORD PTR [rbp - 16], rcx");                       // save the decremented remaining count
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // stash raw_name_ptr across the normalize call
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // stash raw_name_len across the normalize call

    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // key ptr argument for __rt_hash_normalize_key
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // key len argument for __rt_hash_normalize_key
    emitter.instruction("call __rt_hash_normalize_key");                        // rax/rdx = normalized key ptr/len
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // stash the normalized key ptr across the persist call
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // stash the normalized key len across the persist call

    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the raw name ptr (the value source)
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // reload the raw name len
    emitter.instruction("call __rt_str_persist");                               // rax/rdx = owned persisted value ptr/len
    emitter.instruction("mov rcx, rax");                                        // value ptr argument for __rt_hash_set
    emitter.instruction("mov r8, rdx");                                         // value len argument for __rt_hash_set
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // hash_ptr argument for __rt_hash_set
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // normalized key ptr argument for __rt_hash_set
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // normalized key len argument for __rt_hash_set
    emitter.instruction(&format!("mov r9, {}", STR_TAG));                       // value_tag = string
    emitter.instruction("call __rt_hash_set");                                  // rax = the (possibly grown) updated hash
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the updated hash pointer

    emitter.label("__rt_hash_from_name_list_check");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload remaining_count
    emitter.instruction("test rcx, rcx");                                       // any entries left to insert?
    emitter.instruction("jnz __rt_hash_from_name_list_top");                    // insert the next entry while entries remain

    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // the finished hash pointer is the return value
    emitter.instruction("add rsp, 80");                                         // drop the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the built hash in rax
}

