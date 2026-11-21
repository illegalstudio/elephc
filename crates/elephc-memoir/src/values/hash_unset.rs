//! Purpose:
//! Emits the `__rt_hash_unset` runtime helper assembly that removes a single key
//! from an associative-array hash table (PHP `unset($hash[$key])`).
//!
//! Called from:
//! - The compiler's array runtime emitter.
//!
//! Key details:
//! - Callers split and publish the unique table before entry, then this helper probes for
//!   the key, releases its string storage, unlinks and tombstones the entry, and decrements
//!   the live count before releasing the value owner.
//! - Value release can invoke PHP or throw; no table or entry writes occur after that call.
//! - The returned flag asks for cycle collection only when the removed value's own tag says it
//!   may own a graph; leaf values (int, float, bool, string, null), boxed or not, clear it.
//! - A missing key (or null/empty table) is a no-op. Callers must not republish the returned
//!   pointer: a destructor may already have replaced or freed the receiver's table.

use crate::hash_layout;
use crate::{Arch, Emitter};


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
/// Output: x0 = the incoming table pointer, possibly retired by a reentrant destructor;
///         x1 = 1 when the removed value may own a graph that can close a cycle (container,
///              object, boxed Mixed, reference, callable or resource), 0 for a missing key or a
///              removed int, float, bool, string or null
pub fn emit_hash_unset(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::X86_64 {
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
    //   [sp, #48] = removal flag, preserved across reentrant destructors
    //   [sp, #64] = saved x29
    //   [sp, #72] = saved x30
    emitter.instruction("sub sp, sp, #80");                                     // reserve aligned probe state and a callback-stable removal flag
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set up new frame pointer
    emitter.instruction("str xzr, [sp, #48]");                                  // null, empty and absent-key paths report no removal
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

    // -- compute entry address: entries + index * 64 --
    emitter.instruction("ldr x9, [sp, #24]");                                   // load current probe index
    emitter.instruction("mov x11, #64");                                        // entry size = 64 bytes per slot
    hash_layout::emit_entry_address(emitter, "x12", "x5", "x9");

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
    emitter.instruction("mov x9, #1");                                          // presence, including a null value, means this call changes the graph
    emitter.instruction("str x9, [sp, #48]");                                   // preserve this call's result across nested removals and destructors
    emitter.instruction("ldr x0, [sp, #0]");                                    // claim the value owner in the actual separated hash
    emitter.instruction("ldr x1, [sp, #8]");                                    // recover the key before releasing its owned bytes
    emitter.instruction("ldr x2, [sp, #16]");                                   // recover normalized integer or string key identity
    emitter.instruction("bl __rt_hash_write_guard_claim");                      // skip an owner already being destroyed by capture construction
    emitter.instruction("str x0, [sp, #32]");                                   // reuse the finished probe counter for value ownership
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload table pointer (key_eq clobbered registers)
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload matched probe index
    emitter.instruction("mov x11, #64");                                        // entry size = 64 bytes per slot
    hash_layout::emit_entry_address(emitter, "x12", "x5", "x9");
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
    emit_unlink_order(emitter, UnlinkSites::ordinary(), "x5", "x6", "x7", "x9", true);
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
    // -- only a value that may own a graph keeps the collection flag --
    emitter.instruction("cmp x14, #3");                                         // int, string, float and bool tags own no PHP children
    emitter.instruction("b.ls __rt_hash_unset_leaf_value");                     // a leaf value cannot strand a cycle
    emitter.instruction("cmp x14, #8");                                         // null owns nothing either
    emitter.instruction("b.eq __rt_hash_unset_leaf_value");                     // a removed null cannot strand a cycle
    emitter.instruction("cmp x14, #7");                                         // a boxed Mixed value hides its payload tag
    emitter.instruction("b.ne __rt_hash_unset_owner_value");                    // containers, objects and references keep the flag
    emitter.instruction("cbz x0, __rt_hash_unset_leaf_value");                  // a null box pointer reads as PHP null
    emitter.instruction("ldr x15, [x0]");                                       // read the boxed payload tag without unboxing
    emitter.instruction("cmp x15, #3");                                         // a boxed int, string, float or bool owns no children
    emitter.instruction("b.ls __rt_hash_unset_leaf_value");                     // the box itself cannot close a cycle
    emitter.instruction("cmp x15, #8");                                         // a boxed null owns nothing either
    emitter.instruction("b.ne __rt_hash_unset_owner_value");                    // nested boxes and graph payloads keep the flag
    emitter.label("__rt_hash_unset_leaf_value");
    emitter.instruction("str xzr, [sp, #48]");                                  // report a removal that needs no cycle collection
    emitter.label("__rt_hash_unset_owner_value");
    emitter.instruction("ldr x15, [sp, #32]");                                  // a capture guard may already own the detached payload
    emitter.instruction("cbz x15, __rt_hash_unset_done");                       // leave that owner for the outer release
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
    emitter.instruction("ldr x1, [sp, #48]");                                   // report whether this invocation removed an entry
    emitter.instruction("ldr x0, [sp, #0]");                                    // preserve the legacy pointer result without dereferencing it
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate the stack frame
    emitter.instruction("ret");                                                 // return to caller

    // -- null table: return null without touching memory --
    emitter.label("__rt_hash_unset_null");
    emitter.instruction("mov x1, #0");                                          // a null table cannot remove an entry
    emitter.instruction("mov x0, #0");                                          // null table stays null
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate the stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux variant of `__rt_hash_unset`.
///
/// Uses the SysV ABI: rdi = hash table pointer, rsi = key_lo, rdx = key_hi
/// (key_hi = -1 means an integer key). The caller has already split and published the table.
/// Mirrors the AArch64 logic: linear probe, key release, unlink, tombstone, count decrement,
/// then detached value release. The returned input pointer is not safe for receiver writeback.
/// Returns the legacy table pointer in rax and the per-call collection flag in rdx (set only for
/// a removed value that may own a graph).
fn emit_hash_unset_linux_x86_64(emitter: &mut impl Emitter) {
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
    //   [rbp - 56] = removal flag, preserved across reentrant destructors
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 64");                                         // reserve aligned probe state and a callback-stable removal flag
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // null, empty and absent-key paths report no removal
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
    hash_layout::emit_entry_address(emitter, "r8", "r10", "r11");
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
    emitter.instruction("mov QWORD PTR [rbp - 56], 1");                         // preserve presence across callbacks, including null-valued entries
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // claim the value owner in the actual separated hash
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // recover the key before releasing its owned bytes
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // recover normalized integer or string key identity
    emitter.instruction("call __rt_hash_write_guard_claim");                    // skip a value owner already held by an outer release
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // reuse the finished probe counter for value ownership
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload table pointer (key_eq clobbered it)
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload matched probe index
    hash_layout::emit_entry_address(emitter, "r8", "r10", "r11");
    emitter.instruction("mov QWORD PTR [rbp - 48], r8");                        // save the matched entry address

    // -- release the owned string key (integer keys have key_hi == -1 and own nothing) --
    emitter.instruction("cmp QWORD PTR [r8 + 16], -1");                         // inline integer key?
    emitter.instruction("je __rt_hash_unset_after_key");                        // integer keys own no heap storage
    emitter.instruction("mov rax, QWORD PTR [r8 + 8]");                         // load the persisted key pointer
    emitter.instruction("test rax, rax");                                       // defensively skip a missing key pointer
    emitter.instruction("jz __rt_hash_unset_after_key");                        // nothing to release
    emitter.instruction("mov r9, QWORD PTR [rax - 8]");                         // load the key heap kind word
    emitter.instruction("shr r9, 32");                                          // isolate the high-word heap marker
    emitter.instruction(&format!("cmp r9d, 0x{:x}", crate::layout::X86_64_HEAP_MAGIC_HI32)); // elephc-owned key?
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
    emit_unlink_order(emitter, UnlinkSites::ordinary(), "r10", "rsi", "rdi", "rcx", false);
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
    // -- only a value that may own a graph keeps the collection flag --
    emitter.instruction("cmp r9, 3");                                           // int, string, float and bool tags own no PHP children
    emitter.instruction("jbe __rt_hash_unset_leaf_value");                      // a leaf value cannot strand a cycle
    emitter.instruction("cmp r9, 8");                                           // null owns nothing either
    emitter.instruction("je __rt_hash_unset_leaf_value");                       // a removed null cannot strand a cycle
    emitter.instruction("cmp r9, 7");                                           // a boxed Mixed value hides its payload tag
    emitter.instruction("jne __rt_hash_unset_owner_value");                     // containers, objects and references keep the flag
    emitter.instruction("test rax, rax");                                       // a null box pointer reads as PHP null
    emitter.instruction("jz __rt_hash_unset_leaf_value");                       // treat the null box as a leaf
    emitter.instruction("mov r11, QWORD PTR [rax]");                            // read the boxed payload tag without unboxing
    emitter.instruction("cmp r11, 3");                                          // a boxed int, string, float or bool owns no children
    emitter.instruction("jbe __rt_hash_unset_leaf_value");                      // the box itself cannot close a cycle
    emitter.instruction("cmp r11, 8");                                          // a boxed null owns nothing either
    emitter.instruction("jne __rt_hash_unset_owner_value");                     // nested boxes and graph payloads keep the flag
    emitter.label("__rt_hash_unset_leaf_value");
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // report a removal that needs no cycle collection
    emitter.label("__rt_hash_unset_owner_value");
    emitter.instruction("cmp QWORD PTR [rbp - 40], 0");                         // a capture guard may already own the detached payload
    emitter.instruction("je __rt_hash_unset_done");                             // leave that owner for the outer release
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
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // report whether this invocation removed an entry
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // preserve the legacy pointer result without dereferencing it
    emitter.instruction("add rsp, 64");                                         // release the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to caller

    // -- null table: return null without touching memory --
    emitter.label("__rt_hash_unset_null");
    emitter.instruction("xor edx, edx");                                        // a null table cannot remove an entry
    emitter.instruction("xor eax, eax");                                        // null table stays null
    emitter.instruction("add rsp, 64");                                         // release the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// Branch labels around the two insertion-order repairs and final tombstone.
struct UnlinkSites<'a> {
    previous: &'a str,
    fix_next: &'a str,
    following: &'a str,
    tombstone: &'a str,
}

impl UnlinkSites<'_> {
    /// Keeps the ordinary unset helper's existing branch layout.
    fn ordinary() -> Self {
        Self {
            previous: "__rt_hash_unset_prev_entry",
            fix_next: "__rt_hash_unset_fix_next",
            following: "__rt_hash_unset_next_entry",
            tombstone: "__rt_hash_unset_tombstone",
        }
    }
}

/// Removes an entry from insertion order without touching its detached owners.
fn emit_unlink_order(
    emitter: &mut impl Emitter, sites: UnlinkSites<'_>, hash: &str, prev: &str,
    next: &str, scratch: &str, preserve_arm_slot_size_load: bool,
) {
    let arm = emitter.arch() == Arch::AArch64;
    for (index, value, header_offset, link_offset, neighbour, continuation) in [
        (prev, next, 24, 56, sites.previous, sites.fix_next),
        (next, prev, 32, 48, sites.following, sites.tombstone),
    ] {
        let (not_equal, jump) = if arm {
            emitter.instruction(&format!("cmn {index}, #1"));                   // identify an insertion-order endpoint
            ("b.ne", "b")
        } else {
            emitter.instruction(&format!("cmp {index}, -1"));                   // identify an insertion-order endpoint
            ("jne", "jmp")
        };
        emitter.instruction(&format!("{not_equal} {neighbour}"));               // repair the neighboring entry when present
        emitter.emit_store_to_address(value, hash, header_offset);
        emitter.instruction(&format!("{jump} {continuation}"));                 // finish this half of the unlink
        emitter.label(neighbour);
        if preserve_arm_slot_size_load {
            emitter.instruction("mov x11, #64");                                // preserve the ordinary unset helper's register sequence
        }
        hash_layout::emit_entry_address(emitter, scratch, hash, index);
        emitter.emit_store_to_address(value, scratch, link_offset);
        emitter.label(continuation);
    }
}

/// Unlinks a pinned entry and publishes its tombstone and updated count.
/// The caller has staged its owners and will retire them after the mutation.
pub fn emit_unlink_pinned_entry(emitter: &mut impl Emitter, prefix: &str) {
    let arm = emitter.arch() == Arch::AArch64;
    emitter.instruction(if arm { "ldr x5, [sp]" } else { "mov r10, QWORD PTR [rsp]" }); // recover the selected stable hash header
    emitter.instruction(if arm { "ldp x6, x7, [x4, #48]" } else { "mov r9, QWORD PTR [r8 + 48]" }); // recover insertion-order predecessor and successor
    if !arm { emitter.instruction("mov rdx, QWORD PTR [r8 + 56]"); }            // preserve the successor while fixing both links

    let (hash, prev, next, scratch) = if arm {
        ("x5", "x6", "x7", "x9")
    } else {
        ("r10", "r9", "rdx", "r11")
    };
    emit_unlink_order(emitter, UnlinkSites {
        previous: &format!("{prefix}_previous"),
        fix_next: &format!("{prefix}_next"),
        following: &format!("{prefix}_following"),
        tombstone: &format!("{prefix}_tombstone"),
    }, hash, prev, next, scratch, false);
    if arm {
        emitter.instruction("mov x9, #2");                                      // preserve probe chains with a tombstone
        emitter.instruction("str x9, [x4]");                                    // make the removed key absent before callbacks
        emitter.instruction("ldr x9, [x5]");                                    // recover the current number of live entries
        emitter.instruction("sub x9, x9, #1");                                  // account for the completed deletion
        emitter.instruction("str x9, [x5]");                                    // publish count while leaving automatic-key history unchanged
    } else {
        emitter.instruction("mov QWORD PTR [r8], 2");                           // make the removed key absent while preserving probe chains
        emitter.instruction("sub QWORD PTR [r10], 1");                          // publish live count without rewinding automatic-key history
    }
}
