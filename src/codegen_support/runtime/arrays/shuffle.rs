//! Purpose:
//! Emits the `__rt_shuffle`, `__rt_shuffle_loop` runtime helper assembly for shuffle.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Shuffle mutates array order using runtime random helpers while preserving element ownership.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use elephc_memoir::hash_layout::{self, EntryField, HeaderField};

/// shuffle: shuffle an integer array in place using Fisher-Yates algorithm.
/// Input: x0 = array pointer
/// Modifies array in place, no return value.
pub fn emit_shuffle(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_shuffle_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: shuffle ---");
    emitter.label_global("__rt_shuffle");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #32");                                     // allocate 32 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save array pointer

    // -- load array metadata --
    elephc_memoir::values::packed_slots::emit_load_length(emitter, "x9", "x0"); // x9 = array length
    emitter.instruction("str x9, [sp, #8]");                                    // save length to stack

    // -- Fisher-Yates: iterate i from length-1 down to 1 --
    emitter.instruction("sub x19, x9, #1");                                     // x19 = i = length - 1

    emitter.label("__rt_shuffle_loop");
    emitter.instruction("cmp x19, #1");                                         // check if i < 1
    emitter.instruction("b.lt __rt_shuffle_done");                              // if so, shuffling complete

    // -- generate random j in [0, i] --
    emitter.instruction("add x0, x19, #1");                                     // x0 = i + 1 (upper bound, exclusive)
    emitter.instruction("bl __rt_mt_uniform");                                  // x0 = random value in [0, i]

    // -- swap data[i] and data[j] --
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = array pointer
    elephc_memoir::values::packed_slots::emit_packed_payload_aarch64(emitter, "x2", "x1");
    emitter.instruction("ldr x3, [x2, x19, lsl #3]");                           // x3 = data[i]
    emitter.instruction("ldr x4, [x2, x0, lsl #3]");                            // x4 = data[j] (x0 = j from arc4random)
    emitter.instruction("str x4, [x2, x19, lsl #3]");                           // data[i] = data[j]
    emitter.instruction("str x3, [x2, x0, lsl #3]");                            // data[j] = data[i] (complete swap)

    // -- decrement i and continue --
    emitter.instruction("sub x19, x19, #1");                                    // i -= 1
    emitter.instruction("b __rt_shuffle_loop");                                 // continue loop

    // -- tear down stack frame and return --
    emitter.label("__rt_shuffle_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// x86_64 Linux-specific implementation of the Fisher-Yates shuffle.
/// Input: `rdi` = array pointer (System V ABI).
/// Modifies the array payload in place; no return value.
/// Called by `emit_shuffle` when targeting Linux x86_64.
fn emit_shuffle_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: shuffle ---");
    emitter.label_global("__rt_shuffle");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving shuffle spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the array pointer and Fisher-Yates loop cursor
    emitter.instruction("sub rsp, 16");                                         // reserve aligned spill slots for the shuffled array pointer and the current descending Fisher-Yates cursor
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the shuffled indexed-array pointer across random-number helper calls
    elephc_memoir::values::packed_slots::emit_load_length(emitter, "r10", "rdi"); // load the indexed-array logical length once before starting the Fisher-Yates loop
    emitter.instruction("cmp r10, 2");                                          // does the indexed array contain fewer than two elements?
    emitter.instruction("jb __rt_shuffle_done");                                // arrays of length zero or one are already trivially shuffled
    emitter.instruction("sub r10, 1");                                          // initialize the descending Fisher-Yates cursor to the final indexed-array slot
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // preserve the descending Fisher-Yates cursor across random-number helper calls

    emitter.label("__rt_shuffle_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the descending Fisher-Yates cursor before testing the loop termination condition
    emitter.instruction("cmp r10, 1");                                          // has the Fisher-Yates cursor reached the final swap boundary?
    emitter.instruction("jb __rt_shuffle_done");                                // stop once every slot above index zero has been swapped with a random predecessor
    emitter.instruction("lea rdi, [r10 + 1]");                                  // pass the exclusive upper bound i + 1 to the uniform random helper
    emitter.instruction("call __rt_mt_uniform");                                // draw a random slot index j in the inclusive range [0, i]
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // restore the Fisher-Yates cursor after the C-ABI bridge call clobbered r10
    emitter.instruction("mov r11, rax");                                        // preserve the sampled Fisher-Yates partner index before reloading the array base pointer
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the indexed-array pointer after the random helper clobbered caller-saved registers
    elephc_memoir::values::packed_slots::emit_packed_payload_x86_64(emitter, "r9", "r8");
    emitter.instruction("mov rax, QWORD PTR [r9 + r10 * 8]");                   // load the current Fisher-Yates tail element that will be swapped toward the sampled position
    emitter.instruction("mov rdx, QWORD PTR [r9 + r11 * 8]");                   // load the sampled Fisher-Yates partner element before overwriting either slot
    emitter.instruction("mov QWORD PTR [r9 + r10 * 8], rdx");                   // store the sampled partner element into the current Fisher-Yates tail slot
    emitter.instruction("mov QWORD PTR [r9 + r11 * 8], rax");                   // store the saved tail element into the sampled Fisher-Yates partner slot
    emitter.instruction("sub r10, 1");                                          // move the descending Fisher-Yates cursor one slot left after completing the current swap
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // preserve the updated Fisher-Yates cursor for the next loop iteration
    emitter.instruction("jmp __rt_shuffle_loop");                               // continue shuffling until the descending cursor reaches the start of the indexed array

    emitter.label("__rt_shuffle_done");
    emitter.instruction("add rsp, 16");                                         // release the shuffle spill slots before returning to generated code
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning after the in-place shuffle
    emitter.instruction("ret");                                                 // return after shuffling the indexed-array payload in place
}

/// Emits `__rt_hash_shuffle_links`, the insertion-order chain shuffle for hash-backed arrays.
///
/// Input: `x0` / `rax` = hash table. The helper collects the chain's entry slots into a
/// temporary index array, applies Fisher-Vates, and relinks each entry's `prev`/`next`
/// pointers. Every entry payload, including tag-11 reference cells, stays where it is; the
/// caller renumbers the keys afterwards with `__rt_hash_reindex_dense`.
pub fn emit_hash_shuffle_links(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_shuffle_links_linux_x86_64(emitter);
        return;
    }
    emit_hash_shuffle_links_aarch64(emitter);
}

/// Emits the AArch64 chain shuffle.
fn emit_hash_shuffle_links_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_shuffle_links ---");
    emitter.label_global("__rt_hash_shuffle_links");

    emitter.instruction("cbz x0, __rt_hash_shuffle_links_null");                // a null table has no chain to shuffle
    emitter.instruction("sub sp, sp, #96");                                     // reserve the walk, swap, and relink state
    emitter.instruction("stp x29, x30, [sp, #80]");                             // preserve frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the shuffle frame
    emitter.instruction("str x0, [sp, #0]");                                    // save the hash table
    hash_layout::emit_load_header_field(emitter, "x9", "x0", HeaderField::Count);
    emitter.instruction("str x9, [sp, #16]");                                   // save the entry count
    emitter.instruction("cmp x9, #2");                                          // fewer than two entries are already shuffled
    emitter.instruction("b.lt __rt_hash_shuffle_links_return");                 // skip allocation for trivial tables
    elephc_memoir::buffers::native_words::emit_allocate_frame_vector(
        emitter, 64, 72, "__rt_hash_shuffle_links_allocated",
    );
    emitter.label("__rt_hash_shuffle_links_allocated");
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the hash table
    hash_layout::emit_entries(emitter, "x10", "x9");
    emitter.instruction("str x10, [sp, #40]");                                  // save the entry storage base
    hash_layout::emit_order_edge(emitter, "x11", "x9", false);
    emitter.instruction("str x11, [sp, #24]");                                  // save the current slot index
    emitter.instruction("mov x12, #0");                                         // start the slot walk at index zero

    emitter.label("__rt_hash_shuffle_links_walk");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the entry count
    emitter.instruction("cmp x12, x9");                                         // have all entries been collected?
    emitter.instruction("b.ge __rt_hash_shuffle_links_walk_done");              // finish the walk once every slot is recorded
    emitter.instruction("ldr x13, [sp, #8]");                                   // reload the slot-index array
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the current slot index
    emitter.instruction("str x11, [x13, x12, lsl #3]");                         // record this entry's slot index
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the entry storage base
    hash_layout::emit_entry_address_from_storage(emitter, "x14", "x10", "x11");
    hash_layout::emit_load_entry_field(emitter, "x11", "x14", EntryField::Next);
    emitter.instruction("str x11, [sp, #24]");                                  // save the next slot index
    emitter.instruction("add x12, x12, #1");                                    // advance the walk index
    emitter.instruction("b __rt_hash_shuffle_links_walk");                      // continue collecting the chain

    emitter.label("__rt_hash_shuffle_links_walk_done");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the entry count
    emitter.instruction("sub x9, x9, #1");                                      // start Fisher-Yates at the final index
    emitter.instruction("str x9, [sp, #32]");                                   // save the descending cursor

    emitter.label("__rt_hash_shuffle_links_swap");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the descending cursor
    emitter.instruction("cmp x9, #1");                                          // has the cursor reached the final swap boundary?
    emitter.instruction("b.lt __rt_hash_shuffle_links_swap_done");              // stop once every index above zero is swapped
    emitter.instruction("add x0, x9, #1");                                      // pass the exclusive upper bound to the uniform draw
    abi::emit_call_label(emitter, "__rt_mt_uniform");                           // draw j in the inclusive range [0, i]
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the descending cursor after the call
    emitter.instruction("ldr x13, [sp, #8]");                                   // reload the slot-index array
    emitter.instruction("ldr x10, [x13, x9, lsl #3]");                          // load indices[i]
    emitter.instruction("ldr x11, [x13, x0, lsl #3]");                          // load indices[j]
    emitter.instruction("str x11, [x13, x9, lsl #3]");                          // indices[i] = indices[j]
    emitter.instruction("str x10, [x13, x0, lsl #3]");                          // indices[j] = the saved indices[i]
    emitter.instruction("sub x9, x9, #1");                                      // move the cursor one index left
    emitter.instruction("str x9, [sp, #32]");                                   // save the updated cursor
    emitter.instruction("b __rt_hash_shuffle_links_swap");                      // continue the Fisher-Yates draw

    emitter.label("__rt_hash_shuffle_links_swap_done");
    emitter.instruction("mov x12, #0");                                         // start relinking at the new head position

    emitter.label("__rt_hash_shuffle_links_relink");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the entry count
    emitter.instruction("cmp x12, x9");                                         // have all entries been relinked?
    emitter.instruction("b.ge __rt_hash_shuffle_links_relink_done");            // finish once the chain is complete
    emitter.instruction("ldr x13, [sp, #8]");                                   // reload the slot-index array
    emitter.instruction("ldr x10, [x13, x12, lsl #3]");                         // load the shuffled slot index
    emitter.instruction("ldr x11, [sp, #0]");                                   // reload the hash table
    emitter.instruction("ldr x14, [sp, #40]");                                  // reload the entry storage base
    hash_layout::emit_entry_address_from_storage(emitter, "x15", "x14", "x10");
    emitter.instruction("cbnz x12, __rt_hash_shuffle_links_prev");              // the head entry has no predecessor
    emitter.instruction("mov x16, #-1");                                        // the head's prev link is the end sentinel
    emitter.instruction("b __rt_hash_shuffle_links_store_prev");                // store the head sentinel
    emitter.label("__rt_hash_shuffle_links_prev");
    emitter.instruction("sub x17, x12, #1");                                    // index of the predecessor in the shuffled array
    emitter.instruction("ldr x16, [x13, x17, lsl #3]");                         // load the predecessor slot index
    emitter.label("__rt_hash_shuffle_links_store_prev");
    hash_layout::emit_store_entry_field(emitter, "x16", "x15", EntryField::Previous);
    emitter.instruction("add x17, x12, #1");                                    // index of the successor in the shuffled array
    emitter.instruction("cmp x17, x9");                                         // is this the final entry?
    emitter.instruction("b.lt __rt_hash_shuffle_links_next");                   // the tail's next link is the end sentinel
    emitter.instruction("mov x16, #-1");                                        // the tail's next link is the end sentinel
    emitter.instruction("b __rt_hash_shuffle_links_store_next");                // store the tail sentinel
    emitter.label("__rt_hash_shuffle_links_next");
    emitter.instruction("ldr x16, [x13, x17, lsl #3]");                         // load the successor slot index
    emitter.label("__rt_hash_shuffle_links_store_next");
    hash_layout::emit_store_entry_field(emitter, "x16", "x15", EntryField::Next);
    emitter.instruction("cbnz x12, __rt_hash_shuffle_links_no_head");           // only the first entry updates the head
    hash_layout::emit_store_header_field(emitter, "x10", "x11", HeaderField::Head);
    emitter.label("__rt_hash_shuffle_links_no_head");
    emitter.instruction("add x17, x12, #1");                                    // recompute the successor position
    emitter.instruction("cmp x17, x9");                                         // is this the final entry?
    emitter.instruction("b.ne __rt_hash_shuffle_links_no_tail");                // only the last entry updates the tail
    hash_layout::emit_store_header_field(emitter, "x10", "x11", HeaderField::Tail);
    emitter.label("__rt_hash_shuffle_links_no_tail");
    emitter.instruction("add x12, x12, #1");                                    // advance the relink cursor
    emitter.instruction("b __rt_hash_shuffle_links_relink");                    // continue relinking the shuffled chain

    emitter.label("__rt_hash_shuffle_links_relink_done");
    elephc_memoir::buffers::native_words::emit_release_frame_vector(emitter, 72);
    emitter.label("__rt_hash_shuffle_links_return");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the shuffle frame
    emitter.instruction("ret");                                                 // return after relinking the table

    emitter.label("__rt_hash_shuffle_links_null");
    emitter.instruction("ret");                                                 // return immediately for a null table
}

/// Emits the x86_64 chain shuffle with the same index-array contract.
fn emit_hash_shuffle_links_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_shuffle_links ---");
    emitter.label_global("__rt_hash_shuffle_links");

    emitter.instruction("test rax, rax");                                       // a null table has no chain to shuffle
    emitter.instruction("je __rt_hash_shuffle_links_null");                     // return without touching a null table
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the shuffle frame
    emitter.instruction("sub rsp, 64");                                         // reserve the walk, swap, and relink state
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the hash table
    hash_layout::emit_load_header_field(emitter, "r10", "rax", HeaderField::Count);
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the entry count
    emitter.instruction("cmp r10, 2");                                          // fewer than two entries are already shuffled
    emitter.instruction("jb __rt_hash_shuffle_links_return");                   // skip allocation for trivial tables
    elephc_memoir::buffers::native_words::emit_allocate_frame_vector(
        emitter, 24, 16, "__rt_hash_shuffle_links_allocated",
    );
    emitter.label("__rt_hash_shuffle_links_allocated");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the hash table
    hash_layout::emit_entries(emitter, "r11", "r10");
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // save the entry storage base
    hash_layout::emit_order_edge(emitter, "r10", "r10", false);
    emitter.instruction("xor r9d, r9d");                                        // start the slot walk at index zero

    emitter.label("__rt_hash_shuffle_links_walk");
    emitter.instruction("cmp r9, QWORD PTR [rbp - 24]");                        // have all entries been collected?
    emitter.instruction("jge __rt_hash_shuffle_links_walk_done");               // finish the walk once every slot is recorded
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the slot-index array
    emitter.instruction("mov QWORD PTR [r11 + r9 * 8], r10");                   // record this entry's slot index
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // reload the borrowed entry storage
    hash_layout::emit_consume_index_entry_address(emitter, "r11", "r10");
    hash_layout::emit_load_entry_field(emitter, "r10", "r11", EntryField::Next);
    emitter.instruction("add r9, 1");                                           // advance the walk index
    emitter.instruction("jmp __rt_hash_shuffle_links_walk");                    // continue collecting the chain

    emitter.label("__rt_hash_shuffle_links_walk_done");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the entry count
    emitter.instruction("sub r10, 1");                                          // start Fisher-Yates at the final index
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save the descending cursor

    emitter.label("__rt_hash_shuffle_links_swap");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the descending cursor
    emitter.instruction("cmp r10, 1");                                          // has the cursor reached the final swap boundary?
    emitter.instruction("jb __rt_hash_shuffle_links_swap_done");                // stop once every index above zero is swapped
    emitter.instruction("lea rdi, [r10 + 1]");                                  // pass the exclusive upper bound to the uniform draw
    abi::emit_call_label(emitter, "__rt_mt_uniform");                           // draw j in the inclusive range [0, i]
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the descending cursor after the call
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the slot-index array
    emitter.instruction("mov rdx, QWORD PTR [r11 + r10 * 8]");                  // load indices[i]
    emitter.instruction("mov rcx, QWORD PTR [r11 + rax * 8]");                  // load indices[j]
    emitter.instruction("mov QWORD PTR [r11 + r10 * 8], rcx");                  // indices[i] = indices[j]
    emitter.instruction("mov QWORD PTR [r11 + rax * 8], rdx");                  // indices[j] = the saved indices[i]
    emitter.instruction("sub r10, 1");                                          // move the cursor one index left
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save the updated cursor
    emitter.instruction("jmp __rt_hash_shuffle_links_swap");                    // continue the Fisher-Yates draw

    emitter.label("__rt_hash_shuffle_links_swap_done");
    emitter.instruction("xor r9d, r9d");                                        // start relinking at the new head position

    emitter.label("__rt_hash_shuffle_links_relink");
    emitter.instruction("cmp r9, QWORD PTR [rbp - 24]");                        // have all entries been relinked?
    emitter.instruction("jge __rt_hash_shuffle_links_relink_done");             // finish once the chain is complete
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the slot-index array
    emitter.instruction("mov r10, QWORD PTR [r11 + r9 * 8]");                   // load the shuffled slot index
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the borrowed entry storage
    hash_layout::emit_entry_address_from_storage(emitter, "r8", "rcx", "r10");
    emitter.instruction("test r9, r9");                                         // is this the head entry?
    emitter.instruction("jnz __rt_hash_shuffle_links_prev");                    // the head has no predecessor
    emitter.instruction("mov rdx, -1");                                         // the head's prev link is the end sentinel
    emitter.instruction("jmp __rt_hash_shuffle_links_store_prev");              // store the head sentinel
    emitter.label("__rt_hash_shuffle_links_prev");
    emitter.instruction("lea rcx, [r9 - 1]");                                   // index of the predecessor in the shuffled array
    emitter.instruction("mov rdx, QWORD PTR [r11 + rcx * 8]");                  // load the predecessor slot index
    emitter.label("__rt_hash_shuffle_links_store_prev");
    hash_layout::emit_store_entry_field(emitter, "rdx", "r8", EntryField::Previous);
    emitter.instruction("lea rcx, [r9 + 1]");                                   // index of the successor in the shuffled array
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 24]");                       // is this the final entry?
    emitter.instruction("jl __rt_hash_shuffle_links_next");                     // the tail's next link is the end sentinel
    emitter.instruction("mov rdx, -1");                                         // the tail's next link is the end sentinel
    emitter.instruction("jmp __rt_hash_shuffle_links_store_next");              // store the tail sentinel
    emitter.label("__rt_hash_shuffle_links_next");
    emitter.instruction("mov rdx, QWORD PTR [r11 + rcx * 8]");                  // load the successor slot index
    emitter.label("__rt_hash_shuffle_links_store_next");
    hash_layout::emit_store_entry_field(emitter, "rdx", "r8", EntryField::Next);
    emitter.instruction("test r9, r9");                                         // is this the head entry?
    emitter.instruction("jnz __rt_hash_shuffle_links_no_head");                 // only the first entry updates the head
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the hash table
    hash_layout::emit_store_header_field(emitter, "r10", "r8", HeaderField::Head);
    emitter.label("__rt_hash_shuffle_links_no_head");
    emitter.instruction("lea rcx, [r9 + 1]");                                   // recompute the successor position
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 24]");                       // is this the final entry?
    emitter.instruction("jne __rt_hash_shuffle_links_no_tail");                 // only the last entry updates the tail
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the hash table
    hash_layout::emit_store_header_field(emitter, "r10", "r8", HeaderField::Tail);
    emitter.label("__rt_hash_shuffle_links_no_tail");
    emitter.instruction("add r9, 1");                                           // advance the relink cursor
    emitter.instruction("jmp __rt_hash_shuffle_links_relink");                  // continue relinking the shuffled chain

    emitter.label("__rt_hash_shuffle_links_relink_done");
    elephc_memoir::buffers::native_words::emit_release_frame_vector(emitter, 16);
    emitter.label("__rt_hash_shuffle_links_return");
    emitter.instruction("add rsp, 64");                                         // release the shuffle frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return after relinking the table

    emitter.label("__rt_hash_shuffle_links_null");
    emitter.instruction("ret");                                                 // return immediately for a null table
}
