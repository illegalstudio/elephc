//! Purpose:
//! Emits the container-level runtime helpers that back `array_pop()` / `array_shift()`
//! on a boxed Mixed / union receiver: `__rt_mixed_box_raw`, `__rt_indexed_pop`,
//! `__rt_hash_pop`, `__rt_indexed_shift`, and `__rt_hash_shift`. They remove-and-return
//! an element IN PLACE, dispatching on the runtime array kind (indexed vs associative
//! hash) so the caller's array is truly mutated (never a throwaway temp copy).
//!
//! The `*_shift` variants remove the FIRST element (rather than the last) and then reindex
//! integer keys to start from zero, matching PHP `array_shift()` — indexed arrays compact
//! their tail one slot toward the front; associative hashes are rebuilt into a fresh table
//! (modelled on `__rt_hash_grow`) so surviving integer keys are renumbered 0,1,2,… in
//! insertion order while string keys are preserved. Renumbering by rebuild (rather than an
//! in-place key rewrite) is mandatory because the open-addressed hash places each entry in
//! a bucket derived from its key, so a renumbered key must be re-bucketed to keep
//! `$a[$newKey]` lookups correct.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//! - The generated code from `crate::codegen::lower_inst::builtins::arrays::lower_array_pop`
//!   (dynamic Mixed path), which supplies the (already copy-on-write forced) container
//!   pointer and reboxes the returned container back into the by-ref slot.
//!
//! Key details:
//! - Ownership contract mirrors `__rt_array_unshift_grow`: the caller takes one extra
//!   `__rt_incref` on the unboxed container before calling, so the internal
//!   `__rt_*_ensure_unique` ALWAYS copy-on-write splits; the helper returns a uniquely
//!   owned container (refcount 1 = the caller's synthetic ownership) plus the removed
//!   element boxed as an owned Mixed cell.
//! - Indexed removal uses TRANSFER semantics: the removed slot's payload is adopted by
//!   the returned Mixed cell WITHOUT an extra retain, and the container's logical length
//!   is decremented so `__rt_array_free_deep` (which frees `[0, length)`) never touches
//!   it — leak-free and double-free-free, matching the `[0, length)` free bound.
//! - Hash removal uses INCREF semantics balanced by `__rt_hash_unset`: the tail value is
//!   boxed with a retain and `__rt_hash_unset` then releases the hash's own reference,
//!   so the net owner is the returned box while the entry is unlinked/tombstoned/counted.
//! - `__rt_hash_pop` rewinds the hash's next automatic index when it removes the key the last
//!   automatic append used, as php's `array_pop()` does: `$a[] = x` after the pop reuses it.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_mixed_box_raw`, a minimal "box without retain" helper.
///
/// Allocates a 24-byte boxed Mixed cell `[tag][lo][hi]` and stores the supplied
/// `(tag, lo, hi)` verbatim, taking NO ownership (no incref, no string persist). It is
/// used to adopt a payload whose ownership is being transferred out of an indexed array
/// slot, so the boxed cell becomes the sole owner without inflating the refcount.
///
/// Input:  `x0`/`rax` = value tag, `x1`/`rdi` = value lo word, `x2`/`rsi` = value hi word.
/// Output: `x0`/`rax` = pointer to the freshly allocated boxed Mixed cell.
pub fn emit_mixed_box_raw(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_box_raw_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed_box_raw ---");
    emitter.label_global("__rt_mixed_box_raw");
    emitter.instruction("sub sp, sp, #48");                                     // reserve the box-construction frame plus saved fp/lr
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the local frame
    emitter.instruction("str x0, [sp, #0]");                                    // save the runtime value tag across the allocation call
    emitter.instruction("stp x1, x2, [sp, #8]");                                // save the payload words that transfer into the boxed cell
    emitter.instruction("mov x0, #24");                                         // Mixed cells store a tag plus two payload words
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the boxed Mixed cell
    emitter.instruction("mov x9, #5");                                          // heap kind 5 = boxed Mixed cell
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the allocation as a Mixed cell in the uniform header
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the saved runtime value tag
    emitter.instruction("str x10, [x0]");                                       // store the runtime value tag in the Mixed cell
    emitter.instruction("ldp x11, x12, [sp, #8]");                              // reload the transferred payload words
    emitter.instruction("stp x11, x12, [x0, #8]");                              // store the payload words in the Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the box-construction frame
    emitter.instruction("ret");                                                 // return the boxed Mixed cell pointer
}

/// Emits the x86_64 Linux variant of `__rt_mixed_box_raw` (SysV AMD64 ABI).
///
/// Input:  `rax` = value tag, `rdi` = value lo word, `rsi` = value hi word.
/// Output: `rax` = pointer to the freshly allocated boxed Mixed cell.
fn emit_mixed_box_raw_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_box_raw ---");
    emitter.label_global("__rt_mixed_box_raw");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before allocating the box
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for tag and payload words
    emitter.instruction("sub rsp, 32");                                         // reserve helper slots for tag, payload lo, and payload hi
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the runtime value tag across the allocation call
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save the payload lo word across the allocation call
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save the payload hi word across the allocation call
    emitter.instruction("mov rax, 24");                                         // Mixed cells store a tag plus two payload words
    emitter.instruction("call __rt_heap_alloc");                                // allocate the boxed Mixed cell
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(5))); // materialize the x86_64 Mixed heap kind word
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the allocation as a boxed Mixed cell
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the runtime value tag
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store the runtime value tag in the Mixed cell
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the payload lo word
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store the payload lo word in the Mixed cell
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the payload hi word
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                       // store the payload hi word in the Mixed cell
    emitter.instruction("add rsp, 32");                                         // release the helper frame slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed Mixed cell pointer
}

/// Emits `__rt_indexed_pop`, which removes and returns the LAST element of an indexed
/// array in place.
///
/// Copy-on-write splits the (caller-force-increffed) container, boxes the last element
/// as an owned Mixed cell using TRANSFER semantics (no retain), and decrements the
/// logical length so the container's deep free never revisits the removed slot.
///
/// Input:  `x0`/`rdi` = indexed-array pointer (caller holds one synthetic `__rt_incref`).
/// Output: `x0`/`rax` = the unique (copy-on-write split) container pointer,
///         `x1`/`rdx` = the removed element boxed as an owned Mixed cell (boxed null when empty).
pub fn emit_indexed_pop(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_indexed_pop_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: indexed_pop ---");
    emitter.label_global("__rt_indexed_pop");
    // Frame: [sp,#0]=container C, [sp,#8]=removed cell, saved fp/lr at [sp,#32].
    emitter.instruction("sub sp, sp, #48");                                     // reserve the pop frame plus saved fp/lr
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the local frame
    emitter.instruction("bl __rt_array_ensure_unique");                         // copy-on-write split; x0 = uniquely owned container
    emitter.instruction("str x0, [sp, #0]");                                    // save the unique container pointer
    emitter.instruction("ldr x9, [x0]");                                        // x9 = current logical length
    emitter.instruction("cbz x9, __rt_indexed_pop_empty");                      // an empty array pops boxed null
    emitter.instruction("sub x9, x9, #1");                                      // x9 = index of the removed last element
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load the packed indexed-array kind word
    emitter.instruction("lsr x10, x10, #8");                                    // move the packed value_type tag into the low bits
    emitter.instruction("and x10, x10, #0x7f");                                 // isolate the value_type without the persistent copy-on-write flag
    emitter.instruction("add x11, x0, #24");                                    // x11 = payload base (skip the 24-byte header)
    emitter.instruction("cmp x10, #1");                                         // string slots use a 16-byte pointer/length pair
    emitter.instruction("b.eq __rt_indexed_pop_string");                       // handle string payloads separately
    emitter.instruction("cmp x10, #7");                                         // slots already holding boxed Mixed cells
    emitter.instruction("b.eq __rt_indexed_pop_boxed");                        // adopt the stored Mixed cell directly
    // -- generic single-word payload (int/float/bool/null/array/hash/object/callable) --
    emitter.instruction("ldr x1, [x11, x9, lsl #3]");                           // load the removed single-word payload
    emitter.instruction("mov x2, #0");                                          // single-word payloads leave the hi word clear
    emitter.instruction("mov x0, x10");                                         // box with the array's runtime value_type tag
    emitter.instruction("ldr x12, [sp, #0]");                                   // reload the container to shrink its length
    emitter.instruction("str x9, [x12]");                                       // commit the decremented length before boxing (TRANSFER: drop slot ownership)
    emitter.instruction("bl __rt_mixed_box_raw");                               // adopt the payload into an owned Mixed cell without retaining
    emitter.instruction("str x0, [sp, #8]");                                    // save the removed boxed value
    emitter.instruction("b __rt_indexed_pop_ret");                              // converge on the shared return path
    emitter.label("__rt_indexed_pop_string");
    emitter.instruction("lsl x12, x9, #4");                                     // scale the removed index by the 16-byte string slot size
    emitter.instruction("add x12, x11, x12");                                   // x12 = removed string slot address
    emitter.instruction("ldr x1, [x12]");                                       // x1 = removed string pointer
    emitter.instruction("ldr x2, [x12, #8]");                                   // x2 = removed string length
    emitter.instruction("mov x0, #1");                                          // runtime value tag 1 = string
    emitter.instruction("ldr x12, [sp, #0]");                                   // reload the container to shrink its length
    emitter.instruction("str x9, [x12]");                                       // commit the decremented length (TRANSFER the string to the box)
    emitter.instruction("bl __rt_mixed_box_raw");                              // adopt the string pointer without re-persisting it
    emitter.instruction("str x0, [sp, #8]");                                    // save the removed boxed value
    emitter.instruction("b __rt_indexed_pop_ret");                              // converge on the shared return path
    emitter.label("__rt_indexed_pop_boxed");
    emitter.instruction("ldr x0, [x11, x9, lsl #3]");                           // load the stored boxed Mixed cell pointer
    emitter.instruction("cbz x0, __rt_indexed_pop_boxed_null");                 // a null slot pops boxed null
    emitter.instruction("ldr x12, [sp, #0]");                                   // reload the container to shrink its length
    emitter.instruction("str x9, [x12]");                                       // commit the decremented length (TRANSFER the cell to the caller)
    emitter.instruction("str x0, [sp, #8]");                                    // the stored cell IS the removed owned value
    emitter.instruction("b __rt_indexed_pop_ret");                             // converge on the shared return path
    emitter.label("__rt_indexed_pop_boxed_null");
    emitter.instruction("ldr x12, [sp, #0]");                                   // reload the container to shrink its length
    emitter.instruction("str x9, [x12]");                                       // still remove the null slot from the logical length
    emitter.instruction("mov x0, #8");                                          // runtime value tag 8 = null
    emitter.instruction("mov x1, #0");                                          // null payload lo word
    emitter.instruction("mov x2, #0");                                          // null payload hi word
    emitter.instruction("bl __rt_mixed_box_raw");                              // box PHP null for the removed slot
    emitter.instruction("str x0, [sp, #8]");                                    // save the removed boxed value
    emitter.instruction("b __rt_indexed_pop_ret");                             // converge on the shared return path
    emitter.label("__rt_indexed_pop_empty");
    emitter.instruction("mov x0, #8");                                          // runtime value tag 8 = null
    emitter.instruction("mov x1, #0");                                          // null payload lo word
    emitter.instruction("mov x2, #0");                                          // null payload hi word
    emitter.instruction("bl __rt_mixed_box_raw");                              // box PHP null for an empty array_pop
    emitter.instruction("str x0, [sp, #8]");                                    // save the removed boxed value
    emitter.label("__rt_indexed_pop_ret");
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = removed boxed value
    emitter.instruction("ldr x0, [sp, #0]");                                    // x0 = unique container pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                    // release the pop frame
    emitter.instruction("ret");                                                // return (container, removed)
}

/// Emits the x86_64 Linux variant of `__rt_indexed_pop`.
///
/// Input:  `rdi` = indexed-array pointer (caller holds one synthetic `__rt_incref`).
/// Output: `rax` = unique container pointer, `rdx` = removed boxed Mixed cell.
fn emit_indexed_pop_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: indexed_pop ---");
    emitter.label_global("__rt_indexed_pop");
    // Frame: [rbp-8]=container C, [rbp-16]=removed cell.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 32");                                         // reserve slots for the container and removed value
    emitter.instruction("call __rt_array_ensure_unique");                       // copy-on-write split; rax = uniquely owned container (rdi already set)
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the unique container pointer
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // r9 = current logical length
    emitter.instruction("test r9, r9");                                         // an empty array pops boxed null
    emitter.instruction("jz __rt_indexed_pop_empty");                           // branch to the empty path
    emitter.instruction("sub r9, 1");                                           // r9 = index of the removed last element
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the packed indexed-array kind word
    emitter.instruction("shr r10, 8");                                          // move the packed value_type tag into the low bits
    emitter.instruction("and r10, 0x7f");                                       // isolate the value_type without the persistent copy-on-write flag
    emitter.instruction("lea r11, [rax + 24]");                                 // r11 = payload base (skip the 24-byte header)
    emitter.instruction("cmp r10, 1");                                          // string slots use a 16-byte pointer/length pair
    emitter.instruction("je __rt_indexed_pop_string");                          // handle string payloads separately
    emitter.instruction("cmp r10, 7");                                          // slots already holding boxed Mixed cells
    emitter.instruction("je __rt_indexed_pop_boxed");                           // adopt the stored Mixed cell directly
    // -- generic single-word payload --
    emitter.instruction("mov rdi, QWORD PTR [r11 + r9 * 8]");                   // load the removed single-word payload (box lo arg)
    emitter.instruction("xor rsi, rsi");                                        // single-word payloads leave the hi word clear
    emitter.instruction("mov rax, r10");                                        // box with the array's runtime value_type tag
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the container to shrink its length
    emitter.instruction("mov QWORD PTR [r8], r9");                              // commit the decremented length (TRANSFER: drop slot ownership)
    emitter.instruction("call __rt_mixed_box_raw");                             // adopt the payload into an owned Mixed cell without retaining
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the removed boxed value
    emitter.instruction("jmp __rt_indexed_pop_ret");                            // converge on the shared return path
    emitter.label("__rt_indexed_pop_string");
    emitter.instruction("mov rcx, r9");                                         // copy the removed index before scaling it
    emitter.instruction("shl rcx, 4");                                          // scale the removed index by the 16-byte string slot size
    emitter.instruction("lea rcx, [r11 + rcx]");                               // rcx = removed string slot address
    emitter.instruction("mov rdi, QWORD PTR [rcx]");                            // rdi = removed string pointer (box lo arg)
    emitter.instruction("mov rsi, QWORD PTR [rcx + 8]");                        // rsi = removed string length (box hi arg)
    emitter.instruction("mov rax, 1");                                          // runtime value tag 1 = string
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the container to shrink its length
    emitter.instruction("mov QWORD PTR [r8], r9");                              // commit the decremented length (TRANSFER the string to the box)
    emitter.instruction("call __rt_mixed_box_raw");                             // adopt the string pointer without re-persisting it
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the removed boxed value
    emitter.instruction("jmp __rt_indexed_pop_ret");                            // converge on the shared return path
    emitter.label("__rt_indexed_pop_boxed");
    emitter.instruction("mov rax, QWORD PTR [r11 + r9 * 8]");                   // load the stored boxed Mixed cell pointer
    emitter.instruction("test rax, rax");                                       // a null slot pops boxed null
    emitter.instruction("jz __rt_indexed_pop_boxed_null");                      // branch to the null path
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the container to shrink its length
    emitter.instruction("mov QWORD PTR [r8], r9");                              // commit the decremented length (TRANSFER the cell to the caller)
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // the stored cell IS the removed owned value
    emitter.instruction("jmp __rt_indexed_pop_ret");                            // converge on the shared return path
    emitter.label("__rt_indexed_pop_boxed_null");
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the container to shrink its length
    emitter.instruction("mov QWORD PTR [r8], r9");                              // still remove the null slot from the logical length
    emitter.instruction("mov rax, 8");                                          // runtime value tag 8 = null
    emitter.instruction("xor edi, edi");                                        // null payload lo word
    emitter.instruction("xor esi, esi");                                        // null payload hi word
    emitter.instruction("call __rt_mixed_box_raw");                             // box PHP null for the removed slot
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the removed boxed value
    emitter.instruction("jmp __rt_indexed_pop_ret");                            // converge on the shared return path
    emitter.label("__rt_indexed_pop_empty");
    emitter.instruction("mov rax, 8");                                          // runtime value tag 8 = null
    emitter.instruction("xor edi, edi");                                        // null payload lo word
    emitter.instruction("xor esi, esi");                                        // null payload hi word
    emitter.instruction("call __rt_mixed_box_raw");                             // box PHP null for an empty array_pop
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the removed boxed value
    emitter.label("__rt_indexed_pop_ret");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // rdx = removed boxed value
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // rax = unique container pointer
    emitter.instruction("add rsp, 32");                                         // release the pop frame slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return (container, removed)
}

/// Emits `__rt_hash_pop`, which removes and returns the insertion-order TAIL entry of an
/// associative hash in place.
///
/// Copy-on-write splits the (caller-force-increffed) hash, boxes the tail value with a
/// retain, then delegates removal to `__rt_hash_unset` (which releases the hash's own
/// reference to the value, frees the key, unlinks the insertion-order chain, tombstones
/// the slot, and decrements the count) — so the returned box is the net owner.
///
/// Input:  `x0`/`rdi` = hash pointer (caller holds one synthetic `__rt_incref`).
/// Output: `x0`/`rax` = unique (copy-on-write split) hash pointer,
///         `x1`/`rdx` = removed value boxed as an owned Mixed cell (boxed null when empty).
pub fn emit_hash_pop(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_pop_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_pop ---");
    emitter.label_global("__rt_hash_pop");
    // Frame: [sp,#0]=hash C, [sp,#8]=key_lo, [sp,#16]=key_hi, [sp,#24]=removed, saved fp/lr at [sp,#32].
    emitter.instruction("sub sp, sp, #48");                                     // reserve the pop frame plus saved fp/lr
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the local frame
    emitter.instruction("bl __rt_hash_ensure_unique");                          // copy-on-write split; x0 = uniquely owned hash
    emitter.instruction("str x0, [sp, #0]");                                    // save the unique hash pointer
    emitter.instruction("ldr x9, [x0, #32]");                                   // x9 = insertion-order tail slot index
    emitter.instruction("cmn x9, #1");                                          // is the hash empty (tail == -1)?
    emitter.instruction("b.eq __rt_hash_pop_empty");                            // an empty hash pops boxed null
    super::hash_layout::emit_entry_address(emitter, "x11", "x0", "x9");
    emitter.instruction("ldr x12, [x11, #8]");                                  // x12 = tail key lo word
    emitter.instruction("ldr x13, [x11, #16]");                                 // x13 = tail key hi word (-1 marks an integer key)
    emitter.instruction("str x12, [sp, #8]");                                   // save the tail key lo word across helper calls
    emitter.instruction("str x13, [sp, #16]");                                  // save the tail key hi word across helper calls
    emitter.instruction("ldr x3, [x11, #24]");                                  // x3 = tail value lo word
    emitter.instruction("ldr x4, [x11, #32]");                                  // x4 = tail value hi word
    emitter.instruction("ldr x5, [x11, #40]");                                  // x5 = tail value runtime tag
    emitter.instruction("cmp x5, #7");                                          // is the entry already a boxed Mixed cell?
    emitter.instruction("b.eq __rt_hash_pop_retain_stored_box");                            // retain the stored Mixed cell rather than reboxing
    emitter.instruction("mov x0, x5");                                          // mixed_from_value tag argument
    emitter.instruction("mov x1, x3");                                          // mixed_from_value payload lo argument
    emitter.instruction("mov x2, x4");                                          // mixed_from_value payload hi argument
    emitter.instruction("bl __rt_mixed_from_value");                            // box the typed tail value with a retain
    emitter.instruction("b __rt_hash_pop_have_removed");                        // continue to the unset step
    emitter.label("__rt_hash_pop_retain_stored_box");
    emitter.instruction("mov x0, x3");                                          // move the stored Mixed cell into the retain register
    emitter.instruction("bl __rt_incref");                                      // retain it so the returned box owns a reference
    emitter.label("__rt_hash_pop_have_removed");
    emitter.instruction("str x0, [sp, #24]");                                   // save the removed boxed value
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the hash pointer for the unset
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the tail key lo word
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the tail key hi word
    emitter.instruction("bl __rt_hash_unset");                                  // release the hash's own value reference, unlink, tombstone, count--
    emitter.instruction("str x0, [sp, #0]");                                    // persist the (unchanged, already-unique) hash pointer
    emitter.instruction("ldr x12, [sp, #8]");                                   // reload the popped key lo word
    emitter.instruction("ldr x13, [sp, #16]");                                  // reload the popped key hi word
    super::hash_next_index::rewind_after_pop(emitter, "x0", "x12", "x13", "__rt_hash_pop");
    emitter.instruction("b __rt_hash_pop_ret");                                 // converge on the shared return path
    emitter.label("__rt_hash_pop_empty");
    emitter.instruction("mov x0, #8");                                          // runtime value tag 8 = null
    emitter.instruction("mov x1, #0");                                          // null payload lo word
    emitter.instruction("mov x2, #0");                                          // null payload hi word
    emitter.instruction("bl __rt_mixed_box_raw");                              // box PHP null for an empty array_pop
    emitter.instruction("str x0, [sp, #24]");                                   // save the removed boxed value
    emitter.label("__rt_hash_pop_ret");
    emitter.instruction("ldr x1, [sp, #24]");                                   // x1 = removed boxed value
    emitter.instruction("ldr x0, [sp, #0]");                                    // x0 = unique hash pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                    // release the pop frame
    emitter.instruction("ret");                                                // return (hash, removed)
}

/// Emits the x86_64 Linux variant of `__rt_hash_pop`.
///
/// Input:  `rdi` = hash pointer (caller holds one synthetic `__rt_incref`).
/// Output: `rax` = unique hash pointer, `rdx` = removed boxed Mixed cell.
fn emit_hash_pop_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_pop ---");
    emitter.label_global("__rt_hash_pop");
    // Frame: [rbp-8]=hash C, [rbp-16]=key_lo, [rbp-24]=key_hi, [rbp-32]=removed.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 32");                                         // reserve slots for hash/key/removed state
    emitter.instruction("call __rt_hash_ensure_unique");                        // copy-on-write split; rax = uniquely owned hash (rdi already set)
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the unique hash pointer
    emitter.instruction("mov r9, QWORD PTR [rax + 32]");                        // r9 = insertion-order tail slot index
    emitter.instruction("cmp r9, -1");                                          // is the hash empty (tail == -1)?
    emitter.instruction("je __rt_hash_pop_empty");                              // an empty hash pops boxed null
    super::hash_layout::emit_entry_address(emitter, "r10", "rax", "r9");
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // r11 = tail key lo word
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // save the tail key lo word across helper calls
    emitter.instruction("mov r11, QWORD PTR [r10 + 16]");                       // r11 = tail key hi word (-1 marks an integer key)
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // save the tail key hi word across helper calls
    emitter.instruction("mov rdi, QWORD PTR [r10 + 24]");                       // rdi = tail value lo word (mixed_from_value lo arg)
    emitter.instruction("mov rsi, QWORD PTR [r10 + 32]");                       // rsi = tail value hi word (mixed_from_value hi arg)
    emitter.instruction("mov r11, QWORD PTR [r10 + 40]");                       // r11 = tail value runtime tag
    emitter.instruction("cmp r11, 7");                                          // is the entry already a boxed Mixed cell?
    emitter.instruction("je __rt_hash_pop_retain_stored_box");                              // retain the stored Mixed cell rather than reboxing
    emitter.instruction("mov rax, r11");                                        // mixed_from_value tag argument
    emitter.instruction("call __rt_mixed_from_value");                          // box the typed tail value with a retain
    emitter.instruction("jmp __rt_hash_pop_have_removed");                      // continue to the unset step
    emitter.label("__rt_hash_pop_retain_stored_box");
    emitter.instruction("mov rax, rdi");                                        // move the stored Mixed cell into the retain register
    emitter.instruction("call __rt_incref");                                    // retain it so the returned box owns a reference
    emitter.label("__rt_hash_pop_have_removed");
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the removed boxed value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the hash pointer for the unset
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the tail key lo word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the tail key hi word
    emitter.instruction("call __rt_hash_unset");                               // release the hash's own value reference, unlink, tombstone, count--
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // persist the (unchanged, already-unique) hash pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the popped key lo word
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the popped key hi word
    super::hash_next_index::rewind_after_pop(emitter, "rax", "r10", "r9", "__rt_hash_pop");
    emitter.instruction("jmp __rt_hash_pop_ret");                               // converge on the shared return path
    emitter.label("__rt_hash_pop_empty");
    emitter.instruction("mov rax, 8");                                          // runtime value tag 8 = null
    emitter.instruction("xor edi, edi");                                        // null payload lo word
    emitter.instruction("xor esi, esi");                                        // null payload hi word
    emitter.instruction("call __rt_mixed_box_raw");                             // box PHP null for an empty array_pop
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the removed boxed value
    emitter.label("__rt_hash_pop_ret");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // rdx = removed boxed value
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // rax = unique hash pointer
    emitter.instruction("add rsp, 32");                                         // release the pop frame slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return (hash, removed)
}

/// Emits `__rt_indexed_shift`, which removes and returns the FIRST element of an indexed
/// array in place, then compacts the tail one slot toward the front so integer keys stay
/// `0,1,2,…` (PHP `array_shift()` reindexing).
///
/// Copy-on-write splits the (caller-force-increffed) container, boxes the first element as
/// an owned Mixed cell using TRANSFER semantics (no retain), slides every trailing slot one
/// position toward the front, and decrements the logical length so the container's deep free
/// never revisits the now-duplicated last slot.
///
/// Input:  `x0`/`rdi` = indexed-array pointer (caller holds one synthetic `__rt_incref`).
/// Output: `x0`/`rax` = the unique (copy-on-write split) container pointer,
///         `x1`/`rdx` = the removed element boxed as an owned Mixed cell (boxed null when empty).
pub fn emit_indexed_shift(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_indexed_shift_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: indexed_shift ---");
    emitter.label_global("__rt_indexed_shift");
    // Frame: [sp,#0]=container C, [sp,#8]=removed cell, saved fp/lr at [sp,#32].
    emitter.instruction("sub sp, sp, #48");                                     // reserve the shift frame plus saved fp/lr
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the local frame
    emitter.instruction("bl __rt_array_ensure_unique");                         // copy-on-write split; x0 = uniquely owned container
    emitter.instruction("str x0, [sp, #0]");                                    // save the unique container pointer
    emitter.instruction("ldr x9, [x0]");                                        // x9 = current logical length
    emitter.instruction("cbz x9, __rt_indexed_shift_empty");                    // an empty array shifts boxed null
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load the packed indexed-array kind word
    emitter.instruction("lsr x10, x10, #8");                                    // move the packed value_type tag into the low bits
    emitter.instruction("and x10, x10, #0x7f");                                 // isolate the value_type without the persistent copy-on-write flag
    emitter.instruction("add x11, x0, #24");                                    // x11 = payload base (skip the 24-byte header)
    emitter.instruction("cmp x10, #1");                                         // string slots use a 16-byte pointer/length pair
    emitter.instruction("b.eq __rt_indexed_shift_string");                      // handle string payloads separately
    emitter.instruction("cmp x10, #7");                                         // slots already holding boxed Mixed cells
    emitter.instruction("b.eq __rt_indexed_shift_boxed");                       // adopt the stored Mixed cell directly
    // -- generic single-word payload (int/float/bool/null/array/hash/object/callable) --
    emitter.instruction("ldr x1, [x11]");                                       // load the removed first single-word payload
    emitter.instruction("mov x2, #0");                                          // single-word payloads leave the hi word clear
    emitter.instruction("mov x0, x10");                                         // box with the array's runtime value_type tag
    emitter.instruction("bl __rt_mixed_box_raw");                              // adopt the payload into an owned Mixed cell without retaining
    emitter.instruction("str x0, [sp, #8]");                                    // save the removed boxed value
    emitter.instruction("b __rt_indexed_shift_compact");                        // slide the tail toward the front
    emitter.label("__rt_indexed_shift_string");
    emitter.instruction("ldr x1, [x11]");                                       // x1 = removed first string pointer
    emitter.instruction("ldr x2, [x11, #8]");                                   // x2 = removed first string length
    emitter.instruction("mov x0, #1");                                          // runtime value tag 1 = string
    emitter.instruction("bl __rt_mixed_box_raw");                              // adopt the string pointer without re-persisting it
    emitter.instruction("str x0, [sp, #8]");                                    // save the removed boxed value
    emitter.instruction("b __rt_indexed_shift_compact");                        // slide the tail toward the front
    emitter.label("__rt_indexed_shift_boxed");
    emitter.instruction("ldr x0, [x11]");                                       // load the stored boxed Mixed cell pointer
    emitter.instruction("cbz x0, __rt_indexed_shift_boxed_null");               // a null slot shifts boxed null
    emitter.instruction("str x0, [sp, #8]");                                    // the stored cell IS the removed owned value
    emitter.instruction("b __rt_indexed_shift_compact");                        // slide the tail toward the front
    emitter.label("__rt_indexed_shift_boxed_null");
    emitter.instruction("mov x0, #8");                                          // runtime value tag 8 = null
    emitter.instruction("mov x1, #0");                                          // null payload lo word
    emitter.instruction("mov x2, #0");                                          // null payload hi word
    emitter.instruction("bl __rt_mixed_box_raw");                              // box PHP null for the removed slot
    emitter.instruction("str x0, [sp, #8]");                                    // save the removed boxed value
    emitter.label("__rt_indexed_shift_compact");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the container for the compaction loop
    emitter.instruction("ldr x9, [x0]");                                        // reload the original logical length
    emitter.instruction("ldr x10, [x0, #-8]");                                  // reload the packed indexed-array kind word
    emitter.instruction("lsr x10, x10, #8");                                    // move the packed value_type tag into the low bits
    emitter.instruction("and x10, x10, #0x7f");                                 // isolate the value_type
    emitter.instruction("add x11, x0, #24");                                    // x11 = payload base
    emitter.instruction("cmp x10, #1");                                         // string slots slide 16 bytes at a time
    emitter.instruction("b.eq __rt_indexed_shift_c16");                         // use the string compaction loop
    emitter.instruction("mov x13, #1");                                         // start the source cursor at the second live element
    emitter.label("__rt_indexed_shift_c8");
    emitter.instruction("cmp x13, x9");                                         // compare the source cursor with the original length
    emitter.instruction("b.ge __rt_indexed_shift_compact_done");               // finish once every trailing element has moved
    emitter.instruction("ldr x14, [x11, x13, lsl #3]");                         // load the trailing single-word payload
    emitter.instruction("sub x15, x13, #1");                                    // compute the previous slot index
    emitter.instruction("str x14, [x11, x15, lsl #3]");                         // store the trailing payload one slot toward the front
    emitter.instruction("add x13, x13, #1");                                    // advance to the next trailing source slot
    emitter.instruction("b __rt_indexed_shift_c8");                             // continue compacting
    emitter.label("__rt_indexed_shift_c16");
    emitter.instruction("mov x13, #1");                                         // start the source cursor at the second live element
    emitter.label("__rt_indexed_shift_c16_loop");
    emitter.instruction("cmp x13, x9");                                         // compare the source cursor with the original length
    emitter.instruction("b.ge __rt_indexed_shift_compact_done");               // finish once every trailing element has moved
    emitter.instruction("lsl x14, x13, #4");                                    // scale the source cursor by the 16-byte string slot size
    emitter.instruction("add x15, x11, x14");                                   // compute the source string slot address
    emitter.instruction("sub x14, x14, #16");                                   // convert the source byte offset into the destination offset
    emitter.instruction("add x14, x11, x14");                                   // compute the destination string slot address
    emitter.instruction("ldp x16, x17, [x15]");                                 // load the trailing string payload
    emitter.instruction("stp x16, x17, [x14]");                                 // store the trailing string payload one slot toward the front
    emitter.instruction("add x13, x13, #1");                                    // advance to the next trailing source slot
    emitter.instruction("b __rt_indexed_shift_c16_loop");                       // continue compacting
    emitter.label("__rt_indexed_shift_compact_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the container to shrink its length
    emitter.instruction("ldr x9, [x0]");                                        // reload the original length
    emitter.instruction("sub x9, x9, #1");                                      // one fewer element after removing the first
    emitter.instruction("str x9, [x0]");                                        // persist the shortened length (drops the duplicated tail slot)
    emitter.instruction("b __rt_indexed_shift_ret");                            // converge on the shared return path
    emitter.label("__rt_indexed_shift_empty");
    emitter.instruction("mov x0, #8");                                          // runtime value tag 8 = null
    emitter.instruction("mov x1, #0");                                          // null payload lo word
    emitter.instruction("mov x2, #0");                                          // null payload hi word
    emitter.instruction("bl __rt_mixed_box_raw");                              // box PHP null for an empty array_shift
    emitter.instruction("str x0, [sp, #8]");                                    // save the removed boxed value
    emitter.label("__rt_indexed_shift_ret");
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = removed boxed value
    emitter.instruction("ldr x0, [sp, #0]");                                    // x0 = unique container pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                    // release the shift frame
    emitter.instruction("ret");                                                // return (container, removed)
}

/// Emits the x86_64 Linux variant of `__rt_indexed_shift`.
///
/// Input:  `rdi` = indexed-array pointer (caller holds one synthetic `__rt_incref`).
/// Output: `rax` = unique container pointer, `rdx` = removed boxed Mixed cell.
fn emit_indexed_shift_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: indexed_shift ---");
    emitter.label_global("__rt_indexed_shift");
    // Frame: [rbp-8]=container C, [rbp-16]=removed cell.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 32");                                         // reserve slots for the container and removed value
    emitter.instruction("call __rt_array_ensure_unique");                       // copy-on-write split; rax = uniquely owned container (rdi already set)
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the unique container pointer
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // r9 = current logical length
    emitter.instruction("test r9, r9");                                         // an empty array shifts boxed null
    emitter.instruction("jz __rt_indexed_shift_empty");                         // branch to the empty path
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the packed indexed-array kind word
    emitter.instruction("shr r10, 8");                                          // move the packed value_type tag into the low bits
    emitter.instruction("and r10, 0x7f");                                       // isolate the value_type without the persistent copy-on-write flag
    emitter.instruction("lea r11, [rax + 24]");                                 // r11 = payload base (skip the 24-byte header)
    emitter.instruction("cmp r10, 1");                                          // string slots use a 16-byte pointer/length pair
    emitter.instruction("je __rt_indexed_shift_string");                        // handle string payloads separately
    emitter.instruction("cmp r10, 7");                                          // slots already holding boxed Mixed cells
    emitter.instruction("je __rt_indexed_shift_boxed");                         // adopt the stored Mixed cell directly
    // -- generic single-word payload --
    emitter.instruction("mov rdi, QWORD PTR [r11]");                            // load the removed first single-word payload (box lo arg)
    emitter.instruction("xor rsi, rsi");                                        // single-word payloads leave the hi word clear
    emitter.instruction("mov rax, r10");                                        // box with the array's runtime value_type tag
    emitter.instruction("call __rt_mixed_box_raw");                             // adopt the payload into an owned Mixed cell without retaining
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the removed boxed value
    emitter.instruction("jmp __rt_indexed_shift_compact");                      // slide the tail toward the front
    emitter.label("__rt_indexed_shift_string");
    emitter.instruction("mov rdi, QWORD PTR [r11]");                            // rdi = removed first string pointer (box lo arg)
    emitter.instruction("mov rsi, QWORD PTR [r11 + 8]");                        // rsi = removed first string length (box hi arg)
    emitter.instruction("mov rax, 1");                                          // runtime value tag 1 = string
    emitter.instruction("call __rt_mixed_box_raw");                             // adopt the string pointer without re-persisting it
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the removed boxed value
    emitter.instruction("jmp __rt_indexed_shift_compact");                      // slide the tail toward the front
    emitter.label("__rt_indexed_shift_boxed");
    emitter.instruction("mov rax, QWORD PTR [r11]");                            // load the stored boxed Mixed cell pointer
    emitter.instruction("test rax, rax");                                       // a null slot shifts boxed null
    emitter.instruction("jz __rt_indexed_shift_boxed_null");                    // branch to the null path
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // the stored cell IS the removed owned value
    emitter.instruction("jmp __rt_indexed_shift_compact");                      // slide the tail toward the front
    emitter.label("__rt_indexed_shift_boxed_null");
    emitter.instruction("mov rax, 8");                                          // runtime value tag 8 = null
    emitter.instruction("xor edi, edi");                                        // null payload lo word
    emitter.instruction("xor esi, esi");                                        // null payload hi word
    emitter.instruction("call __rt_mixed_box_raw");                             // box PHP null for the removed slot
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the removed boxed value
    emitter.label("__rt_indexed_shift_compact");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the container for the compaction loop
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // reload the original logical length
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // reload the packed indexed-array kind word
    emitter.instruction("shr r10, 8");                                          // move the packed value_type tag into the low bits
    emitter.instruction("and r10, 0x7f");                                       // isolate the value_type
    emitter.instruction("lea r11, [rax + 24]");                                 // r11 = payload base
    emitter.instruction("cmp r10, 1");                                          // string slots slide 16 bytes at a time
    emitter.instruction("je __rt_indexed_shift_c16");                           // use the string compaction loop
    emitter.instruction("mov rcx, 1");                                          // start the source cursor at the second live element
    emitter.label("__rt_indexed_shift_c8");
    emitter.instruction("cmp rcx, r9");                                         // compare the source cursor with the original length
    emitter.instruction("jge __rt_indexed_shift_compact_done");                 // finish once every trailing element has moved
    emitter.instruction("mov rdx, QWORD PTR [r11 + rcx * 8]");                  // load the trailing single-word payload
    emitter.instruction("mov QWORD PTR [r11 + rcx * 8 - 8], rdx");              // store the trailing payload one slot toward the front
    emitter.instruction("add rcx, 1");                                          // advance to the next trailing source slot
    emitter.instruction("jmp __rt_indexed_shift_c8");                           // continue compacting
    emitter.label("__rt_indexed_shift_c16");
    emitter.instruction("mov rcx, 1");                                          // start the source cursor at the second live element
    emitter.label("__rt_indexed_shift_c16_loop");
    emitter.instruction("cmp rcx, r9");                                         // compare the source cursor with the original length
    emitter.instruction("jge __rt_indexed_shift_compact_done");                 // finish once every trailing element has moved
    emitter.instruction("mov rdx, rcx");                                        // copy the string source cursor before scaling it
    emitter.instruction("shl rdx, 4");                                          // scale the source cursor by the 16-byte string slot size
    emitter.instruction("lea rdi, [r11 + rdx]");                                // compute the source string slot address
    emitter.instruction("sub rdx, 16");                                         // convert the source byte offset into the destination offset
    emitter.instruction("lea rsi, [r11 + rdx]");                                // compute the destination string slot address
    emitter.instruction("mov rdx, QWORD PTR [rdi]");                            // load the trailing string pointer
    emitter.instruction("mov QWORD PTR [rsi], rdx");                            // store the trailing string pointer one slot toward the front
    emitter.instruction("mov rdx, QWORD PTR [rdi + 8]");                        // load the trailing string length
    emitter.instruction("mov QWORD PTR [rsi + 8], rdx");                        // store the trailing string length one slot toward the front
    emitter.instruction("add rcx, 1");                                          // advance to the next trailing source slot
    emitter.instruction("jmp __rt_indexed_shift_c16_loop");                     // continue compacting
    emitter.label("__rt_indexed_shift_compact_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the container to shrink its length
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // reload the original length
    emitter.instruction("sub r9, 1");                                           // one fewer element after removing the first
    emitter.instruction("mov QWORD PTR [rax], r9");                             // persist the shortened length (drops the duplicated tail slot)
    emitter.instruction("jmp __rt_indexed_shift_ret");                          // converge on the shared return path
    emitter.label("__rt_indexed_shift_empty");
    emitter.instruction("mov rax, 8");                                          // runtime value tag 8 = null
    emitter.instruction("xor edi, edi");                                        // null payload lo word
    emitter.instruction("xor esi, esi");                                        // null payload hi word
    emitter.instruction("call __rt_mixed_box_raw");                             // box PHP null for an empty array_shift
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the removed boxed value
    emitter.label("__rt_indexed_shift_ret");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // rdx = removed boxed value
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // rax = unique container pointer
    emitter.instruction("add rsp, 32");                                         // release the shift frame slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                // return (container, removed)
}

/// Emits `__rt_hash_shift`, which removes and returns the insertion-order HEAD entry of an
/// associative hash and rebuilds the survivors into a fresh table so integer keys are
/// renumbered `0,1,2,…` while string keys are preserved (PHP `array_shift()` on a hash).
///
/// Modelled on `__rt_hash_grow`: copy-on-write splits the (caller-force-increffed) hash,
/// boxes the head value with TRANSFER semantics (adopting a stored Mixed cell directly, or
/// `__rt_mixed_box_raw` for a typed payload), frees the head's owned string key, allocates a
/// fresh destination table, walks the remaining entries in insertion order moving each one
/// via `__rt_hash_insert_owned` (renumbering integer keys, keeping string keys), then
/// shallow-frees the old table block. Ownership transfers exactly like a grow: values/keys
/// move into the destination without any extra retain, so nothing is leaked or double-freed.
///
/// Input:  `x0`/`rdi` = hash pointer (caller holds one synthetic `__rt_incref`).
/// Output: `x0`/`rax` = the rebuilt (or, when empty, unchanged) hash pointer,
///         `x1`/`rdx` = removed value boxed as an owned Mixed cell (boxed null when empty).
pub fn emit_hash_shift(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_shift_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_shift ---");
    emitter.label_global("__rt_hash_shift");
    // Frame: [sp,#0]=old table H, [sp,#8]=new table H2, [sp,#16]=removed cell,
    //        [sp,#24]=iterator cursor, [sp,#32]=next integer key,
    //        [sp,#40]=head value_lo, [sp,#48]=head value_hi, [sp,#56]=head value_tag,
    //        saved fp/lr at [sp,#64].
    emitter.instruction("sub sp, sp, #80");                                     // reserve the rebuild frame plus saved fp/lr
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish the local frame
    emitter.instruction("bl __rt_hash_ensure_unique");                          // copy-on-write split; x0 = uniquely owned hash
    emitter.instruction("str x0, [sp, #0]");                                    // save the unique old-table pointer
    emitter.instruction("ldr x9, [x0, #24]");                                   // x9 = insertion-order head slot index
    emitter.instruction("cmn x9, #1");                                          // is the hash empty (head == -1)?
    emitter.instruction("b.eq __rt_hash_shift_empty");                          // an empty hash shifts boxed null
    // -- allocate the destination table (same capacity and value_type) --
    emitter.instruction("ldr x1, [x0, #16]");                                   // x1 = table-wide runtime value_type
    emitter.instruction("ldr x0, [x0, #8]");                                    // x0 = old capacity (destination capacity)
    emitter.instruction("bl __rt_hash_new");                                    // allocate the destination hash table
    emitter.instruction("str x0, [sp, #8]");                                    // save the destination table pointer
    emitter.instruction("str xzr, [sp, #32]");                                  // next integer key starts at zero
    // -- consume the head entry (removed value + free its owned string key) --
    emitter.instruction("ldr x0, [sp, #0]");                                    // old table pointer
    emitter.instruction("mov x1, #0");                                          // cursor 0 starts a fresh insertion-order walk at the head
    emitter.instruction("bl __rt_hash_iter_next");                              // x0=next cursor, x1=key_ptr, x2=key_len, x3=val_lo, x4=val_hi, x5=val_tag
    emitter.instruction("str x0, [sp, #24]");                                   // save the cursor for the survivor walk
    emitter.instruction("str x3, [sp, #40]");                                   // save the head value low word
    emitter.instruction("str x4, [sp, #48]");                                   // save the head value high word
    emitter.instruction("str x5, [sp, #56]");                                   // save the head value runtime tag
    emitter.instruction("cmn x2, #1");                                          // is the head key an inline integer key (key_len == -1)?
    emitter.instruction("b.eq __rt_hash_shift_head_val");                       // integer keys own no heap storage to free
    emitter.instruction("mov x0, x1");                                          // move the persisted head string-key pointer into the free register
    emitter.instruction("ldr w15, [x0, #-12]");                                 // load the key refcount from the heap header
    emitter.instruction("subs w15, w15, #1");                                   // drop the old table's ownership of the head key
    emitter.instruction("str w15, [x0, #-12]");                                 // store the decremented key refcount
    emitter.instruction("b.ne __rt_hash_shift_head_val");                       // key still shared: keep it alive
    emitter.instruction("bl __rt_heap_free");                                   // last owner: free the persisted head-key storage
    emitter.label("__rt_hash_shift_head_val");
    emitter.instruction("ldr x5, [sp, #56]");                                   // reload the head value runtime tag
    emitter.instruction("cmp x5, #7");                                          // is the head value already a boxed Mixed cell?
    emitter.instruction("b.eq __rt_hash_shift_head_boxed");                     // adopt the stored Mixed cell directly
    emitter.instruction("ldr x0, [sp, #56]");                                   // box_raw tag argument (the head value tag)
    emitter.instruction("ldr x1, [sp, #40]");                                   // box_raw payload low word
    emitter.instruction("ldr x2, [sp, #48]");                                   // box_raw payload high word
    emitter.instruction("bl __rt_mixed_box_raw");                              // adopt the typed head payload without retaining (TRANSFER)
    emitter.instruction("str x0, [sp, #16]");                                   // save the removed boxed value
    emitter.instruction("b __rt_hash_shift_loop");                              // continue to the survivor walk
    emitter.label("__rt_hash_shift_head_boxed");
    emitter.instruction("ldr x0, [sp, #40]");                                   // the stored Mixed cell IS the removed owned value
    emitter.instruction("str x0, [sp, #16]");                                   // save the removed boxed value
    // -- walk the surviving entries in insertion order, renumbering integer keys --
    emitter.label("__rt_hash_shift_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // old table pointer
    emitter.instruction("ldr x1, [sp, #24]");                                   // current insertion-order cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // x0=next cursor, x1=key_ptr, x2=key_len, x3=val_lo, x4=val_hi, x5=val_tag
    emitter.instruction("cmn x0, #1");                                          // did the iterator signal end-of-walk?
    emitter.instruction("b.eq __rt_hash_shift_free");                          // finish once every survivor has moved
    emitter.instruction("str x0, [sp, #24]");                                   // save the next insertion-order cursor
    emitter.instruction("cmn x2, #1");                                          // is this survivor's key an inline integer key?
    emitter.instruction("b.ne __rt_hash_shift_ins");                           // string keys keep their pointer/length unchanged
    emitter.instruction("ldr x1, [sp, #32]");                                   // load the next integer key to assign
    emitter.instruction("add x9, x1, #1");                                      // pre-compute the following integer key
    emitter.instruction("str x9, [sp, #32]");                                   // persist the incremented integer-key counter
    emitter.instruction("mov x2, #-1");                                         // mark the renumbered key as an integer key
    emitter.label("__rt_hash_shift_ins");
    emitter.instruction("ldr x0, [sp, #8]");                                    // destination table pointer
    emitter.instruction("bl __rt_hash_insert_owned");                          // move the (possibly renumbered) owned entry into the destination
    emitter.instruction("str x0, [sp, #8]");                                    // persist the destination pointer (may change on defensive grow)
    emitter.instruction("b __rt_hash_shift_loop");                             // continue the survivor walk
    // -- free the old table block (all keys/values already transferred) --
    emitter.label("__rt_hash_shift_free");
    emitter.instruction("ldr x0, [sp, #0]");                                    // old table pointer
    emitter.instruction("ldr x0, [x0, #40]");                                   // the header owns its entry storage as a separate allocation
    emitter.instruction("bl __rt_heap_free");                                   // release the old entry storage once every owner has moved
    emitter.instruction("ldr x0, [sp, #0]");                                    // old table pointer
    emitter.instruction("bl __rt_heap_free");                                   // shallow-free the old table header
    emitter.instruction("ldr x0, [sp, #8]");                                    // x0 = rebuilt destination table
    emitter.instruction("ldr x1, [sp, #16]");                                   // x1 = removed boxed value
    emitter.instruction("ldp x29, x30, [sp, #64]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                    // release the rebuild frame
    emitter.instruction("ret");                                                // return (rebuilt hash, removed)
    // -- empty hash: return it unchanged with a boxed null --
    emitter.label("__rt_hash_shift_empty");
    emitter.instruction("mov x0, #8");                                          // runtime value tag 8 = null
    emitter.instruction("mov x1, #0");                                          // null payload lo word
    emitter.instruction("mov x2, #0");                                          // null payload hi word
    emitter.instruction("bl __rt_mixed_box_raw");                              // box PHP null for an empty array_shift
    emitter.instruction("str x0, [sp, #16]");                                   // save the removed boxed value
    emitter.instruction("ldr x0, [sp, #0]");                                    // x0 = the unchanged hash pointer
    emitter.instruction("ldr x1, [sp, #16]");                                   // x1 = removed boxed value
    emitter.instruction("ldp x29, x30, [sp, #64]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                    // release the rebuild frame
    emitter.instruction("ret");                                                // return (hash, removed)
}

/// Emits the x86_64 Linux variant of `__rt_hash_shift`.
///
/// Input:  `rdi` = hash pointer (caller holds one synthetic `__rt_incref`).
/// Output: `rax` = rebuilt (or unchanged) hash pointer, `rdx` = removed boxed Mixed cell.
fn emit_hash_shift_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_shift ---");
    emitter.label_global("__rt_hash_shift");
    // Frame: [rbp-8]=old table H, [rbp-16]=new table H2, [rbp-24]=removed cell,
    //        [rbp-32]=iterator cursor, [rbp-40]=next integer key,
    //        [rbp-48]=head value_lo, [rbp-56]=head value_hi, [rbp-64]=head value_tag.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 64");                                         // reserve the rebuild spill slots
    emitter.instruction("call __rt_hash_ensure_unique");                        // copy-on-write split; rax = uniquely owned hash (rdi already set)
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the unique old-table pointer
    emitter.instruction("mov r10, QWORD PTR [rax + 24]");                       // r10 = insertion-order head slot index
    emitter.instruction("cmp r10, -1");                                         // is the hash empty (head == -1)?
    emitter.instruction("je __rt_hash_shift_empty");                            // an empty hash shifts boxed null
    // -- allocate the destination table (same capacity and value_type) --
    emitter.instruction("mov rsi, QWORD PTR [rax + 16]");                       // rsi = table-wide runtime value_type
    emitter.instruction("mov rdi, QWORD PTR [rax + 8]");                        // rdi = old capacity (destination capacity)
    emitter.instruction("call __rt_hash_new");                                  // allocate the destination hash table
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the destination table pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // next integer key starts at zero
    // -- consume the head entry (removed value + free its owned string key) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // old table pointer
    emitter.instruction("xor esi, esi");                                        // cursor 0 starts a fresh insertion-order walk at the head
    emitter.instruction("call __rt_hash_iter_next");                            // rax=next cursor, rdi=key_ptr, rdx=key_len, rcx=val_lo, r8=val_hi, r9=val_tag
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the cursor for the survivor walk
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // save the head value low word
    emitter.instruction("mov QWORD PTR [rbp - 56], r8");                        // save the head value high word
    emitter.instruction("mov QWORD PTR [rbp - 64], r9");                        // save the head value runtime tag
    emitter.instruction("cmp rdx, -1");                                         // is the head key an inline integer key (key_len == -1)?
    emitter.instruction("je __rt_hash_shift_head_val");                         // integer keys own no heap storage to free
    emitter.instruction("mov rax, rdi");                                        // move the persisted head string-key pointer into the free register
    emitter.instruction("test rax, rax");                                       // defensively skip a missing key pointer
    emitter.instruction("jz __rt_hash_shift_head_val");                         // nothing to release
    emitter.instruction("mov r9, QWORD PTR [rax - 8]");                         // load the key heap kind word
    emitter.instruction("shr r9, 32");                                          // isolate the high-word heap marker
    emitter.instruction(&format!("cmp r9d, 0x{:x}", crate::codegen_support::sentinels::X86_64_HEAP_MAGIC_HI32)); // elephc-owned key?
    emitter.instruction("jne __rt_hash_shift_head_val");                        // skip foreign key pointers
    emitter.instruction("mov r9d, DWORD PTR [rax - 12]");                       // load the key refcount from the header
    emitter.instruction("sub r9d, 1");                                          // drop the old table's ownership of the head key
    emitter.instruction("mov DWORD PTR [rax - 12], r9d");                       // store the decremented key refcount
    emitter.instruction("jnz __rt_hash_shift_head_val");                        // key still shared: keep it alive
    emitter.instruction("call __rt_heap_free");                                 // last owner: free the persisted head key (rax = ptr)
    emitter.label("__rt_hash_shift_head_val");
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload the head value runtime tag
    emitter.instruction("cmp r9, 7");                                           // is the head value already a boxed Mixed cell?
    emitter.instruction("je __rt_hash_shift_head_boxed");                       // adopt the stored Mixed cell directly
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // box_raw tag argument (the head value tag)
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // box_raw payload low word
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // box_raw payload high word
    emitter.instruction("call __rt_mixed_box_raw");                             // adopt the typed head payload without retaining (TRANSFER)
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the removed boxed value
    emitter.instruction("jmp __rt_hash_shift_loop");                            // continue to the survivor walk
    emitter.label("__rt_hash_shift_head_boxed");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // the stored Mixed cell IS the removed owned value
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the removed boxed value
    // -- walk the surviving entries in insertion order, renumbering integer keys --
    emitter.label("__rt_hash_shift_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // old table pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // current insertion-order cursor
    emitter.instruction("call __rt_hash_iter_next");                            // rax=next cursor, rdi=key_ptr, rdx=key_len, rcx=val_lo, r8=val_hi, r9=val_tag
    emitter.instruction("cmp rax, -1");                                         // did the iterator signal end-of-walk?
    emitter.instruction("je __rt_hash_shift_free");                            // finish once every survivor has moved
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the next insertion-order cursor
    emitter.instruction("cmp rdx, -1");                                         // is this survivor's key an inline integer key?
    emitter.instruction("jne __rt_hash_shift_ins");                            // string keys keep their pointer/length unchanged
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // load the next integer key to assign
    emitter.instruction("mov r11, rdi");                                        // copy it before advancing the counter
    emitter.instruction("add r11, 1");                                          // pre-compute the following integer key
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // persist the incremented integer-key counter
    emitter.instruction("mov rdx, -1");                                         // mark the renumbered key as an integer key
    emitter.label("__rt_hash_shift_ins");
    emitter.instruction("mov rsi, rdi");                                        // move the key pointer/integer into the insert key register
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // destination table pointer
    emitter.instruction("call __rt_hash_insert_owned");                        // move the (possibly renumbered) owned entry into the destination
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // persist the destination pointer (may change on defensive grow)
    emitter.instruction("jmp __rt_hash_shift_loop");                           // continue the survivor walk
    // -- free the old table block (all keys/values already transferred) --
    emitter.label("__rt_hash_shift_free");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // old table pointer
    emitter.instruction("mov rax, QWORD PTR [rax + 40]");                       // the header owns its entry storage as a separate allocation
    emitter.instruction("call __rt_heap_free");                                 // release the old entry storage once every owner has moved
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // old table pointer
    emitter.instruction("call __rt_heap_free");                                 // shallow-free the old table header
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // rax = rebuilt destination table
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // rdx = removed boxed value
    emitter.instruction("add rsp, 64");                                         // release the rebuild spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                // return (rebuilt hash, removed)
    // -- empty hash: return it unchanged with a boxed null --
    emitter.label("__rt_hash_shift_empty");
    emitter.instruction("mov rax, 8");                                          // runtime value tag 8 = null
    emitter.instruction("xor edi, edi");                                        // null payload lo word
    emitter.instruction("xor esi, esi");                                        // null payload hi word
    emitter.instruction("call __rt_mixed_box_raw");                             // box PHP null for an empty array_shift
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the removed boxed value
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // rax = the unchanged hash pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // rdx = removed boxed value
    emitter.instruction("add rsp, 64");                                         // release the rebuild spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                // return (hash, removed)
}
