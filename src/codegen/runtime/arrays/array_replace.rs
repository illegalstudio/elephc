//! Purpose:
//! Emits the `__rt_array_replace` runtime helper assembly for array_replace.
//! Clones the first associative array, then overwrites/appends every entry of the second (right-wins).
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Operates on hash tables; heap-backed and string values are retained for the cloned result before insertion.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_replace: replace/append entries of hash1 with entries of hash2 (later wins).
/// Input:  x0 = first hash pointer, x1 = second hash pointer
/// Output: x0 = new owned hash pointer (clone of hash1 with hash2 entries inserted)
///
/// hash1 is shallow-cloned (keys re-persisted, child values retained), then every
/// entry of hash2 is inserted via `__rt_hash_set`, which overwrites matching keys in
/// place (preserving their position) and appends new keys. Heap and string values from
/// hash2 are retained for the new owner before insertion.
pub fn emit_array_replace(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_replace_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_replace ---");
    emitter.label_global("__rt_array_replace");
    emitter.instruction("sub sp, sp, #80");                                     // allocate the array_replace stack frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set up the new frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the second hash pointer
    emitter.instruction("bl __rt_hash_clone_shallow");                          // clone hash1 into an owned result hash, x0 = result
    emitter.instruction("str x0, [sp, #8]");                                    // save the cloned result hash pointer
    emitter.instruction("str xzr, [sp, #16]");                                  // iterator cursor = 0 (start from hash2 head)
    emitter.label("__rt_array_replace_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // x0 = hash2 pointer
    emitter.instruction("ldr x1, [sp, #16]");                                   // x1 = current iterator cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // next hash2 entry: x0=cursor,x1=kptr,x2=klen,x3=vlo,x4=vhi,x5=vtag
    emitter.instruction("cmn x0, #1");                                          // has iteration reached the end (cursor == -1)?
    emitter.instruction("b.eq __rt_array_replace_done");                        // stop once every hash2 entry has been inserted
    emitter.instruction("str x0, [sp, #16]");                                   // save the next iterator cursor
    emitter.instruction("str x1, [sp, #24]");                                   // save key pointer
    emitter.instruction("str x2, [sp, #32]");                                   // save key length
    emitter.instruction("str x3, [sp, #40]");                                   // save value low word
    emitter.instruction("str x4, [sp, #48]");                                   // save value high word
    emitter.instruction("str x5, [sp, #56]");                                   // save value runtime tag
    emitter.instruction("cmp x5, #1");                                          // is the value a string?
    emitter.instruction("b.eq __rt_array_replace_persist");                     // strings are persisted as an independent copy
    emitter.instruction("cmp x5, #4");                                          // is the value below the heap-backed tag range?
    emitter.instruction("b.lt __rt_array_replace_insert");                      // scalar values need no retain
    emitter.instruction("cmp x5, #7");                                          // is the value above the heap-backed tag range?
    emitter.instruction("b.gt __rt_array_replace_insert");                      // non-heap tags need no retain
    emitter.instruction("ldr x0, [sp, #40]");                                   // load the heap-backed value pointer from the saved value low word
    emitter.instruction("bl __rt_incref");                                      // retain the heap-backed value for the result hash owner
    emitter.instruction("b __rt_array_replace_insert");                         // continue to the insertion
    emitter.label("__rt_array_replace_persist");
    emitter.instruction("ldr x1, [sp, #40]");                                   // string pointer to persist
    emitter.instruction("ldr x2, [sp, #48]");                                   // string length to persist
    emitter.instruction("bl __rt_str_persist");                                 // copy the string into an independent heap block, x1 = new pointer
    emitter.instruction("str x1, [sp, #40]");                                   // store the persisted string pointer
    emitter.instruction("str x2, [sp, #48]");                                   // store the persisted string length
    emitter.label("__rt_array_replace_insert");
    emitter.instruction("ldr x0, [sp, #8]");                                    // x0 = result hash pointer
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload key pointer
    emitter.instruction("ldr x2, [sp, #32]");                                   // reload key length
    emitter.instruction("ldr x3, [sp, #40]");                                   // reload value low word
    emitter.instruction("ldr x4, [sp, #48]");                                   // reload value high word
    emitter.instruction("ldr x5, [sp, #56]");                                   // reload value runtime tag
    emitter.instruction("bl __rt_hash_set");                                    // overwrite or append the entry into the result hash
    emitter.instruction("str x0, [sp, #8]");                                    // update the result pointer after possible reallocation
    emitter.instruction("b __rt_array_replace_loop");                           // continue with the next hash2 entry
    emitter.label("__rt_array_replace_done");
    emitter.instruction("ldr x0, [sp, #8]");                                    // x0 = result hash pointer
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate the stack frame
    emitter.instruction("ret");                                                 // return the result hash in x0
}

/// x86_64 Linux implementation of `__rt_array_replace`.
/// Input:  rdi = first hash pointer, rsi = second hash pointer
/// Output: rax = new owned hash pointer (clone of hash1 with hash2 entries inserted)
fn emit_array_replace_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_replace ---");
    emitter.label_global("__rt_array_replace");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 64");                                         // reserve local spill slots for the replace loop state
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the second hash pointer
    emitter.instruction("call __rt_hash_clone_shallow");                        // clone hash1 into an owned result hash, rax = result
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the cloned result hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // iterator cursor = 0 (start from hash2 head)
    emitter.label("__rt_array_replace_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // rdi = hash2 pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // rsi = current iterator cursor
    emitter.instruction("call __rt_hash_iter_next");                            // next hash2 entry: rax=cursor,rdi=kptr,rdx=klen,rcx=vlo,r8=vhi,r9=vtag
    emitter.instruction("cmp rax, -1");                                         // has iteration reached the end?
    emitter.instruction("je __rt_array_replace_done");                          // stop once every hash2 entry has been inserted
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the next iterator cursor
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // save key pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // save key length
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // save value low word
    emitter.instruction("mov QWORD PTR [rbp - 56], r8");                        // save value high word
    emitter.instruction("mov QWORD PTR [rbp - 64], r9");                        // save value runtime tag
    emitter.instruction("cmp r9, 1");                                           // is the value a string?
    emitter.instruction("je __rt_array_replace_persist");                       // strings are persisted as an independent copy
    emitter.instruction("cmp r9, 4");                                           // is the value below the heap-backed tag range?
    emitter.instruction("jl __rt_array_replace_insert");                        // scalar values need no retain
    emitter.instruction("cmp r9, 7");                                           // is the value above the heap-backed tag range?
    emitter.instruction("jg __rt_array_replace_insert");                        // non-heap tags need no retain
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // load the heap-backed value pointer from the saved value low word
    emitter.instruction("call __rt_incref");                                    // retain the heap-backed value for the result hash owner
    emitter.instruction("jmp __rt_array_replace_insert");                       // continue to the insertion
    emitter.label("__rt_array_replace_persist");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // string pointer to persist
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // string length to persist
    emitter.instruction("call __rt_str_persist");                               // copy the string into an independent heap block, rax = new pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // store the persisted string pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // store the persisted string length
    emitter.label("__rt_array_replace_insert");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // rdi = result hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload key pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // reload key length
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload value low word
    emitter.instruction("mov r8, QWORD PTR [rbp - 56]");                        // reload value high word
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload value runtime tag
    emitter.instruction("call __rt_hash_set");                                  // overwrite or append the entry into the result hash
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // update the result pointer after possible reallocation
    emitter.instruction("jmp __rt_array_replace_loop");                         // continue with the next hash2 entry
    emitter.label("__rt_array_replace_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // rax = result hash pointer
    emitter.instruction("add rsp, 64");                                         // release the local spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the result hash in rax
}

