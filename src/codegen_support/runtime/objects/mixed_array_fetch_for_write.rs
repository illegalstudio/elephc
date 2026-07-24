//! Purpose:
//! Emits the fetch-for-write runtime helpers used by nested array assignments
//! whose parent element may be missing (`$a[7][1] = ...` when `$a[7]` does not
//! exist): `__rt_mixed_array_get_for_write`, `__rt_array_ensure_elem_for_write`,
//! `__rt_mixed_cell_promote_to_hash`, and `__rt_mixed_new_empty_array_cell`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::objects`.
//!
//! Key details:
//! - `__rt_mixed_array_get_for_write(cell, key_lo, key_hi)` mirrors
//!   `__rt_mixed_array_get` but with PHP write-context semantics (issue #555):
//!   a missing / null-gap / boxed-null element is autovivified as an empty
//!   indexed array whose boxed cell is INSTALLED into the parent storage, and
//!   the STORED cell is returned (retained) so a following
//!   `__rt_mixed_array_set` writes through the parent container. No
//!   undefined-key warning is emitted: PHP is silent for legal autovivifying
//!   writes.
//! - Indexed payloads are first normalized through `__rt_array_to_mixed`
//!   (COW split + boxed slots) and the unique pointer is republished into the
//!   receiver cell; this also returns STORED cells for concrete intermediate
//!   slots, fixing write-through for 3+ level chains (#553 residual).
//! - A string key on an indexed payload promotes the payload to hash storage
//!   via `__rt_mixed_cell_promote_to_hash` (PHP arrays accept mixed keys);
//!   incompatible scalar parents are NOT converted: the stored cell is
//!   returned unchanged and the following set drops the write.
//! - `__rt_array_ensure_elem_for_write(container, tag, key_lo, key_hi)` wraps
//!   the same logic for concrete `array<mixed>` / assoc locals: it builds a
//!   transient cell on the stack, delegates to the fetch-for-write helper,
//!   drops the returned child reference (the slot keeps its own), and returns
//!   the possibly reallocated container pointer for the local storeback.
//! - The receiver cell is never refcounted by these helpers, so transient
//!   stack cells are safe receivers.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Dispatches the fetch-for-write helper family to the target-specific emitters.
pub fn emit_mixed_array_fetch_for_write(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_new_empty_array_cell_x86_64(emitter);
        emit_mixed_cell_promote_to_hash_x86_64(emitter);
        emit_mixed_array_get_for_write_x86_64(emitter);
        emit_array_ensure_elem_for_write_x86_64(emitter);
        return;
    }
    emit_mixed_new_empty_array_cell_aarch64(emitter);
    emit_mixed_cell_promote_to_hash_aarch64(emitter);
    emit_mixed_array_get_for_write_aarch64(emitter);
    emit_array_ensure_elem_for_write_aarch64(emitter);
}

/// Emits `__rt_mixed_new_empty_array_cell` for ARM64.
///
/// Output: `x0` = fresh boxed Mixed cell (tag 4) owning a fresh empty indexed
/// array. The cell starts with refcount 1; the array's initial reference is
/// transferred to the cell (no extra retain, unlike `__rt_mixed_from_value`).
fn emit_mixed_new_empty_array_cell_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_new_empty_array_cell ---");
    emitter.label_global("__rt_mixed_new_empty_array_cell");

    emitter.instruction("sub sp, sp, #32");                                     // reserve frame for the fresh array pointer and saved fp/lr
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the helper frame pointer
    emitter.instruction("mov x0, #0");                                          // request a zero-capacity indexed array (grown on demand)
    emitter.instruction("mov x1, #8");                                          // autovivified arrays use 8-byte slots (boxed cells fit)
    emitter.instruction("bl __rt_array_new");                                   // allocate the empty indexed array (refcount 1)
    emitter.instruction("str x0, [sp, #0]");                                    // save the fresh array across the cell allocation
    emitter.instruction("mov x0, #24");                                         // mixed cells store tag plus two payload words
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the mixed cell storage (refcount 1)
    emitter.instruction("mov x9, #5");                                          // low byte 5 = mixed cell heap kind
    emitter.instruction("str x9, [x0, #-8]");                                   // install the mixed-cell heap kind in the uniform header
    emitter.instruction("mov x9, #4");                                          // runtime value tag 4 = indexed array payload
    emitter.instruction("str x9, [x0]");                                        // store the indexed-array tag at mixed[0]
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the fresh empty array
    emitter.instruction("str x9, [x0, #8]");                                    // transfer the array's initial reference into the cell payload
    emitter.instruction("str xzr, [x0, #16]");                                  // clear the unused high payload word
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the fresh boxed array cell in x0
}

/// Emits `__rt_mixed_cell_promote_to_hash` for ARM64.
///
/// Input:  `x0` = boxed Mixed cell with a NON-NULL indexed-array payload (tag 4).
/// Output: `x0` = the new hash payload installed into the cell (tag rewritten to 5).
///
/// Builds a fresh hash from the indexed entries (`__rt_array_to_hash` retains
/// payloads, `__rt_hash_to_mixed` boxes raw scalar entries), installs it into
/// the cell, and releases the replaced indexed payload. Shared source arrays
/// keep their other owners untouched (the fresh hash copies, COW-style).
fn emit_mixed_cell_promote_to_hash_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_cell_promote_to_hash ---");
    emitter.label_global("__rt_mixed_cell_promote_to_hash");

    emitter.instruction("sub sp, sp, #48");                                     // reserve frame for cell, source array, hash, and saved fp/lr
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the boxed Mixed cell being promoted
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the indexed-array payload from the cell
    emitter.instruction("str x0, [sp, #8]");                                    // save the source array for the release after the copy
    emitter.instruction("bl __rt_array_to_hash");                               // build an owned hash from the indexed entries (payloads retained)
    emitter.instruction("bl __rt_hash_to_mixed");                               // box any raw scalar entries so the hash holds uniform Mixed cells
    emitter.instruction("str x0, [sp, #16]");                                   // save the promoted hash across the source release
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the boxed Mixed cell
    emitter.instruction("mov x10, #5");                                         // runtime value tag 5 = associative array payload
    emitter.instruction("str x10, [x9]");                                       // retag the cell as an associative payload
    emitter.instruction("str x0, [x9, #8]");                                    // install the promoted hash into the cell payload
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the replaced indexed payload
    emitter.instruction("bl __rt_decref_array");                                // release the cell's reference to the replaced indexed array
    emitter.instruction("ldr x0, [sp, #16]");                                   // return the promoted hash pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the installed hash in x0
}

/// Emits `__rt_mixed_array_get_for_write` for ARM64 (AAPCS64 ABI).
///
/// Inputs arrive in `x0` = mixed cell, `x1` = key_lo, `x2` = key_hi (integer
/// keys use the `-1` sentinel, matching `emit_normalized_hash_key`). Returns
/// an OWNED pointer to a boxed Mixed cell in `x0`.
///
/// Write-context lookup: missing indexed elements (beyond length), null gap
/// slots, and boxed `Mixed(null)` slots are autovivified as empty arrays
/// whose cells are installed into the parent storage; missing hash keys are
/// inserted the same way. Existing non-null cells are returned retained
/// (STORED, so the following set writes through the parent). Non-container
/// receivers fall back to `__rt_mixed_array_get` read semantics.
fn emit_mixed_array_get_for_write_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_array_get_for_write ---");
    emitter.label_global("__rt_mixed_array_get_for_write");

    // Stack:
    //   [sp, #0]  = mixed cell
    //   [sp, #8]  = key_lo
    //   [sp, #16] = key_hi
    //   [sp, #24] = unique array pointer (indexed path)
    //   [sp, #32] = target index / fresh child cell
    //   [sp, #40] = original logical length
    //   [sp, #48] = saved x29
    //   [sp, #56] = saved x30
    emitter.instruction("sub sp, sp, #64");                                     // reserve frame: 3 inputs + 3 scratch slots + saved fp/lr
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the receiver cell
    emitter.instruction("str x1, [sp, #8]");                                    // save key_lo
    emitter.instruction("str x2, [sp, #16]");                                   // save key_hi

    emitter.instruction("cbz x0, __rt_mixed_array_gfw_detached_null");          // null receivers yield a detached Mixed(null); the set drops the write
    emitter.instruction("ldr x9, [x0]");                                        // load the boxed payload tag
    emitter.instruction("cmp x9, #4");                                          // is the payload an indexed array?
    emitter.instruction("b.eq __rt_mixed_array_gfw_indexed");                   // route indexed payloads to the slot-based write-context lookup
    emitter.instruction("cmp x9, #5");                                          // is the payload an associative array?
    emitter.instruction("b.eq __rt_mixed_array_gfw_assoc");                     // route hash payloads to the key-based write-context lookup
    // Objects, scalars, and null payloads keep the plain reader's behavior:
    // PHP raises for scalar intermediates and elephc's set drops the write.
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the receiver cell for the plain reader
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload key_lo for the plain reader
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload key_hi for the plain reader
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address before the tail call
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame before the tail call
    emitter.instruction("b __rt_mixed_array_get");                              // delegate non-container receivers to the plain reader

    // Indexed array payload: integer keys mutate slots, string keys promote.
    emitter.label("__rt_mixed_array_gfw_indexed");
    emitter.instruction("ldr x10, [x0, #8]");                                   // load the indexed-array pointer from the cell payload
    emitter.instruction("cbz x10, __rt_mixed_array_gfw_detached_null");         // defensive: null payloads cannot be autovivified through
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload key_hi
    emitter.instruction("cmn x11, #1");                                         // does key_hi carry the integer-key sentinel?
    emitter.instruction("b.ne __rt_mixed_array_gfw_promote");                   // string keys promote the indexed payload to hash storage
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the requested integer index
    emitter.instruction("cmp x9, #0");                                          // negative indexes are not representable in dense storage
    emitter.instruction("b.lt __rt_mixed_array_gfw_detached_null");             // return a detached null quietly; the set drops the write
    emitter.instruction("ldr x12, [x10, #-8]");                                 // load the packed indexed-array metadata
    emitter.instruction("ubfx x1, x12, #8, #7");                                // pass the source value_type tag to the Mixed conversion helper
    emitter.instruction("mov x0, x10");                                         // pass the indexed array to the Mixed conversion helper
    emitter.instruction("bl __rt_array_to_mixed");                              // COW-split shared arrays and box slots so cells are stored in place
    emitter.instruction("str x0, [sp, #24]");                                   // save the unique array pointer
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the receiver cell after the helper call
    emitter.instruction("str x0, [x10, #8]");                                   // publish the unique array pointer back into the cell
    emitter.instruction("ldr x11, [x0]");                                       // load the post-conversion logical length
    emitter.instruction("str x11, [sp, #40]");                                  // preserve the original length for the extension path
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the requested integer index
    emitter.instruction("str x9, [sp, #32]");                                   // preserve the target index across helper calls
    emitter.instruction("cmp x9, x11");                                         // is the requested index beyond the logical length?
    emitter.instruction("b.hs __rt_mixed_array_gfw_extend");                    // missing elements extend the array and autovivify
    emitter.instruction("add x13, x0, #24");                                    // compute the indexed-array data base
    emitter.instruction("ldr x0, [x13, x9, lsl #3]");                           // load the stored boxed cell from the slot
    emitter.instruction("cbz x0, __rt_mixed_array_gfw_fill_slot");              // null gap slots autovivify in place
    emitter.instruction("ldr x11, [x0]");                                       // load the stored cell's payload tag
    emitter.instruction("cmp x11, #8");                                         // does the slot hold a boxed Mixed(null)?
    emitter.instruction("b.eq __rt_mixed_array_gfw_replace_null");              // PHP autovivifies null elements: replace the slot cell
    emitter.instruction("bl __rt_incref");                                      // retain the STORED cell so the caller owns the returned result
    emitter.instruction("b __rt_mixed_array_gfw_return");                       // existing elements are returned as-is (set decides compatibility)
    emitter.label("__rt_mixed_array_gfw_replace_null");
    emitter.instruction("bl __rt_decref_mixed");                                // drop the slot's reference; aliases keep their own null value
    emitter.label("__rt_mixed_array_gfw_fill_slot");
    emitter.instruction("bl __rt_mixed_new_empty_array_cell");                  // allocate the autovivified empty-array cell
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the unique array pointer
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the target index
    emitter.instruction("add x13, x10, #24");                                   // compute the indexed-array data base for the store
    emitter.instruction("str x0, [x13, x9, lsl #3]");                           // install the fresh cell into the parent slot (slot owns one ref)
    emitter.instruction("bl __rt_incref");                                      // retain the installed cell so the caller owns the returned result
    emitter.instruction("b __rt_mixed_array_gfw_return");                       // finish after installing the autovivified element

    emitter.label("__rt_mixed_array_gfw_extend");
    emitter.label("__rt_mixed_array_gfw_grow_check");
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the current unique array pointer
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the target index
    emitter.instruction("ldr x12, [x10, #8]");                                  // load the current capacity
    emitter.instruction("cmp x9, x12");                                         // does the target index fit in the current capacity?
    emitter.instruction("b.lo __rt_mixed_array_gfw_grow_ready");                // skip growth once the destination slot is addressable
    emitter.instruction("mov x0, x10");                                         // pass the current array pointer to the growth helper
    emitter.instruction("bl __rt_array_grow");                                  // grow the unique array until the target slot fits
    emitter.instruction("str x0, [sp, #24]");                                   // save the possibly reallocated array pointer
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the receiver cell
    emitter.instruction("str x0, [x10, #8]");                                   // publish the grown array pointer back into the cell
    emitter.instruction("b __rt_mixed_array_gfw_grow_check");                   // continue growing until the target slot fits
    emitter.label("__rt_mixed_array_gfw_grow_ready");
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the final array pointer
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the target index
    emitter.instruction("ldr x12, [sp, #40]");                                  // start zero-filling at the old logical end
    emitter.label("__rt_mixed_array_gfw_zero_fill");
    emitter.instruction("cmp x12, x9");                                         // have all gap slots before the target been initialized?
    emitter.instruction("b.ge __rt_mixed_array_gfw_store_len");                 // stop once the loop reaches the autovivified slot
    emitter.instruction("add x13, x10, #24");                                   // compute the indexed-array data base for the gap slot
    emitter.instruction("str xzr, [x13, x12, lsl #3]");                         // initialize the gap slot to null (reads as PHP null)
    emitter.instruction("add x12, x12, #1");                                    // advance to the next gap slot
    emitter.instruction("b __rt_mixed_array_gfw_zero_fill");                    // continue zero-filling until the target slot is reached
    emitter.label("__rt_mixed_array_gfw_store_len");
    emitter.instruction("add x12, x9, #1");                                     // compute the new logical length
    emitter.instruction("str x12, [x10]");                                      // store the extended logical length
    emitter.instruction("b __rt_mixed_array_gfw_fill_slot");                    // install the autovivified element into the new slot

    // String key on an indexed payload: promote to hash, then use the hash path.
    emitter.label("__rt_mixed_array_gfw_promote");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the receiver cell for the promotion helper
    emitter.instruction("bl __rt_mixed_cell_promote_to_hash");                  // convert the indexed payload to hash storage in the cell
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the receiver cell (now tag 5)
    emitter.instruction("b __rt_mixed_array_gfw_assoc");                        // continue with the associative write-context lookup

    // Associative array payload: hash lookup with write-context semantics.
    emitter.label("__rt_mixed_array_gfw_assoc");
    emitter.instruction("ldr x10, [x0, #8]");                                   // load the hash pointer from the cell payload
    emitter.instruction("cbz x10, __rt_mixed_array_gfw_detached_null");         // defensive: null payloads cannot be autovivified through
    emitter.instruction("mov x0, x10");                                         // pass the hash pointer to hash_get
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the normalized key low word
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the normalized key high word
    emitter.instruction("bl __rt_hash_get");                                    // x0=found, x1=value_lo, x2=value_hi, x3=value_tag
    emitter.instruction("cbz x0, __rt_mixed_array_gfw_assoc_create");           // missing keys autovivify a fresh element
    emitter.instruction("cmp x3, #7");                                          // is the entry already a boxed Mixed cell?
    emitter.instruction("b.ne __rt_mixed_array_gfw_assoc_box");                 // typed entries keep the plain reader's detached-box behavior
    emitter.instruction("cbz x1, __rt_mixed_array_gfw_assoc_create");           // defensive: boxed entries without a cell are treated as missing
    emitter.instruction("ldr x9, [x1]");                                        // load the stored cell's payload tag
    emitter.instruction("cmp x9, #8");                                          // does the entry hold a boxed Mixed(null)?
    emitter.instruction("b.eq __rt_mixed_array_gfw_assoc_create");              // PHP autovivifies null elements: overwrite (hash_set releases it)
    emitter.instruction("mov x0, x1");                                          // move the stored cell into the retain register
    emitter.instruction("bl __rt_incref");                                      // retain the STORED cell so the caller owns the returned result
    emitter.instruction("b __rt_mixed_array_gfw_return");                       // existing elements are returned as-is (set decides compatibility)
    emitter.label("__rt_mixed_array_gfw_assoc_box");
    emitter.instruction("mov x0, x3");                                          // x0 = value_tag for mixed_from_value (x1/x2 hold the payload)
    emitter.instruction("bl __rt_mixed_from_value");                            // box the typed entry into a detached Mixed cell (read-path parity)
    emitter.instruction("b __rt_mixed_array_gfw_return");                       // the following set drops writes through detached boxes
    emitter.label("__rt_mixed_array_gfw_assoc_create");
    emitter.instruction("bl __rt_mixed_new_empty_array_cell");                  // allocate the autovivified empty-array cell
    emitter.instruction("str x0, [sp, #32]");                                   // save the fresh child cell across the hash insertion
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the receiver cell
    emitter.instruction("ldr x0, [x9, #8]");                                    // reload the current hash pointer
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the normalized key low word
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the normalized key high word
    emitter.instruction("ldr x3, [sp, #32]");                                   // pass the fresh child cell (ownership consumed by the insert)
    emitter.instruction("mov x4, xzr");                                         // boxed Mixed hash values only use the low payload word
    emitter.instruction("mov x5, #7");                                          // runtime value tag 7 = boxed Mixed
    emitter.instruction("bl __rt_hash_set");                                    // insert or overwrite the entry (overwrites release the old value)
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the receiver cell after hash mutation
    emitter.instruction("str x0, [x9, #8]");                                    // publish the possibly-reallocated hash back into the cell
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the installed child cell
    emitter.instruction("bl __rt_incref");                                      // retain the installed cell so the caller owns the returned result
    emitter.instruction("b __rt_mixed_array_gfw_return");                       // finish after installing the autovivified element

    emitter.label("__rt_mixed_array_gfw_detached_null");
    emitter.instruction("mov x0, #8");                                          // tag = 8 (null)
    emitter.instruction("mov x1, #0");                                          // value_lo = 0
    emitter.instruction("mov x2, #0");                                          // value_hi = 0
    emitter.instruction("bl __rt_mixed_from_value");                            // box a detached Mixed(null); the following set drops the write
    emitter.label("__rt_mixed_array_gfw_return");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the owned Mixed cell in x0
}

/// Emits `__rt_array_ensure_elem_for_write` for ARM64.
///
/// Inputs: `x0` = container pointer (indexed array or hash), `x1` = payload
/// tag (4 = indexed, 5 = hash), `x2` = key_lo, `x3` = key_hi. Output: `x0` =
/// the possibly promoted/reallocated container pointer to store back into the
/// local. The container's slot for `key` is guaranteed to exist afterwards
/// (unless the key shape is invalid), holding either its previous non-null
/// value or an autovivified empty-array cell.
fn emit_array_ensure_elem_for_write_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_ensure_elem_for_write ---");
    emitter.label_global("__rt_array_ensure_elem_for_write");

    // Stack:
    //   [sp, #0]  = spare (keeps the transient cell 16-byte aligned)
    //   [sp, #16] = transient cell: payload tag
    //   [sp, #24] = transient cell: container pointer (payload)
    //   [sp, #32] = transient cell: high payload word
    //   [sp, #48] = saved x29
    //   [sp, #56] = saved x30
    emitter.instruction("sub sp, sp, #64");                                     // reserve frame for the transient receiver cell and saved fp/lr
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x1, [sp, #16]");                                   // install the payload tag into the transient cell
    emitter.instruction("str x0, [sp, #24]");                                   // install the container pointer into the transient cell payload
    emitter.instruction("str xzr, [sp, #32]");                                  // clear the transient cell's high payload word
    emitter.instruction("add x0, sp, #16");                                     // pass the transient cell as the fetch-for-write receiver
    emitter.instruction("mov x1, x2");                                          // forward key_lo into the fetch-for-write key register
    emitter.instruction("mov x2, x3");                                          // forward key_hi into the fetch-for-write key register
    emitter.instruction("bl __rt_mixed_array_get_for_write");                   // autovivify the element and publish container changes to the cell
    emitter.instruction("bl __rt_decref_mixed");                                // drop the returned child reference; the container slot keeps its own
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the possibly promoted/reallocated container pointer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the container pointer for the local storeback
}

/// Emits `__rt_mixed_new_empty_array_cell` for x86_64 (same contract as ARM64).
fn emit_mixed_new_empty_array_cell_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_new_empty_array_cell ---");
    emitter.label_global("__rt_mixed_new_empty_array_cell");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 16");                                         // reserve one slot for the fresh array pointer
    emitter.instruction("mov rdi, 0");                                          // request a zero-capacity indexed array (grown on demand)
    emitter.instruction("mov rsi, 8");                                          // autovivified arrays use 8-byte slots (boxed cells fit)
    emitter.instruction("call __rt_array_new");                                 // allocate the empty indexed array (refcount 1)
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the fresh array across the cell allocation
    emitter.instruction("mov rax, 24");                                         // mixed cells store tag plus two payload words
    emitter.instruction("call __rt_heap_alloc");                                // allocate the mixed cell storage (refcount 1)
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(5))); // materialize the mixed-cell heap kind word with the x86_64 marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // install the mixed-cell heap kind in the uniform header
    emitter.instruction("mov QWORD PTR [rax], 4");                              // store the indexed-array tag at mixed[0]
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the fresh empty array
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // transfer the array's initial reference into the cell payload
    emitter.instruction("mov QWORD PTR [rax + 16], 0");                         // clear the unused high payload word
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the fresh boxed array cell in rax
}

/// Emits `__rt_mixed_cell_promote_to_hash` for x86_64.
///
/// Input: `rdi` = boxed Mixed cell with a non-null indexed payload.
/// Output: `rax` = the new hash payload installed into the cell (tag 5).
fn emit_mixed_cell_promote_to_hash_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_cell_promote_to_hash ---");
    emitter.label_global("__rt_mixed_cell_promote_to_hash");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 32");                                         // reserve slots for cell, source array, and promoted hash
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the boxed Mixed cell being promoted
    emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");                        // load the indexed-array payload from the cell
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save the source array for the release after the copy
    emitter.instruction("call __rt_array_to_hash");                             // build an owned hash from the indexed entries (payloads retained)
    emitter.instruction("mov rdi, rax");                                        // pass the fresh hash to the Mixed-entry conversion helper
    emitter.instruction("call __rt_hash_to_mixed");                             // box any raw scalar entries so the hash holds uniform Mixed cells
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the promoted hash across the source release
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the boxed Mixed cell
    emitter.instruction("mov QWORD PTR [r10], 5");                              // retag the cell as an associative payload
    emitter.instruction("mov QWORD PTR [r10 + 8], rax");                        // install the promoted hash into the cell payload
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the replaced indexed payload
    emitter.instruction("call __rt_decref_array");                              // release the cell's reference to the replaced indexed array
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the promoted hash pointer
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the installed hash in rax
}

/// Emits `__rt_mixed_array_get_for_write` for x86_64 (SysV ABI).
///
/// Inputs arrive in `rdi` = mixed cell, `rsi` = key_lo, `rdx` = key_hi.
/// Returns an OWNED pointer to a boxed Mixed cell in `rax`. Same dispatch and
/// autovivification semantics as the ARM64 emitter.
fn emit_mixed_array_get_for_write_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_array_get_for_write ---");
    emitter.label_global("__rt_mixed_array_get_for_write");

    // Frame slots:
    //   [rbp - 8]  = mixed cell
    //   [rbp - 16] = key_lo
    //   [rbp - 24] = key_hi
    //   [rbp - 32] = unique array pointer (indexed path)
    //   [rbp - 40] = target index / fresh child cell
    //   [rbp - 48] = original logical length
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 48");                                         // reserve slots for the 3 inputs and 3 scratch words
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the receiver cell
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save key_lo
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save key_hi

    emitter.instruction("test rdi, rdi");                                       // null receivers yield a detached Mixed(null); the set drops the write
    emitter.instruction("je __rt_mixed_array_gfw_detached_null");               // branch to the detached-null fallback
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the boxed payload tag
    emitter.instruction("cmp r10, 4");                                          // is the payload an indexed array?
    emitter.instruction("je __rt_mixed_array_gfw_indexed");                     // route indexed payloads to the slot-based write-context lookup
    emitter.instruction("cmp r10, 5");                                          // is the payload an associative array?
    emitter.instruction("je __rt_mixed_array_gfw_assoc");                       // route hash payloads to the key-based write-context lookup
    // Objects, scalars, and null payloads keep the plain reader's behavior.
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the receiver cell for the plain reader
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload key_lo for the plain reader
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload key_hi for the plain reader
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame before the tail call
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before the tail call
    emitter.instruction("jmp __rt_mixed_array_get");                            // delegate non-container receivers to the plain reader

    emitter.label("__rt_mixed_array_gfw_indexed");
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load the indexed-array pointer from the cell payload
    emitter.instruction("test r10, r10");                                       // defensive: null payloads cannot be autovivified through
    emitter.instruction("je __rt_mixed_array_gfw_detached_null");               // branch to the detached-null fallback
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload key_hi
    emitter.instruction("cmp r11, -1");                                         // does key_hi carry the integer-key sentinel?
    emitter.instruction("jne __rt_mixed_array_gfw_promote");                    // string keys promote the indexed payload to hash storage
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the requested integer index
    emitter.instruction("cmp r9, 0");                                           // negative indexes are not representable in dense storage
    emitter.instruction("jl __rt_mixed_array_gfw_detached_null");               // return a detached null quietly; the set drops the write
    emitter.instruction("mov r8, QWORD PTR [r10 - 8]");                         // load the packed indexed-array metadata
    emitter.instruction("shr r8, 8");                                           // shift the runtime element value_type tag into the low bits
    emitter.instruction("and r8, 0x7f");                                        // remove the persistent COW flag from the extracted tag
    emitter.instruction("mov rsi, r8");                                         // pass the source value_type tag to the Mixed conversion helper
    emitter.instruction("mov rdi, r10");                                        // pass the indexed array to the Mixed conversion helper
    emitter.instruction("call __rt_array_to_mixed");                            // COW-split shared arrays and box slots so cells are stored in place
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the unique array pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the receiver cell after the helper call
    emitter.instruction("mov QWORD PTR [r10 + 8], rax");                        // publish the unique array pointer back into the cell
    emitter.instruction("mov r11, QWORD PTR [rax]");                            // load the post-conversion logical length
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // preserve the original length for the extension path
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the requested integer index
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // preserve the target index across helper calls
    emitter.instruction("cmp r9, r11");                                         // is the requested index beyond the logical length?
    emitter.instruction("jae __rt_mixed_array_gfw_extend");                     // missing elements extend the array and autovivify
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the unique array pointer
    emitter.instruction("mov rax, QWORD PTR [r10 + 24 + r9 * 8]");              // load the stored boxed cell from the slot
    emitter.instruction("test rax, rax");                                       // null gap slots autovivify in place
    emitter.instruction("je __rt_mixed_array_gfw_fill_slot");                   // branch to the slot-install path
    emitter.instruction("mov r11, QWORD PTR [rax]");                            // load the stored cell's payload tag
    emitter.instruction("cmp r11, 8");                                          // does the slot hold a boxed Mixed(null)?
    emitter.instruction("je __rt_mixed_array_gfw_replace_null");                // PHP autovivifies null elements: replace the slot cell
    abi::emit_push_reg(emitter, "rax");
    emitter.instruction("call __rt_incref");                                    // retain the STORED cell so the caller owns the returned result
    abi::emit_pop_reg(emitter, "rax");
    emitter.instruction("jmp __rt_mixed_array_gfw_return");                     // existing elements are returned as-is (set decides compatibility)
    emitter.label("__rt_mixed_array_gfw_replace_null");
    emitter.instruction("call __rt_decref_mixed");                              // drop the slot's reference; aliases keep their own null value
    emitter.label("__rt_mixed_array_gfw_fill_slot");
    emitter.instruction("call __rt_mixed_new_empty_array_cell");                // allocate the autovivified empty-array cell
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the unique array pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the target index
    emitter.instruction("mov QWORD PTR [r10 + 24 + r9 * 8], rax");              // install the fresh cell into the parent slot (slot owns one ref)
    abi::emit_push_reg(emitter, "rax");
    emitter.instruction("call __rt_incref");                                    // retain the installed cell so the caller owns the returned result
    abi::emit_pop_reg(emitter, "rax");
    emitter.instruction("jmp __rt_mixed_array_gfw_return");                     // finish after installing the autovivified element

    emitter.label("__rt_mixed_array_gfw_extend");
    emitter.label("__rt_mixed_array_gfw_grow_check");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current unique array pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the target index
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // load the current capacity
    emitter.instruction("cmp r9, r11");                                         // does the target index fit in the current capacity?
    emitter.instruction("jb __rt_mixed_array_gfw_grow_ready");                  // skip growth once the destination slot is addressable
    emitter.instruction("mov rdi, r10");                                        // pass the current array pointer to the growth helper
    emitter.instruction("call __rt_array_grow");                                // grow the unique array until the target slot fits
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the possibly reallocated array pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the receiver cell
    emitter.instruction("mov QWORD PTR [r10 + 8], rax");                        // publish the grown array pointer back into the cell
    emitter.instruction("jmp __rt_mixed_array_gfw_grow_check");                 // continue growing until the target slot fits
    emitter.label("__rt_mixed_array_gfw_grow_ready");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the final array pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the target index
    emitter.instruction("mov r8, QWORD PTR [rbp - 48]");                        // start zero-filling at the old logical end
    emitter.label("__rt_mixed_array_gfw_zero_fill");
    emitter.instruction("cmp r8, r9");                                          // have all gap slots before the target been initialized?
    emitter.instruction("jae __rt_mixed_array_gfw_store_len");                  // stop once the loop reaches the autovivified slot
    emitter.instruction("mov QWORD PTR [r10 + 24 + r8 * 8], 0");                // initialize the gap slot to null (reads as PHP null)
    emitter.instruction("add r8, 1");                                           // advance to the next gap slot
    emitter.instruction("jmp __rt_mixed_array_gfw_zero_fill");                  // continue zero-filling until the target slot is reached
    emitter.label("__rt_mixed_array_gfw_store_len");
    emitter.instruction("lea r8, [r9 + 1]");                                    // compute the new logical length
    emitter.instruction("mov QWORD PTR [r10], r8");                             // store the extended logical length
    emitter.instruction("jmp __rt_mixed_array_gfw_fill_slot");                  // install the autovivified element into the new slot

    emitter.label("__rt_mixed_array_gfw_promote");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the receiver cell for the promotion helper
    emitter.instruction("call __rt_mixed_cell_promote_to_hash");                // convert the indexed payload to hash storage in the cell
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the receiver cell (now tag 5)
    emitter.instruction("jmp __rt_mixed_array_gfw_assoc");                      // continue with the associative write-context lookup

    emitter.label("__rt_mixed_array_gfw_assoc");
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load the hash pointer from the cell payload
    emitter.instruction("test r10, r10");                                       // defensive: null payloads cannot be autovivified through
    emitter.instruction("je __rt_mixed_array_gfw_detached_null");               // branch to the detached-null fallback
    emitter.instruction("mov rdi, r10");                                        // pass the hash pointer to hash_get
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the normalized key low word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the normalized key high word
    emitter.instruction("call __rt_hash_get");                                  // rax=found, rdi=value_lo, rsi=value_hi, rcx=value_tag
    emitter.instruction("test rax, rax");                                       // was the key found?
    emitter.instruction("je __rt_mixed_array_gfw_assoc_create");                // missing keys autovivify a fresh element
    emitter.instruction("cmp rcx, 7");                                          // is the entry already a boxed Mixed cell?
    emitter.instruction("jne __rt_mixed_array_gfw_assoc_box");                  // typed entries keep the plain reader's detached-box behavior
    emitter.instruction("test rdi, rdi");                                       // defensive: boxed entries without a cell are treated as missing
    emitter.instruction("je __rt_mixed_array_gfw_assoc_create");                // branch to the autovivify path
    emitter.instruction("mov r11, QWORD PTR [rdi]");                            // load the stored cell's payload tag
    emitter.instruction("cmp r11, 8");                                          // does the entry hold a boxed Mixed(null)?
    emitter.instruction("je __rt_mixed_array_gfw_assoc_create");                // PHP autovivifies null elements: overwrite (hash_set releases it)
    emitter.instruction("mov rax, rdi");                                        // move the stored cell into the retain register
    abi::emit_push_reg(emitter, "rax");
    emitter.instruction("call __rt_incref");                                    // retain the STORED cell so the caller owns the returned result
    abi::emit_pop_reg(emitter, "rax");
    emitter.instruction("jmp __rt_mixed_array_gfw_return");                     // existing elements are returned as-is (set decides compatibility)
    emitter.label("__rt_mixed_array_gfw_assoc_box");
    emitter.instruction("mov rax, rcx");                                        // rax = value_tag for mixed_from_value (rdi/rsi hold the payload)
    emitter.instruction("call __rt_mixed_from_value");                          // box the typed entry into a detached Mixed cell (read-path parity)
    emitter.instruction("jmp __rt_mixed_array_gfw_return");                     // the following set drops writes through detached boxes
    emitter.label("__rt_mixed_array_gfw_assoc_create");
    emitter.instruction("call __rt_mixed_new_empty_array_cell");                // allocate the autovivified empty-array cell
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the fresh child cell across the hash insertion
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the receiver cell
    emitter.instruction("mov rdi, QWORD PTR [r10 + 8]");                        // reload the current hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the normalized key low word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the normalized key high word
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // pass the fresh child cell (ownership consumed by the insert)
    emitter.instruction("xor r8, r8");                                          // boxed Mixed hash values only use the low payload word
    emitter.instruction("mov r9, 7");                                           // runtime value tag 7 = boxed Mixed
    emitter.instruction("call __rt_hash_set");                                  // insert or overwrite the entry (overwrites release the old value)
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the receiver cell after hash mutation
    emitter.instruction("mov QWORD PTR [r10 + 8], rax");                        // publish the possibly-reallocated hash back into the cell
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the installed child cell
    abi::emit_push_reg(emitter, "rax");
    emitter.instruction("call __rt_incref");                                    // retain the installed cell so the caller owns the returned result
    abi::emit_pop_reg(emitter, "rax");
    emitter.instruction("jmp __rt_mixed_array_gfw_return");                     // finish after installing the autovivified element

    emitter.label("__rt_mixed_array_gfw_detached_null");
    emitter.instruction("mov rax, 8");                                          // tag = 8 (null) for mixed_from_value
    emitter.instruction("mov rdi, 0");                                          // value_lo = 0
    emitter.instruction("mov rsi, 0");                                          // value_hi = 0
    emitter.instruction("call __rt_mixed_from_value");                          // box a detached Mixed(null); the following set drops the write
    emitter.label("__rt_mixed_array_gfw_return");
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the owned Mixed cell in rax
}

/// Emits `__rt_array_ensure_elem_for_write` for x86_64.
///
/// Inputs (SysV): `rdi` = container pointer, `rsi` = payload tag (4/5),
/// `rdx` = key_lo, `rcx` = key_hi. Output: `rax` = the possibly promoted or
/// reallocated container pointer for the local storeback.
fn emit_array_ensure_elem_for_write_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_ensure_elem_for_write ---");
    emitter.label_global("__rt_array_ensure_elem_for_write");

    // Frame slots (transient receiver cell):
    //   [rbp - 40] = transient cell: payload tag
    //   [rbp - 32] = transient cell: container pointer (payload)
    //   [rbp - 24] = transient cell: high payload word
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 48");                                         // reserve the transient receiver cell (16-byte aligned)
    emitter.instruction("mov QWORD PTR [rbp - 40], rsi");                       // install the payload tag into the transient cell
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // install the container pointer into the transient cell payload
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // clear the transient cell's high payload word
    emitter.instruction("lea rdi, [rbp - 40]");                                 // pass the transient cell as the fetch-for-write receiver
    emitter.instruction("mov rsi, rdx");                                        // forward key_lo into the fetch-for-write key register
    emitter.instruction("mov rdx, rcx");                                        // forward key_hi into the fetch-for-write key register
    emitter.instruction("call __rt_mixed_array_get_for_write");                 // autovivify the element and publish container changes to the cell
    emitter.instruction("call __rt_decref_mixed");                              // drop the returned child reference; the container slot keeps its own
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the possibly promoted/reallocated container pointer
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the container pointer for the local storeback
}
