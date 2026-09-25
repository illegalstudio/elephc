//! Purpose:
//! Emits the hash-entry reference-cell helpers that back PHP reference sets stored in
//! associative-array entries: `__rt_hash_entry_make_reference`, `__rt_hash_entry_deref`,
//! `__rt_hash_iter_next_value` and `__rt_hash_iter_resync`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - A hash entry that joins a PHP reference set stores runtime value tag 11 with `value_lo`
//!   pointing at a managed reference cell (`REFERENCE_CELL_HEAP_KIND`) whose single payload word owns exactly
//!   one boxed Mixed value. `value_hi` is cleared; no interior table address ever escapes.
//! - `__rt_hash_entry_deref` is the borrowed value view of that layout and never allocates.
//! - `__rt_hash_iter_resync` re-derives an insertion-order cursor after the table was
//!   reallocated by growth or copy-on-write, using the last yielded key as the anchor.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Runtime value tag marking a slot whose payload word is a managed reference cell.
const REFERENCE_CELL_VALUE_TAG: i64 = 11;

/// Payload descriptor stored in a hash-entry reference cell; 7 selects a boxed Mixed value.
const REFERENCE_CELL_PAYLOAD_TAG: i64 = 7;

/// Key high word marking "this walk has no successor entry to resume from".
///
/// A live key uses -1 for an integer key and a non-negative length for a string key, so -2 can
/// never collide with one. `IterNext` stores it whenever the cursor it just produced is the
/// post-last or done sentinel.
pub(super) const NO_SUCCESSOR_KEY_MARKER: i64 = -2;

/// Emits every hash-entry reference helper for the current target.
pub fn emit_hash_entry_reference(emitter: &mut Emitter) {
    emit_make_reference(emitter);
    emit_entry_deref(emitter);
    emit_iter_next_value(emitter);
    emit_iter_resync(emitter);
}

/// Emits `__rt_hash_entry_make_reference`, which promotes a boxed Mixed hash entry into a
/// managed reference cell and returns that cell.
///
/// `__rt_hash_to_mixed` widens every entry present when the walk starts, so `value_lo` normally
/// owns a boxed Mixed cell already. An entry INSERTED after that point carries its own concrete
/// runtime tag — `$a[] = 3` inside `foreach ($a as &$v)` writes a plain integer — so the payload
/// is widened here first, taking over the entry's ownership without a retain. Skipping that step
/// let a raw scalar be published as a reference-cell payload and crash the first read of `$v`.
/// A shallow hash clone retains, rather than copies, each boxed Mixed entry, so the same zval can
/// back two arrays' buckets after `$b = $a`. A reference set must own its zval exclusively — PHP
/// separates on the way into a reference — so a shared cell is copied here before it is wrapped.
/// Without that, `foreach ($a[0] as &$v)` republished the promoted container through `$b`'s bucket
/// too.
///
/// Ownership of the box then moves into the new reference cell and the cell itself becomes the
/// entry payload, so the net refcount change is zero. The helper is idempotent: an entry that
/// already carries tag 11 returns its existing cell untouched, which is what keeps a repeated
/// by-reference foreach from restamping a live reference entry.
///
/// Input: argument 0 = address of `entry.value_lo` (the by-reference foreach value address).
/// Output: integer result register = managed reference-cell pointer.
fn emit_make_reference(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_entry_make_reference ---");
    emitter.label_global("__rt_hash_entry_make_reference");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [x0, #16]");                           // read the entry runtime value tag before wrapping the payload
            emitter.instruction(&format!("cmp x9, #{REFERENCE_CELL_VALUE_TAG}")); // is this entry already part of a PHP reference set?
            emitter.instruction("b.eq __rt_hash_entry_make_reference_existing"); // reuse the live cell instead of restamping the entry
            emitter.instruction(&format!("cmp x9, #{REFERENCE_CELL_PAYLOAD_TAG}")); // does the entry already own a boxed Mixed payload?
            emitter.instruction("b.eq __rt_hash_entry_make_reference_boxed");   // entries widened at iteration start need no conversion
            emitter.instruction("stp x29, x30, [sp, #-32]!");                   // save frame pointer and return address across the widening
            emitter.instruction("mov x29, sp");                                 // establish the widening frame
            emitter.instruction("str x0, [sp, #16]");                           // save the mutable entry value address across the allocation
            emitter.instruction("ldr x1, [x0]");                                // take the concrete low payload word out of the entry
            emitter.instruction("ldr x2, [x0, #8]");                            // take the concrete high payload word out of the entry
            emitter.instruction("mov x0, x9");                                  // pass the entry runtime value tag to the owned-box helper
            emitter.instruction("bl __rt_hash_to_mixed_box_owned");             // box the payload without adding a retain
            emitter.instruction("ldr x9, [sp, #16]");                           // reload the mutable entry value address
            emitter.instruction("str x0, [x9]");                                // publish the boxed Mixed pointer in value_lo
            emitter.instruction("str xzr, [x9, #8]");                           // boxed Mixed entries carry no high payload word
            emitter.instruction(&format!("mov x10, #{REFERENCE_CELL_PAYLOAD_TAG}")); // runtime value tag 7 = boxed Mixed
            emitter.instruction("str x10, [x9, #16]");                          // stamp the widened entry as boxed Mixed
            emitter.instruction("mov x0, x9");                                  // restore the entry value address as the promotion argument
            emitter.instruction("ldp x29, x30, [sp], #32");                     // restore frame pointer and return address
            emitter.label("__rt_hash_entry_make_reference_boxed");
            emitter.instruction("ldr x9, [x0]");                                // load the boxed Mixed cell this entry owns
            emitter.instruction("cbz x9, __rt_hash_entry_make_reference_separated"); // an absent payload has nothing to separate
            emitter.instruction("ldr w10, [x9, #-12]");                         // read the cell refcount from the uniform heap header
            emitter.instruction("cmp w10, #1");                                 // is this zval shared with another array's bucket?
            emitter.instruction("b.ls __rt_hash_entry_make_reference_separated"); // a sole owner can join the reference set in place
            emitter.instruction("stp x29, x30, [sp, #-32]!");                   // save frame pointer and return address across the separation
            emitter.instruction("mov x29, sp");                                 // establish the separation frame
            emitter.instruction("str x0, [sp, #16]");                           // save the mutable entry value address across the copy
            emitter.instruction("ldr x2, [x9, #16]");                           // copy the shared cell high payload word
            emitter.instruction("ldr x1, [x9, #8]");                            // copy the shared cell low payload word
            emitter.instruction("ldr x0, [x9]");                                // copy the shared cell runtime value tag
            emitter.instruction("bl __rt_mixed_from_value");                    // allocate this entry a private zval that retains the payload
            emitter.instruction("ldr x9, [sp, #16]");                           // reload the mutable entry value address
            emitter.instruction("ldr x10, [x9]");                               // reload the shared cell this entry is giving up
            emitter.instruction("str x0, [x9]");                                // publish the private copy as the entry payload
            emitter.instruction("mov x0, x10");                                 // release this entry's share of the old cell
            emitter.instruction("bl __rt_decref_mixed");                        // the other bucket keeps the shared cell alive
            emitter.instruction("ldr x0, [sp, #16]");                           // restore the entry value address as the promotion argument
            emitter.instruction("ldp x29, x30, [sp], #32");                     // restore frame pointer and return address
            emitter.label("__rt_hash_entry_make_reference_separated");
            emitter.instruction("stp x29, x30, [sp, #-32]!");                   // save frame pointer and return address across the allocation
            emitter.instruction("mov x29, sp");                                 // establish the promotion frame
            emitter.instruction("str x0, [sp, #16]");                           // save the mutable entry value address across the allocation
            emitter.instruction(&format!("mov x0, #{REFERENCE_CELL_PAYLOAD_TAG}")); // payload descriptor 7 = boxed Mixed
            emitter.instruction("bl __rt_reference_cell_new");                  // allocate the managed reference cell
            emitter.instruction("ldr x9, [sp, #16]");                           // reload the mutable entry value address
            emitter.instruction("ldr x10, [x9]");                               // take the owned boxed Mixed pointer out of value_lo
            emitter.instruction("str x10, [x0]");                               // move that ownership into the cell payload word
            emitter.instruction("str xzr, [x0, #8]");                           // reference cells keep their second word cleared
            emitter.instruction("str x0, [x9]");                                // publish the cell as the entry payload
            emitter.instruction("str xzr, [x9, #8]");                           // reference entries carry no high payload word
            emitter.instruction(&format!("mov x10, #{REFERENCE_CELL_VALUE_TAG}")); // runtime value tag 11 = managed reference cell
            emitter.instruction("str x10, [x9, #16]");                          // stamp the entry as a PHP reference set member
            emitter.instruction("ldp x29, x30, [sp], #32");                     // restore frame pointer and return address
            emitter.instruction("ret");                                         // return the managed reference cell to the caller
            emitter.label("__rt_hash_entry_make_reference_existing");
            emitter.instruction("ldr x0, [x0]");                                // return the reference cell the entry already owns
            emitter.instruction("ret");                                         // idempotent promotion keeps the existing reference set intact
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp QWORD PTR [rdi + 16], {REFERENCE_CELL_VALUE_TAG}")); // is this entry already part of a PHP reference set?
            emitter.instruction("je __rt_hash_entry_make_reference_existing");  // reuse the live cell instead of restamping the entry
            emitter.instruction(&format!("cmp QWORD PTR [rdi + 16], {REFERENCE_CELL_PAYLOAD_TAG}")); // does the entry already own a boxed Mixed payload?
            emitter.instruction("je __rt_hash_entry_make_reference_boxed");     // entries widened at iteration start need no conversion
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer across the widening
            emitter.instruction("mov rbp, rsp");                                // establish the widening frame
            emitter.instruction("sub rsp, 16");                                 // reserve one aligned spill slot for the entry value address
            emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                // save the mutable entry value address across the allocation
            emitter.instruction("mov rax, QWORD PTR [rdi + 16]");               // pass the entry runtime value tag to the owned-box helper
            emitter.instruction("mov rsi, QWORD PTR [rdi + 8]");                // take the concrete high payload word out of the entry
            emitter.instruction("mov rdi, QWORD PTR [rdi]");                    // take the concrete low payload word out of the entry
            emitter.instruction("call __rt_hash_to_mixed_x86_box_owned");       // box the payload without adding a retain
            emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                // reload the mutable entry value address
            emitter.instruction("mov QWORD PTR [r10], rax");                    // publish the boxed Mixed pointer in value_lo
            emitter.instruction("mov QWORD PTR [r10 + 8], 0");                  // boxed Mixed entries carry no high payload word
            emitter.instruction(&format!("mov QWORD PTR [r10 + 16], {REFERENCE_CELL_PAYLOAD_TAG}")); // stamp the widened entry as boxed Mixed
            emitter.instruction("mov rdi, r10");                                // restore the entry value address as the promotion argument
            emitter.instruction("add rsp, 16");                                 // release the widening spill slot
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.label("__rt_hash_entry_make_reference_boxed");
            emitter.instruction("mov r10, QWORD PTR [rdi]");                    // load the boxed Mixed cell this entry owns
            emitter.instruction("test r10, r10");                               // an absent payload has nothing to separate
            emitter.instruction("jz __rt_hash_entry_make_reference_separated"); // fall through to the ordinary promotion
            emitter.instruction("mov r11d, DWORD PTR [r10 - 12]");              // read the cell refcount from the uniform heap header
            emitter.instruction("cmp r11d, 1");                                 // is this zval shared with another array's bucket?
            emitter.instruction("jbe __rt_hash_entry_make_reference_separated"); // a sole owner can join the reference set in place
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer across the separation
            emitter.instruction("mov rbp, rsp");                                // establish the separation frame
            emitter.instruction("sub rsp, 16");                                 // reserve one aligned spill slot for the entry value address
            emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                // save the mutable entry value address across the copy
            emitter.instruction("mov rsi, QWORD PTR [r10 + 16]");               // copy the shared cell high payload word
            emitter.instruction("mov rdi, QWORD PTR [r10 + 8]");                // copy the shared cell low payload word
            emitter.instruction("mov rax, QWORD PTR [r10]");                    // copy the shared cell runtime value tag
            emitter.instruction("call __rt_mixed_from_value");                  // allocate this entry a private zval that retains the payload
            emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                // reload the mutable entry value address
            emitter.instruction("mov r11, QWORD PTR [r10]");                    // reload the shared cell this entry is giving up
            emitter.instruction("mov QWORD PTR [r10], rax");                    // publish the private copy as the entry payload
            emitter.instruction("mov rax, r11");                                // release this entry's share of the old cell
            emitter.instruction("call __rt_decref_mixed");                      // the other bucket keeps the shared cell alive
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                // restore the entry value address as the promotion argument
            emitter.instruction("add rsp, 16");                                 // release the separation spill slot
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.label("__rt_hash_entry_make_reference_separated");
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer across the allocation
            emitter.instruction("mov rbp, rsp");                                // establish the promotion frame
            emitter.instruction("sub rsp, 16");                                 // reserve one aligned spill slot for the entry value address
            emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                // save the mutable entry value address across the allocation
            emitter.instruction(&format!("mov rdi, {REFERENCE_CELL_PAYLOAD_TAG}")); // payload descriptor 7 = boxed Mixed
            emitter.instruction("call __rt_reference_cell_new");                // allocate the managed reference cell
            emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                // reload the mutable entry value address
            emitter.instruction("mov r11, QWORD PTR [r10]");                    // take the owned boxed Mixed pointer out of value_lo
            emitter.instruction("mov QWORD PTR [rax], r11");                    // move that ownership into the cell payload word
            emitter.instruction("mov QWORD PTR [rax + 8], 0");                  // reference cells keep their second word cleared
            emitter.instruction("mov QWORD PTR [r10], rax");                    // publish the cell as the entry payload
            emitter.instruction("mov QWORD PTR [r10 + 8], 0");                  // reference entries carry no high payload word
            emitter.instruction(&format!("mov QWORD PTR [r10 + 16], {REFERENCE_CELL_VALUE_TAG}")); // stamp the entry as a PHP reference set member
            emitter.instruction("add rsp, 16");                                 // release the promotion spill slot
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.instruction("ret");                                         // return the managed reference cell to the caller
            emitter.label("__rt_hash_entry_make_reference_existing");
            emitter.instruction("mov rax, QWORD PTR [rdi]");                    // return the reference cell the entry already owns
            emitter.instruction("ret");                                         // idempotent promotion keeps the existing reference set intact
        }
    }
}

/// Emits `__rt_hash_entry_deref`, the allocation-free borrowed value view of a hash payload.
///
/// Operates in place on the payload registers of the `__rt_hash_iter_next` result tuple so it
/// can be chained directly behind that helper: `x3`/`x4`/`x5` on AArch64 and `rcx`/`r8`/`r9` on
/// x86_64. A tag-11 payload is replaced by the boxed Mixed value the reference cell owns, which
/// is tag 7; every other tag is returned verbatim. The cursor, key and entry-address results are
/// untouched, and no refcount changes because the result is a borrow.
fn emit_entry_deref(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_entry_deref ---");
    emitter.label_global("__rt_hash_entry_deref");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp x5, #{REFERENCE_CELL_VALUE_TAG}")); // does this payload hold a managed reference cell?
            emitter.instruction("b.ne __rt_hash_entry_deref_done");             // ordinary payloads are already value shaped
            emitter.instruction("ldr x3, [x3]");                                // read the boxed Mixed value the reference cell owns
            emitter.instruction("mov x4, #0");                                  // boxed Mixed payloads carry no high word
            emitter.instruction(&format!("mov x5, #{REFERENCE_CELL_PAYLOAD_TAG}")); // report the dereferenced payload as boxed Mixed
            emitter.label("__rt_hash_entry_deref_done");
            emitter.instruction("ret");                                         // return the borrowed value view to the caller
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp r9, {REFERENCE_CELL_VALUE_TAG}")); // does this payload hold a managed reference cell?
            emitter.instruction("jne __rt_hash_entry_deref_done");              // ordinary payloads are already value shaped
            emitter.instruction("mov rcx, QWORD PTR [rcx]");                    // read the boxed Mixed value the reference cell owns
            emitter.instruction("xor r8d, r8d");                                // boxed Mixed payloads carry no high word
            emitter.instruction(&format!("mov r9, {REFERENCE_CELL_PAYLOAD_TAG}")); // report the dereferenced payload as boxed Mixed
            emitter.label("__rt_hash_entry_deref_done");
            emitter.instruction("ret");                                         // return the borrowed value view to the caller
        }
    }
}

/// Emits an inline borrowed dereference of one hash-entry payload triple.
///
/// For the direct readers that load `value_tag`, `value_lo` and `value_hi` straight out of an
/// entry instead of going through the iterator. A tag-11 payload is replaced by the boxed Mixed
/// value its reference cell owns, so the reader retains, boxes or compares a VALUE rather than
/// the cell itself. `array_pop` and `array_shift` depend on this: without it the removed element
/// would carry the cell away, and reading it after the source array is destroyed would dangle.
///
/// Emitted inline rather than as a call because several of these sites sit in tail position or
/// keep other registers live, so a `bl`/`call` would clobber the link register or the ABI. Only
/// the three named registers and the condition flags are touched. `done_label` must be unique
/// within the emitted program.
pub(super) fn emit_inline_entry_deref(
    emitter: &mut Emitter,
    done_label: &str,
    tag: &str,
    lo: &str,
    hi: &str,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {tag}, #{REFERENCE_CELL_VALUE_TAG}")); // does this entry belong to a PHP reference set?
            emitter.instruction(&format!("b.ne {done_label}"));                 // ordinary entries already carry a value payload
            emitter.instruction(&format!("ldr {lo}, [{lo}]"));                  // read the boxed Mixed value the reference cell owns
            emitter.instruction(&format!("mov {hi}, #0"));                      // boxed Mixed payloads carry no high word
            emitter.instruction(&format!("mov {tag}, #{REFERENCE_CELL_PAYLOAD_TAG}")); // report the dereferenced payload as boxed Mixed
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {tag}, {REFERENCE_CELL_VALUE_TAG}")); // does this entry belong to a PHP reference set?
            emitter.instruction(&format!("jne {done_label}"));                  // ordinary entries already carry a value payload
            emitter.instruction(&format!("mov {lo}, QWORD PTR [{lo}]"));        // read the boxed Mixed value the reference cell owns
            emitter.instruction(&format!("xor {hi}, {hi}"));                    // boxed Mixed payloads carry no high word
            emitter.instruction(&format!("mov {tag}, {REFERENCE_CELL_PAYLOAD_TAG}")); // report the dereferenced payload as boxed Mixed
        }
    }
    emitter.label(done_label);
}

/// Emits `__rt_hash_iter_next_value`, the dereferencing sibling of `__rt_hash_iter_next`.
///
/// Same inputs, same result tuple and same cursor protocol, except that a tag-11 entry yields
/// the referenced value instead of the reference cell. Value walkers (output, serialization,
/// comparison, set operations) must use this entry point. Ownership-moving walkers such as
/// `__rt_hash_grow`, `__rt_hash_clone_shallow` and `__rt_hash_to_mixed` must keep using the raw
/// `__rt_hash_iter_next` so the cell itself is what they relocate or retain.
fn emit_iter_next_value(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_iter_next_value ---");
    emitter.label_global("__rt_hash_iter_next_value");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x29, x30, [sp, #-16]!");                   // save frame pointer and return address across the raw walk
            emitter.instruction("mov x29, sp");                                 // establish the dereferencing wrapper frame
            emitter.instruction("bl __rt_hash_iter_next");                      // advance the raw insertion-order walk
            emitter.instruction("bl __rt_hash_entry_deref");                    // replace a reference-cell payload with the value it owns
            emitter.instruction("ldp x29, x30, [sp], #16");                     // restore frame pointer and return address
            emitter.instruction("ret");                                         // return the dereferenced entry tuple to the caller
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer across the raw walk
            emitter.instruction("mov rbp, rsp");                                // establish the dereferencing wrapper frame
            emitter.instruction("call __rt_hash_iter_next");                    // advance the raw insertion-order walk
            emitter.instruction("call __rt_hash_entry_deref");                  // replace a reference-cell payload with the value it owns
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.instruction("ret");                                         // return the dereferenced entry tuple to the caller
        }
    }
}

/// Emits `__rt_hash_iter_resync`, which rebuilds an insertion-order cursor after the table
/// backing a live iteration was replaced by growth or copy-on-write.
///
/// The primary anchor is the SUCCESSOR key, the key of the entry the cursor was about to yield,
/// not the key that was last yielded. A second anchor names the following entry. If the loop body
/// deletes the immediate successor before relocating the table, the fallback lets iteration skip
/// that removed entry and continue in insertion order.
///
/// Rehashing permutes slot indices, so the resumed cursor is derived from where the anchor
/// actually landed in the live entry storage: `(entry - entries) / entry_size + 1`, which is the
/// same "slot index plus one" encoding `__rt_hash_iter_next` returns.
///
/// Input: argument 0 = live table pointer, argument 1 = successor key low word,
/// argument 2 = successor key high word, arguments 3 and 4 = fallback key low/high words.
/// A high word of -1 marks an integer key and -2 marks an absent anchor.
/// Output: integer result register = resumed cursor, `-1` when there is nothing to resume from.
fn emit_iter_resync(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_iter_resync ---");
    emitter.label_global("__rt_hash_iter_resync");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp x2, #{NO_SUCCESSOR_KEY_MARKER}")); // did the walk already pass its last entry?
            emitter.instruction("b.eq __rt_hash_iter_resync_done");             // there is no successor to resume from
            emitter.instruction("stp x29, x30, [sp, #-48]!");                   // save frame pointer and reserve aligned anchor spills
            emitter.instruction("mov x29, sp");                                 // establish the resync frame
            emitter.instruction("str x0, [sp, #16]");                           // save the live table base for the slot computation
            emitter.instruction("str x3, [sp, #24]");                           // save the fallback key low word across the primary probe
            emitter.instruction("str x4, [sp, #32]");                           // save the fallback key high word across the primary probe
            emitter.instruction("bl __rt_hash_get");                            // probe the live table for the primary successor key
            emitter.instruction("cbnz x0, __rt_hash_iter_resync_found");        // a surviving primary anchor is the next entry to yield
            emitter.instruction("ldr x2, [sp, #32]");                           // reload the fallback key high word
            emitter.instruction(&format!("cmp x2, #{NO_SUCCESSOR_KEY_MARKER}")); // was there an entry after the primary anchor?
            emitter.instruction("b.eq __rt_hash_iter_resync_missing");          // both anchors vanished, so the walk is safely complete
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the live table for the fallback probe
            emitter.instruction("ldr x1, [sp, #24]");                           // reload the fallback key low word
            emitter.instruction("bl __rt_hash_get");                            // probe the entry after the deleted primary anchor
            emitter.instruction("cbz x0, __rt_hash_iter_resync_missing");       // neither owned anchor survived in the live table
            emitter.label("__rt_hash_iter_resync_found");
            emitter.instruction("ldr x9, [sp, #16]");                           // reload the live table base
            emitter.instruction("ldr x9, [x9, #40]");                           // locate the separately allocated entry storage
            emitter.instruction("sub x0, x4, x9");                              // byte offset of the anchor inside entry storage
            emitter.instruction("lsr x0, x0, #6");                              // 64 bytes per entry gives the slot index
            emitter.instruction("add x0, x0, #1");                              // encode the resumed cursor as slot index plus one
            emitter.instruction("ldp x29, x30, [sp], #48");                     // restore frame pointer and release anchor spills
            emitter.instruction("ret");                                         // return the resumed cursor to the iterator
            emitter.label("__rt_hash_iter_resync_missing");
            emitter.instruction("mov x0, #-1");                                 // neither anchor survived, so stop without reading stale storage
            emitter.instruction("ldp x29, x30, [sp], #48");                     // restore frame pointer and release anchor spills
            emitter.instruction("ret");                                         // return the done cursor to the iterator
            emitter.label("__rt_hash_iter_resync_done");
            emitter.instruction("mov x0, #-1");                                 // a walk with no successor is already finished
            emitter.instruction("ret");                                         // return the done cursor without touching the stack
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp rdx, {NO_SUCCESSOR_KEY_MARKER}")); // did the walk already pass its last entry?
            emitter.instruction("je __rt_hash_iter_resync_done");               // there is no successor to resume from
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer across the probe
            emitter.instruction("mov rbp, rsp");                                // establish the resync frame
            emitter.instruction("sub rsp, 32");                                 // reserve aligned table and fallback-key spills
            emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                // save the live table base for the slot computation
            emitter.instruction("mov QWORD PTR [rbp - 16], rcx");               // save the fallback key low word across the primary probe
            emitter.instruction("mov QWORD PTR [rbp - 24], r8");                // save the fallback key high word across the primary probe
            emitter.instruction("call __rt_hash_get");                          // probe the live table for the primary successor key
            emitter.instruction("test rax, rax");                               // did the live table still contain the anchor key?
            emitter.instruction("jnz __rt_hash_iter_resync_found");             // a surviving primary anchor is the next entry to yield
            emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");               // reload the fallback key high word
            emitter.instruction(&format!("cmp rdx, {NO_SUCCESSOR_KEY_MARKER}")); // was there an entry after the primary anchor?
            emitter.instruction("je __rt_hash_iter_resync_missing");            // both anchors vanished, so the walk is safely complete
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                // reload the live table for the fallback probe
            emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");               // reload the fallback key low word
            emitter.instruction("call __rt_hash_get");                          // probe the entry after the deleted primary anchor
            emitter.instruction("test rax, rax");                               // did the fallback survive in the live table?
            emitter.instruction("jz __rt_hash_iter_resync_missing");            // neither owned anchor survived in the live table
            emitter.label("__rt_hash_iter_resync_found");
            emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                // reload the live table base
            emitter.instruction("mov rcx, QWORD PTR [rcx + 40]");               // locate the separately allocated entry storage
            emitter.instruction("mov rax, r8");                                 // the probe returned the matching entry address
            emitter.instruction("sub rax, rcx");                                // byte offset of the anchor inside entry storage
            emitter.instruction("shr rax, 6");                                  // 64 bytes per entry gives the slot index
            emitter.instruction("add rax, 1");                                  // encode the resumed cursor as slot index plus one
            emitter.instruction("add rsp, 32");                                 // release table and fallback-key spills
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.instruction("ret");                                         // return the resumed cursor to the iterator
            emitter.label("__rt_hash_iter_resync_missing");
            emitter.instruction("mov rax, -1");                                 // neither anchor survived, so stop without reading stale storage
            emitter.instruction("add rsp, 32");                                 // release table and fallback-key spills
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.instruction("ret");                                         // return the done cursor to the iterator
            emitter.label("__rt_hash_iter_resync_done");
            emitter.instruction("mov rax, -1");                                 // a walk with no successor is already finished
            emitter.instruction("ret");                                         // return the done cursor without touching the stack
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::emit::Emitter;
    use crate::codegen_support::platform::Target;

    /// Every supported target name, so a helper cannot regress to a single-architecture port.
    const SUPPORTED_TARGETS: [&str; 5] = [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ];

    /// Emits the hash-entry reference helpers for one target and returns the assembly text.
    fn emit_for(name: &str) -> String {
        let target = Target::parse(name).unwrap();
        let mut emitter = Emitter::new(target);
        emit_hash_entry_reference(&mut emitter);
        emitter.output()
    }

    /// Pins that every supported target defines all managed entry-reference symbols.
    #[test]
    fn hash_entry_reference_defines_every_symbol_on_every_target() {
        for name in SUPPORTED_TARGETS {
            let asm = emit_for(name);
            for symbol in [
                "__rt_hash_entry_make_reference",
                "__rt_hash_entry_deref",
                "__rt_hash_iter_next_value",
                "__rt_hash_iter_resync",
            ] {
                assert!(asm.contains(symbol), "{name}: {symbol} missing");
            }
        }
    }

    /// Pins that promotion allocates a managed reference cell and stamps runtime value tag 11.
    #[test]
    fn make_reference_allocates_a_managed_cell_and_stamps_tag_eleven() {
        for name in SUPPORTED_TARGETS {
            let asm = emit_for(name);
            assert!(asm.contains("__rt_reference_cell_new"), "{name}");
            assert!(
                asm.contains(&format!("{REFERENCE_CELL_VALUE_TAG}")),
                "{name}: reference value tag missing"
            );
            assert!(
                asm.contains("__rt_hash_entry_make_reference_existing"),
                "{name}: promotion is not idempotent"
            );
        }
    }

    /// Pins that the dereferencing iterator layers the raw walk and the borrowed value view.
    #[test]
    fn hash_iter_next_value_chains_the_raw_walk_and_the_deref_helper() {
        for name in SUPPORTED_TARGETS {
            let asm = emit_for(name);
            let body = asm
                .split_once("__rt_hash_iter_next_value:")
                .unwrap()
                .1
                .split_once("__rt_hash_iter_resync")
                .unwrap()
                .0;
            assert!(body.contains("__rt_hash_iter_next"), "{name}");
            assert!(body.contains("__rt_hash_entry_deref"), "{name}");
        }
    }

    /// Pins that a resync reports done for a missing anchor and for an absent successor.
    #[test]
    fn hash_iter_resync_reports_done_for_a_missing_anchor_key() {
        for name in SUPPORTED_TARGETS {
            let asm = emit_for(name);
            assert!(asm.contains("__rt_hash_iter_resync_missing"), "{name}");
            assert!(asm.contains("__rt_hash_iter_resync_done"), "{name}");
            assert!(asm.contains("__rt_hash_get"), "{name}");
        }
    }
}
