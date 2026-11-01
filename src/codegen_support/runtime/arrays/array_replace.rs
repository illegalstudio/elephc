//! Purpose:
//! Emits the `__rt_array_replace` runtime helper assembly for array_replace.
//! Clones the first associative array, then overwrites/appends every entry of the second (right-wins).
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Operates on hash tables; heap-backed and string values are retained for the cloned result before insertion.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// array_replace: replace/append entries of hash1 with entries of hash2 (later wins).
/// Input:  x0 = first hash pointer, x1 = second hash pointer
/// Output: x0 = new owned hash pointer (clone of hash1 with hash2 entries inserted)
///
/// hash1 is shallow-cloned (keys re-persisted, child values retained), then every
/// entry of hash2 is inserted via `__rt_hash_set_value`, which overwrites matching keys in
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
    emitter.instruction("sub sp, sp, #96");                                     // allocate the array_replace stack frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // set up the new frame pointer
    emitter.instruction("str x1, [sp, #72]");                                   // save the second hash pointer for merge mode's second pass
    emitter.instruction("str x2, [sp, #64]");                                   // save replace-vs-merge mode across helper calls
    emitter.instruction("cbnz x2, __rt_array_replace_merge_init");              // merge mode rebuilds both inputs to renumber integer keys
    emitter.instruction("str x1, [sp, #0]");                                    // replace mode iterates only the second hash
    emitter.instruction("bl __rt_hash_clone_shallow");                          // clone hash1 into an owned result hash, x0 = result
    emitter.instruction("b __rt_array_replace_init_done");                      // skip merge-mode empty result allocation
    emitter.label("__rt_array_replace_merge_init");
    emitter.instruction("str x0, [sp, #0]");                                    // merge mode starts by iterating the first hash
    emitter.instruction("mov x0, #8");                                          // initial result hash capacity
    emitter.instruction("mov x1, #7");                                          // mixed-valued hash storage
    emitter.instruction("bl __rt_hash_new");                                    // allocate the empty renumbering destination
    emitter.label("__rt_array_replace_init_done");
    emitter.instruction("str x0, [sp, #8]");                                    // save the cloned result hash pointer
    emitter.instruction("str xzr, [sp, #16]");                                  // iterator cursor = 0 (start from hash2 head)
    emitter.label("__rt_array_replace_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // x0 = hash2 pointer
    emitter.instruction("ldr x1, [sp, #16]");                                   // x1 = current iterator cursor
    emitter.instruction("bl __rt_hash_iter_next_value");                        // next hash2 entry: x0=cursor,x1=kptr,x2=klen,x3=vlo,x4=vhi,x5=vtag
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
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload replace-vs-merge mode
    emitter.instruction("cbz x9, __rt_array_replace_set");                      // replace mode preserves integer keys
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the normalized key length
    emitter.instruction("cmn x9, #1");                                          // integer keys use the -1 sentinel
    emitter.instruction("b.ne __rt_array_replace_set");                         // string keys overwrite in both modes
    emitter.instruction("ldr x0, [sp, #8]");                                    // result hash pointer for automatic integer append
    emitter.instruction("ldr x1, [sp, #40]");                                   // value low word for hash_append
    emitter.instruction("ldr x2, [sp, #48]");                                   // value high word for hash_append
    emitter.instruction("ldr x3, [sp, #56]");                                   // value runtime tag for hash_append
    emitter.instruction("bl __rt_hash_append");                                 // array_merge renumbers and appends integer keys
    emitter.instruction("str x0, [sp, #8]");                                    // update result pointer after possible growth
    emitter.instruction("b __rt_array_replace_loop");                           // continue with the next source entry
    emitter.label("__rt_array_replace_set");
    emitter.instruction("ldr x0, [sp, #8]");                                    // x0 = result hash pointer
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload key pointer
    emitter.instruction("ldr x2, [sp, #32]");                                   // reload key length
    emitter.instruction("ldr x3, [sp, #40]");                                   // reload value low word
    emitter.instruction("ldr x4, [sp, #48]");                                   // reload value high word
    emitter.instruction("ldr x5, [sp, #56]");                                   // reload value runtime tag
    emitter.instruction("bl __rt_hash_set_value");                              // overwrite or append the entry into the result hash
    emitter.instruction("str x0, [sp, #8]");                                    // update the result pointer after possible reallocation
    emitter.instruction("b __rt_array_replace_loop");                           // continue with the next hash2 entry
    emitter.label("__rt_array_replace_done");
    emitter.instruction("ldr x9, [sp, #64]");                                   // inspect merge pass state
    emitter.instruction("cmp x9, #1");                                          // first merge pass has just exhausted hash1
    emitter.instruction("b.ne __rt_array_replace_final");                       // replace mode or second merge pass is complete
    emitter.instruction("mov x9, #2");                                          // mark the second merge pass active
    emitter.instruction("str x9, [sp, #64]");                                   // preserve the updated pass state
    emitter.instruction("ldr x9, [sp, #72]");                                   // load hash2 as the next iteration source
    emitter.instruction("str x9, [sp, #0]");                                    // publish hash2 to the loop
    emitter.instruction("str xzr, [sp, #16]");                                  // restart iteration from hash2's head
    emitter.instruction("b __rt_array_replace_loop");                           // merge the second input into the same result
    emitter.label("__rt_array_replace_final");
    emitter.instruction("ldr x0, [sp, #8]");                                    // x0 = result hash pointer
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // deallocate the stack frame
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
    emitter.instruction("sub rsp, 80");                                         // reserve local spill slots for the replace loop state
    emitter.instruction("mov QWORD PTR [rbp - 80], rsi");                       // save the second hash pointer for merge mode's second pass
    emitter.instruction("mov QWORD PTR [rbp - 72], rdx");                       // save replace-vs-merge mode across helper calls
    emitter.instruction("test rdx, rdx");                                       // merge mode rebuilds both inputs to renumber integer keys
    emitter.instruction("jnz __rt_array_replace_x86_merge_init");               // branch to empty result allocation for merge mode
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // replace mode iterates only the second hash
    emitter.instruction("call __rt_hash_clone_shallow");                        // clone hash1 into an owned result hash, rax = result
    emitter.instruction("jmp __rt_array_replace_x86_init_done");                // skip merge-mode empty result allocation
    emitter.label("__rt_array_replace_x86_merge_init");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // merge mode starts by iterating the first hash
    emitter.instruction("mov rdi, 8");                                          // initial result hash capacity
    emitter.instruction("mov rsi, 7");                                          // mixed-valued hash storage
    emitter.instruction("call __rt_hash_new");                                  // allocate the empty renumbering destination
    emitter.label("__rt_array_replace_x86_init_done");
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the cloned result hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // iterator cursor = 0 (start from hash2 head)
    emitter.label("__rt_array_replace_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // rdi = hash2 pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // rsi = current iterator cursor
    emitter.instruction("call __rt_hash_iter_next_value");                      // next hash2 entry: rax=cursor,rdi=kptr,rdx=klen,rcx=vlo,r8=vhi,r9=vtag
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
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // load the heap-backed value pointer into the incref input register
    emitter.instruction("call __rt_incref");                                    // retain the heap-backed value for the result hash owner
    emitter.instruction("jmp __rt_array_replace_insert");                       // continue to the insertion
    emitter.label("__rt_array_replace_persist");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // string pointer to persist
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // string length to persist
    emitter.instruction("call __rt_str_persist");                               // copy the string into an independent heap block, rax = new pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // store the persisted string pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // store the persisted string length
    emitter.label("__rt_array_replace_insert");
    emitter.instruction("cmp QWORD PTR [rbp - 72], 0");                         // check replace-vs-merge mode
    emitter.instruction("je __rt_array_replace_x86_set");                       // replace mode preserves integer keys
    emitter.instruction("cmp QWORD PTR [rbp - 40], -1");                        // integer keys use the -1 length sentinel
    emitter.instruction("jne __rt_array_replace_x86_set");                      // string keys overwrite in both modes
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // result hash pointer for automatic integer append
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // value low word for hash_append
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // value high word for hash_append
    emitter.instruction("mov rcx, QWORD PTR [rbp - 64]");                       // value runtime tag for hash_append
    emitter.instruction("call __rt_hash_append");                               // array_merge renumbers and appends integer keys
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // update result pointer after possible growth
    emitter.instruction("jmp __rt_array_replace_loop");                         // continue with the next source entry
    emitter.label("__rt_array_replace_x86_set");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // rdi = result hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload key pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // reload key length
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload value low word
    emitter.instruction("mov r8, QWORD PTR [rbp - 56]");                        // reload value high word
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload value runtime tag
    emitter.instruction("call __rt_hash_set_value");                            // overwrite or append the entry into the result hash
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // update the result pointer after possible reallocation
    emitter.instruction("jmp __rt_array_replace_loop");                         // continue with the next hash2 entry
    emitter.label("__rt_array_replace_done");
    emitter.instruction("cmp QWORD PTR [rbp - 72], 1");                         // first merge pass has just exhausted hash1
    emitter.instruction("jne __rt_array_replace_x86_final");                    // replace mode or second merge pass is complete
    emitter.instruction("mov QWORD PTR [rbp - 72], 2");                         // mark the second merge pass active
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // load hash2 as the next iteration source
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // publish hash2 to the loop
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // restart iteration from hash2's head
    emitter.instruction("jmp __rt_array_replace_loop");                         // merge the second input into the same result
    emitter.label("__rt_array_replace_x86_final");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // rax = result hash pointer
    emitter.instruction("add rsp, 80");                                         // release the local spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the result hash in rax
}
