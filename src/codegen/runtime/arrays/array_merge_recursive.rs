//! Purpose:
//! Emits the `__rt_amr_box_value` and `__rt_array_merge_recursive` runtime helpers for array_merge_recursive.
//! Merges two associative arrays, recursing on array-valued key collisions and combining scalar collisions into lists.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Integer keys append with renumbering; string-key collisions recurse (both assoc) or wrap-and-merge; temporaries are released.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// amr_box_value: wrap a single runtime value into a new one-element list hash {0: value}.
/// Input:  x0 = value tag, x1 = value low word, x2 = value high word
/// Output: x0 = new owned hash whose only entry is integer key 0 -> the (retained) value
pub fn emit_amr_box_value(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_amr_box_value_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: amr_box_value ---");
    emitter.label_global("__rt_amr_box_value");
    emitter.instruction("sub sp, sp, #48");                                     // allocate the box-value stack frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the value tag
    emitter.instruction("str x1, [sp, #8]");                                    // save the value low word
    emitter.instruction("str x2, [sp, #16]");                                   // save the value high word
    emitter.instruction("cmp x0, #1");                                          // is the value a string?
    emitter.instruction("b.eq __rt_amr_box_value_persist");                     // strings are persisted as an independent copy
    emitter.instruction("cmp x0, #4");                                          // is the value below the heap-backed tag range?
    emitter.instruction("b.lt __rt_amr_box_value_new");                         // scalar values need no retain
    emitter.instruction("cmp x0, #7");                                          // is the value above the heap-backed tag range?
    emitter.instruction("b.gt __rt_amr_box_value_new");                         // non-heap tags need no retain
    emitter.instruction("ldr x0, [sp, #8]");                                    // load the heap-backed value low word
    emitter.instruction("bl __rt_incref");                                      // retain the heap-backed value for the wrapper hash
    emitter.instruction("b __rt_amr_box_value_new");                            // continue to wrapper allocation
    emitter.label("__rt_amr_box_value_persist");
    emitter.instruction("ldr x1, [sp, #8]");                                    // string pointer to persist
    emitter.instruction("ldr x2, [sp, #16]");                                   // string length to persist
    emitter.instruction("bl __rt_str_persist");                                 // copy the string to an independent heap block, x1 = new pointer
    emitter.instruction("str x1, [sp, #8]");                                    // store the persisted string pointer as the wrapper value
    emitter.instruction("str x2, [sp, #16]");                                   // store the persisted string length
    emitter.label("__rt_amr_box_value_new");
    emitter.instruction("mov x0, #8");                                          // initial capacity for the wrapper hash
    emitter.instruction("mov x1, #7");                                          // value_type 7 = mixed
    emitter.instruction("bl __rt_hash_new");                                    // create the wrapper hash, x0 = wrapper
    emitter.instruction("mov x1, #0");                                          // integer key 0 for the single entry
    emitter.instruction("mov x2, #-1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("ldr x3, [sp, #8]");                                    // value low word
    emitter.instruction("ldr x4, [sp, #16]");                                   // value high word
    emitter.instruction("ldr x5, [sp, #0]");                                    // value runtime tag
    emitter.instruction("bl __rt_hash_set");                                    // insert the value at key 0, x0 = wrapper (maybe realloc)
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate the stack frame
    emitter.instruction("ret");                                                 // return the wrapper hash in x0
}

/// x86_64 Linux implementation of `__rt_amr_box_value`.
/// Input:  rdi = value tag, rsi = value low word, rdx = value high word
/// Output: rax = new owned one-element list hash {0: value}
fn emit_amr_box_value_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: amr_box_value ---");
    emitter.label_global("__rt_amr_box_value");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 32");                                         // reserve local slots for the value triple
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the value tag
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the value low word
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the value high word
    emitter.instruction("cmp rdi, 1");                                          // is the value a string?
    emitter.instruction("je __rt_amr_box_value_persist");                       // strings are persisted as an independent copy
    emitter.instruction("cmp rdi, 4");                                          // is the value below the heap-backed tag range?
    emitter.instruction("jl __rt_amr_box_value_new");                           // scalar values need no retain
    emitter.instruction("cmp rdi, 7");                                          // is the value above the heap-backed tag range?
    emitter.instruction("jg __rt_amr_box_value_new");                           // non-heap tags need no retain
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // load the heap-backed value low word
    emitter.instruction("call __rt_incref");                                    // retain the heap-backed value for the wrapper hash
    emitter.instruction("jmp __rt_amr_box_value_new");                          // continue to wrapper allocation
    emitter.label("__rt_amr_box_value_persist");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // string pointer to persist
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // string length to persist
    emitter.instruction("call __rt_str_persist");                               // copy the string to an independent heap block, rax = new pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // store the persisted string pointer as the wrapper value
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // store the persisted string length
    emitter.label("__rt_amr_box_value_new");
    emitter.instruction("mov rdi, 8");                                          // initial capacity for the wrapper hash
    emitter.instruction("mov rsi, 7");                                          // value_type 7 = mixed
    emitter.instruction("call __rt_hash_new");                                  // create the wrapper hash, rax = wrapper
    emitter.instruction("mov rdi, rax");                                        // wrapper hash pointer for hash_set
    emitter.instruction("mov rsi, 0");                                          // integer key 0 for the single entry
    emitter.instruction("mov rdx, -1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // value low word
    emitter.instruction("mov r8, QWORD PTR [rbp - 24]");                        // value high word
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // value runtime tag
    emitter.instruction("call __rt_hash_set");                                  // insert the value at key 0, rax = wrapper (maybe realloc)
    emitter.instruction("add rsp, 32");                                         // release the local slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the wrapper hash in rax
}

/// array_merge_recursive: PHP-style recursive merge of two associative arrays.
/// Input:  x0 = first hash, x1 = second hash
/// Output: x0 = new owned merged hash
///
/// Integer-keyed entries from both inputs append with sequential renumbering. String keys
/// that collide recurse when both values are associative arrays, otherwise each value is
/// wrapped to a list and merged (combining scalars). Kept values are retained; wrapper
/// temporaries are released. Nested indexed-array values are treated as opaque (wrapped).
pub fn emit_array_merge_recursive(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_merge_recursive_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_merge_recursive ---");
    emitter.label_global("__rt_array_merge_recursive");
    emitter.instruction("sub sp, sp, #192");                                    // allocate the merge-recursive stack frame
    emitter.instruction("stp x29, x30, [sp, #176]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #176");                                   // set up the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save first hash pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save second hash pointer
    emitter.instruction("mov x0, #16");                                         // initial capacity for the result hash
    emitter.instruction("mov x1, #7");                                          // value_type 7 = mixed
    emitter.instruction("bl __rt_hash_new");                                    // create the result hash, x0 = result
    emitter.instruction("str x0, [sp, #16]");                                   // save the result hash pointer
    emitter.instruction("str xzr, [sp, #24]");                                  // next integer key counter = 0
    emitter.instruction("str xzr, [sp, #32]");                                  // source selector which = 0
    emitter.label("__rt_amr_which_loop");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the source selector
    emitter.instruction("cmp x9, #2");                                          // have both sources been processed?
    emitter.instruction("b.ge __rt_amr_done");                                  // finish once both inputs are merged
    emitter.instruction("cbz x9, __rt_amr_pick_a");                             // selector 0 chooses the first input
    emitter.instruction("ldr x10, [sp, #8]");                                   // selector 1 chooses the second input
    emitter.instruction("b __rt_amr_pick_done");                                // store the chosen source
    emitter.label("__rt_amr_pick_a");
    emitter.instruction("ldr x10, [sp, #0]");                                   // load the first input as the current source
    emitter.label("__rt_amr_pick_done");
    emitter.instruction("str x10, [sp, #48]");                                  // save the current source pointer
    emitter.instruction("str xzr, [sp, #40]");                                  // iterator cursor = 0
    emitter.label("__rt_amr_entry_loop");
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload the current source pointer
    emitter.instruction("ldr x1, [sp, #40]");                                   // reload the iterator cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // next entry: x0=cursor,x1=kptr,x2=klen,x3=vlo,x4=vhi,x5=vtag
    emitter.instruction("cmn x0, #1");                                          // has iteration reached the end?
    emitter.instruction("b.eq __rt_amr_next_which");                            // advance to the next source when done
    emitter.instruction("str x0, [sp, #40]");                                   // save the next iterator cursor
    emitter.instruction("str x1, [sp, #56]");                                   // save key pointer
    emitter.instruction("str x2, [sp, #64]");                                   // save key length
    emitter.instruction("str x3, [sp, #72]");                                   // save value low word
    emitter.instruction("str x4, [sp, #80]");                                   // save value high word
    emitter.instruction("str x5, [sp, #88]");                                   // save value runtime tag
    emitter.instruction("cmn x2, #1");                                          // is this an integer key (key length == -1)?
    emitter.instruction("b.eq __rt_amr_int_key");                               // integer keys append with renumbering
    emitter.comment("-- string key: look it up in the result --");
    emitter.instruction("ldr x0, [sp, #16]");                                   // result hash pointer
    emitter.instruction("ldr x1, [sp, #56]");                                   // key pointer
    emitter.instruction("ldr x2, [sp, #64]");                                   // key length
    emitter.instruction("bl __rt_hash_get");                                    // look up the key: x0=found,x1=e_lo,x2=e_hi,x3=e_tag
    emitter.instruction("cbz x0, __rt_amr_str_new");                            // absent key is added directly
    emitter.instruction("str x1, [sp, #96]");                                   // save existing value low word
    emitter.instruction("str x2, [sp, #104]");                                  // save existing value high word
    emitter.instruction("str x3, [sp, #112]");                                  // save existing value runtime tag
    emitter.comment("-- build the existing operand (keep assoc arrays, wrap others) --");
    emitter.instruction("cmp x3, #5");                                          // is the existing value an associative array?
    emitter.instruction("b.ne __rt_amr_ea_wrap");                               // wrap non-assoc existing values into a list
    emitter.instruction("str x1, [sp, #120]");                                  // keep the associative array as the existing operand
    emitter.instruction("str xzr, [sp, #136]");                                 // existing operand is borrowed (not newly created)
    emitter.instruction("b __rt_amr_na");                                       // build the new operand next
    emitter.label("__rt_amr_ea_wrap");
    emitter.instruction("ldr x0, [sp, #112]");                                  // existing value tag
    emitter.instruction("ldr x1, [sp, #96]");                                   // existing value low word
    emitter.instruction("ldr x2, [sp, #104]");                                  // existing value high word
    emitter.instruction("bl __rt_amr_box_value");                               // wrap the existing value into a list, x0 = wrapper
    emitter.instruction("str x0, [sp, #120]");                                  // save the existing operand
    emitter.instruction("mov x9, #1");                                          // mark the existing operand as newly created
    emitter.instruction("str x9, [sp, #136]");                                  // save the existing-operand ownership flag
    emitter.label("__rt_amr_na");
    emitter.instruction("ldr x3, [sp, #88]");                                   // reload the new value tag
    emitter.instruction("cmp x3, #5");                                          // is the new value an associative array?
    emitter.instruction("b.ne __rt_amr_na_wrap");                               // wrap non-assoc new values into a list
    emitter.instruction("ldr x9, [sp, #72]");                                   // reload the new value low word
    emitter.instruction("str x9, [sp, #128]");                                  // keep the associative array as the new operand
    emitter.instruction("str xzr, [sp, #144]");                                 // new operand is borrowed (not newly created)
    emitter.instruction("b __rt_amr_merge");                                    // merge the two operands
    emitter.label("__rt_amr_na_wrap");
    emitter.instruction("ldr x0, [sp, #88]");                                   // new value tag
    emitter.instruction("ldr x1, [sp, #72]");                                   // new value low word
    emitter.instruction("ldr x2, [sp, #80]");                                   // new value high word
    emitter.instruction("bl __rt_amr_box_value");                               // wrap the new value into a list, x0 = wrapper
    emitter.instruction("str x0, [sp, #128]");                                  // save the new operand
    emitter.instruction("mov x9, #1");                                          // mark the new operand as newly created
    emitter.instruction("str x9, [sp, #144]");                                  // save the new-operand ownership flag
    emitter.label("__rt_amr_merge");
    emitter.instruction("ldr x0, [sp, #120]");                                  // existing operand
    emitter.instruction("ldr x1, [sp, #128]");                                  // new operand
    emitter.instruction("bl __rt_array_merge_recursive");                       // recursively merge the two operands, x0 = merged
    emitter.instruction("mov x3, x0");                                          // merged hash becomes the new value low word
    emitter.instruction("ldr x0, [sp, #16]");                                   // result hash pointer
    emitter.instruction("ldr x1, [sp, #56]");                                   // key pointer
    emitter.instruction("ldr x2, [sp, #64]");                                   // key length
    emitter.instruction("mov x4, #0");                                          // array values carry no high word
    emitter.instruction("mov x5, #5");                                          // value tag 5 = associative array
    emitter.instruction("bl __rt_hash_set");                                    // store the merged value (releases the previous value)
    emitter.instruction("str x0, [sp, #16]");                                   // update the result pointer after possible reallocation
    emitter.instruction("ldr x9, [sp, #136]");                                  // reload the existing-operand ownership flag
    emitter.instruction("cbz x9, __rt_amr_free_na");                            // skip releasing a borrowed existing operand
    emitter.instruction("ldr x0, [sp, #120]");                                  // load the newly created existing operand
    emitter.instruction("bl __rt_decref_hash");                                 // release the existing-operand wrapper
    emitter.label("__rt_amr_free_na");
    emitter.instruction("ldr x9, [sp, #144]");                                  // reload the new-operand ownership flag
    emitter.instruction("cbz x9, __rt_amr_entry_loop");                         // skip releasing a borrowed new operand
    emitter.instruction("ldr x0, [sp, #128]");                                  // load the newly created new operand
    emitter.instruction("bl __rt_decref_hash");                                 // release the new-operand wrapper
    emitter.instruction("b __rt_amr_entry_loop");                               // continue with the next entry
    emitter.label("__rt_amr_str_new");
    emitter.instruction("ldr x9, [sp, #88]");                                   // reload the value runtime tag
    emitter.instruction("cmp x9, #1");                                          // is the value a string?
    emitter.instruction("b.eq __rt_amr_str_new_persist");                       // strings are persisted as an independent copy
    emitter.instruction("cmp x9, #4");                                          // is the value below the heap-backed tag range?
    emitter.instruction("b.lt __rt_amr_str_new_set");                           // scalar values need no retain
    emitter.instruction("cmp x9, #7");                                          // is the value above the heap-backed tag range?
    emitter.instruction("b.gt __rt_amr_str_new_set");                           // non-heap tags need no retain
    emitter.instruction("ldr x0, [sp, #72]");                                   // load the heap-backed value low word
    emitter.instruction("bl __rt_incref");                                      // retain the heap-backed value for the result
    emitter.instruction("b __rt_amr_str_new_set");                              // continue to the insertion
    emitter.label("__rt_amr_str_new_persist");
    emitter.instruction("ldr x1, [sp, #72]");                                   // string pointer to persist
    emitter.instruction("ldr x2, [sp, #80]");                                   // string length to persist
    emitter.instruction("bl __rt_str_persist");                                 // copy the string into an independent heap block, x1 = new pointer
    emitter.instruction("str x1, [sp, #72]");                                   // store the persisted string pointer
    emitter.instruction("str x2, [sp, #80]");                                   // store the persisted string length
    emitter.label("__rt_amr_str_new_set");
    emitter.instruction("ldr x0, [sp, #16]");                                   // result hash pointer
    emitter.instruction("ldr x1, [sp, #56]");                                   // key pointer
    emitter.instruction("ldr x2, [sp, #64]");                                   // key length
    emitter.instruction("ldr x3, [sp, #72]");                                   // value low word
    emitter.instruction("ldr x4, [sp, #80]");                                   // value high word
    emitter.instruction("ldr x5, [sp, #88]");                                   // value runtime tag
    emitter.instruction("bl __rt_hash_set");                                    // insert the new string-keyed entry
    emitter.instruction("str x0, [sp, #16]");                                   // update the result pointer after possible reallocation
    emitter.instruction("b __rt_amr_entry_loop");                               // continue with the next entry
    emitter.label("__rt_amr_int_key");
    emitter.instruction("ldr x9, [sp, #88]");                                   // reload the value runtime tag
    emitter.instruction("cmp x9, #1");                                          // is the value a string?
    emitter.instruction("b.eq __rt_amr_int_persist");                           // strings are persisted as an independent copy
    emitter.instruction("cmp x9, #4");                                          // is the value below the heap-backed tag range?
    emitter.instruction("b.lt __rt_amr_int_set");                               // scalar values need no retain
    emitter.instruction("cmp x9, #7");                                          // is the value above the heap-backed tag range?
    emitter.instruction("b.gt __rt_amr_int_set");                               // non-heap tags need no retain
    emitter.instruction("ldr x0, [sp, #72]");                                   // load the heap-backed value low word
    emitter.instruction("bl __rt_incref");                                      // retain the heap-backed value for the result
    emitter.instruction("b __rt_amr_int_set");                                  // continue to the insertion
    emitter.label("__rt_amr_int_persist");
    emitter.instruction("ldr x1, [sp, #72]");                                   // string pointer to persist
    emitter.instruction("ldr x2, [sp, #80]");                                   // string length to persist
    emitter.instruction("bl __rt_str_persist");                                 // copy the string into an independent heap block, x1 = new pointer
    emitter.instruction("str x1, [sp, #72]");                                   // store the persisted string pointer
    emitter.instruction("str x2, [sp, #80]");                                   // store the persisted string length
    emitter.label("__rt_amr_int_set");
    emitter.instruction("ldr x0, [sp, #16]");                                   // result hash pointer
    emitter.instruction("ldr x1, [sp, #24]");                                   // next integer key
    emitter.instruction("mov x2, #-1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("ldr x3, [sp, #72]");                                   // value low word
    emitter.instruction("ldr x4, [sp, #80]");                                   // value high word
    emitter.instruction("ldr x5, [sp, #88]");                                   // value runtime tag
    emitter.instruction("bl __rt_hash_set");                                    // append the integer-keyed entry with the renumbered key
    emitter.instruction("str x0, [sp, #16]");                                   // update the result pointer after possible reallocation
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the next integer key counter
    emitter.instruction("add x9, x9, #1");                                      // advance the integer key counter
    emitter.instruction("str x9, [sp, #24]");                                   // save the advanced integer key counter
    emitter.instruction("b __rt_amr_entry_loop");                               // continue with the next entry
    emitter.label("__rt_amr_next_which");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the source selector
    emitter.instruction("add x9, x9, #1");                                      // advance to the next source
    emitter.instruction("str x9, [sp, #32]");                                   // save the advanced source selector
    emitter.instruction("b __rt_amr_which_loop");                               // process the next source
    emitter.label("__rt_amr_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = result hash pointer
    emitter.instruction("ldp x29, x30, [sp, #176]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #192");                                    // deallocate the stack frame
    emitter.instruction("ret");                                                 // return the merged hash in x0
}

/// x86_64 Linux implementation of `__rt_array_merge_recursive`.
/// Input:  rdi = first hash, rsi = second hash
/// Output: rax = new owned merged hash
fn emit_array_merge_recursive_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_merge_recursive ---");
    emitter.label_global("__rt_array_merge_recursive");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 160");                                        // reserve local spill slots for the merge state
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save first hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save second hash pointer
    emitter.instruction("mov rdi, 16");                                         // initial capacity for the result hash
    emitter.instruction("mov rsi, 7");                                          // value_type 7 = mixed
    emitter.instruction("call __rt_hash_new");                                  // create the result hash, rax = result
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the result hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // next integer key counter = 0
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // source selector which = 0
    emitter.label("__rt_amr_which_loop");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the source selector
    emitter.instruction("cmp rax, 2");                                          // have both sources been processed?
    emitter.instruction("jge __rt_amr_done");                                   // finish once both inputs are merged
    emitter.instruction("test rax, rax");                                       // is the selector zero (first input)?
    emitter.instruction("jne __rt_amr_pick_b");                                 // selector 1 chooses the second input
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // load the first input as the current source
    emitter.instruction("jmp __rt_amr_pick_done");                              // store the chosen source
    emitter.label("__rt_amr_pick_b");
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // load the second input as the current source
    emitter.label("__rt_amr_pick_done");
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");                       // save the current source pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // iterator cursor = 0
    emitter.label("__rt_amr_entry_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // reload the current source pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // reload the iterator cursor
    emitter.instruction("call __rt_hash_iter_next");                            // next entry: rax=cursor,rdi=kptr,rdx=klen,rcx=vlo,r8=vhi,r9=vtag
    emitter.instruction("cmp rax, -1");                                         // has iteration reached the end?
    emitter.instruction("je __rt_amr_next_which");                              // advance to the next source when done
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the next iterator cursor
    emitter.instruction("mov QWORD PTR [rbp - 64], rdi");                       // save key pointer
    emitter.instruction("mov QWORD PTR [rbp - 72], rdx");                       // save key length
    emitter.instruction("mov QWORD PTR [rbp - 80], rcx");                       // save value low word
    emitter.instruction("mov QWORD PTR [rbp - 88], r8");                        // save value high word
    emitter.instruction("mov QWORD PTR [rbp - 96], r9");                        // save value runtime tag
    emitter.instruction("cmp rdx, -1");                                         // is this an integer key (key length == -1)?
    emitter.instruction("je __rt_amr_int_key");                                 // integer keys append with renumbering
    emitter.comment("-- string key: look it up in the result --");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // result hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // key pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 72]");                       // key length
    emitter.instruction("call __rt_hash_get");                                  // look up the key: rax=found,rdi=e_lo,rsi=e_hi,rcx=e_tag
    emitter.instruction("test rax, rax");                                       // was the key already present?
    emitter.instruction("jz __rt_amr_str_new");                                 // absent key is added directly
    emitter.instruction("mov QWORD PTR [rbp - 104], rdi");                      // save existing value low word
    emitter.instruction("mov QWORD PTR [rbp - 112], rsi");                      // save existing value high word
    emitter.instruction("mov QWORD PTR [rbp - 120], rcx");                      // save existing value runtime tag
    emitter.comment("-- build the existing operand (keep assoc arrays, wrap others) --");
    emitter.instruction("cmp rcx, 5");                                          // is the existing value an associative array?
    emitter.instruction("jne __rt_amr_ea_wrap");                                // wrap non-assoc existing values into a list
    emitter.instruction("mov QWORD PTR [rbp - 128], rdi");                      // keep the associative array as the existing operand
    emitter.instruction("mov QWORD PTR [rbp - 144], 0");                        // existing operand is borrowed (not newly created)
    emitter.instruction("jmp __rt_amr_na");                                     // build the new operand next
    emitter.label("__rt_amr_ea_wrap");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 120]");                      // existing value tag
    emitter.instruction("mov rsi, QWORD PTR [rbp - 104]");                      // existing value low word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 112]");                      // existing value high word
    emitter.instruction("call __rt_amr_box_value");                             // wrap the existing value into a list, rax = wrapper
    emitter.instruction("mov QWORD PTR [rbp - 128], rax");                      // save the existing operand
    emitter.instruction("mov QWORD PTR [rbp - 144], 1");                        // mark the existing operand as newly created
    emitter.label("__rt_amr_na");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 96]");                       // reload the new value tag
    emitter.instruction("cmp rcx, 5");                                          // is the new value an associative array?
    emitter.instruction("jne __rt_amr_na_wrap");                                // wrap non-assoc new values into a list
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // reload the new value low word
    emitter.instruction("mov QWORD PTR [rbp - 136], rax");                      // keep the associative array as the new operand
    emitter.instruction("mov QWORD PTR [rbp - 152], 0");                        // new operand is borrowed (not newly created)
    emitter.instruction("jmp __rt_amr_merge");                                  // merge the two operands
    emitter.label("__rt_amr_na_wrap");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 96]");                       // new value tag
    emitter.instruction("mov rsi, QWORD PTR [rbp - 80]");                       // new value low word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 88]");                       // new value high word
    emitter.instruction("call __rt_amr_box_value");                             // wrap the new value into a list, rax = wrapper
    emitter.instruction("mov QWORD PTR [rbp - 136], rax");                      // save the new operand
    emitter.instruction("mov QWORD PTR [rbp - 152], 1");                        // mark the new operand as newly created
    emitter.label("__rt_amr_merge");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 128]");                      // existing operand
    emitter.instruction("mov rsi, QWORD PTR [rbp - 136]");                      // new operand
    emitter.instruction("call __rt_array_merge_recursive");                     // recursively merge the two operands, rax = merged
    emitter.instruction("mov rcx, rax");                                        // merged hash becomes the new value low word
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // result hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // key pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 72]");                       // key length
    emitter.instruction("xor r8, r8");                                          // array values carry no high word
    emitter.instruction("mov r9, 5");                                           // value tag 5 = associative array
    emitter.instruction("call __rt_hash_set");                                  // store the merged value (releases the previous value)
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // update the result pointer after possible reallocation
    emitter.instruction("mov rax, QWORD PTR [rbp - 144]");                      // reload the existing-operand ownership flag
    emitter.instruction("test rax, rax");                                       // was the existing operand newly created?
    emitter.instruction("jz __rt_amr_free_na");                                 // skip releasing a borrowed existing operand
    emitter.instruction("mov rdi, QWORD PTR [rbp - 128]");                      // load the newly created existing operand
    emitter.instruction("call __rt_decref_hash");                               // release the existing-operand wrapper
    emitter.label("__rt_amr_free_na");
    emitter.instruction("mov rax, QWORD PTR [rbp - 152]");                      // reload the new-operand ownership flag
    emitter.instruction("test rax, rax");                                       // was the new operand newly created?
    emitter.instruction("jz __rt_amr_entry_loop");                              // skip releasing a borrowed new operand
    emitter.instruction("mov rdi, QWORD PTR [rbp - 136]");                      // load the newly created new operand
    emitter.instruction("call __rt_decref_hash");                               // release the new-operand wrapper
    emitter.instruction("jmp __rt_amr_entry_loop");                             // continue with the next entry
    emitter.label("__rt_amr_str_new");
    emitter.instruction("mov rax, QWORD PTR [rbp - 96]");                       // reload the value runtime tag
    emitter.instruction("cmp rax, 1");                                          // is the value a string?
    emitter.instruction("je __rt_amr_str_new_persist");                         // strings are persisted as an independent copy
    emitter.instruction("cmp rax, 4");                                          // is the value below the heap-backed tag range?
    emitter.instruction("jl __rt_amr_str_new_set");                             // scalar values need no retain
    emitter.instruction("cmp rax, 7");                                          // is the value above the heap-backed tag range?
    emitter.instruction("jg __rt_amr_str_new_set");                             // non-heap tags need no retain
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // load the heap-backed value low word
    emitter.instruction("call __rt_incref");                                    // retain the heap-backed value for the result
    emitter.instruction("jmp __rt_amr_str_new_set");                            // continue to the insertion
    emitter.label("__rt_amr_str_new_persist");
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // string pointer to persist
    emitter.instruction("mov rdx, QWORD PTR [rbp - 88]");                       // string length to persist
    emitter.instruction("call __rt_str_persist");                               // copy the string into an independent heap block, rax = new pointer
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // store the persisted string pointer
    emitter.instruction("mov QWORD PTR [rbp - 88], rdx");                       // store the persisted string length
    emitter.label("__rt_amr_str_new_set");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // result hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // key pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 72]");                       // key length
    emitter.instruction("mov rcx, QWORD PTR [rbp - 80]");                       // value low word
    emitter.instruction("mov r8, QWORD PTR [rbp - 88]");                        // value high word
    emitter.instruction("mov r9, QWORD PTR [rbp - 96]");                        // value runtime tag
    emitter.instruction("call __rt_hash_set");                                  // insert the new string-keyed entry
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // update the result pointer after possible reallocation
    emitter.instruction("jmp __rt_amr_entry_loop");                             // continue with the next entry
    emitter.label("__rt_amr_int_key");
    emitter.instruction("mov rax, QWORD PTR [rbp - 96]");                       // reload the value runtime tag
    emitter.instruction("cmp rax, 1");                                          // is the value a string?
    emitter.instruction("je __rt_amr_int_persist");                             // strings are persisted as an independent copy
    emitter.instruction("cmp rax, 4");                                          // is the value below the heap-backed tag range?
    emitter.instruction("jl __rt_amr_int_set");                                 // scalar values need no retain
    emitter.instruction("cmp rax, 7");                                          // is the value above the heap-backed tag range?
    emitter.instruction("jg __rt_amr_int_set");                                 // non-heap tags need no retain
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // load the heap-backed value low word
    emitter.instruction("call __rt_incref");                                    // retain the heap-backed value for the result
    emitter.instruction("jmp __rt_amr_int_set");                                // continue to the insertion
    emitter.label("__rt_amr_int_persist");
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // string pointer to persist
    emitter.instruction("mov rdx, QWORD PTR [rbp - 88]");                       // string length to persist
    emitter.instruction("call __rt_str_persist");                               // copy the string into an independent heap block, rax = new pointer
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // store the persisted string pointer
    emitter.instruction("mov QWORD PTR [rbp - 88], rdx");                       // store the persisted string length
    emitter.label("__rt_amr_int_set");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // result hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // next integer key
    emitter.instruction("mov rdx, -1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("mov rcx, QWORD PTR [rbp - 80]");                       // value low word
    emitter.instruction("mov r8, QWORD PTR [rbp - 88]");                        // value high word
    emitter.instruction("mov r9, QWORD PTR [rbp - 96]");                        // value runtime tag
    emitter.instruction("call __rt_hash_set");                                  // append the integer-keyed entry with the renumbered key
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // update the result pointer after possible reallocation
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the next integer key counter
    emitter.instruction("add rax, 1");                                          // advance the integer key counter
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the advanced integer key counter
    emitter.instruction("jmp __rt_amr_entry_loop");                             // continue with the next entry
    emitter.label("__rt_amr_next_which");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the source selector
    emitter.instruction("add rax, 1");                                          // advance to the next source
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the advanced source selector
    emitter.instruction("jmp __rt_amr_which_loop");                             // process the next source
    emitter.label("__rt_amr_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // rax = result hash pointer
    emitter.instruction("add rsp, 160");                                        // release the local spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the merged hash in rax
}

