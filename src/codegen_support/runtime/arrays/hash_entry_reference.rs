//! Purpose:
//! Emits the hash-entry reference-cell helpers that back PHP reference sets stored in
//! associative-array entries: `__rt_hash_entry_make_reference`, `__rt_hash_entry_deref`,
//! `__rt_hash_iter_next_value`, `__rt_hash_iter_resync` and `__rt_hash_iter_resync_successor`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Storage promotion and borrowed views delegate to Memoir; iterator cursor policy stays here.
//! - A hash entry that joins a PHP reference set stores runtime value tag 11 with `value_lo`
//!   pointing at a managed reference cell (`REFERENCE_CELL_HEAP_KIND`) whose single payload word owns exactly
//!   one boxed Mixed value. `value_hi` is cleared; no interior table address ever escapes.
//! - `__rt_hash_entry_deref` is the borrowed value view of that layout and never allocates.
//! - `__rt_hash_iter_resync` re-derives an insertion-order cursor after the table was
//!   reallocated by growth or copy-on-write, using the last yielded key as the anchor.
//! - `__rt_hash_iter_resync_successor` resumes from the entry AFTER a named key, which is how a
//!   by-reference walk that already yielded the tail reaches an element appended afterwards.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use elephc_memoir::hash_layout::{self, EntryField};

#[cfg(test)]
use elephc_memoir::hash_entry_references::REFERENCE_CELL_VALUE_TAG;

/// Key high word marking "this walk has no successor entry to resume from".
///
/// A live key uses -1 for an integer key and a non-negative length for a string key, so -2 can
/// never collide with one. `IterNext` stores it for a done cursor and for a positive cursor whose
/// successor has no following entry. A post-last cursor instead stores the TAIL key it just
/// yielded, so the walk can resume from that key's live successor.
use elephc_memoir::values::iterator_storage::NO_SUCCESSOR_KEY_MARKER;

/// Emits every hash-entry reference helper for the current target.
pub fn emit_hash_entry_reference(emitter: &mut Emitter) {
    elephc_memoir::hash_entry_references::emit(emitter);
    emit_iter_next_value(emitter);
    emit_iter_resync(emitter);
    emit_iter_resync_successor(emitter);
}

/// Emits an inline borrowed hash payload view using the caller's live registers.
pub(super) fn emit_inline_entry_deref(
    emitter: &mut Emitter,
    done_label: &str,
    tag: &str,
    lo: &str,
    hi: &str,
) {
    elephc_memoir::hash_entry_references::emit_inline_entry_deref(emitter, done_label, tag, lo, hi);
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
            hash_layout::emit_entries(emitter, "x9", "x9");
            hash_layout::emit_entry_index(emitter, "x0", "x4", "x9");
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
            hash_layout::emit_entries(emitter, "rcx", "rcx");
            hash_layout::emit_entry_index(emitter, "rax", "r8", "rcx");
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

/// Emits `__rt_hash_iter_resync_successor`, which resumes a walk from the entry AFTER a named key.
///
/// A by-reference `foreach` returns the post-last cursor (`-2`) with the tail entry, and captures
/// the TAIL key as its anchor. If the loop body appends an element after that point — with or
/// without a rehash — the walk must visit it, as PHP does. Probing the tail key in the live table
/// and following its insertion-order `next` yields exactly that successor, so a fresh append is
/// reached instead of the walk stopping at the old tail.
///
/// Input: argument 0 = live table pointer, argument 1 = anchor key low word,
/// argument 2 = anchor key high word (`-1` for an integer key, a length for a string key).
/// Output: integer result register = the successor cursor (`slot index + 1`), or `-1` when the
/// anchor is gone or has no live successor.
fn emit_iter_resync_successor(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_iter_resync_successor ---");
    emitter.label_global("__rt_hash_iter_resync_successor");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x29, x30, [sp, #-16]!");                   // save frame pointer and return address across the probe
            emitter.instruction("mov x29, sp");                                 // establish the successor-resync frame
            emitter.instruction("bl __rt_hash_get");                            // locate the anchor entry in the live table
            emitter.instruction("cbz x0, __rt_hash_iter_resync_successor_missing"); // a vanished anchor has no successor
            hash_layout::emit_load_entry_field(emitter, "x9", "x4", EntryField::Next);
            emitter.instruction("cmn x9, #1");                                  // did the anchor remain the tail?
            emitter.instruction("b.eq __rt_hash_iter_resync_successor_missing"); // no appended successor to resume from
            emitter.instruction("add x0, x9, #1");                              // encode the successor cursor as slot index plus one
            emitter.instruction("ldp x29, x30, [sp], #16");                     // restore frame pointer and return address
            emitter.instruction("ret");                                         // return the successor cursor to the iterator
            emitter.label("__rt_hash_iter_resync_successor_missing");
            emitter.instruction("mov x0, #-1");                                 // stop without reading a stale successor slot
            emitter.instruction("ldp x29, x30, [sp], #16");                     // restore frame pointer and return address
            emitter.instruction("ret");                                         // return the done cursor to the iterator
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer across the probe
            emitter.instruction("mov rbp, rsp");                                // establish the successor-resync frame
            emitter.instruction("call __rt_hash_get");                          // locate the anchor entry in the live table
            emitter.instruction("test rax, rax");                               // did the live table still contain the anchor key?
            emitter.instruction("jz __rt_hash_iter_resync_successor_missing");  // a vanished anchor has no successor
            hash_layout::emit_load_entry_field(emitter, "r9", "r8", EntryField::Next);
            emitter.instruction("cmp r9, -1");                                  // did the anchor remain the tail?
            emitter.instruction("je __rt_hash_iter_resync_successor_missing");  // no appended successor to resume from
            emitter.instruction("mov rax, r9");                                 // copy the successor slot before encoding the cursor
            emitter.instruction("add rax, 1");                                  // encode the successor cursor as slot index plus one
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.instruction("ret");                                         // return the successor cursor to the iterator
            emitter.label("__rt_hash_iter_resync_successor_missing");
            emitter.instruction("mov rax, -1");                                 // stop without reading a stale successor slot
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.instruction("ret");                                         // return the done cursor to the iterator
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
                "__rt_hash_entry_bind_reference",
                "__rt_hash_entry_deref",
                "__rt_hash_iter_next_value",
                "__rt_hash_iter_resync",
                "__rt_hash_iter_resync_successor",
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

    /// Pins that the tail-successor resync probes the anchor and reports done when it vanished or
    /// stayed the tail, on every supported target.
    #[test]
    fn hash_iter_resync_successor_reports_done_for_a_vanished_or_final_anchor() {
        for name in SUPPORTED_TARGETS {
            let asm = emit_for(name);
            assert!(
                asm.contains("__rt_hash_iter_resync_successor_missing"),
                "{name}: missing-anchor arm absent"
            );
            let body = asm
                .split_once("__rt_hash_iter_resync_successor:")
                .unwrap()
                .1
                .split_once("__rt_hash_iter_resync_successor_missing:")
                .unwrap()
                .0;
            assert!(body.contains("__rt_hash_get"), "{name}: anchor probe absent");
            // The anchor's own successor is at entry offset 56, the same word the raw walk reads.
            let successor_load = if name == "linux-x86_64" {
                "QWORD PTR [r8 + 56]"
            } else {
                "[x4, #56]"
            };
            assert!(body.contains(successor_load), "{name}: successor load absent");
        }
    }
}
