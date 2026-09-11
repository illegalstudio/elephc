//! Purpose:
//! Emits the `__rt_hash_grow`, `__rt_hash_ensure_unique` runtime helper assembly for hash grow.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Hash helpers must normalize PHP keys and preserve bucket layout, ownership, and iteration conventions.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_hash_grow` runtime helper for the active target.
/// Doubles entry capacity while preserving the header identity and insertion order.
/// The ordinary entry first splits shared storage; `__rt_hash_grow_owned` skips
/// that split for internal construction writers that deliberately update aliases.
/// - ARM64 input/output: x0 = hash header, unchanged after owned growth.
/// - x86_64 input/output: rdi = hash header, returned in rax.
/// Calls `__rt_hash_ensure_unique` before rehash to split shared storage, then
/// iterates via `__rt_hash_iter_next` and inserts via `__rt_hash_insert_owned`.
pub fn emit_hash_grow(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_grow_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_grow ---");
    emitter.label_global("__rt_hash_grow");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve linkage during normal copy-on-write separation
    emitter.instruction("mov x29, sp");                                         // establish the separation frame
    emitter.instruction("bl __rt_hash_ensure_unique");                          // split ordinary shared PHP values before growing
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore caller linkage
    emitter.instruction("b __rt_hash_grow_owned");                              // grow the resulting unique header
    emitter.label_global("__rt_hash_grow_owned");


    // -- set up stack frame --
    // Stack layout:
    //   [sp, #0]  = insertion-order iterator cursor
    //   [sp, #32] = saved x19 (callee-saved)
    //   [sp, #40] = saved x20 (callee-saved)
    //   [sp, #48] = saved x29
    //   [sp, #56] = saved x30
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("stp x19, x20, [sp, #32]");                             // save callee-saved registers
    emitter.instruction("mov x20, x0");                                         // x20 = unique old table pointer

    // -- read old table header --
    emitter.instruction("ldr x9, [x20, #8]");                                   // x9 = old capacity
    emitter.instruction("ldr x1, [x20, #16]");                                  // x1 = runtime value_type

    // -- create new table with 2x capacity --
    emitter.instruction("lsl x0, x9, #1");                                      // x0 = old_capacity * 2
                                           // x1 = value_type (already set)
    emitter.instruction("cmp x0, #0");                                          // detect an empty source capacity
    emitter.instruction("mov x10, #1");                                         // choose a minimum grown capacity
    emitter.instruction("csel x0, x0, x10, ne");                                // allow the first insertion into an empty hash
    emitter.instruction("bl __rt_hash_new");                                    // allocate new table → x0
    emitter.instruction("mov x19, x0");                                         // x19 = new table (callee-saved)

    // -- iterate old entries in insertion order and reinsert them --
    emitter.instruction("str xzr, [sp, #0]");                                   // iterator cursor = 0 (start from header.head)

    emitter.label("__rt_hash_grow_loop");
    emitter.instruction("mov x0, x20");                                         // x0 = old table pointer
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = current insertion-order cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // get next owned entry in insertion order, including its per-entry value tag
    emitter.instruction("cmn x0, #1");                                          // did the iterator signal end-of-walk?
    emitter.instruction("b.eq __rt_hash_grow_free");                            // finish once every entry has been moved
    emitter.instruction("str x0, [sp, #0]");                                    // save the next insertion-order cursor
    emitter.instruction("mov x0, x19");                                         // x0 = destination table
    emitter.instruction("bl __rt_hash_insert_owned");                           // rehash and move existing key/value ownership with the original per-entry tag
    emitter.instruction("mov x19, x0");                                         // update new table ptr (hash_set returns it)
    emitter.instruction("b __rt_hash_grow_loop");                               // continue iterating

    // Transfer only the six value-layout words, preserving the original lifetime pins.
    emitter.label("__rt_hash_grow_free");
    emitter.instruction("ldr x0, [x20, #40]");                                  // retire the original raw entry allocation
    emitter.instruction("bl __rt_heap_free");                                   // release storage after moving all key/value owners
    emitter.instruction("ldp x9, x10, [x19]");                                  // recover replacement count and capacity
    emitter.instruction("stp x9, x10, [x20]");                                  // publish replacement size in the stable header
    emitter.instruction("ldp x9, x10, [x19, #16]");                             // recover value metadata and insertion-order head
    emitter.instruction("stp x9, x10, [x20, #16]");                             // preserve the rebuilt insertion-order chain
    emitter.instruction("ldp x9, x10, [x19, #32]");                             // recover tail and the new entry allocation
    emitter.instruction("stp x9, x10, [x20, #32]");                             // transfer entry ownership to the stable header
    emitter.instruction("mov x0, x19");                                         // discard only the temporary replacement header
    emitter.instruction("bl __rt_heap_free");                                   // keep its transferred entry allocation alive
    emitter.instruction("mov x0, x20");                                         // return the original header identity

    // -- restore frame and return --
    emitter.instruction("ldp x19, x20, [sp, #32]");                             // restore callee-saved registers
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = new table
}

/// x86_64-specific implementation of `__rt_hash_grow` using the System V ABI.
/// Follows the same grow/iterate/reinsert/free sequence as the ARM64 variant
/// but uses callee-saved registers r12/r13, a frame-based spill layout, and
/// the SysV register convention (rdi = first arg, rax = return).
fn emit_hash_grow_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_grow ---");
    emitter.label_global("__rt_hash_grow");
    emitter.instruction("push rbp");                                            // preserve linkage during normal copy-on-write separation
    emitter.instruction("mov rbp, rsp");                                        // establish the separation frame
    emitter.instruction("call __rt_hash_ensure_unique");                        // split ordinary shared PHP values before growing
    emitter.instruction("mov rdi, rax");                                        // forward the separated header to owned growth
    emitter.instruction("pop rbp");                                             // restore caller linkage
    emitter.instruction("jmp __rt_hash_grow_owned");                            // grow the resulting unique header
    emitter.label_global("__rt_hash_grow_owned");
    emitter.instruction("mov rax, rdi");                                        // accept the declared SysV input independently of the caller's return register


    // -- set up stack frame --
    // Frame layout:
    //   [rbp - 8]  = insertion-order iterator cursor
    //   [rbp - 16] = old unique hash table
    //   [rbp - 24] = new hash table
    //   [rbp - 32] = saved r12
    //   [rbp - 40] = saved r13
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving grow spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for rehashing state
    emitter.instruction("sub rsp, 48");                                         // reserve local storage while keeping nested calls aligned
    emitter.instruction("mov QWORD PTR [rbp - 32], r12");                       // preserve r12 because SysV treats it as callee-saved
    emitter.instruction("mov QWORD PTR [rbp - 40], r13");                       // preserve r13 because SysV treats it as callee-saved
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the unique old hash table

    // -- create new table with 2x capacity --
    emitter.instruction("mov r12, QWORD PTR [rax + 8]");                        // load the old table capacity
    emitter.instruction("mov rsi, QWORD PTR [rax + 16]");                       // load the table-wide runtime value tag
    emitter.instruction("lea rdi, [r12 + r12]");                                // requested capacity = old capacity * 2
    emitter.instruction("test rdi, rdi");                                       // protect callers that grow a zero-capacity hash
    emitter.instruction("jne __rt_hash_grow_capacity_ready_x");                 // non-zero capacities can be used as-is
    emitter.instruction("mov rdi, 1");                                          // minimum grown capacity for an empty table
    emitter.label("__rt_hash_grow_capacity_ready_x");
    emitter.instruction("call __rt_hash_new");                                  // allocate the destination hash table
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the destination hash table
    emitter.instruction("mov QWORD PTR [rbp - 8], 0");                          // iterator cursor = fresh insertion-order walk

    // -- iterate old entries in insertion order and reinsert them --
    emitter.label("__rt_hash_grow_loop_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // old table pointer for hash_iter_next
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // current insertion-order cursor
    emitter.instruction("call __rt_hash_iter_next");                            // fetch the next owned entry in insertion order
    emitter.instruction("cmp rax, -1");                                         // did the iterator signal end-of-walk?
    emitter.instruction("je __rt_hash_grow_free_x");                            // finish after every entry has been moved
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the next insertion-order cursor
    emitter.instruction("mov rsi, rdi");                                        // move returned key pointer into hash_insert_owned arg1
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // destination table for hash_insert_owned
                                                                                 // rdx/rcx/r8/r9 still hold key_len/value_lo/value_hi/value_tag
    emitter.instruction("call __rt_hash_insert_owned");                         // rehash and move existing entry ownership
    // The new table is allocated at 2x old capacity, so reinserting exactly
    // old_count entries should not need another grow. Still retain the return
    // value because hash_insert_owned is allowed to grow defensively if the
    // load-factor invariant changes later.
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // keep the latest destination table pointer
    emitter.instruction("jmp __rt_hash_grow_loop_x");                           // continue rehashing entries

    // Transfer only the six value-layout words, preserving the original lifetime pins.
    emitter.label("__rt_hash_grow_free_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // recover the original stable header
    emitter.instruction("mov rax, QWORD PTR [rax + 40]");                       // retire its original raw entry allocation
    emitter.instruction("call __rt_heap_free");                                 // release storage after moving all key/value owners
    emitter.instruction("mov r12, QWORD PTR [rbp - 16]");                       // recover the stable destination header
    emitter.instruction("mov r13, QWORD PTR [rbp - 24]");                       // recover the temporary replacement header
    for offset in [0, 8, 16, 24, 32, 40] {
        emitter.instruction(&format!("mov r10, QWORD PTR [r13 + {offset}]"));   // read one replacement header field
        emitter.instruction(&format!("mov QWORD PTR [r12 + {offset}], r10"));   // transfer metadata and entry ownership
    }
    emitter.instruction("mov rax, r13");                                        // discard only the temporary replacement header
    emitter.instruction("call __rt_heap_free");                                 // keep its transferred entry allocation alive
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the original header identity
    emitter.instruction("mov r13, QWORD PTR [rbp - 40]");                       // restore caller r13
    emitter.instruction("mov r12, QWORD PTR [rbp - 32]");                       // restore caller r12
    emitter.instruction("add rsp, 48");                                         // release the grow spill frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return with rax = new table
}
