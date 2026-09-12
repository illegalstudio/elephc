//! Purpose:
//! Emits the `__rt_hash_unset` runtime helper assembly that removes a single key
//! from an associative-array hash table (PHP `unset($hash[$key])`).
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Callers split and publish the unique table before entry, then this helper probes for
//!   the key, releases its string storage, unlinks and tombstones the entry, and decrements
//!   the live count before releasing the value owner.
//! - Value release can invoke PHP or throw; no table or entry writes occur after that call.
//! - A missing key (or null/empty table) is a no-op. Callers must not republish the returned
//!   pointer: a destructor may already have replaced or freed the receiver's table.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;


/// Emits the `__rt_hash_unset` runtime helper.
///
/// Removes the entry matching the supplied key from a hash table, releasing its owned key
/// and value payloads and preserving probe chains (tombstone) and insertion order (chain
/// unlink). The caller must pass a unique table already installed in its owner storage;
/// no copy-on-write split or receiver writeback may happen after removal starts.
///
/// Input:  x0 = hash table pointer, x1 = key_lo, x2 = key_hi
///         (key_hi = -1 means an integer key with key_lo = value; otherwise a string key
///          with key_lo = pointer and key_hi = length)
/// Output: x0 = the incoming table pointer, possibly retired by a reentrant destructor
pub fn emit_hash_unset(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_unset_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_unset ---");
    emitter.label_global("__rt_hash_unset");

    // -- set up stack frame --
    // Stack layout:
    //   [sp, #0]  = unique hash table pointer
    //   [sp, #8]  = key_lo
    //   [sp, #16] = key_hi
    //   [sp, #24] = current probe index
    //   [sp, #32] = probe count
    //   [sp, #40] = matched entry address
    //   [sp, #48] = saved x29
    //   [sp, #56] = saved x30
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("cbz x0, __rt_hash_unset_null");                        // null tables hold no keys to remove
    emitter.instruction("str x1, [sp, #8]");                                    // save key_lo across the COW/hash helper calls
    emitter.instruction("str x2, [sp, #16]");                                   // save key_hi across the COW/hash helper calls

    // -- the caller has already published unique storage before any destructor can run --
    emitter.instruction("str x0, [sp, #0]");                                    // save the unique table pointer
    emitter.instruction("ldr x5, [x0, #8]");                                    // load capacity before hashing to avoid divide-by-zero
    emitter.instruction("cbz x5, __rt_hash_unset_done");                        // empty table: nothing to remove

    // -- hash the key to find the starting slot --
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload key_lo for the hash helper
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload key_hi for the hash helper
    emitter.instruction("bl __rt_hash_key_hash");                               // x0 = 64-bit hash of the normalized key
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload the unique table pointer
    emitter.instruction("ldr x6, [x5, #8]");                                    // x6 = capacity from header
    emitter.instruction("udiv x7, x0, x6");                                     // x7 = hash / capacity
    emitter.instruction("msub x8, x7, x6, x0");                                 // x8 = hash % capacity = initial slot
    emitter.instruction("str x8, [sp, #24]");                                   // save the initial probe index
    emitter.instruction("str xzr, [sp, #32]");                                  // probe count = 0

    // -- linear probe loop --
    emitter.label("__rt_hash_unset_probe");
    emitter.instruction("ldr x10, [sp, #32]");                                  // load probe count
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload table pointer
    emitter.instruction("ldr x6, [x5, #8]");                                    // reload capacity
    emitter.instruction("cmp x10, x6");                                         // probed every slot yet?
    emitter.instruction("b.ge __rt_hash_unset_done");                           // full table without a match: no-op

    // -- compute entry address: base + 40 + index * 64 --
    emitter.instruction("ldr x9, [sp, #24]");                                   // load current probe index
    emitter.instruction("mov x11, #64");                                        // entry size = 64 bytes per slot
    emitter.instruction("mul x12, x9, x11");                                    // x12 = index * 64
    emitter.instruction("add x12, x5, x12");                                    // x12 = table base + index * 64
    emitter.instruction("add x12, x12, #40");                                   // x12 = entry address (skip 40-byte header)

    // -- inspect the occupied marker --
    emitter.instruction("ldr x13, [x12]");                                      // x13 = occupied flag
    emitter.instruction("cbz x13, __rt_hash_unset_done");                       // empty slot (0): key absent, no-op
    emitter.instruction("cmp x13, #2");                                         // tombstone?
    emitter.instruction("b.eq __rt_hash_unset_next");                           // tombstones never match: keep probing

    // -- occupied slot: compare keys --
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = our key_lo
    emitter.instruction("ldr x2, [sp, #16]");                                   // x2 = our key_hi
    emitter.instruction("ldr x3, [x12, #8]");                                   // x3 = entry key_lo
    emitter.instruction("ldr x4, [x12, #16]");                                  // x4 = entry key_hi
    emitter.instruction("bl __rt_hash_key_eq");                                 // x0 = 1 when the normalized keys match
    emitter.instruction("cbnz x0, __rt_hash_unset_found");                      // matched: remove this entry

    // -- advance to the next slot (wrapping) --
    emitter.label("__rt_hash_unset_next");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload current probe index
    emitter.instruction("add x9, x9, #1");                                      // index += 1
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload table pointer
    emitter.instruction("ldr x6, [x5, #8]");                                    // reload capacity
    emitter.instruction("udiv x7, x9, x6");                                     // x7 = index / capacity
    emitter.instruction("msub x9, x7, x6, x9");                                 // x9 = index % capacity (wrap around)
    emitter.instruction("str x9, [sp, #24]");                                   // save updated probe index
    emitter.instruction("ldr x10, [sp, #32]");                                  // load probe count
    emitter.instruction("add x10, x10, #1");                                    // probe count += 1
    emitter.instruction("str x10, [sp, #32]");                                  // save updated probe count
    emitter.instruction("b __rt_hash_unset_probe");                             // probe the next slot

    // -- key found: release the key, unlink the entry, then release its value --
    emitter.label("__rt_hash_unset_found");
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload table pointer (key_eq clobbered registers)
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload matched probe index
    emitter.instruction("mov x11, #64");                                        // entry size = 64 bytes per slot
    emitter.instruction("mul x12, x9, x11");                                    // x12 = index * 64
    emitter.instruction("add x12, x5, x12");                                    // x12 = table base + index * 64
    emitter.instruction("add x12, x12, #40");                                   // x12 = matched entry address
    emitter.instruction("str x12, [sp, #40]");                                  // save the matched entry address

    // -- release the owned string key (integer keys have key_hi == -1 and own nothing) --
    emitter.instruction("ldr x15, [x12, #16]");                                 // load the entry key_hi
    emitter.instruction("cmn x15, #1");                                         // is this an inline integer key (key_hi == -1)?
    emitter.instruction("b.eq __rt_hash_unset_after_key");                      // integer keys own no heap storage
    emitter.instruction("ldr x0, [x12, #8]");                                   // load the persisted key pointer
    emitter.instruction("ldr w15, [x0, #-12]");                                 // load the key refcount from the heap header
    emitter.instruction("subs w15, w15, #1");                                   // drop this table's ownership of the key
    emitter.instruction("str w15, [x0, #-12]");                                 // store the decremented key refcount
    emitter.instruction("b.ne __rt_hash_unset_after_key");                      // key still shared: keep it alive
    emitter.instruction("bl __rt_heap_free");                                   // last owner: free the persisted key storage

    emitter.label("__rt_hash_unset_after_key");
    // -- unlink the entry from the insertion-order chain --
    emitter.label("__rt_hash_unset_unlink");
    emitter.instruction("ldr x12, [sp, #40]");                                  // reload the matched entry address
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload the table pointer
    emitter.instruction("ldr x6, [x12, #48]");                                  // x6 = prev slot index (-1 when first)
    emitter.instruction("ldr x7, [x12, #56]");                                  // x7 = next slot index (-1 when last)
    emitter.instruction("cmn x6, #1");                                          // does this entry have no predecessor (prev == -1)?
    emitter.instruction("b.ne __rt_hash_unset_prev_entry");                     // patch the predecessor's forward link instead
    emitter.instruction("str x7, [x5, #24]");                                   // head = next: new first entry in iteration order
    emitter.instruction("b __rt_hash_unset_fix_next");                          // continue to fix the successor's back link

    emitter.label("__rt_hash_unset_prev_entry");
    emitter.instruction("mov x11, #64");                                        // entry size = 64 bytes per slot
    emitter.instruction("mul x9, x6, x11");                                     // x9 = prev_index * 64
    emitter.instruction("add x9, x5, x9");                                      // x9 = table base + prev_index * 64
    emitter.instruction("add x9, x9, #40");                                     // x9 = predecessor entry address
    emitter.instruction("str x7, [x9, #56]");                                   // predecessor.next = our next, skipping us

    emitter.label("__rt_hash_unset_fix_next");
    emitter.instruction("cmn x7, #1");                                          // does this entry have no successor (next == -1)?
    emitter.instruction("b.ne __rt_hash_unset_next_entry");                     // patch the successor's back link instead
    emitter.instruction("str x6, [x5, #32]");                                   // tail = prev: new last entry in iteration order
    emitter.instruction("b __rt_hash_unset_tombstone");                         // continue to tombstone the slot

    emitter.label("__rt_hash_unset_next_entry");
    emitter.instruction("mov x11, #64");                                        // entry size = 64 bytes per slot
    emitter.instruction("mul x9, x7, x11");                                     // x9 = next_index * 64
    emitter.instruction("add x9, x5, x9");                                      // x9 = table base + next_index * 64
    emitter.instruction("add x9, x9, #40");                                     // x9 = successor entry address
    emitter.instruction("str x6, [x9, #48]");                                   // successor.prev = our prev, skipping us

    // -- tombstone the slot and decrement the live count --
    emitter.label("__rt_hash_unset_tombstone");
    emitter.instruction("ldr x12, [sp, #40]");                                  // reload the matched entry address
    emitter.instruction("mov x13, #2");                                         // tombstone marker keeps probe chains intact
    emitter.instruction("str x13, [x12]");                                      // mark the slot as a tombstone
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload the table pointer
    emitter.instruction("ldr x14, [x5, #0]");                                   // load the live entry count
    emitter.instruction("sub x14, x14, #1");                                    // one fewer live entry after removal
    emitter.instruction("str x14, [x5, #0]");                                   // store the decremented count

    // -- detach the retired payload before any PHP destructor can reenter this table --
    emitter.instruction("ldr x0, [x12, #24]");                                  // take the removed value owner before its tombstone can be reused
    emitter.instruction("ldr x14, [x12, #40]");                                 // keep the value tag outside the retired entry
    emitter.instruction("stp xzr, xzr, [x12, #8]");                             // clear the released key pointer and key length
    emitter.instruction("stp xzr, xzr, [x12, #24]");                            // clear the removed payload words before callback entry
    emitter.instruction("str xzr, [x12, #40]");                                 // retire the old value tag with its payload
    emitter.instruction("cmp x14, #8");                                         // null values have no heap owner
    emitter.instruction("b.eq __rt_hash_unset_done");                           // return with the removal already committed
    emitter.instruction("cmp x14, #1");                                         // string values release through the uniform dispatcher
    emitter.instruction("b.eq __rt_hash_unset_release_any");                    // retire the detached string owner
    emitter.instruction("cmp x14, #10");                                        // callable descriptors require their dedicated release
    emitter.instruction("b.eq __rt_hash_unset_release_callable");               // retire the detached descriptor owner
    emitter.instruction("cmp x14, #4");                                         // container and object tags carry heap owners
    emitter.instruction("b.lo __rt_hash_unset_done");                           // scalar values need no release
    emitter.label("__rt_hash_unset_release_any");
    emitter.instruction("bl __rt_decref_any");                                  // release only after the table is consistent for reentrant PHP
    emitter.instruction("b __rt_hash_unset_done");                              // never rewrite a table that the destructor may have replaced
    emitter.label("__rt_hash_unset_release_callable");
    emitter.instruction("bl __rt_callable_descriptor_release");                 // release captures after retiring their table entry

    // -- return the unique table pointer --
    emitter.label("__rt_hash_unset_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the unique (possibly cloned) table
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate the stack frame
    emitter.instruction("ret");                                                 // return to caller

    // -- null table: return null without touching memory --
    emitter.label("__rt_hash_unset_null");
    emitter.instruction("mov x0, #0");                                          // null table stays null
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate the stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux variant of `__rt_hash_unset`.
///
/// Uses the SysV ABI: rdi = hash table pointer, rsi = key_lo, rdx = key_hi
/// (key_hi = -1 means an integer key). The caller has already split and published the table.
/// Mirrors the AArch64 logic: linear probe, key release, unlink, tombstone, count decrement,
/// then detached value release. The returned input pointer is not safe for receiver writeback.
fn emit_hash_unset_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_unset ---");
    emitter.label_global("__rt_hash_unset");

    // Stack layout (relative to rbp):
    //   [rbp - 8]  = unique hash table pointer
    //   [rbp - 16] = key_lo
    //   [rbp - 24] = key_hi
    //   [rbp - 32] = current probe index
    //   [rbp - 40] = probe count
    //   [rbp - 48] = matched entry address
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 48");                                         // reserve spill slots for table/key/probe state
    emitter.instruction("test rdi, rdi");                                       // null tables hold no keys to remove
    emitter.instruction("jz __rt_hash_unset_null");                             // return null without touching memory
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save key_lo across helper calls
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save key_hi across helper calls

    // -- the caller has already published unique storage before any destructor can run --
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the unique table pointer
    emitter.instruction("mov r11, QWORD PTR [rdi + 8]");                        // load capacity before hashing
    emitter.instruction("test r11, r11");                                       // empty table?
    emitter.instruction("jz __rt_hash_unset_done");                             // nothing to remove from an empty table

    // -- hash the key to find the starting slot --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // key_lo for the hash helper
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // key_hi for the hash helper
    emitter.instruction("call __rt_hash_key_hash");                             // rax = 64-bit hash of the normalized key
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the unique table pointer
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // r11 = capacity
    emitter.instruction("xor edx, edx");                                        // clear the high dividend half before dividing
    emitter.instruction("div r11");                                             // rdx = hash % capacity = initial slot
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // save the initial probe index
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // probe count = 0

    // -- linear probe loop --
    emitter.label("__rt_hash_unset_probe");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload table pointer
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // reload capacity
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // load probe count
    emitter.instruction("cmp rdx, r11");                                        // probed every slot yet?
    emitter.instruction("jae __rt_hash_unset_done");                            // full table without a match: no-op
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // load current probe index
    emitter.instruction("mov r8, r11");                                         // copy index before scaling it
    emitter.instruction("shl r8, 6");                                           // index * 64 = byte offset
    emitter.instruction("add r8, r10");                                         // table base + offset
    emitter.instruction("add r8, 40");                                          // skip the 40-byte header to the entry
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the occupied marker
    emitter.instruction("test r9, r9");                                         // empty slot (0)?
    emitter.instruction("jz __rt_hash_unset_done");                             // key absent: no-op
    emitter.instruction("cmp r9, 2");                                           // tombstone?
    emitter.instruction("je __rt_hash_unset_next");                             // tombstones never match: keep probing
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // our key_lo
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // our key_hi
    emitter.instruction("mov rdx, QWORD PTR [r8 + 8]");                         // entry key_lo
    emitter.instruction("mov rcx, QWORD PTR [r8 + 16]");                        // entry key_hi
    emitter.instruction("call __rt_hash_key_eq");                               // rax = 1 when the normalized keys match
    emitter.instruction("test rax, rax");                                       // matched?
    emitter.instruction("jne __rt_hash_unset_found");                           // remove this entry

    // -- advance to the next slot (wrapping) --
    emitter.label("__rt_hash_unset_next");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload table pointer
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // reload capacity
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // load current probe index
    emitter.instruction("add rdx, 1");                                          // index += 1
    emitter.instruction("cmp rdx, r11");                                        // reached capacity?
    emitter.instruction("jb __rt_hash_unset_store_probe");                      // still in bounds: keep the index
    emitter.instruction("xor edx, edx");                                        // wrap the probe cursor back to slot zero

    emitter.label("__rt_hash_unset_store_probe");
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // persist the updated probe index
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // load probe count
    emitter.instruction("add rdx, 1");                                          // probe count += 1
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // persist the updated probe count
    emitter.instruction("jmp __rt_hash_unset_probe");                           // probe the next slot

    // -- key found: release the key, unlink the entry, then release its value --
    emitter.label("__rt_hash_unset_found");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload table pointer (key_eq clobbered it)
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload matched probe index
    emitter.instruction("mov r8, r11");                                         // copy index before scaling it
    emitter.instruction("shl r8, 6");                                           // index * 64 = byte offset
    emitter.instruction("add r8, r10");                                         // table base + offset
    emitter.instruction("add r8, 40");                                          // skip the 40-byte header to the entry
    emitter.instruction("mov QWORD PTR [rbp - 48], r8");                        // save the matched entry address

    // -- release the owned string key (integer keys have key_hi == -1 and own nothing) --
    emitter.instruction("cmp QWORD PTR [r8 + 16], -1");                         // inline integer key?
    emitter.instruction("je __rt_hash_unset_after_key");                        // integer keys own no heap storage
    emitter.instruction("mov rax, QWORD PTR [r8 + 8]");                         // load the persisted key pointer
    emitter.instruction("test rax, rax");                                       // defensively skip a missing key pointer
    emitter.instruction("jz __rt_hash_unset_after_key");                        // nothing to release
    emitter.instruction("mov r9, QWORD PTR [rax - 8]");                         // load the key heap kind word
    emitter.instruction("shr r9, 32");                                          // isolate the high-word heap marker
    emitter.instruction(&format!("cmp r9d, 0x{:x}", crate::codegen_support::sentinels::X86_64_HEAP_MAGIC_HI32)); // elephc-owned key?
    emitter.instruction("jne __rt_hash_unset_after_key");                       // skip foreign key pointers
    emitter.instruction("mov r9d, DWORD PTR [rax - 12]");                       // load the key refcount from the header
    emitter.instruction("sub r9d, 1");                                          // drop this table's ownership of the key
    emitter.instruction("mov DWORD PTR [rax - 12], r9d");                       // store the decremented key refcount
    emitter.instruction("jnz __rt_hash_unset_after_key");                       // key still shared: keep it alive
    emitter.instruction("call __rt_heap_free");                                 // last owner: free the persisted key (rax = ptr)

    emitter.label("__rt_hash_unset_after_key");
    // -- unlink the entry from the insertion-order chain --
    emitter.label("__rt_hash_unset_unlink");
    emitter.instruction("mov r8, QWORD PTR [rbp - 48]");                        // reload the matched entry address
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the table pointer
    emitter.instruction("mov rsi, QWORD PTR [r8 + 48]");                        // rsi = prev slot index (-1 when first)
    emitter.instruction("mov rdi, QWORD PTR [r8 + 56]");                        // rdi = next slot index (-1 when last)
    emitter.instruction("cmp rsi, -1");                                         // does this entry have no predecessor?
    emitter.instruction("jne __rt_hash_unset_prev_entry");                      // patch the predecessor's forward link
    emitter.instruction("mov QWORD PTR [r10 + 24], rdi");                       // head = next: new first entry in order
    emitter.instruction("jmp __rt_hash_unset_fix_next");                        // continue to fix the successor's back link

    emitter.label("__rt_hash_unset_prev_entry");
    emitter.instruction("mov rcx, rsi");                                        // copy prev index before scaling it
    emitter.instruction("shl rcx, 6");                                          // prev_index * 64 = byte offset
    emitter.instruction("add rcx, r10");                                        // table base + offset
    emitter.instruction("add rcx, 40");                                         // skip the header to the predecessor entry
    emitter.instruction("mov QWORD PTR [rcx + 56], rdi");                       // predecessor.next = our next, skipping us

    emitter.label("__rt_hash_unset_fix_next");
    emitter.instruction("cmp rdi, -1");                                         // does this entry have no successor?
    emitter.instruction("jne __rt_hash_unset_next_entry");                      // patch the successor's back link
    emitter.instruction("mov QWORD PTR [r10 + 32], rsi");                       // tail = prev: new last entry in order
    emitter.instruction("jmp __rt_hash_unset_tombstone");                       // continue to tombstone the slot

    emitter.label("__rt_hash_unset_next_entry");
    emitter.instruction("mov rcx, rdi");                                        // copy next index before scaling it
    emitter.instruction("shl rcx, 6");                                          // next_index * 64 = byte offset
    emitter.instruction("add rcx, r10");                                        // table base + offset
    emitter.instruction("add rcx, 40");                                         // skip the header to the successor entry
    emitter.instruction("mov QWORD PTR [rcx + 48], rsi");                       // successor.prev = our prev, skipping us

    // -- tombstone the slot and decrement the live count --
    emitter.label("__rt_hash_unset_tombstone");
    emitter.instruction("mov r8, QWORD PTR [rbp - 48]");                        // reload the matched entry address
    emitter.instruction("mov QWORD PTR [r8], 2");                               // tombstone marker keeps probe chains intact
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the table pointer
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // load the live entry count
    emitter.instruction("sub rax, 1");                                          // one fewer live entry after removal
    emitter.instruction("mov QWORD PTR [r10], rax");                            // store the decremented count

    // -- detach the retired payload before any PHP destructor can reenter this table --
    emitter.instruction("mov rax, QWORD PTR [r8 + 24]");                        // take the removed value owner before its tombstone can be reused
    emitter.instruction("mov r9, QWORD PTR [r8 + 40]");                         // keep the value tag outside the retired entry
    emitter.instruction("mov QWORD PTR [r8 + 8], 0");                           // clear the released key pointer
    emitter.instruction("mov QWORD PTR [r8 + 16], 0");                          // clear the retired key length
    emitter.instruction("mov QWORD PTR [r8 + 24], 0");                          // detach the payload before callback entry
    emitter.instruction("mov QWORD PTR [r8 + 32], 0");                          // clear the payload high word
    emitter.instruction("mov QWORD PTR [r8 + 40], 0");                          // retire the old value tag with its payload
    emitter.instruction("cmp r9, 8");                                           // null values have no heap owner
    emitter.instruction("je __rt_hash_unset_done");                             // return with the removal already committed
    emitter.instruction("cmp r9, 1");                                           // string values release through the uniform dispatcher
    emitter.instruction("je __rt_hash_unset_release_any");                      // retire the detached string owner
    emitter.instruction("cmp r9, 10");                                          // callable descriptors require their dedicated release
    emitter.instruction("je __rt_hash_unset_release_callable");                 // retire the detached descriptor owner
    emitter.instruction("cmp r9, 4");                                           // container and object tags carry heap owners
    emitter.instruction("jb __rt_hash_unset_done");                             // scalar values need no release
    emitter.label("__rt_hash_unset_release_any");
    emitter.instruction("call __rt_decref_any");                                // release only after the table is consistent for reentrant PHP
    emitter.instruction("jmp __rt_hash_unset_done");                            // never rewrite a table that the destructor may have replaced
    emitter.label("__rt_hash_unset_release_callable");
    emitter.instruction("call __rt_callable_descriptor_release");               // release captures after retiring their table entry

    // -- return the unique table pointer --
    emitter.label("__rt_hash_unset_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the unique (possibly cloned) table
    emitter.instruction("add rsp, 48");                                         // release the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to caller

    // -- null table: return null without touching memory --
    emitter.label("__rt_hash_unset_null");
    emitter.instruction("xor eax, eax");                                        // null table stays null
    emitter.instruction("add rsp, 48");                                         // release the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{AppleVariant, Platform, Target};

    /// All targets detach the entry before callbacks and leave reentrant receiver updates intact.
    #[test]
    fn hash_unset_commits_removal_before_releasing_values_on_every_target() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new_apple(Arch::AArch64, AppleVariant::IOS),
            Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_hash_unset(&mut emitter);
            let asm = emitter.output();
            assert!(!asm.contains("__rt_hash_ensure_unique"), "{target:?}: {asm}");
            let unlink = asm.find("__rt_hash_unset_unlink:").unwrap();
            let tombstone = asm.find("__rt_hash_unset_tombstone:").unwrap();
            let release = asm.find("__rt_hash_unset_release_any:").unwrap();
            let (count_write, tag_clear, heap_call, descriptor_call) = match target.arch {
                Arch::AArch64 => (
                    "str x14, [x5, #0]", "str xzr, [x12, #40]",
                    "bl __rt_decref_any", "bl __rt_callable_descriptor_release",
                ),
                Arch::X86_64 => (
                    "mov QWORD PTR [r10], rax", "mov QWORD PTR [r8 + 40], 0",
                    "call __rt_decref_any", "call __rt_callable_descriptor_release",
                ),
            };
            let count = asm.find(count_write).unwrap();
            let clear = asm.find(tag_clear).unwrap();
            assert!(unlink < tombstone && tombstone < count && count < clear && clear < release,
                "removal must be visible before callbacks on {target:?}: {asm}");
            assert!(asm.find(heap_call).unwrap() > release, "{target:?}: {asm}");
            assert!(asm.find(descriptor_call).unwrap() > release, "{target:?}: {asm}");
            let after_release = &asm[release..];
            for table_register in ["[x12", "[x5", "[r8", "[r10"] {
                assert!(!after_release.contains(table_register),
                    "retired table accessed after callback on {target:?}: {after_release}");
            }
        }
    }
}
