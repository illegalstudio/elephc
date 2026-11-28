//! Purpose:
//! Emits `__rt_hash_reindex_dense`, which rebuilds a sorted hash with keys `0..n-1`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::managed` through the array runtime catalog.
//!
//! Key details:
//! - Entries are MOVED, not copied: the source shell is emptied entry by entry and owns
//!   nothing afterwards, so the caller can release it through the ordinary hash decref path.
//! - The per-entry runtime tag is preserved, including tag 11 (managed reference cells).
//!   This is what keeps `sort()`/`rsort()` aliases live through the renumbering.
//! - Replaced string keys are released once; integer keys are inline and own nothing.
//! - Both architectures are written target-aware and share the same cursor protocol as
//!   `__rt_hash_iter_next` (0 = head, slot index + 1 = next, -2 = tail, -1 = done).

use crate::{abi, Arch, Emitter};

/// Emits the helper for the active target.
pub fn emit_hash_reindex_dense(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::X86_64 {
        emit_hash_reindex_dense_linux_x86_64(emitter);
        return;
    }
    emit_hash_reindex_dense_aarch64(emitter);
}

/// Emits the AArch64 helper: iterate the sorted chain, insert under dense integer keys, then
/// empty the source shell so the caller can release it without touching the moved values.
fn emit_hash_reindex_dense_aarch64(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_reindex_dense ---");
    emitter.label_global("__rt_hash_reindex_dense");

    emitter.instruction("cbz x0, __rt_hash_reindex_dense_null");                // a null table has no keys to renumber
    emitter.instruction("sub sp, sp, #96");                                     // reserve the move state and linkage
    emitter.instruction("stp x29, x30, [sp, #80]");                             // preserve frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the reindex frame
    emitter.instruction("str x0, [sp, #0]");                                    // save the sorted source table
    emitter.instruction("ldr x9, [x0]");                                        // load the source entry count
    emitter.instruction("mov x0, #8");                                          // prepare the minimum destination capacity
    emitter.instruction("cmp x9, #8");                                          // does the source exceed the minimum?
    emitter.instruction("csel x0, x9, x0, gt");                                 // size the destination from the source count
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the source table
    emitter.instruction("ldr x1, [x1, #16]");                                   // keep the source value_type
    abi::emit_call_label(emitter, "__rt_hash_new");                             // allocate the dense destination
    emitter.instruction("str x0, [sp, #8]");                                    // save the destination table
    emitter.instruction("str xzr, [sp, #16]");                                  // start the source walk at the head
    emitter.instruction("str xzr, [sp, #24]");                                  // the first dense key is zero

    emitter.label("__rt_hash_reindex_dense_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source table
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload the source cursor
    abi::emit_call_label(emitter, "__rt_hash_iter_next");                       // read the next source entry
    emitter.instruction("cmn x0, #1");                                          // has the source walk reached its end?
    emitter.instruction("b.eq __rt_hash_reindex_dense_done");                   // finish once every entry has moved
    emitter.instruction("str x0, [sp, #16]");                                   // save the next source cursor
    emitter.instruction("str x1, [sp, #32]");                                   // save the source key payload for release
    emitter.instruction("str x2, [sp, #40]");                                   // save the source key high word
    emitter.instruction("str x6, [sp, #48]");                                   // save the source entry value address
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the destination table
    emitter.instruction("ldr x1, [sp, #24]");                                   // use the dense index as the new integer key
    emitter.instruction("mov x2, #-1");                                         // key_hi = -1 marks an integer key
    abi::emit_call_label(emitter, "__rt_hash_insert_owned");                    // move the entry under its dense key
    emitter.instruction("str x0, [sp, #8]");                                    // keep the possibly grown destination

    // -- release the replaced string key (integer keys own nothing) --
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the source key high word
    emitter.instruction("cmn x9, #1");                                          // is the source key an inline integer?
    emitter.instruction("b.eq __rt_hash_reindex_dense_clear");                  // integer keys have no heap bytes to release
    emitter.instruction("ldr x0, [sp, #32]");                                   // load the owned string key pointer
    emitter.instruction("cbz x0, __rt_hash_reindex_dense_clear");               // empty keys own no storage
    emitter.instruction("ldr w9, [x0, #-12]");                                  // load the key refcount
    emitter.instruction("subs w9, w9, #1");                                     // drop the source table's ownership
    emitter.instruction("str w9, [x0, #-12]");                                  // store the decremented refcount
    emitter.instruction("b.ne __rt_hash_reindex_dense_clear");                  // shared keys stay alive
    abi::emit_call_label(emitter, "__rt_heap_free");                            // free the last owner of the key

    emitter.label("__rt_hash_reindex_dense_clear");
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the source entry value address
    emitter.instruction("str xzr, [x9, #-24]");                                 // clear occupied so the old shell owns no moved value
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the dense index
    emitter.instruction("add x9, x9, #1");                                      // advance to the next dense key
    emitter.instruction("str x9, [sp, #24]");                                   // save the advanced index
    emitter.instruction("b __rt_hash_reindex_dense_loop");                      // continue moving source entries

    emitter.label("__rt_hash_reindex_dense_done");
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the emptied source table
    emitter.instruction("str xzr, [x9]");                                       // the source has no live entries left
    emitter.instruction("mov x10, #-1");                                        // materialize the empty head/tail sentinel
    emitter.instruction("str x10, [x9, #24]");                                  // clear the source insertion-order head
    emitter.instruction("str x10, [x9, #32]");                                  // clear the source insertion-order tail
    emitter.instruction("ldr x0, [sp, #8]");                                    // return the dense destination
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the reindex frame
    emitter.instruction("ret");                                                 // return the dense table

    emitter.label("__rt_hash_reindex_dense_null");
    emitter.instruction("mov x0, #0");                                          // a null source reindexes to null
    emitter.instruction("ret");                                                 // return without allocating
}

/// Emits the x86_64 helper with the same move contract and cursor protocol.
fn emit_hash_reindex_dense_linux_x86_64(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_reindex_dense ---");
    emitter.label_global("__rt_hash_reindex_dense");

    emitter.instruction("test rax, rax");                                       // a null table has no keys to renumber
    emitter.instruction("je __rt_hash_reindex_dense_null");                     // return null without allocating
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the reindex frame
    emitter.instruction("sub rsp, 96");                                         // reserve the move state
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the sorted source table
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load the source entry count
    emitter.instruction("mov rdi, 8");                                          // prepare the minimum destination capacity
    emitter.instruction("cmp r10, 8");                                          // does the source exceed the minimum?
    emitter.instruction("cmovg rdi, r10");                                      // size the destination from the source count
    emitter.instruction("mov rsi, QWORD PTR [rax + 16]");                       // keep the source value_type
    abi::emit_call_label(emitter, "__rt_hash_new");                             // allocate the dense destination
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the destination table
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // start the source walk at the head
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // the first dense key is zero

    emitter.label("__rt_hash_reindex_dense_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the source table
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the source cursor
    abi::emit_call_label(emitter, "__rt_hash_iter_next");                       // read the next source entry
    emitter.instruction("cmp rax, -1");                                         // has the source walk reached its end?
    emitter.instruction("je __rt_hash_reindex_dense_done");                     // finish once every entry has moved
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the next source cursor
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // save the source key payload for release
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // save the source key high word
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");                       // save the source entry value address
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the destination table
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // use the dense index as the new integer key
    emitter.instruction("mov rdx, -1");                                         // key_hi = -1 marks an integer key
    abi::emit_call_label(emitter, "__rt_hash_insert_owned");                    // move the entry under its dense key
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // keep the possibly grown destination

    // -- release the replaced string key (integer keys own nothing) --
    emitter.instruction("cmp QWORD PTR [rbp - 48], -1");                        // is the source key an inline integer?
    emitter.instruction("je __rt_hash_reindex_dense_clear");                    // integer keys have no heap bytes to release
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // load the owned string key pointer
    emitter.instruction("test rax, rax");                                       // empty keys own no storage
    emitter.instruction("jz __rt_hash_reindex_dense_clear");                    // skip a missing key pointer
    emitter.instruction("mov r9, QWORD PTR [rax - 8]");                         // load the key heap header
    emitter.instruction("shr r9, 32");                                          // isolate the high-word heap marker
    emitter.instruction(&format!("cmp r9d, 0x{:x}", crate::layout::X86_64_HEAP_MAGIC_HI32)); // is this an elephc-owned key?
    emitter.instruction("jne __rt_hash_reindex_dense_clear");                   // foreign key pointers are not ours to free
    emitter.instruction("mov r9d, DWORD PTR [rax - 12]");                       // load the key refcount
    emitter.instruction("sub r9d, 1");                                          // drop the source table's ownership
    emitter.instruction("mov DWORD PTR [rax - 12], r9d");                       // store the decremented refcount
    emitter.instruction("jnz __rt_hash_reindex_dense_clear");                   // shared keys stay alive
    abi::emit_call_label(emitter, "__rt_heap_free");                            // free the last owner of the key

    emitter.label("__rt_hash_reindex_dense_clear");
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // reload the source entry value address
    emitter.instruction("mov QWORD PTR [r10 - 24], 0");                         // clear occupied so the old shell owns no moved value
    emitter.instruction("add QWORD PTR [rbp - 32], 1");                         // advance to the next dense key
    emitter.instruction("jmp __rt_hash_reindex_dense_loop");                    // continue moving source entries

    emitter.label("__rt_hash_reindex_dense_done");
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the emptied source table
    emitter.instruction("mov QWORD PTR [r9], 0");                               // the source has no live entries left
    emitter.instruction("mov rax, -1");                                         // materialize the empty head/tail sentinel
    emitter.instruction("mov QWORD PTR [r9 + 24], rax");                        // clear the source insertion-order head
    emitter.instruction("mov QWORD PTR [r9 + 32], rax");                        // clear the source insertion-order tail
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the dense destination
    emitter.instruction("add rsp, 96");                                         // release the reindex frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the dense table

    emitter.label("__rt_hash_reindex_dense_null");
    emitter.instruction("xor eax, eax");                                        // a null source reindexes to null
    emitter.instruction("ret");                                                 // return without allocating
}
