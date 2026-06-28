//! Purpose:
//! Emits the `__rt_array_replace_recursive` runtime helper for array_replace_recursive.
//! Recursively merges hash2 into a clone of hash1, recursing when both values at a key are associative arrays.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Self-recursive over nested associative arrays; `__rt_hash_set` releases overwritten values, keeping refcounts balanced.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_replace_recursive: deep right-wins merge of two associative arrays.
/// Input:  x0 = hash1 pointer, x1 = hash2 pointer
/// Output: x0 = new owned hash pointer
///
/// Clones hash1, then for every hash2 entry: if the key exists in hash1 and both values
/// are associative arrays (tag 5), recurses and stores the merged sub-array; otherwise the
/// hash2 value overwrites/appends (right-wins). `__rt_hash_set` releases the previous value
/// on overwrite, so the recursively cloned children stay refcount-balanced.
pub fn emit_array_replace_recursive(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_replace_recursive_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_replace_recursive ---");
    emitter.label_global("__rt_array_replace_recursive");
    emitter.instruction("sub sp, sp, #112");                                    // allocate the recursive-replace stack frame
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // set up the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save hash1 pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save hash2 pointer
    emitter.instruction("bl __rt_hash_clone_shallow");                          // clone hash1 into an owned result hash, x0 = result
    emitter.instruction("str x0, [sp, #16]");                                   // save the result hash pointer
    emitter.instruction("str xzr, [sp, #24]");                                  // iterator cursor = 0 (start from hash2 head)
    emitter.label("__rt_array_replace_recursive_loop");
    emitter.instruction("ldr x0, [sp, #8]");                                    // x0 = hash2 pointer
    emitter.instruction("ldr x1, [sp, #24]");                                   // x1 = current iterator cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // next hash2 entry: x0=cursor,x1=kptr,x2=klen,x3=vlo,x4=vhi,x5=vtag
    emitter.instruction("cmn x0, #1");                                          // has iteration reached the end (cursor == -1)?
    emitter.instruction("b.eq __rt_array_replace_recursive_done");              // stop once every hash2 entry is merged
    emitter.instruction("str x0, [sp, #24]");                                   // save the next iterator cursor
    emitter.instruction("str x1, [sp, #32]");                                   // save key pointer
    emitter.instruction("str x2, [sp, #40]");                                   // save key length
    emitter.instruction("str x3, [sp, #48]");                                   // save hash2 value low word
    emitter.instruction("str x4, [sp, #56]");                                   // save hash2 value high word
    emitter.instruction("str x5, [sp, #64]");                                   // save hash2 value runtime tag
    emitter.instruction("cmp x5, #5");                                          // is the hash2 value an associative array?
    emitter.instruction("b.ne __rt_array_replace_recursive_over");              // non-array values overwrite directly
    emitter.instruction("ldr x0, [sp, #0]");                                    // x0 = hash1 pointer
    emitter.instruction("ldr x1, [sp, #32]");                                   // x1 = key low word
    emitter.instruction("ldr x2, [sp, #40]");                                   // x2 = key high word (-1 marks an integer key)
    emitter.instruction("bl __rt_hash_get");                                    // look up the key in hash1: x0=found,x1=vlo,x2=vhi,x3=vtag
    emitter.instruction("cbz x0, __rt_array_replace_recursive_over");           // absent in hash1 means append, not recurse
    emitter.instruction("cmp x3, #5");                                          // is the hash1 value also an associative array?
    emitter.instruction("b.ne __rt_array_replace_recursive_over");              // only recurse when both values are arrays
    emitter.instruction("str x1, [sp, #72]");                                   // save the hash1 nested array pointer
    emitter.instruction("ldr x0, [sp, #72]");                                   // x0 = hash1 nested array (recursion arg1)
    emitter.instruction("ldr x1, [sp, #48]");                                   // x1 = hash2 nested array (recursion arg2)
    emitter.instruction("bl __rt_array_replace_recursive");                     // recurse into the nested arrays, x0 = merged sub-array
    emitter.instruction("mov x3, x0");                                          // merged sub-array becomes the new value low word
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = result hash pointer
    emitter.instruction("ldr x1, [sp, #32]");                                   // reload key pointer
    emitter.instruction("ldr x2, [sp, #40]");                                   // reload key length
    emitter.instruction("mov x4, #0");                                          // array values use no high word
    emitter.instruction("mov x5, #5");                                          // value tag 5 = associative array
    emitter.instruction("bl __rt_hash_set");                                    // store the merged sub-array (releases the previous value)
    emitter.instruction("str x0, [sp, #16]");                                   // update the result pointer after possible reallocation
    emitter.instruction("b __rt_array_replace_recursive_loop");                 // continue with the next hash2 entry
    emitter.label("__rt_array_replace_recursive_over");
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload the hash2 value runtime tag
    emitter.instruction("cmp x9, #1");                                          // is the value a string?
    emitter.instruction("b.eq __rt_array_replace_recursive_persist");           // strings are persisted as an independent copy
    emitter.instruction("cmp x9, #4");                                          // is the value below the heap-backed tag range?
    emitter.instruction("b.lt __rt_array_replace_recursive_insert");            // scalar values need no retain
    emitter.instruction("cmp x9, #7");                                          // is the value above the heap-backed tag range?
    emitter.instruction("b.gt __rt_array_replace_recursive_insert");            // non-heap tags need no retain
    emitter.instruction("ldr x0, [sp, #48]");                                   // load the heap-backed value low word
    emitter.instruction("bl __rt_incref");                                      // retain the heap-backed value for the result hash owner
    emitter.instruction("b __rt_array_replace_recursive_insert");               // continue to the insertion
    emitter.label("__rt_array_replace_recursive_persist");
    emitter.instruction("ldr x1, [sp, #48]");                                   // string pointer to persist
    emitter.instruction("ldr x2, [sp, #56]");                                   // string length to persist
    emitter.instruction("bl __rt_str_persist");                                 // copy the string into an independent heap block, x1 = new pointer
    emitter.instruction("str x1, [sp, #48]");                                   // store the persisted string pointer
    emitter.instruction("str x2, [sp, #56]");                                   // store the persisted string length
    emitter.label("__rt_array_replace_recursive_insert");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = result hash pointer
    emitter.instruction("ldr x1, [sp, #32]");                                   // reload key pointer
    emitter.instruction("ldr x2, [sp, #40]");                                   // reload key length
    emitter.instruction("ldr x3, [sp, #48]");                                   // reload value low word
    emitter.instruction("ldr x4, [sp, #56]");                                   // reload value high word
    emitter.instruction("ldr x5, [sp, #64]");                                   // reload value runtime tag
    emitter.instruction("bl __rt_hash_set");                                    // overwrite or append the value into the result hash
    emitter.instruction("str x0, [sp, #16]");                                   // update the result pointer after possible reallocation
    emitter.instruction("b __rt_array_replace_recursive_loop");                 // continue with the next hash2 entry
    emitter.label("__rt_array_replace_recursive_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = result hash pointer
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // deallocate the stack frame
    emitter.instruction("ret");                                                 // return the result hash in x0
}

/// x86_64 Linux implementation of `__rt_array_replace_recursive`.
/// Input:  rdi = hash1 pointer, rsi = hash2 pointer
/// Output: rax = new owned hash pointer
fn emit_array_replace_recursive_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_replace_recursive ---");
    emitter.label_global("__rt_array_replace_recursive");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 96");                                         // reserve local spill slots for the recursive merge state
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save hash1 pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save hash2 pointer
    emitter.instruction("call __rt_hash_clone_shallow");                        // clone hash1 into an owned result hash, rax = result
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the result hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // iterator cursor = 0 (start from hash2 head)
    emitter.label("__rt_array_replace_recursive_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // rdi = hash2 pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // rsi = current iterator cursor
    emitter.instruction("call __rt_hash_iter_next");                            // next hash2 entry: rax=cursor,rdi=kptr,rdx=klen,rcx=vlo,r8=vhi,r9=vtag
    emitter.instruction("cmp rax, -1");                                         // has iteration reached the end?
    emitter.instruction("je __rt_array_replace_recursive_done");                // stop once every hash2 entry is merged
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the next iterator cursor
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // save key pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // save key length
    emitter.instruction("mov QWORD PTR [rbp - 56], rcx");                       // save hash2 value low word
    emitter.instruction("mov QWORD PTR [rbp - 64], r8");                        // save hash2 value high word
    emitter.instruction("mov QWORD PTR [rbp - 72], r9");                        // save hash2 value runtime tag
    emitter.instruction("cmp r9, 5");                                           // is the hash2 value an associative array?
    emitter.instruction("jne __rt_array_replace_recursive_over");               // non-array values overwrite directly
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // rdi = hash1 pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // rsi = key low word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // rdx = key high word (-1 marks an integer key)
    emitter.instruction("call __rt_hash_get");                                  // look up the key in hash1: rax=found,rdi=vlo,rsi=vhi,rcx=vtag
    emitter.instruction("test rax, rax");                                       // was the key present in hash1?
    emitter.instruction("je __rt_array_replace_recursive_over");                // absent in hash1 means append, not recurse
    emitter.instruction("cmp rcx, 5");                                          // is the hash1 value also an associative array?
    emitter.instruction("jne __rt_array_replace_recursive_over");               // only recurse when both values are arrays
    emitter.instruction("mov QWORD PTR [rbp - 80], rdi");                       // save the hash1 nested array pointer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // rdi = hash1 nested array (recursion arg1)
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // rsi = hash2 nested array (recursion arg2)
    emitter.instruction("call __rt_array_replace_recursive");                   // recurse into the nested arrays, rax = merged sub-array
    emitter.instruction("mov rcx, rax");                                        // merged sub-array becomes the new value low word
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // rdi = result hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // reload key pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // reload key length
    emitter.instruction("xor r8, r8");                                          // array values use no high word
    emitter.instruction("mov r9, 5");                                           // value tag 5 = associative array
    emitter.instruction("call __rt_hash_set");                                  // store the merged sub-array (releases the previous value)
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // update the result pointer after possible reallocation
    emitter.instruction("jmp __rt_array_replace_recursive_loop");               // continue with the next hash2 entry
    emitter.label("__rt_array_replace_recursive_over");
    emitter.instruction("mov r10, QWORD PTR [rbp - 72]");                       // reload the hash2 value runtime tag
    emitter.instruction("cmp r10, 1");                                          // is the value a string?
    emitter.instruction("je __rt_array_replace_recursive_persist");             // strings are persisted as an independent copy
    emitter.instruction("cmp r10, 4");                                          // is the value below the heap-backed tag range?
    emitter.instruction("jl __rt_array_replace_recursive_insert");              // scalar values need no retain
    emitter.instruction("cmp r10, 7");                                          // is the value above the heap-backed tag range?
    emitter.instruction("jg __rt_array_replace_recursive_insert");              // non-heap tags need no retain
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // load the heap-backed value low word
    emitter.instruction("call __rt_incref");                                    // retain the heap-backed value for the result hash owner
    emitter.instruction("jmp __rt_array_replace_recursive_insert");             // continue to the insertion
    emitter.label("__rt_array_replace_recursive_persist");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // string pointer to persist
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // string length to persist
    emitter.instruction("call __rt_str_persist");                               // copy the string into an independent heap block, rax = new pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // store the persisted string pointer
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");                       // store the persisted string length
    emitter.label("__rt_array_replace_recursive_insert");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // rdi = result hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // reload key pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // reload key length
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // reload value low word
    emitter.instruction("mov r8, QWORD PTR [rbp - 64]");                        // reload value high word
    emitter.instruction("mov r9, QWORD PTR [rbp - 72]");                        // reload value runtime tag
    emitter.instruction("call __rt_hash_set");                                  // overwrite or append the value into the result hash
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // update the result pointer after possible reallocation
    emitter.instruction("jmp __rt_array_replace_recursive_loop");               // continue with the next hash2 entry
    emitter.label("__rt_array_replace_recursive_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // rax = result hash pointer
    emitter.instruction("add rsp, 96");                                         // release the local spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the result hash in rax
}

