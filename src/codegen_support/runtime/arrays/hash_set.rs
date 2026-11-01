//! Purpose:
//! Emits the `__rt_hash_set`, `__rt_hash_ensure_unique` runtime helper assembly for hash set.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Hash helpers must normalize PHP keys and preserve bucket layout, ownership, and iteration conventions.
//! - `__rt_hash_set` is the PHP element-assignment entry point. Overwriting an entry that
//!   carries runtime value tag 11 writes through its managed reference cell: the previous cell
//!   payload is released and the replacement is published to every alias in the set, so a live
//!   `&$v` is never detached by `$a[$k] = x`.
//! - A Mixed-valued variadic argument hash can instead contain a boxed invoker marker. Its outer
//!   entry tag is 7 and its inner tag is 11. Updates write through the marker's caller-storage
//!   pointer while leaving the argument container intact.
//! - `__rt_hash_set_value` is the bucket-replacing sibling for array-building helpers whose
//!   destination was cloned from a user array. It detaches a reference entry first, matching
//!   `zend_hash_update`, so building a new array never mutates the array it was copied from.
//! - `__rt_hash_set_shared` is restricted to a pending append selected before a diagnostic
//!   callback. It preserves the exact payload retained by a surviving child copy.
//! - An element reference made by `$a[$k] = &$v` or a literal `[&$v]` is a boxed marker
//!   `{12|13, cell, source_tag}` in a tag-7 entry. A plain write goes through it
//!   (`__rt_ref_marker_store`), an incoming reference rebinds it, and the bucket keeps its marker.

use crate::codegen_support::callable_invoker_args::{
    ARRAY_GLOBAL_REF_CELL_TAG, ARRAY_LOCAL_REF_CELL_TAG, INVOKER_ARG_REF_CELL_TAG,
};
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use super::hash_layout;

/// hash_set: insert or update a normalized int/string key-value pair in the hash table.
/// Grows the table automatically if load factor exceeds 75%, persists newly
/// inserted string keys, and releases overwritten heap-backed values on update.
/// Input:  x0=hash_table_ptr, x1=key_lo, x2=key_hi (-1 means integer key), x3=value_lo, x4=value_hi, x5=value_tag
/// Output: x0=hash_table_ptr (may differ if table was reallocated)
pub fn emit_hash_set(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_set_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_set ---");
    emitter.label_global("__rt_hash_set");
    emitter.instruction("mov x9, #0");                                          // ordinary writes retain the hash COW boundary
    emitter.instruction("b __rt_hash_set_entry");                               // share insertion and update machinery
    emitter.label_global("__rt_hash_set_shared");
    emitter.instruction("mov x9, #1");                                          // pending append mutates its pre-callback selected payload
    emitter.label_shared("__rt_hash_set_entry");

    // -- set up stack frame, save all inputs --
    // Stack layout:
    //   [sp, #0]  = hash_table_ptr (x0)
    //   [sp, #8]  = key_ptr (x1)
    //   [sp, #16] = key_len (x2)
    //   [sp, #24] = value_lo (x3)
    //   [sp, #32] = value_hi (x4)
    //   [sp, #40] = value_tag (x5)
    //   [sp, #48] = probe index (saved across calls)
    //   [sp, #56] = selected shared-payload mode
    //   [sp, #64] = saved x29
    //   [sp, #72] = saved x30
    emitter.instruction("sub sp, sp, #80");                                     // allocate 80 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set up new frame pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save key_ptr
    emitter.instruction("str x2, [sp, #16]");                                   // save key_len
    emitter.instruction("str x3, [sp, #24]");                                   // save value_lo
    emitter.instruction("str x4, [sp, #32]");                                   // save value_hi
    emitter.instruction("str x5, [sp, #40]");                                   // save value_tag
    emitter.instruction("str x9, [sp, #56]");                                   // preserve the pending-append selection mode
    emitter.instruction("str x0, [sp, #0]");                                    // save the candidate hash table before null normalization
    emitter.instruction("cbnz x0, __rt_hash_set_have_table");                   // a live table skips the null-hash allocation
    emitter.instruction("mov x0, #4");                                          // allocate a four-entry hash for a null dynamic-property slot
    emitter.instruction("ldr x1, [sp, #40]");                                   // preserve the caller's value tag as the hash value type
    emitter.instruction("bl __rt_hash_new");                                    // create the missing hash table
    emitter.instruction("str x0, [sp, #0]");                                    // publish the freshly allocated table
    emitter.label("__rt_hash_set_have_table");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the candidate table
    emitter.instruction("cbnz x9, __rt_hash_set_ready");                        // selected payloads must remain visible through surviving copies
    emitter.instruction("bl __rt_hash_ensure_unique");                          // split ordinary shared hash tables before mutation
    emitter.label("__rt_hash_set_ready");
    emitter.instruction("str x0, [sp, #0]");                                    // save the unique hash_table_ptr

    // -- check load factor: grow if count * 4 >= capacity * 3 (75%) --
    emitter.instruction("ldr x5, [x0]");                                        // x5 = count
    emitter.instruction("ldr x6, [x0, #8]");                                    // x6 = capacity
    emitter.instruction("lsl x7, x5, #2");                                      // x7 = count * 4
    emitter.instruction("mov x8, #3");                                          // multiplier
    emitter.instruction("mul x8, x6, x8");                                      // x8 = capacity * 3
    emitter.instruction("cmp x7, x8");                                          // count*4 >= capacity*3?
    emitter.instruction("b.lt __rt_hash_set_no_grow");                          // skip growth if under threshold

    // -- grow the hash table --
    emitter.instruction("bl __rt_hash_grow");                                   // x0 = new table (doubled capacity)
    emitter.instruction("str x0, [sp, #0]");                                    // update saved table pointer

    emitter.label("__rt_hash_set_no_grow");

    // -- hash the key --
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload incoming key_ptr
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload key_len
    emitter.instruction("bl __rt_hash_key_hash");                               // compute the typed key hash, result in x0

    // -- compute slot index: hash % capacity --
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload hash_table_ptr
    emitter.instruction("ldr x6, [x5, #8]");                                    // x6 = capacity from header
    emitter.instruction("udiv x7, x0, x6");                                     // x7 = hash / capacity
    emitter.instruction("msub x8, x7, x6, x0");                                 // x8 = hash - (hash/capacity)*capacity = hash % capacity

    // -- linear probe loop --
    // A tombstone does NOT end the probe: the key may live further along the chain, and
    // inserting at the first tombstone put a second copy of it in the table (an unset ahead of
    // a live key on its probe path, then a write of that key, and `count()` saw both). The
    // first tombstone is remembered ([sp, #56] = its index + 1, 0 = none) and reused once an
    // empty slot proves the key absent.
    emitter.instruction("str x8, [sp, #48]");                                   // save initial probe index
    emitter.instruction("str xzr, [sp, #56]");                                  // no tombstone seen yet on this probe path
    emitter.instruction("mov x10, #0");                                         // x10 = probe count (to detect full table)

    emitter.label("__rt_hash_set_probe");
    emitter.instruction("cmp x10, x6");                                         // check if we've probed all slots
    emitter.instruction("b.ge __rt_hash_set_probe_exhausted");                  // every slot probed: no empty slot ends this chain

    // -- compute entry address: entries + index * 64 --
    emitter.instruction("ldr x9, [sp, #48]");                                   // load current probe index
    emitter.instruction("mov x11, #64");                                        // entry size = 64 bytes with per-entry tags and insertion-order links
    hash_layout::emit_entry_address(emitter, "x12", "x5", "x9");

    // -- check occupied field --
    emitter.instruction("ldr x13, [x12]");                                      // x13 = occupied flag of this entry
    emitter.instruction("cmp x13, #1");                                         // check if slot is occupied
    emitter.instruction("b.eq __rt_hash_set_occupied");                         // a live entry: compare its key
    emitter.instruction("cbz x13, __rt_hash_set_probe_exhausted");              // an empty slot proves the key absent
    emitter.instruction("ldr x14, [sp, #56]");                                  // a tombstone: has an earlier one been remembered?
    emitter.instruction("cbnz x14, __rt_hash_set_advance");                     // yes: keep the first one and probe on
    emitter.instruction("add x14, x9, #1");                                     // remember this tombstone's index + 1
    emitter.instruction("str x14, [sp, #56]");                                  // save the first tombstone for a later insert
    emitter.instruction("b __rt_hash_set_advance");                             // the key may still live further along the chain

    // -- the key is absent: insert into the first tombstone, or into this empty slot --
    emitter.label("__rt_hash_set_probe_exhausted");
    emitter.instruction("ldr x14, [sp, #56]");                                  // x14 = first tombstone index + 1, or 0
    emitter.instruction("cbz x14, __rt_hash_set_probe_end");                    // none: use the slot the probe stopped at
    emitter.instruction("sub x9, x14, #1");                                     // x9 = the first tombstone's index
    emitter.instruction("str x9, [sp, #48]");                                   // the insert writes the tombstone slot
    emitter.instruction("b __rt_hash_set_insert");                              // reuse the tombstone for the new key
    emitter.label("__rt_hash_set_probe_end");
    emitter.instruction("cmp x10, x6");                                         // did the probe stop at an empty slot, or run out of slots?
    emitter.instruction("b.lt __rt_hash_set_insert");                           // an empty slot: insert here
    emitter.instruction("b __rt_hash_set_done");                                // a full table with no tombstone (the load factor prevents it)

    emitter.label("__rt_hash_set_occupied");

    // -- slot is occupied: compare keys to check for update --
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = our key_ptr
    emitter.instruction("ldr x2, [sp, #16]");                                   // x2 = our key_len
    emitter.instruction("ldr x3, [x12, #8]");                                   // x3 = existing key_ptr in entry
    emitter.instruction("ldr x4, [x12, #16]");                                  // x4 = existing key_len in entry
    emitter.instruction("bl __rt_hash_key_eq");                                 // compare normalized typed keys, x0=1 if equal

    // -- restore probe state after str_eq --
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload hash_table_ptr
    emitter.instruction("ldr x6, [x5, #8]");                                    // reload capacity
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload probe index

    // -- recompute entry address after call clobbered registers --
    emitter.instruction("mov x11, #64");                                        // entry size = 64 bytes with per-entry tags and insertion-order links
    hash_layout::emit_entry_address(emitter, "x12", "x5", "x9");

    emitter.instruction("cbnz x0, __rt_hash_set_update");                       // if keys match, update existing entry

    // -- keys don't match, advance to next slot --
    emitter.label("__rt_hash_set_advance");
    emitter.instruction("add x9, x9, #1");                                      // index += 1
    emitter.instruction("udiv x7, x9, x6");                                     // x7 = index / capacity
    emitter.instruction("msub x9, x7, x6, x9");                                 // x9 = index % capacity (wrap around)
    emitter.instruction("str x9, [sp, #48]");                                   // save updated probe index

    // -- increment probe count and continue --
    emitter.instruction("add x10, x10, #1");                                    // probe_count += 1
    emitter.instruction("b __rt_hash_set_probe");                               // try next slot

    // -- insert into empty/tombstone slot --
    emitter.label("__rt_hash_set_insert");
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload inserted key_ptr
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload inserted key_len
    emitter.instruction("cmn x2, #1");                                          // check whether the inserted key is an integer key
    emitter.instruction("b.eq __rt_hash_set_key_ready");                        // integer keys are stored inline and do not need persistence
    emitter.instruction("bl __rt_str_persist");                                 // persist inserted string key into heap storage
    emitter.instruction("str x1, [sp, #8]");                                    // save persistent key pointer for slot write
    emitter.instruction("str x2, [sp, #16]");                                   // save persistent key length for slot write
    emitter.label("__rt_hash_set_key_ready");
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload hash table pointer after helper call
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload probe index after helper call
    emitter.instruction("mov x11, #64");                                        // entry size = 64 bytes with per-entry tags and insertion-order links
    hash_layout::emit_entry_address(emitter, "x12", "x5", "x9");
    emitter.instruction("mov x13, #1");                                         // occupied = 1
    emitter.instruction("str x13, [x12]");                                      // set entry as occupied
    emitter.instruction("ldr x13, [sp, #8]");                                   // load persistent key_ptr
    emitter.instruction("str x13, [x12, #8]");                                  // store key_ptr in entry
    emitter.instruction("ldr x13, [sp, #16]");                                  // load key_len
    emitter.instruction("str x13, [x12, #16]");                                 // store key_len in entry
    emitter.instruction("ldr x13, [sp, #24]");                                  // load value_lo
    emitter.instruction("str x13, [x12, #24]");                                 // store value_lo in entry
    emitter.instruction("ldr x13, [sp, #32]");                                  // load value_hi
    emitter.instruction("str x13, [x12, #32]");                                 // store value_hi in entry
    emitter.instruction("ldr x13, [sp, #40]");                                  // load value_tag
    emitter.instruction("str x13, [x12, #40]");                                 // store value_tag in entry
    emitter.instruction("ldr x14, [x5, #32]");                                  // load the previous tail slot for insertion-order linking
    emitter.instruction("str x14, [x12, #48]");                                 // store prev = old tail on the new entry
    emitter.instruction("mov x15, #-1");                                        // sentinel index for end of the insertion-order chain
    emitter.instruction("str x15, [x12, #56]");                                 // store next = none on the new tail entry
    emitter.instruction("ldr x15, [x5, #24]");                                  // load the current head slot
    emitter.instruction("cmp x15, #-1");                                        // is this the first insertion into the table?
    emitter.instruction("b.ne __rt_hash_set_link_tail");                        // existing tables append after the previous tail
    emitter.instruction("str x9, [x5, #24]");                                   // initialize head = inserted slot
    emitter.instruction("str x9, [x5, #32]");                                   // initialize tail = inserted slot
    emitter.instruction("b __rt_hash_set_insert_header");                       // skip the tail-link update for the first entry
    emitter.label("__rt_hash_set_link_tail");
    emitter.instruction("mov x16, #64");                                        // x16 = hash entry size for tail-slot addressing
    hash_layout::emit_entry_address(emitter, "x17", "x5", "x14");
    emitter.instruction("str x9, [x17, #56]");                                  // link old tail.next = inserted slot
    emitter.instruction("str x9, [x5, #32]");                                   // update tail = inserted slot
    emitter.label("__rt_hash_set_insert_header");

    // -- increment count --
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload hash_table_ptr
    emitter.instruction("ldr x13, [x5]");                                       // load current count
    emitter.instruction("add x13, x13, #1");                                    // count += 1
    emitter.instruction("str x13, [x5]");                                       // store updated count
    super::hash_next_index::record_insert(emitter, "x5", "x12", "__rt_hash_set");
    emitter.instruction("b __rt_hash_set_done");                                // done inserting

    // -- update existing entry's value --
    emitter.label("__rt_hash_set_update");
    emitter.instruction("mov x0, x5");                                          // inspect ownership in the actual separated hash
    emitter.instruction("ldr x1, [sp, #8]");                                    // recover the mutating key payload
    emitter.instruction("ldr x2, [sp, #16]");                                   // recover its normalized high word
    emitter.instruction("bl __rt_hash_write_guard_claim");                      // avoid releasing an owner already consumed by an outer construction write
    emitter.instruction("ldr x5, [sp, #0]");                                    // recover the stable header after key comparison
    emitter.instruction("ldr x9, [sp, #48]");                                   // recover the previously selected slot index
    hash_layout::emit_entry_address(emitter, "x12", "x5", "x9");
    emitter.instruction("cbz x0, __rt_hash_set_write_value");                   // replace the borrowing entry without releasing its old value twice
    emitter.instruction("ldr x13, [x12, #40]");                                 // load the overwritten entry's per-entry value_tag
    emitter.instruction("cmp x13, #7");                                         // can this Mixed entry contain an invoker reference marker?
    emitter.instruction("b.ne __rt_hash_set_check_reference");                  // concrete entries cannot contain the boxed marker shape
    emitter.instruction("ldr x14, [x12, #24]");                                 // load the existing boxed Mixed payload
    emitter.instruction("cbz x14, __rt_hash_set_check_reference");              // a null Mixed handle is an ordinary overwrite
    emitter.instruction("ldr x15, [x14]");                                      // inspect the boxed Mixed runtime tag
    emitter.instruction(&format!("cmp x15, #{}", INVOKER_ARG_REF_CELL_TAG));    // does this box alias caller storage for a ref argument?
    emitter.instruction("b.eq __rt_hash_set_invoker_reference_write");          // preserve the marker and write through to its caller local
    emitter.instruction(&format!("cmp x15, #{}", ARRAY_GLOBAL_REF_CELL_TAG));   // does this marker own global-variable reference storage?
    emitter.instruction("b.eq __rt_hash_set_update_owned_ref");                 // a plain write goes through the reference it owns
    emitter.instruction(&format!("cmp x15, #{}", ARRAY_LOCAL_REF_CELL_TAG));    // does this marker own promoted local reference storage?
    emitter.instruction("b.eq __rt_hash_set_update_owned_ref");                 // a plain write goes through the reference it owns
    emitter.label("__rt_hash_set_check_reference");
    emitter.instruction(&format!("cmp x13, #{}", INVOKER_ARG_REF_CELL_TAG));    // does the existing entry belong to a PHP reference set?
    emitter.instruction("b.eq __rt_hash_set_reference_write");                  // reference entries are written through, never detached
    emitter.instruction("cmp x13, #8");                                         // is the overwritten value null?
    emitter.instruction("b.eq __rt_hash_set_write_value");                      // null has no heap pointer, skip release
    emitter.instruction("cmp x13, #1");                                         // is the overwritten value a string?
    emitter.instruction("b.eq __rt_hash_set_release_any");                      // strings release through the uniform dispatcher
    emitter.instruction("cmp x13, #10");                                        // is the overwritten value a callable descriptor?
    emitter.instruction("b.eq __rt_hash_set_release_callable");                 // callable descriptors release through the descriptor helper
    emitter.instruction("cmp x13, #4");                                         // is the overwritten value a heap-backed payload?
    emitter.instruction("b.hs __rt_hash_set_release_any");                      // tags 4-7 all release through the uniform dispatcher
    emitter.instruction("b __rt_hash_set_write_value");                         // scalars/bools/floats do not need release before overwrite

    // -- an element reference marker `{12|13, cell, source_tag}`: `=&` rebinds it, any other write goes through --
    emitter.label("__rt_hash_set_update_owned_ref");
    emitter.instruction("ldr x10, [sp, #40]");                                  // load the incoming hash value tag
    emitter.instruction("cmp x10, #7");                                         // only a boxed incoming value can itself be a reference
    emitter.instruction("b.ne __rt_hash_set_update_store_through");             // a plain value is written through the reference
    emitter.instruction("ldr x10, [sp, #24]");                                  // load the incoming box
    emitter.instruction("cbz x10, __rt_hash_set_update_store_through");         // a null box is a plain value
    emitter.instruction("ldr x11, [x10]");                                      // inspect the incoming box's tag
    emitter.instruction(&format!("cmp x11, #{}", INVOKER_ARG_REF_CELL_TAG));    // is the incoming value a reference too?
    emitter.instruction("b.eq __rt_hash_set_release_any");                      // `=&` rebinds: release this bucket's share, store the new one
    emitter.instruction(&format!("cmp x11, #{}", ARRAY_GLOBAL_REF_CELL_TAG));   // is the incoming value a global reference?
    emitter.instruction("b.eq __rt_hash_set_release_any");                      // `=&` rebinds: release this bucket's share, store the new one
    emitter.instruction(&format!("cmp x11, #{}", ARRAY_LOCAL_REF_CELL_TAG));    // is the incoming value a local reference?
    emitter.instruction("b.eq __rt_hash_set_release_any");                      // `=&` rebinds: release this bucket's share, store the new one
    emitter.label("__rt_hash_set_update_store_through");
    emitter.instruction("mov x0, x14");                                         // the bucket's marker box
    emitter.instruction("ldr x1, [sp, #24]");                                   // the incoming low payload word
    emitter.instruction("ldr x2, [sp, #32]");                                   // the incoming high payload word
    emitter.instruction("ldr x3, [sp, #40]");                                   // the incoming value tag
    emitter.instruction("bl __rt_ref_marker_store");                            // write the value into the shared reference cell
    emitter.instruction("cbnz x0, __rt_hash_set_done");                         // the bucket keeps its marker; nothing else to store
    emitter.instruction("ldr x5, [sp, #0]");                                    // a typed cell: reload the hash table pointer
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the probe index
    hash_layout::emit_entry_address(emitter, "x12", "x5", "x9");
    emitter.instruction("ldr x9, [x12, #24]");                                  // reload the bucket's marker box
    emitter.instruction("ldr x10, [sp, #40]");                                  // load the incoming hash value tag
    emitter.instruction("cmp x10, #7");                                         // typed-marker writes arrive as fresh boxed Mixed values
    emitter.instruction("b.ne __rt_hash_set_release_any");                      // keep the conservative replacement path for non-boxed callers
    emitter.instruction("ldr x10, [x9, #8]");                                   // load the original variable storage address
    emitter.instruction("ldr x11, [x9, #16]");                                  // load the original variable's runtime storage tag
    emitter.instruction("cmp x11, #7");                                         // does the original variable store a boxed Mixed handle?
    emitter.instruction("b.eq __rt_hash_set_update_typed_marker_mixed");        // transfer the replacement box directly for Mixed storage
    emitter.instruction("ldr x9, [sp, #24]");                                   // load the fresh replacement Mixed wrapper
    emitter.instruction("ldr x11, [x9, #8]");                                   // extract its low payload word
    emitter.instruction("str x11, [x10]");                                      // write the low payload through to the original variable
    emitter.instruction("ldr x11, [x9, #16]");                                  // extract its high payload word
    emitter.instruction("str x11, [x10, #8]");                                  // write the high payload through to the original variable
    emitter.instruction("mov x0, x9");                                          // pass only the consumed wrapper to the raw heap free helper
    emitter.instruction("bl __rt_heap_free");                                   // keep the transferred payload alive while releasing its wrapper
    emitter.instruction("b __rt_hash_set_done");                                // leave the boxed marker installed in the hash bucket
    emitter.label("__rt_hash_set_update_typed_marker_mixed");
    emitter.instruction("ldr x9, [sp, #24]");                                   // load the fresh boxed Mixed handle
    emitter.instruction("str x9, [x10]");                                       // transfer the handle into the original Mixed variable slot
    emitter.instruction("b __rt_hash_set_done");                                // leave the boxed marker installed in the hash bucket

    // Publish-before-retire, php's assignment order: the replacement is in the entry before the
    // old value is released, because that release can run a destructor that writes this same key
    // (`$this->h->a['k'] = ...` from `__destruct`). Retiring first let the write that resumed
    // afterwards overwrite the destructor's value and lose a reference.
    emitter.label("__rt_hash_set_release_any");
    emitter.instruction("ldr x0, [x12, #24]");                                  // load the previous heap-backed value pointer from the entry
    emitter.instruction("str x0, [sp, #56]");                                   // keep it for the release that follows the publish
    emit_write_entry_value_aarch64(emitter);
    emitter.instruction("ldr x0, [sp, #56]");                                   // the previous payload, no longer reachable from the entry
    emitter.instruction("bl __rt_decref_any");                                  // release the overwritten payload through the uniform dispatcher
    emitter.instruction("b __rt_hash_set_done");                                // the entry already holds the replacement

    emitter.label("__rt_hash_set_release_callable");
    emitter.instruction("ldr x0, [x12, #24]");                                  // load the previous callable descriptor pointer from the entry
    emitter.instruction("str x0, [sp, #56]");                                   // keep it for the release that follows the publish
    emit_write_entry_value_aarch64(emitter);
    emitter.instruction("ldr x0, [sp, #56]");                                   // the previous descriptor, no longer reachable from the entry
    emitter.instruction("bl __rt_callable_descriptor_release");                 // release the overwritten callable descriptor
    emitter.instruction("b __rt_hash_set_done");                                // the entry already holds the replacement

    // -- write through a boxed descriptor-invoker marker stored in a Mixed hash entry --
    emitter.label("__rt_hash_set_invoker_reference_write");
    emitter.instruction("ldr x15, [x14, #16]");                                 // load the caller storage's runtime representation tag
    emitter.instruction("cmp x15, #7");                                         // does the caller local hold a boxed Mixed handle?
    emitter.instruction("b.ne __rt_hash_set_invoker_reference_raw");            // concrete caller slots receive their native payload words
    emitter.instruction("ldr x13, [sp, #40]");                                  // load the replacement runtime value tag
    emitter.instruction("cmp x13, #7");                                         // is the replacement already an owned boxed Mixed cell?
    emitter.instruction("b.eq __rt_hash_set_invoker_reference_mixed_ready");    // owned Mixed values can transfer directly to caller storage
    emitter.instruction("mov x0, x13");                                         // pass the replacement runtime tag to the owned-box helper
    emitter.instruction("ldr x1, [sp, #24]");                                   // pass the replacement low payload word
    emitter.instruction("ldr x2, [sp, #32]");                                   // pass the replacement high payload word
    emitter.instruction("bl __rt_hash_to_mixed_box_owned");                     // consume the replacement into one owned Mixed cell
    emitter.instruction("str x0, [sp, #24]");                                   // save the boxed replacement across entry reconstruction
    emitter.label("__rt_hash_set_invoker_reference_mixed_ready");
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload the hash table after optional boxing
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the current probe index
    hash_layout::emit_entry_address(emitter, "x12", "x5", "x9");
    emitter.instruction("ldr x14, [x12, #24]");                                 // reload the boxed invoker marker
    emitter.instruction("ldr x14, [x14, #8]");                                  // load the caller's Mixed storage address
    emitter.instruction("ldr x0, [x14]");                                       // save the previous boxed Mixed owner for release
    emitter.instruction("ldr x13, [sp, #24]");                                  // load the owned replacement box
    emitter.instruction("str x13, [x14]");                                      // publish the replacement before a destructor can throw
    emitter.instruction("bl __rt_decref_mixed");                                // retire the previous caller-local Mixed owner
    emitter.instruction("b __rt_hash_set_done");                                // the argument hash still owns the unchanged marker wrapper

    emitter.label("__rt_hash_set_invoker_reference_raw");
    emitter.instruction("ldr x14, [x14, #8]");                                  // load the concrete caller-storage address
    emitter.instruction("ldr x0, [x14]");                                       // retain the previous low payload for optional release
    emitter.instruction("ldr x13, [sp, #24]");                                  // load replacement value_lo
    emitter.instruction("str x13, [x14]");                                      // publish replacement value_lo through the marker
    emitter.instruction("ldr x13, [sp, #32]");                                  // load replacement value_hi
    emitter.instruction("str x13, [x14, #8]");                                  // publish replacement value_hi through the marker
    emitter.instruction("cmp x15, #1");                                         // strings own their low payload
    emitter.instruction("b.eq __rt_hash_set_invoker_reference_release_any");    // release the overwritten string after publication
    emitter.instruction("cmp x15, #10");                                        // callable descriptors use their dedicated release helper
    emitter.instruction("b.eq __rt_hash_set_invoker_reference_release_callable"); // retire the overwritten callable descriptor
    emitter.instruction("cmp x15, #4");                                         // arrays, hashes and objects own their low payload
    emitter.instruction("b.lo __rt_hash_set_done");                             // scalar caller slots need no release
    emitter.instruction("cmp x15, #6");                                         // tag 7 was handled by the boxed-Mixed path above
    emitter.instruction("b.ls __rt_hash_set_invoker_reference_release_any");    // release tags 4 through 6 uniformly
    emitter.instruction("b __rt_hash_set_done");                                // null and other non-owning tags are complete
    emitter.label("__rt_hash_set_invoker_reference_release_any");
    emitter.instruction("bl __rt_decref_any");                                  // release the overwritten heap payload after publication
    emitter.instruction("b __rt_hash_set_done");                                // return the unchanged argument hash
    emitter.label("__rt_hash_set_invoker_reference_release_callable");
    emitter.instruction("bl __rt_callable_descriptor_release");                 // release the overwritten callable after publication
    emitter.instruction("b __rt_hash_set_done");                                // return the unchanged argument hash

    // -- write through an existing PHP reference set instead of replacing the entry --
    emitter.label("__rt_hash_set_reference_write");
    emitter.instruction("ldr x13, [sp, #40]");                                  // load the replacement runtime value tag
    emitter.instruction("cmp x13, #7");                                         // is the replacement already an owned boxed Mixed cell?
    emitter.instruction("b.eq __rt_hash_set_reference_release");                // an owned box transfers straight into the reference cell
    emitter.instruction("mov x0, x13");                                         // pass the replacement runtime value tag to the owned-box helper
    emitter.instruction("ldr x1, [sp, #24]");                                   // pass the replacement low payload word to the owned-box helper
    emitter.instruction("ldr x2, [sp, #32]");                                   // pass the replacement high payload word to the owned-box helper
    emitter.instruction("bl __rt_hash_to_mixed_box_owned");                     // box the replacement without adding a retain
    emitter.instruction("str x0, [sp, #24]");                                   // save the boxed replacement as the new reference payload

    emitter.label("__rt_hash_set_reference_release");
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload hash table pointer after the boxing helper
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload probe index after the boxing helper
    hash_layout::emit_entry_address(emitter, "x12", "x5", "x9");
    emitter.instruction("ldr x0, [x12, #24]");                                  // load the managed reference cell this entry owns
    emitter.instruction("str x0, [sp, #56]");                                   // save the cell across the payload release
    emitter.instruction("bl __rt_reference_cell_value_release");                // release the value the reference set currently holds
    emitter.instruction("ldr x0, [sp, #56]");                                   // reload the managed reference cell
    emitter.instruction("ldr x13, [sp, #24]");                                  // load the boxed replacement payload
    emitter.instruction("str x13, [x0]");                                       // publish the replacement to every alias in the reference set
    emitter.instruction("b __rt_hash_set_done");                                // value_lo, value_hi and value_tag keep describing the cell

    emitter.label("__rt_hash_set_write_value");
    emit_write_entry_value_aarch64(emitter);

    // -- tear down stack frame and return --
    emitter.label("__rt_hash_set_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return table pointer (may be new after grow)
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    emit_hash_set_value_aarch64(emitter);
}

/// Emits the ARM64 `__rt_hash_set_value`, the bucket-replacing sibling of `__rt_hash_set`.
///
/// Same ABI as `__rt_hash_set`. The difference is the reference rule: this entry point DETACHES
/// an existing reference entry instead of writing through it, which is what an array-building
/// helper needs when it populates a destination that was cloned from a user array. The clone
/// shares its reference cells with the source, so writing through would mutate the source too.
///
/// Copy-on-write runs first so the detach can never touch a table another owner still shares,
/// and the unique table is what the tail call to `__rt_hash_set` then updates.
fn emit_hash_set_value_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_set_value ---");
    emitter.label_global("__rt_hash_set_value");
    emitter.instruction("sub sp, sp, #64");                                     // reserve slots for the forwarded arguments and the unique table
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the detach frame
    emitter.instruction("str x1, [sp, #0]");                                    // save key_lo across the split and probe
    emitter.instruction("str x2, [sp, #8]");                                    // save key_hi across the split and probe
    emitter.instruction("str x3, [sp, #16]");                                   // save value_lo across the split and probe
    emitter.instruction("str x4, [sp, #24]");                                   // save value_hi across the split and probe
    emitter.instruction("str x5, [sp, #32]");                                   // save value_tag across the split and probe
    emitter.instruction("bl __rt_hash_ensure_unique");                          // split a shared table before detaching anything in it
    emitter.instruction("str x0, [sp, #40]");                                   // save the unique table pointer
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload key_lo for the entry probe
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload key_hi for the entry probe
    emitter.instruction("bl __rt_hash_get");                                    // x0 = found, x4 = matching entry address
    emitter.instruction("cbz x0, __rt_hash_set_value_forward");                 // a missing key is an ordinary insert
    emitter.instruction("ldr x9, [x4, #40]");                                   // load the existing entry's runtime value tag
    emitter.instruction("cmp x9, #11");                                         // does this entry belong to a PHP reference set?
    emitter.instruction("b.ne __rt_hash_set_value_forward");                    // ordinary entries need no detach
    emitter.instruction("ldr x0, [x4, #24]");                                   // load the managed reference cell this entry owns
    emitter.instruction("str xzr, [x4, #24]");                                  // retire the detached payload before releasing it
    emitter.instruction("str xzr, [x4, #32]");                                  // reference entries carry no high payload word
    emitter.instruction("mov x9, #8");                                          // runtime value tag 8 = null
    emitter.instruction("str x9, [x4, #40]");                                   // the slot reads as null until the replacement lands
    emitter.instruction("bl __rt_decref_any");                                  // drop this table's ownership of the reference cell
    emitter.label("__rt_hash_set_value_forward");
    emitter.instruction("ldr x0, [sp, #40]");                                   // forward the unique table pointer
    emitter.instruction("ldr x1, [sp, #0]");                                    // forward key_lo
    emitter.instruction("ldr x2, [sp, #8]");                                    // forward key_hi
    emitter.instruction("ldr x3, [sp, #16]");                                   // forward value_lo
    emitter.instruction("ldr x4, [sp, #24]");                                   // forward value_hi
    emitter.instruction("ldr x5, [sp, #32]");                                   // forward value_tag
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the detach frame
    emitter.instruction("b __rt_hash_set");                                     // the entry is now ordinary, so the shared path applies
}

/// Emits the x86_64 Linux implementation of `__rt_hash_set`.
///
/// Implements the same linear-probe insert-or-update algorithm as the ARM64
/// variant using the SystemV AMD64 ABI: rdi=hash_table_ptr, rsi=key_ptr,
/// rdx=key_len, rcx=value_lo, r8=value_hi, r9=value_tag. Returns the table
/// pointer (possibly new after grow) in rax. Preserves r12/r13/r14 as
/// callee-saved registers; all other caller-saved regs are clobbered.
fn emit_hash_set_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_set ---");
    emitter.label_global("__rt_hash_set");
    emitter.instruction("xor r10d, r10d");                                      // ordinary writes retain the hash COW boundary
    emitter.instruction("jmp __rt_hash_set_x86_entry");                         // share insertion and update machinery
    emitter.label_global("__rt_hash_set_shared");
    emitter.instruction("mov r10d, 1");                                         // pending append mutates its pre-callback selected payload
    emitter.label("__rt_hash_set_x86_entry");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving hash-insert spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved table/key/value tuple
    emitter.instruction("sub rsp, 112");                                        // reserve local storage plus callee-saved register spills while keeping nested calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 72], r12");                       // preserve r12 because SysV requires callees to restore it before returning
    emitter.instruction("mov QWORD PTR [rbp - 80], r13");                       // preserve r13 because the hash probe logic reuses it as a long-lived scratch register
    emitter.instruction("mov QWORD PTR [rbp - 88], r14");                       // preserve r14 because the insertion-order linker uses it as a callee-saved scratch register
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the hash-table pointer across helper calls and probe iterations
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the incoming key pointer across helper calls and probe iterations
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the incoming key length across helper calls and probe iterations
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the low payload word across the probe and optional update paths
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // save the high payload word across the probe and optional update paths
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // save the runtime value tag across the probe and optional update paths
    emitter.instruction("mov QWORD PTR [rbp - 64], r10");                       // preserve the pending-append selection mode
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the candidate hash-table pointer before null normalization
    emitter.instruction("test rdi, rdi");                                       // is the dynamic-property hash slot still null?
    emitter.instruction("jnz __rt_hash_set_x86_have_table");                    // a live table skips the null-hash allocation
    emitter.instruction("mov rdi, 4");                                          // allocate a four-entry hash for a null dynamic-property slot
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // preserve the caller's value tag as the hash value type
    emitter.instruction("call __rt_hash_new");                                  // create the missing hash table
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // publish the freshly allocated table
    emitter.instruction("mov rdi, rax");                                        // keep the new table as the mutation target
    emitter.label("__rt_hash_set_x86_have_table");
    emitter.instruction("mov rax, rdi");                                        // selected mode returns the original table without a COW helper call
    emitter.instruction("test r10, r10");                                       // was this table selected before a callback retained its payload?
    emitter.instruction("jnz __rt_hash_set_x86_ready");                         // yes, preserve that exact payload identity
    emitter.instruction("call __rt_hash_ensure_unique");                        // split ordinary shared associative arrays before mutation
    emitter.label("__rt_hash_set_x86_ready");
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the unique hash-table pointer that subsequent probe logic must mutate

    // -- check load factor: grow if count * 4 >= capacity * 3 (75%) --
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load the current live-entry count
    emitter.instruction("mov r11, QWORD PTR [rax + 8]");                        // load the current table capacity
    emitter.instruction("mov r12, r10");                                        // copy count before scaling it
    emitter.instruction("shl r12, 2");                                          // scaled_count = count * 4
    emitter.instruction("lea r13, [r11 + r11 * 2]");                            // scaled_capacity = capacity * 3
    emitter.instruction("cmp r12, r13");                                        // has the table reached the growth threshold?
    emitter.instruction("jl __rt_hash_set_no_grow_x");                          // skip growth while under the load threshold
    emitter.instruction("mov rdi, rax");                                        // pass the unique table to the grow helper
    emitter.instruction("call __rt_hash_grow");                                 // double the hash table capacity before probing
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the grown table pointer
    emitter.label("__rt_hash_set_no_grow_x");

    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the incoming key low word after the copy-on-write helper clobbered caller-saved argument registers
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the incoming key high word after the copy-on-write helper clobbered caller-saved argument registers
    emitter.instruction("call __rt_hash_key_hash");                             // compute the 64-bit hash for the normalized inserted key
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the hash-table pointer after the hash helper returns
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // load the table capacity for the modulo operation and linear-probe loop
    emitter.instruction("xor edx, edx");                                        // clear the high dividend half before dividing the 64-bit hash by the capacity
    emitter.instruction("div r11");                                             // compute hash % capacity using the SysV integer divide remainder register
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // save the initial probe index so the loop can survive helper calls
    // A tombstone does NOT end the probe (see the AArch64 arm): [rbp - 96] remembers the first
    // one (index + 1, 0 = none) and [rbp - 104] counts probes so a table without an empty slot
    // still terminates.
    emitter.instruction("mov QWORD PTR [rbp - 96], 0");                         // no tombstone seen yet on this probe path
    emitter.instruction("mov QWORD PTR [rbp - 104], 0");                        // no slot probed yet

    emitter.label("__rt_hash_set_probe");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the hash-table pointer to read its capacity
    emitter.instruction("mov r11, QWORD PTR [rbp - 104]");                      // load the number of slots probed so far
    emitter.instruction("cmp r11, QWORD PTR [r10 + 8]");                        // has every slot been probed?
    emitter.instruction("jae __rt_hash_set_probe_exhausted");                   // yes: no empty slot ends this chain
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the hash-table pointer at the top of every probe iteration
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the current probe index before deriving the slot address
    hash_layout::emit_entry_address(emitter, "r12", "r10", "r11");
    emitter.instruction("mov r13, QWORD PTR [r12]");                            // load the occupied marker for the probed hash-entry slot
    emitter.instruction("cmp r13, 1");                                          // check whether the current probe landed on an occupied entry
    emitter.instruction("je __rt_hash_set_occupied");                           // a live entry: compare its key
    emitter.instruction("test r13, r13");                                       // an empty slot?
    emitter.instruction("jz __rt_hash_set_probe_exhausted");                    // an empty slot proves the key absent
    emitter.instruction("cmp QWORD PTR [rbp - 96], 0");                         // a tombstone: has an earlier one been remembered?
    emitter.instruction("jne __rt_hash_set_advance");                           // yes: keep the first one and probe on
    emitter.instruction("lea r13, [r11 + 1]");                                  // remember this tombstone's index + 1
    emitter.instruction("mov QWORD PTR [rbp - 96], r13");                       // save the first tombstone for a later insert
    emitter.instruction("jmp __rt_hash_set_advance");                           // the key may still live further along the chain

    // -- the key is absent: insert into the first tombstone, or into this empty slot --
    emitter.label("__rt_hash_set_probe_exhausted");
    emitter.instruction("mov r13, QWORD PTR [rbp - 96]");                       // r13 = first tombstone index + 1, or 0
    emitter.instruction("test r13, r13");                                       // was a tombstone remembered?
    emitter.instruction("jz __rt_hash_set_probe_end");                          // none: use the slot the probe stopped at
    emitter.instruction("sub r13, 1");                                          // r13 = the first tombstone's index
    emitter.instruction("mov QWORD PTR [rbp - 56], r13");                       // the insert writes the tombstone slot
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the hash-table pointer to address the tombstone
    hash_layout::emit_entry_address(emitter, "r12", "r10", "r13");
    emitter.instruction("jmp __rt_hash_set_insert");                            // reuse the tombstone for the new key
    emitter.label("__rt_hash_set_probe_end");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the hash-table pointer to read its capacity
    emitter.instruction("mov r11, QWORD PTR [rbp - 104]");                      // reload the number of slots probed
    emitter.instruction("cmp r11, QWORD PTR [r10 + 8]");                        // did the probe stop at an empty slot, or run out of slots?
    emitter.instruction("jb __rt_hash_set_insert");                             // an empty slot (r12 still addresses it): insert here
    emitter.instruction("jmp __rt_hash_set_done_x");                            // a full table with no tombstone (the load factor prevents it)

    emitter.label("__rt_hash_set_occupied");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // pass the incoming key pointer to the x86_64 string-equality helper
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // pass the incoming key length to the x86_64 string-equality helper
    emitter.instruction("mov rdx, QWORD PTR [r12 + 8]");                        // pass the stored entry key pointer to the equality helper
    emitter.instruction("mov rcx, QWORD PTR [r12 + 16]");                       // pass the stored entry key length to the equality helper
    emitter.instruction("call __rt_hash_key_eq");                               // compare the existing normalized key with the inserted key before deciding between update and probe
    emitter.instruction("test rax, rax");                                       // check whether the probed slot already stores the same logical key
    emitter.instruction("jne __rt_hash_set_update");                            // overwrite the existing payload instead of probing further when the keys match
    emitter.label("__rt_hash_set_advance");
    emitter.instruction("add QWORD PTR [rbp - 104], 1");                        // count this probed slot
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the hash-table pointer after the equality helper clobbered caller-saved registers
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // reload the table capacity before advancing the linear-probe cursor
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // reload the current probe index before incrementing it
    emitter.instruction("add rdx, 1");                                          // advance to the next linear-probe slot after a key mismatch
    emitter.instruction("cmp rdx, r11");                                        // detect wraparound once the probe index reaches the table capacity
    emitter.instruction("jb __rt_hash_set_store_probe");                        // keep the incremented probe index when the cursor remains in bounds
    emitter.instruction("xor edx, edx");                                        // wrap the probe cursor back to slot zero once the end of the table is reached

    emitter.label("__rt_hash_set_store_probe");
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // persist the updated probe index before the next loop iteration
    emitter.instruction("jmp __rt_hash_set_probe");                             // continue probing until an empty slot or existing matching key is found

    emitter.label("__rt_hash_set_insert");
    emitter.instruction("mov QWORD PTR [r12], 1");                              // mark the selected entry slot as occupied before filling its payload fields
    emitter.instruction("mov QWORD PTR [rbp - 64], r12");                       // preserve the selected entry pointer across the string-persist helper call for computed keys
    emitter.instruction("cmp QWORD PTR [rbp - 24], -1");                        // check whether the inserted key is an integer key
    emitter.instruction("je __rt_hash_set_key_ready");                          // integer keys are stored inline and do not need persistence
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // move the inserted key pointer into the x86_64 string helper input register
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // move the inserted key length into the paired x86_64 string helper register
    emitter.instruction("call __rt_str_persist");                               // duplicate the inserted key so hash-table storage does not alias transient concat-buffer bytes
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the owned key pointer for the selected hash entry
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the owned key length for the selected hash entry
    emitter.label("__rt_hash_set_key_ready");
    emitter.instruction("mov r12, QWORD PTR [rbp - 64]");                       // restore the selected entry pointer after persisting the inserted key
    emitter.instruction("mov r13, QWORD PTR [rbp - 16]");                       // reload the normalized key low word for storage in the selected hash entry
    emitter.instruction("mov QWORD PTR [r12 + 8], r13");                        // store the owned string pointer or integer payload into the selected hash entry
    emitter.instruction("mov r13, QWORD PTR [rbp - 24]");                       // reload the normalized key high word for storage in the selected hash entry
    emitter.instruction("mov QWORD PTR [r12 + 16], r13");                       // store the string length or integer sentinel into the selected hash entry
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the hash-table pointer after the string-persist helper clobbered caller-saved registers
    emitter.instruction("mov r13, QWORD PTR [rbp - 32]");                       // reload the low payload word that belongs to the inserted hash value
    emitter.instruction("mov QWORD PTR [r12 + 24], r13");                       // store the low payload word into the hash entry payload area
    emitter.instruction("mov r13, QWORD PTR [rbp - 40]");                       // reload the high payload word that belongs to the inserted hash value
    emitter.instruction("mov QWORD PTR [r12 + 32], r13");                       // store the high payload word into the hash entry payload area
    emitter.instruction("mov r13, QWORD PTR [rbp - 48]");                       // reload the runtime value tag that describes the inserted payload
    emitter.instruction("mov QWORD PTR [r12 + 40], r13");                       // store the runtime value tag into the selected hash entry
    emitter.instruction("mov r13, QWORD PTR [r10 + 32]");                       // load the previous insertion-order tail slot from the hash header
    emitter.instruction("mov QWORD PTR [r12 + 48], r13");                       // link the new entry back to the previous tail slot
    emitter.instruction("mov r14, -1");                                         // materialize the end-of-chain sentinel for the inserted tail entry
    emitter.instruction("mov QWORD PTR [r12 + 56], r14");                       // initialize the new entry next-pointer as the tail sentinel
    emitter.instruction("cmp r13, -1");                                         // detect the first insertion so the hash header head/tail can be initialized together
    emitter.instruction("jne __rt_hash_set_link_tail");                         // existing tables need an extra step to wire the previous tail forward to the inserted slot
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the inserted slot index to seed the insertion-order head pointer
    emitter.instruction("mov QWORD PTR [r10 + 24], r11");                       // header[24]: initialize the insertion-order head for the first occupied entry
    emitter.instruction("mov QWORD PTR [r10 + 32], r11");                       // header[32]: initialize the insertion-order tail for the first occupied entry
    emitter.instruction("jmp __rt_hash_set_bump_count");                        // skip the previous-tail forward-link update on the very first insertion

    emitter.label("__rt_hash_set_link_tail");
    hash_layout::emit_entry_address(emitter, "r14", "r10", "r13");
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the inserted slot index for the previous-tail forward-link store
    emitter.instruction("mov QWORD PTR [r14 + 56], r11");                       // update the previous tail entry to point at the inserted slot as its logical successor
    emitter.instruction("mov QWORD PTR [r10 + 32], r11");                       // publish the inserted slot as the new insertion-order tail in the hash header

    emitter.label("__rt_hash_set_bump_count");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the current live-entry count from the hash header before incrementing it
    emitter.instruction("add r11, 1");                                          // increment the live-entry count after claiming a previously empty hash slot
    emitter.instruction("mov QWORD PTR [r10], r11");                            // store the updated live-entry count back into the hash header
    super::hash_next_index::record_insert(emitter, "r10", "r12", "__rt_hash_set");
    emitter.instruction("mov rax, r10");                                        // return the original hash-table pointer after a successful insertion
    emitter.instruction("mov r14, QWORD PTR [rbp - 88]");                       // restore the caller's r14 before leaving the hash-set helper
    emitter.instruction("mov r13, QWORD PTR [rbp - 80]");                       // restore the caller's r13 before leaving the hash-set helper
    emitter.instruction("mov r12, QWORD PTR [rbp - 72]");                       // restore the caller's r12 before leaving the hash-set helper
    emitter.instruction("add rsp, 112");                                        // release the local spill area that held the saved table/key/value tuple
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to the insertion caller
    emitter.instruction("ret");                                                 // return to the caller with the hash-table pointer in rax

    // -- a full table with no tombstone: leave it unchanged (the load factor prevents it) --
    emitter.label("__rt_hash_set_done_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the unchanged hash-table pointer
    emitter.instruction("mov r14, QWORD PTR [rbp - 88]");                       // restore the caller's r14 before leaving the hash-set helper
    emitter.instruction("mov r13, QWORD PTR [rbp - 80]");                       // restore the caller's r13 before leaving the hash-set helper
    emitter.instruction("mov r12, QWORD PTR [rbp - 72]");                       // restore the caller's r12 before leaving the hash-set helper
    emitter.instruction("add rsp, 112");                                        // release the local spill area
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller with the hash-table pointer in rax

    emitter.label("__rt_hash_set_update");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // inspect ownership in the actual separated hash
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // recover the mutating key payload
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // recover its normalized high word
    emitter.instruction("call __rt_hash_write_guard_claim");                    // avoid releasing an owner already consumed by an outer construction write
    emitter.instruction("test rax, rax");                                       // determine whether this update owns its previous value
    emitter.instruction("jz __rt_hash_set_write_value_x");                      // retain the callee-saved selected entry while skipping duplicate release
    emitter.instruction("mov r13, QWORD PTR [r12 + 40]");                       // load the overwritten entry's runtime value tag before replacing it
    emitter.instruction("cmp r13, 7");                                          // can this Mixed entry contain an invoker reference marker?
    emitter.instruction("jne __rt_hash_set_check_reference_x");                 // concrete entries cannot contain the boxed marker shape
    emitter.instruction("mov r14, QWORD PTR [r12 + 24]");                       // load the existing boxed Mixed payload
    emitter.instruction("test r14, r14");                                       // guard the boxed marker dereference
    emitter.instruction("jz __rt_hash_set_check_reference_x");                  // a null Mixed handle is an ordinary overwrite
    let boxed_marker_compare = format!("cmp QWORD PTR [r14], {}", INVOKER_ARG_REF_CELL_TAG);
    emitter.instruction(&boxed_marker_compare);                                 // does this box alias caller storage for a ref argument?
    emitter.instruction("je __rt_hash_set_invoker_reference_write_x");          // preserve the marker and write through to its caller local
    emitter.instruction(&format!("cmp QWORD PTR [r14], {}", ARRAY_GLOBAL_REF_CELL_TAG)); // does this marker own global-variable reference storage?
    emitter.instruction("je __rt_hash_set_update_owned_ref_x");                 // a plain write goes through the reference it owns
    emitter.instruction(&format!("cmp QWORD PTR [r14], {}", ARRAY_LOCAL_REF_CELL_TAG)); // does this marker own promoted local reference storage?
    emitter.instruction("je __rt_hash_set_update_owned_ref_x");                 // a plain write goes through the reference it owns
    emitter.label("__rt_hash_set_check_reference_x");
    emitter.instruction(&format!("cmp r13, {}", INVOKER_ARG_REF_CELL_TAG));     // does the existing entry belong to a PHP reference set?
    emitter.instruction("je __rt_hash_set_reference_write_x");                  // reference entries are written through, never detached
    emitter.instruction("cmp r13, 8");                                          // check whether the overwritten value is PHP null
    emitter.instruction("je __rt_hash_set_write_value_x");                      // null owns no heap payload and can be overwritten directly
    emitter.instruction("cmp r13, 1");                                          // check whether the overwritten value is an owned string
    emitter.instruction("je __rt_hash_set_release_any_x");                      // string values release through the uniform dispatcher
    emitter.instruction("cmp r13, 10");                                         // check whether the overwritten value is a callable descriptor
    emitter.instruction("je __rt_hash_set_release_callable_x");                 // callable descriptors release through the descriptor helper
    emitter.instruction("cmp r13, 4");                                          // check whether the overwritten value is heap backed
    emitter.instruction("jae __rt_hash_set_release_any_x");                     // arrays, hashes, objects, and boxed mixed values need release
    emitter.instruction("jmp __rt_hash_set_write_value_x");                     // scalar values own no heap payload

    // -- an element reference marker `{12|13, cell, source_tag}`: `=&` rebinds it, any other write goes through --
    emitter.label("__rt_hash_set_update_owned_ref_x");
    emitter.instruction("cmp QWORD PTR [rbp - 48], 7");                         // only a boxed incoming value can itself be a reference
    emitter.instruction("jne __rt_hash_set_update_store_through_x");            // a plain value is written through the reference
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // load the incoming box
    emitter.instruction("test r10, r10");                                       // a null box is a plain value
    emitter.instruction("jz __rt_hash_set_update_store_through_x");             // write it through the reference
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // inspect the incoming box's tag
    emitter.instruction(&format!("cmp r11, {}", INVOKER_ARG_REF_CELL_TAG));     // is the incoming value a reference too?
    emitter.instruction("je __rt_hash_set_release_any_x");                      // `=&` rebinds: release this bucket's share, store the new one
    emitter.instruction(&format!("cmp r11, {}", ARRAY_GLOBAL_REF_CELL_TAG));    // is the incoming value a global reference?
    emitter.instruction("je __rt_hash_set_release_any_x");                      // `=&` rebinds: release this bucket's share, store the new one
    emitter.instruction(&format!("cmp r11, {}", ARRAY_LOCAL_REF_CELL_TAG));     // is the incoming value a local reference?
    emitter.instruction("je __rt_hash_set_release_any_x");                      // `=&` rebinds: release this bucket's share, store the new one
    emitter.label("__rt_hash_set_update_store_through_x");
    emitter.instruction("mov rdi, r14");                                        // the bucket's marker box
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // the incoming low payload word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // the incoming high payload word
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // the incoming value tag
    emitter.instruction("call __rt_ref_marker_store");                          // write the value into the shared reference cell
    emitter.instruction("test rax, rax");                                       // was the value written through?
    emitter.instruction("jnz __rt_hash_set_reference_done_x");                  // the bucket keeps its marker; nothing else to store
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // a typed cell: reload the hash-table pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the probe index
    hash_layout::emit_entry_address(emitter, "r12", "r10", "r11");
    emitter.instruction("mov r14, QWORD PTR [r12 + 24]");                       // reload the bucket's marker box
    emitter.instruction("cmp QWORD PTR [rbp - 48], 7");                         // typed-marker writes arrive as fresh boxed Mixed values
    emitter.instruction("jne __rt_hash_set_release_any_x");                     // keep the conservative replacement path for non-boxed callers
    emitter.instruction("mov r10, QWORD PTR [r14 + 8]");                        // load the original variable storage address
    emitter.instruction("mov r11, QWORD PTR [r14 + 16]");                       // load the original variable's runtime storage tag
    emitter.instruction("cmp r11, 7");                                          // does the original variable store a boxed Mixed handle?
    emitter.instruction("je __rt_hash_set_update_typed_marker_mixed_x");        // transfer the replacement box directly for Mixed storage
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // load the fresh replacement Mixed wrapper
    emitter.instruction("mov r11, QWORD PTR [rax + 8]");                        // extract its low payload word
    emitter.instruction("mov QWORD PTR [r10], r11");                            // write the low payload through to the original variable
    emitter.instruction("mov r11, QWORD PTR [rax + 16]");                       // extract its high payload word
    emitter.instruction("mov QWORD PTR [r10 + 8], r11");                        // write the high payload through to the original variable
    emitter.instruction("call __rt_heap_free");                                 // keep the transferred payload alive while releasing its wrapper
    emitter.instruction("jmp __rt_hash_set_reference_done_x");                  // leave the boxed marker installed in the hash bucket
    emitter.label("__rt_hash_set_update_typed_marker_mixed_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // load the fresh boxed Mixed handle
    emitter.instruction("mov QWORD PTR [r10], rax");                            // transfer the handle into the original Mixed variable slot
    emitter.instruction("jmp __rt_hash_set_reference_done_x");                  // leave the boxed marker installed in the hash bucket

    // Publish-before-retire (see the AArch64 arm).
    emitter.label("__rt_hash_set_release_any_x");
    emitter.instruction("mov rax, QWORD PTR [r12 + 24]");                       // load the overwritten heap-backed payload from the entry
    emitter.instruction("mov QWORD PTR [rbp - 112], rax");                      // keep it for the release that follows the publish
    emit_write_entry_value_x86_64(emitter);
    emitter.instruction("mov rax, QWORD PTR [rbp - 112]");                      // the previous payload, no longer reachable from the entry
    emitter.instruction("call __rt_decref_any");                                // release the overwritten hash value after storing the replacement
    emitter.instruction("jmp __rt_hash_set_done_x");                            // the entry already holds the replacement

    emitter.label("__rt_hash_set_release_callable_x");
    emitter.instruction("mov rax, QWORD PTR [r12 + 24]");                       // load the overwritten callable descriptor from the entry
    emitter.instruction("mov QWORD PTR [rbp - 112], rax");                      // keep it for the release that follows the publish
    emit_write_entry_value_x86_64(emitter);
    emitter.instruction("mov rax, QWORD PTR [rbp - 112]");                      // the previous descriptor, no longer reachable from the entry
    emitter.instruction("call __rt_callable_descriptor_release");               // release the overwritten callable descriptor after storing the replacement
    emitter.instruction("jmp __rt_hash_set_done_x");                            // the entry already holds the replacement

    emitter.label("__rt_hash_set_invoker_reference_write_x");
    emitter.instruction("mov r13, QWORD PTR [r14 + 16]");                       // load the caller storage's runtime representation tag
    emitter.instruction("cmp r13, 7");                                          // does the caller local hold a boxed Mixed handle?
    emitter.instruction("jne __rt_hash_set_invoker_reference_raw_x");           // concrete caller slots receive their native payload words
    emitter.instruction("cmp QWORD PTR [rbp - 48], 7");                         // is the replacement already an owned boxed Mixed cell?
    emitter.instruction("je __rt_hash_set_invoker_reference_mixed_ready_x");    // owned Mixed values can transfer directly to caller storage
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // pass the replacement runtime tag to the owned-box helper
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // pass the replacement low payload word
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // pass the replacement high payload word
    emitter.instruction("call __rt_hash_to_mixed_x86_box_owned");               // consume the replacement into one owned Mixed cell
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the boxed replacement across entry reconstruction
    emitter.label("__rt_hash_set_invoker_reference_mixed_ready_x");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the hash table after optional boxing
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the current probe index
    hash_layout::emit_entry_address(emitter, "r12", "r10", "r11");
    emitter.instruction("mov r14, QWORD PTR [r12 + 24]");                       // reload the boxed invoker marker
    emitter.instruction("mov r14, QWORD PTR [r14 + 8]");                        // load the caller's Mixed storage address
    emitter.instruction("mov rax, QWORD PTR [r14]");                            // save the previous boxed Mixed owner for release
    emitter.instruction("mov r13, QWORD PTR [rbp - 32]");                       // load the owned replacement box
    emitter.instruction("mov QWORD PTR [r14], r13");                            // publish the replacement before a destructor can throw
    emitter.instruction("call __rt_decref_mixed");                              // retire the previous caller-local Mixed owner
    emitter.instruction("jmp __rt_hash_set_reference_done_x");                  // the argument hash still owns the unchanged marker wrapper

    emitter.label("__rt_hash_set_invoker_reference_raw_x");
    emitter.instruction("mov r14, QWORD PTR [r14 + 8]");                        // load the concrete caller-storage address
    emitter.instruction("mov rax, QWORD PTR [r14]");                            // retain the previous low payload for optional release
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // load replacement value_lo
    emitter.instruction("mov QWORD PTR [r14], r11");                            // publish replacement value_lo through the marker
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // load replacement value_hi
    emitter.instruction("mov QWORD PTR [r14 + 8], r11");                        // publish replacement value_hi through the marker
    emitter.instruction("cmp r13, 1");                                          // strings own their low payload
    emitter.instruction("je __rt_hash_set_invoker_reference_release_any_x");    // release the overwritten string after publication
    emitter.instruction("cmp r13, 10");                                         // callable descriptors use their dedicated release helper
    emitter.instruction("je __rt_hash_set_invoker_reference_release_callable_x"); // retire the overwritten callable descriptor
    emitter.instruction("cmp r13, 4");                                          // arrays, hashes and objects own their low payload
    emitter.instruction("jb __rt_hash_set_reference_done_x");                   // scalar caller slots need no release
    emitter.instruction("cmp r13, 6");                                          // tag 7 was handled by the boxed-Mixed path above
    emitter.instruction("jbe __rt_hash_set_invoker_reference_release_any_x");   // release tags 4 through 6 uniformly
    emitter.instruction("jmp __rt_hash_set_reference_done_x");                  // null and other non-owning tags are complete
    emitter.label("__rt_hash_set_invoker_reference_release_any_x");
    emitter.instruction("call __rt_decref_any");                                // release the overwritten heap payload after publication
    emitter.instruction("jmp __rt_hash_set_reference_done_x");                  // return the unchanged argument hash
    emitter.label("__rt_hash_set_invoker_reference_release_callable_x");
    emitter.instruction("call __rt_callable_descriptor_release");               // release the overwritten callable after publication
    emitter.instruction("jmp __rt_hash_set_reference_done_x");                  // return the unchanged argument hash

    emitter.label("__rt_hash_set_reference_write_x");
    emitter.instruction("mov r13, QWORD PTR [rbp - 48]");                       // reload the replacement runtime value tag
    emitter.instruction("cmp r13, 7");                                          // is the replacement already an owned boxed Mixed cell?
    emitter.instruction("je __rt_hash_set_reference_release_x");                // an owned box transfers straight into the reference cell
    emitter.instruction("mov rax, r13");                                        // pass the replacement runtime value tag to the owned-box helper
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // pass the replacement low payload word to the owned-box helper
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // pass the replacement high payload word to the owned-box helper
    emitter.instruction("call __rt_hash_to_mixed_x86_box_owned");               // box the replacement without adding a retain
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the boxed replacement as the new reference payload

    emitter.label("__rt_hash_set_reference_release_x");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the hash-table pointer after the boxing helper
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the current probe index for entry-address reconstruction
    hash_layout::emit_entry_address(emitter, "r12", "r10", "r11");
    emitter.instruction("mov rax, QWORD PTR [r12 + 24]");                       // load the managed reference cell this entry owns
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // save the cell across the payload release
    emitter.instruction("call __rt_reference_cell_value_release");              // release the value the reference set currently holds
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // reload the managed reference cell
    emitter.instruction("mov r13, QWORD PTR [rbp - 32]");                       // load the boxed replacement payload
    emitter.instruction("mov QWORD PTR [rax], r13");                            // publish the replacement to every alias in the reference set
    emitter.label("__rt_hash_set_reference_done_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the unchanged hash-table pointer after a write-through
    emitter.instruction("mov r14, QWORD PTR [rbp - 88]");                       // restore the caller's r14 before leaving the write-through path
    emitter.instruction("mov r13, QWORD PTR [rbp - 80]");                       // restore the caller's r13 before leaving the write-through path
    emitter.instruction("mov r12, QWORD PTR [rbp - 72]");                       // restore the caller's r12 before leaving the write-through path
    emitter.instruction("add rsp, 112");                                        // release the local spill area before leaving the write-through path
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to the caller
    emitter.instruction("ret");                                                 // value_lo, value_hi and value_tag keep describing the cell

    emitter.label("__rt_hash_set_write_value_x");
    emit_write_entry_value_x86_64(emitter);
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the unchanged hash-table pointer after an in-place value update
    emitter.instruction("mov r14, QWORD PTR [rbp - 88]");                       // restore the caller's r14 before leaving the update path
    emitter.instruction("mov r13, QWORD PTR [rbp - 80]");                       // restore the caller's r13 before leaving the update path
    emitter.instruction("mov r12, QWORD PTR [rbp - 72]");                       // restore the caller's r12 before leaving the update path
    emitter.instruction("add rsp, 112");                                        // release the local spill area before leaving the update path
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to the insertion caller
    emitter.instruction("ret");                                                 // return to the caller with the existing hash-table pointer in rax

    emit_hash_set_value_linux_x86_64(emitter);
}

/// Emits the x86_64 `__rt_hash_set_value`, mirroring [`emit_hash_set_value_aarch64`].
fn emit_hash_set_value_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_set_value ---");
    emitter.label_global("__rt_hash_set_value");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before the split and probe
    emitter.instruction("mov rbp, rsp");                                        // establish the detach frame
    emitter.instruction("sub rsp, 64");                                         // reserve slots for the forwarded arguments and the unique table
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save key_lo across the split and probe
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save key_hi across the split and probe
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save value_lo across the split and probe
    emitter.instruction("mov QWORD PTR [rbp - 32], r8");                        // save value_hi across the split and probe
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // save value_tag across the split and probe
    emitter.instruction("call __rt_hash_ensure_unique");                        // split a shared table before detaching anything in it
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the unique table pointer
    emitter.instruction("mov rdi, rax");                                        // probe the unique table for the destination key
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload key_lo for the entry probe
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload key_hi for the entry probe
    emitter.instruction("call __rt_hash_get");                                  // rax = found, r8 = matching entry address
    emitter.instruction("test rax, rax");                                       // did the destination already contain this key?
    emitter.instruction("jz __rt_hash_set_value_forward_x");                    // a missing key is an ordinary insert
    emitter.instruction("cmp QWORD PTR [r8 + 40], 11");                         // does this entry belong to a PHP reference set?
    emitter.instruction("jne __rt_hash_set_value_forward_x");                   // ordinary entries need no detach
    emitter.instruction("mov rax, QWORD PTR [r8 + 24]");                        // load the managed reference cell this entry owns
    emitter.instruction("mov QWORD PTR [r8 + 24], 0");                          // retire the detached payload before releasing it
    emitter.instruction("mov QWORD PTR [r8 + 32], 0");                          // reference entries carry no high payload word
    emitter.instruction("mov QWORD PTR [r8 + 40], 8");                          // the slot reads as null until the replacement lands
    emitter.instruction("call __rt_decref_any");                                // drop this table's ownership of the reference cell
    emitter.label("__rt_hash_set_value_forward_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // forward the unique table pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // forward key_lo
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // forward key_hi
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // forward value_lo
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // forward value_hi
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // forward value_tag
    emitter.instruction("add rsp, 64");                                         // release the detach frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("jmp __rt_hash_set");                                   // the entry is now ordinary, so the shared path applies
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Mixed variadic hashes preserve boxed invoker markers and update caller storage on every ABI.
    #[test]
    fn hash_set_writes_through_boxed_invoker_reference_markers_on_every_target() {
        for name in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_hash_set(&mut emitter);
            let assembly = emitter.output();
            let update = assembly.split("__rt_hash_set_update:\n").nth(1).unwrap();
            let (marker_branch, write_label, publish, release) = if name == "linux-x86_64" {
                (
                    "je __rt_hash_set_invoker_reference_write_x",
                    "__rt_hash_set_invoker_reference_write_x:",
                    "mov QWORD PTR [r14], r13",
                    "call __rt_decref_mixed",
                )
            } else {
                (
                    "b.eq __rt_hash_set_invoker_reference_write",
                    "__rt_hash_set_invoker_reference_write:",
                    "str x13, [x14]",
                    "bl __rt_decref_mixed",
                )
            };
            assert!(update.contains(marker_branch), "{name}: missing nested marker detection");
            let write_through = update.split(write_label).nth(1).unwrap();
            assert!(write_through.contains("__rt_hash_to_mixed"), "{name}: replacement must be boxed for Mixed caller storage");
            assert!(
                write_through.find(publish).unwrap() < write_through.find(release).unwrap(),
                "{name}: publish replacement before releasing the previous Mixed owner"
            );
        }
    }
}

/// Writes the saved replacement `(lo, hi, tag)` into the selected entry (`x12`).
fn emit_write_entry_value_aarch64(emitter: &mut Emitter) {
    emitter.instruction("ldr x13, [sp, #24]");                                  // load value_lo
    emitter.instruction("str x13, [x12, #24]");                                 // update value_lo in entry
    emitter.instruction("ldr x13, [sp, #32]");                                  // load value_hi
    emitter.instruction("str x13, [x12, #32]");                                 // update value_hi in entry
    emitter.instruction("ldr x13, [sp, #40]");                                  // load value_tag
    emitter.instruction("str x13, [x12, #40]");                                 // update value_tag in entry
}

/// Writes the saved replacement `(lo, hi, tag)` into the selected entry (`r12`).
fn emit_write_entry_value_x86_64(emitter: &mut Emitter) {
    emitter.instruction("mov r13, QWORD PTR [rbp - 32]");                       // reload the replacement low payload word for the existing key slot
    emitter.instruction("mov QWORD PTR [r12 + 24], r13");                       // overwrite the stored low payload word in the existing hash entry
    emitter.instruction("mov r13, QWORD PTR [rbp - 40]");                       // reload the replacement high payload word for the existing key slot
    emitter.instruction("mov QWORD PTR [r12 + 32], r13");                       // overwrite the stored high payload word in the existing hash entry
    emitter.instruction("mov r13, QWORD PTR [rbp - 48]");                       // reload the replacement runtime value tag for the existing key slot
    emitter.instruction("mov QWORD PTR [r12 + 40], r13");                       // overwrite the stored runtime value tag in the existing hash entry
}
