//! Purpose:
//! Emits the `__rt_array_replace_recursive` runtime helper for array_replace_recursive.
//! Recursively merges hash2 into a clone of hash1, recursing when both values at a key are arrays.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Self-recursive over indexed, associative, and boxed-Mixed array children; converted indexed
//!   children and overwritten values stay refcount-balanced across recursion.
//! - The result is a clone of the first array, so it shares any reference cell that array holds.
//!   The bucket-replacing setter detaches such an entry instead of writing through it, which is
//!   what keeps `array_replace_recursive` from mutating its own input.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// array_replace_recursive: deep right-wins merge of two associative arrays.
/// Input:  x0 = hash1 pointer, x1 = hash2 pointer
/// Output: x0 = new owned hash pointer
///
/// Clones hash1, then for every hash2 entry: if the key exists in hash1 and both values
/// are arrays (indexed tag 4 or associative tag 5), converts indexed children to temporary
/// integer-keyed hashes, recurses, and stores the merged sub-array; boxed Mixed children are
/// unwrapped for the shape check and reboxed for Mixed-valued results. Otherwise the hash2 value
/// overwrites/appends (right-wins). Converted child temporaries are released after recursion.
pub fn emit_array_replace_recursive(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_replace_recursive_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_replace_recursive ---");
    emitter.label_global("__rt_array_replace_recursive");
    emitter.instruction("sub sp, sp, #160");                                    // allocate the recursive-replace stack frame
    emitter.instruction("stp x29, x30, [sp, #144]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #144");                                   // set up the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save hash1 pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save hash2 pointer
    emitter.instruction("bl __rt_hash_clone_shallow");                          // clone hash1 into an owned result hash, x0 = result
    emitter.instruction("str x0, [sp, #16]");                                   // save the result hash pointer
    emitter.instruction("str xzr, [sp, #24]");                                  // iterator cursor = 0 (start from hash2 head)
    emitter.label("__rt_array_replace_recursive_loop");
    emitter.instruction("ldr x0, [sp, #8]");                                    // x0 = hash2 pointer
    emitter.instruction("ldr x1, [sp, #24]");                                   // x1 = current iterator cursor
    emitter.instruction("bl __rt_hash_iter_next_value");                        // next hash2 entry: x0=cursor,x1=kptr,x2=klen,x3=vlo,x4=vhi,x5=vtag
    emitter.instruction("cmn x0, #1");                                          // has iteration reached the end (cursor == -1)?
    emitter.instruction("b.eq __rt_array_replace_recursive_done");              // stop once every hash2 entry is merged
    emitter.instruction("str x0, [sp, #24]");                                   // save the next iterator cursor
    emitter.instruction("str x1, [sp, #32]");                                   // save key pointer
    emitter.instruction("str x2, [sp, #40]");                                   // save key length
    emitter.instruction("str x3, [sp, #48]");                                   // save hash2 value low word
    emitter.instruction("str x4, [sp, #56]");                                   // save hash2 value high word
    emitter.instruction("str x5, [sp, #64]");                                   // save hash2 value runtime tag
    emitter.instruction("cmp x5, #7");                                          // is the hash2 value a boxed Mixed cell?
    emitter.instruction("b.ne __rt_array_replace_recursive_second_concrete");   // concrete values already expose their runtime tag
    emitter.instruction("mov x0, x3");                                          // pass the borrowed Mixed cell to the unbox helper
    emitter.instruction("bl __rt_mixed_unbox");                                 // x0=concrete tag, x1=value low, x2=value high
    emitter.instruction("str x1, [sp, #88]");                                   // save the unboxed second child pointer candidate
    emitter.instruction("str x0, [sp, #112]");                                  // save the unboxed second value tag
    emitter.instruction("b __rt_array_replace_recursive_second_tag_ready");     // inspect the concrete second value shape
    emitter.label("__rt_array_replace_recursive_second_concrete");
    emitter.instruction("str x3, [sp, #88]");                                   // save the concrete second child pointer candidate
    emitter.instruction("str x5, [sp, #112]");                                  // save the concrete second value tag
    emitter.label("__rt_array_replace_recursive_second_tag_ready");
    emitter.instruction("ldr x9, [sp, #112]");                                  // load the concrete hash2 value tag
    emitter.instruction("cmp x9, #4");                                          // is the hash2 value an indexed array?
    emitter.instruction("b.eq __rt_array_replace_recursive_lookup");            // indexed children participate in recursive replacement
    emitter.instruction("cmp x9, #5");                                          // is the hash2 value an associative array?
    emitter.instruction("b.ne __rt_array_replace_recursive_over");              // non-array values overwrite in their original representation
    emitter.label("__rt_array_replace_recursive_lookup");
    emitter.instruction("ldr x0, [sp, #0]");                                    // x0 = hash1 pointer
    emitter.instruction("ldr x1, [sp, #32]");                                   // x1 = key low word
    emitter.instruction("ldr x2, [sp, #40]");                                   // x2 = key high word (-1 marks an integer key)
    emitter.instruction("bl __rt_hash_get");                                    // look up the key in hash1: x0=found,x1=vlo,x2=vhi,x3=vtag
    emitter.instruction("cbz x0, __rt_array_replace_recursive_over");           // absent in hash1 means append, not recurse
    emitter.instruction("cmp x3, #7");                                          // is the hash1 value a boxed Mixed cell?
    emitter.instruction("b.ne __rt_array_replace_recursive_first_concrete");    // concrete values already expose their runtime tag
    emitter.instruction("mov x0, x1");                                          // pass the borrowed Mixed cell to the unbox helper
    emitter.instruction("bl __rt_mixed_unbox");                                 // x0=concrete tag, x1=value low, x2=value high
    emitter.instruction("str x1, [sp, #72]");                                   // save the unboxed first child pointer candidate
    emitter.instruction("str x0, [sp, #104]");                                  // save the unboxed first value tag
    emitter.instruction("b __rt_array_replace_recursive_first_tag_ready");      // inspect the concrete first value shape
    emitter.label("__rt_array_replace_recursive_first_concrete");
    emitter.instruction("str x1, [sp, #72]");                                   // save the concrete first child pointer candidate
    emitter.instruction("str x3, [sp, #104]");                                  // save the concrete first value tag
    emitter.label("__rt_array_replace_recursive_first_tag_ready");
    emitter.instruction("ldr x9, [sp, #104]");                                  // load the concrete hash1 value tag
    emitter.instruction("cmp x9, #4");                                          // is the hash1 value an indexed array?
    emitter.instruction("b.eq __rt_array_replace_recursive_first_indexed");     // convert indexed children before hash recursion
    emitter.instruction("cmp x9, #5");                                          // is the hash1 value an associative array?
    emitter.instruction("b.ne __rt_array_replace_recursive_over");              // only recurse when both values are arrays
    emitter.instruction("str xzr, [sp, #80]");                                  // mark the first child as borrowed
    emitter.instruction("b __rt_array_replace_recursive_prepare_second");       // continue with the second child conversion
    emitter.label("__rt_array_replace_recursive_first_indexed");
    emitter.instruction("ldr x0, [sp, #72]");                                   // indexed hash1 child becomes the conversion operand
    emitter.instruction("bl __rt_array_to_hash");                               // convert the indexed child to an owned integer-keyed hash
    emitter.instruction("str x0, [sp, #72]");                                   // save the owned first child hash
    emitter.instruction("mov x9, #1");                                          // mark the first child for release after recursion
    emitter.instruction("str x9, [sp, #80]");                                   // persist the first-child ownership flag
    emitter.label("__rt_array_replace_recursive_prepare_second");
    emitter.instruction("ldr x9, [sp, #112]");                                  // reload the concrete hash2 child runtime tag
    emitter.instruction("cmp x9, #4");                                          // does the second child need indexed-to-hash conversion?
    emitter.instruction("b.eq __rt_array_replace_recursive_second_indexed");    // materialize an owned hash for indexed children
    emitter.instruction("str xzr, [sp, #96]");                                  // mark the second child as borrowed
    emitter.instruction("b __rt_array_replace_recursive_recurse");              // both child hash pointers are ready
    emitter.label("__rt_array_replace_recursive_second_indexed");
    emitter.instruction("ldr x0, [sp, #88]");                                   // load the indexed hash2 child for conversion
    emitter.instruction("bl __rt_array_to_hash");                               // convert the indexed child to an owned integer-keyed hash
    emitter.instruction("str x0, [sp, #88]");                                   // save the owned second child hash
    emitter.instruction("mov x9, #1");                                          // mark the second child for release after recursion
    emitter.instruction("str x9, [sp, #96]");                                   // persist the second-child ownership flag
    emitter.label("__rt_array_replace_recursive_recurse");
    emitter.instruction("ldr x0, [sp, #72]");                                   // x0 = normalized hash1 child (recursion arg1)
    emitter.instruction("ldr x1, [sp, #88]");                                   // x1 = normalized hash2 child (recursion arg2)
    emitter.instruction("bl __rt_array_replace_recursive");                     // recurse into the nested arrays, x0 = merged sub-array
    emitter.instruction("str x0, [sp, #128]");                                  // preserve the merged child while releasing conversions
    emitter.instruction("ldr x9, [sp, #80]");                                   // reload the first-child ownership flag
    emitter.instruction("cbz x9, __rt_array_replace_recursive_release_second"); // skip borrowed first children
    emitter.instruction("ldr x0, [sp, #72]");                                   // reload the converted first child hash
    emitter.instruction("bl __rt_decref_hash");                                 // release the owned first child conversion
    emitter.label("__rt_array_replace_recursive_release_second");
    emitter.instruction("ldr x9, [sp, #96]");                                   // reload the second-child ownership flag
    emitter.instruction("cbz x9, __rt_array_replace_recursive_children_released"); // skip borrowed second children
    emitter.instruction("ldr x0, [sp, #88]");                                   // reload the converted second child hash
    emitter.instruction("bl __rt_decref_hash");                                 // release the owned second child conversion
    emitter.label("__rt_array_replace_recursive_children_released");
    emitter.instruction("ldr x3, [sp, #128]");                                  // restore the owned recursively merged child
    emitter.instruction("ldr x9, [sp, #16]");                                   // load the result hash pointer for value representation
    emitter.instruction("ldr x9, [x9, #16]");                                   // load the result hash runtime value type
    emitter.instruction("cmp x9, #7");                                          // does the result hash store boxed Mixed cells?
    emitter.instruction("b.ne __rt_array_replace_recursive_child_concrete");    // concrete hashes store the merged child directly
    emitter.instruction("mov x0, #5");                                          // boxed payload tag 5 = associative array
    emitter.instruction("mov x1, x3");                                          // transfer the owned merged hash into the new box
    emitter.instruction("mov x2, #0");                                          // associative payloads use no high word
    emitter.instruction("bl __rt_mixed_box_raw");                               // adopt the merged hash in a fresh Mixed cell
    emitter.instruction("mov x3, x0");                                          // boxed Mixed pointer becomes the stored low word
    emitter.instruction("mov x4, #0");                                          // boxed Mixed entries use no high word
    emitter.instruction("mov x5, #7");                                          // runtime value tag 7 = boxed Mixed
    emitter.instruction("b __rt_array_replace_recursive_child_ready");          // continue with the normalized merged value
    emitter.label("__rt_array_replace_recursive_child_concrete");
    emitter.instruction("mov x4, #0");                                          // associative array values use no high word
    emitter.instruction("mov x5, #5");                                          // runtime value tag 5 = associative array
    emitter.label("__rt_array_replace_recursive_child_ready");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = result hash pointer
    emitter.instruction("ldr x1, [sp, #32]");                                   // reload key pointer
    emitter.instruction("ldr x2, [sp, #40]");                                   // reload key length
    emitter.instruction("bl __rt_hash_set_value");                              // store the merged sub-array (releases the previous value)
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
    emitter.instruction("bl __rt_hash_set_value");                              // overwrite or append the value into the result hash
    emitter.instruction("str x0, [sp, #16]");                                   // update the result pointer after possible reallocation
    emitter.instruction("b __rt_array_replace_recursive_loop");                 // continue with the next hash2 entry
    emitter.label("__rt_array_replace_recursive_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = result hash pointer
    emitter.instruction("ldp x29, x30, [sp, #144]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #160");                                    // deallocate the stack frame
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
    emitter.instruction("sub rsp, 144");                                        // reserve local spill slots for the recursive merge state
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save hash1 pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save hash2 pointer
    emitter.instruction("call __rt_hash_clone_shallow");                        // clone hash1 into an owned result hash, rax = result
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the result hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // iterator cursor = 0 (start from hash2 head)
    emitter.label("__rt_array_replace_recursive_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // rdi = hash2 pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // rsi = current iterator cursor
    emitter.instruction("call __rt_hash_iter_next_value");                      // next hash2 entry: rax=cursor,rdi=kptr,rdx=klen,rcx=vlo,r8=vhi,r9=vtag
    emitter.instruction("cmp rax, -1");                                         // has iteration reached the end?
    emitter.instruction("je __rt_array_replace_recursive_done");                // stop once every hash2 entry is merged
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the next iterator cursor
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // save key pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // save key length
    emitter.instruction("mov QWORD PTR [rbp - 56], rcx");                       // save hash2 value low word
    emitter.instruction("mov QWORD PTR [rbp - 64], r8");                        // save hash2 value high word
    emitter.instruction("mov QWORD PTR [rbp - 72], r9");                        // save hash2 value runtime tag
    emitter.instruction("cmp r9, 7");                                           // is the hash2 value a boxed Mixed cell?
    emitter.instruction("jne __rt_array_replace_recursive_second_concrete");    // concrete values already expose their runtime tag
    emitter.instruction("mov rax, rcx");                                        // pass the borrowed Mixed cell to the unbox helper
    emitter.instruction("call __rt_mixed_unbox");                               // rax=concrete tag, rdi=value low, rdx=value high
    emitter.instruction("mov QWORD PTR [rbp - 96], rdi");                       // save the unboxed second child pointer candidate
    emitter.instruction("mov QWORD PTR [rbp - 120], rax");                      // save the unboxed second value tag
    emitter.instruction("jmp __rt_array_replace_recursive_second_tag_ready");   // inspect the concrete second value shape
    emitter.label("__rt_array_replace_recursive_second_concrete");
    emitter.instruction("mov QWORD PTR [rbp - 96], rcx");                       // save the concrete second child pointer candidate
    emitter.instruction("mov QWORD PTR [rbp - 120], r9");                       // save the concrete second value tag
    emitter.label("__rt_array_replace_recursive_second_tag_ready");
    emitter.instruction("cmp QWORD PTR [rbp - 120], 4");                        // is the hash2 value an indexed array?
    emitter.instruction("je __rt_array_replace_recursive_lookup");              // indexed children participate in recursive replacement
    emitter.instruction("cmp QWORD PTR [rbp - 120], 5");                        // is the hash2 value an associative array?
    emitter.instruction("jne __rt_array_replace_recursive_over");               // non-array values overwrite in their original representation
    emitter.label("__rt_array_replace_recursive_lookup");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // rdi = hash1 pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // rsi = key low word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // rdx = key high word (-1 marks an integer key)
    emitter.instruction("call __rt_hash_get");                                  // look up the key in hash1: rax=found,rdi=vlo,rsi=vhi,rcx=vtag
    emitter.instruction("test rax, rax");                                       // was the key present in hash1?
    emitter.instruction("je __rt_array_replace_recursive_over");                // absent in hash1 means append, not recurse
    emitter.instruction("cmp rcx, 7");                                          // is the hash1 value a boxed Mixed cell?
    emitter.instruction("jne __rt_array_replace_recursive_first_concrete");     // concrete values already expose their runtime tag
    emitter.instruction("mov rax, rdi");                                        // pass the borrowed Mixed cell to the unbox helper
    emitter.instruction("call __rt_mixed_unbox");                               // rax=concrete tag, rdi=value low, rdx=value high
    emitter.instruction("mov QWORD PTR [rbp - 80], rdi");                       // save the unboxed first child pointer candidate
    emitter.instruction("mov QWORD PTR [rbp - 112], rax");                      // save the unboxed first value tag
    emitter.instruction("jmp __rt_array_replace_recursive_first_tag_ready");    // inspect the concrete first value shape
    emitter.label("__rt_array_replace_recursive_first_concrete");
    emitter.instruction("mov QWORD PTR [rbp - 80], rdi");                       // save the concrete first child pointer candidate
    emitter.instruction("mov QWORD PTR [rbp - 112], rcx");                      // save the concrete first value tag
    emitter.label("__rt_array_replace_recursive_first_tag_ready");
    emitter.instruction("cmp QWORD PTR [rbp - 112], 4");                        // is the hash1 value an indexed array?
    emitter.instruction("je __rt_array_replace_recursive_first_indexed");       // convert indexed children before hash recursion
    emitter.instruction("cmp QWORD PTR [rbp - 112], 5");                        // is the hash1 value an associative array?
    emitter.instruction("jne __rt_array_replace_recursive_over");               // only recurse when both values are arrays
    emitter.instruction("mov QWORD PTR [rbp - 88], 0");                         // mark the first child as borrowed
    emitter.instruction("jmp __rt_array_replace_recursive_prepare_second");     // continue with the second child conversion
    emitter.label("__rt_array_replace_recursive_first_indexed");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // load the indexed first child for conversion
    emitter.instruction("call __rt_array_to_hash");                             // convert the indexed child to an owned integer-keyed hash
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // save the owned first child hash
    emitter.instruction("mov QWORD PTR [rbp - 88], 1");                         // mark the first child for release after recursion
    emitter.label("__rt_array_replace_recursive_prepare_second");
    emitter.instruction("cmp QWORD PTR [rbp - 120], 4");                        // does the second child need indexed-to-hash conversion?
    emitter.instruction("je __rt_array_replace_recursive_second_indexed");      // materialize an owned hash for indexed children
    emitter.instruction("mov QWORD PTR [rbp - 104], 0");                        // mark the second child as borrowed
    emitter.instruction("jmp __rt_array_replace_recursive_recurse");            // both child hash pointers are ready
    emitter.label("__rt_array_replace_recursive_second_indexed");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 96]");                       // load the indexed hash2 child for conversion
    emitter.instruction("call __rt_array_to_hash");                             // convert the indexed child to an owned integer-keyed hash
    emitter.instruction("mov QWORD PTR [rbp - 96], rax");                       // save the owned second child hash
    emitter.instruction("mov QWORD PTR [rbp - 104], 1");                        // mark the second child for release after recursion
    emitter.label("__rt_array_replace_recursive_recurse");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // rdi = normalized hash1 child (recursion arg1)
    emitter.instruction("mov rsi, QWORD PTR [rbp - 96]");                       // rsi = normalized hash2 child (recursion arg2)
    emitter.instruction("call __rt_array_replace_recursive");                   // recurse into the nested arrays, rax = merged sub-array
    emitter.instruction("mov QWORD PTR [rbp - 128], rax");                      // preserve the merged child while releasing conversions
    emitter.instruction("cmp QWORD PTR [rbp - 88], 0");                         // was the first child converted to an owned hash?
    emitter.instruction("je __rt_array_replace_recursive_release_second");      // skip borrowed first children
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // reload the converted first child hash
    emitter.instruction("call __rt_decref_hash");                               // release the owned first child conversion
    emitter.label("__rt_array_replace_recursive_release_second");
    emitter.instruction("cmp QWORD PTR [rbp - 104], 0");                        // was the second child converted to an owned hash?
    emitter.instruction("je __rt_array_replace_recursive_children_released");   // skip borrowed second children
    emitter.instruction("mov rdi, QWORD PTR [rbp - 96]");                       // reload the converted second child hash
    emitter.instruction("call __rt_decref_hash");                               // release the owned second child conversion
    emitter.label("__rt_array_replace_recursive_children_released");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 128]");                      // restore the owned recursively merged child
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // load the result hash pointer for value representation
    emitter.instruction("cmp QWORD PTR [r10 + 16], 7");                         // does the result hash store boxed Mixed cells?
    emitter.instruction("jne __rt_array_replace_recursive_child_concrete");     // concrete hashes store the merged child directly
    emitter.instruction("mov rax, 5");                                          // boxed payload tag 5 = associative array
    emitter.instruction("mov rdi, rcx");                                        // transfer the owned merged hash into the new box
    emitter.instruction("xor esi, esi");                                        // associative payloads use no high word
    emitter.instruction("call __rt_mixed_box_raw");                             // adopt the merged hash in a fresh Mixed cell
    emitter.instruction("mov rcx, rax");                                        // boxed Mixed pointer becomes the stored low word
    emitter.instruction("xor r8, r8");                                          // boxed Mixed entries use no high word
    emitter.instruction("mov r9, 7");                                           // runtime value tag 7 = boxed Mixed
    emitter.instruction("jmp __rt_array_replace_recursive_child_ready");        // continue with the normalized merged value
    emitter.label("__rt_array_replace_recursive_child_concrete");
    emitter.instruction("xor r8, r8");                                          // associative array values use no high word
    emitter.instruction("mov r9, 5");                                           // runtime value tag 5 = associative array
    emitter.label("__rt_array_replace_recursive_child_ready");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // rdi = result hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // reload key pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // reload key length
    emitter.instruction("call __rt_hash_set_value");                            // store the merged sub-array (releases the previous value)
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
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // load the heap-backed value into the incref input register
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
    emitter.instruction("call __rt_hash_set_value");                            // overwrite or append the value into the result hash
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // update the result pointer after possible reallocation
    emitter.instruction("jmp __rt_array_replace_recursive_loop");               // continue with the next hash2 entry
    emitter.label("__rt_array_replace_recursive_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // rax = result hash pointer
    emitter.instruction("add rsp, 144");                                        // release the local spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the result hash in rax
}
