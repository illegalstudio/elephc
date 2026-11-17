//! Purpose:
//! Emits owner transfer into managed hash-entry reference cells and borrowed payload views.
//!
//! Called from:
//! - The compiler's hash-reference adapter and direct hash payload readers.
//!
//! Key details:
//! - Promotion separates shared Mixed boxes before transferring the entry's existing owner.
//! - Dereference borrows without allocation; iterator cursor policy stays in the compiler.

use crate::{Arch, AssemblyEmitter, TargetAbi, ValueTag};

/// Runtime value tag marking a payload word that owns a managed reference cell.
pub const REFERENCE_CELL_VALUE_TAG: ValueTag = ValueTag::new(11);

/// Payload descriptor selecting the boxed Mixed value owned by an entry reference cell.
const REFERENCE_CELL_PAYLOAD_TAG: ValueTag = ValueTag::new(7);

/// Emits entry promotion and borrowed reads in the compiler's existing helper order.
pub fn emit<E: AssemblyEmitter + TargetAbi>(emitter: &mut E) {
    emit_make_reference(emitter);
    emit_bind_reference(emitter);
    emit_entry_deref(emitter);
}

/// Emits `__rt_hash_entry_make_reference`, which promotes a boxed Mixed hash entry into a
/// managed reference cell and returns that cell.
///
/// `__rt_hash_to_mixed` widens every entry present when the walk starts, so `value_lo` normally
/// owns a boxed Mixed cell already. An entry INSERTED after that point carries its own concrete
/// runtime tag (`$a[] = 3` inside `foreach ($a as &$v)` writes a plain integer), so the payload
/// is widened here first, taking over the entry's ownership without a retain. Skipping that step
/// let a raw scalar be published as a reference-cell payload and crash the first read of `$v`.
/// A shallow hash clone retains, rather than copies, each boxed Mixed entry, so the same zval can
/// back two arrays' buckets after `$b = $a`. A reference set must own its zval exclusively. PHP
/// separates on the way into a reference, so a shared cell is copied here before it is wrapped.
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
fn emit_make_reference<E: AssemblyEmitter + TargetAbi>(emitter: &mut E) {
    emitter.blank();
    emitter.comment("--- runtime: hash_entry_make_reference ---");
    emitter.label_global("__rt_hash_entry_make_reference");
    match emitter.arch() {
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

+/// Emits `__rt_hash_entry_bind_reference`, which points an existing reference entry at one
/// already-live managed reference cell.
///
/// The caller has just inserted the source value and may address its value word. A
/// non-reference entry is promoted first (allocating and then discarding its fresh cell), so
/// this helper only has to swap the payload cell and transfer the reference counts. Every
/// `[&$x]` element after the first therefore shares `$x`'s cell instead of rebinding `$x` to a
/// private copy, which is what keeps earlier elements observing later writes.
///
/// Input: argument 0 = address of `entry.value_lo`, argument 1 = borrowed reference cell.
fn emit_bind_reference<E: AssemblyEmitter + TargetAbi>(emitter: &mut E) {
    emitter.blank();
    emitter.comment("--- runtime: hash_entry_bind_reference ---");
    emitter.label_global("__rt_hash_entry_bind_reference");
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x9, [x0, #16]"));                 // read the entry runtime value tag
            emitter.instruction(&format!("cmp x9, #{REFERENCE_CELL_VALUE_TAG}")); // is the entry already a reference marker?
            emitter.instruction("b.eq __rt_hash_entry_bind_reference_bound");   // a promoted entry is ready for the swap
            emitter.instruction("stp x29, x30, [sp, #-32]!");                   // save the frame across the promotion call
            emitter.instruction("mov x29, sp");                                 // establish the promotion frame
            emitter.instruction("str x0, [sp, #16]");                           // save the entry value address
            emitter.instruction("str x1, [sp, #24]");                           // save the cell that must replace the promoted one
            emitter.instruction("bl __rt_hash_entry_make_reference");           // allocate the entry's temporary reference cell
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the entry value address
            emitter.instruction("ldr x1, [sp, #24]");                           // reload the caller's live cell
            emitter.instruction("ldp x29, x30, [sp], #32");                     // release the promotion frame
            emitter.label("__rt_hash_entry_bind_reference_bound");
            emitter.instruction("ldr x2, [x0]");                                // read the cell the entry currently owns
            emitter.instruction("cmp x2, x1");                                  // is the entry already bound to this cell?
            emitter.instruction("b.eq __rt_hash_entry_bind_reference_done");    // the idempotent path changes no reference counts
            emitter.instruction("stp x29, x30, [sp, #-48]!");                   // save the frame across the reference-count transfer
            emitter.instruction("mov x29, sp");                                 // establish the transfer frame
            emitter.instruction("str x0, [sp, #16]");                           // save the entry value address
            emitter.instruction("str x1, [sp, #24]");                           // save the replacement cell
            emitter.instruction("str x2, [sp, #32]");                           // save the cell the entry is giving up
            emitter.instruction("mov x0, x1");                                  // retain the replacement cell for the entry
            emitter.instruction("bl __rt_incref");                              // the entry takes its own reference
            emitter.instruction("ldr x0, [sp, #32]");                           // load the old cell
            emitter.instruction("bl __rt_reference_cell_release");              // release the entry's share of the old cell
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the entry value address
            emitter.instruction("ldr x1, [sp, #24]");                           // reload the replacement cell
            emitter.instruction("str x1, [x0]");                                // publish the shared cell as the entry payload
            emitter.instruction("ldp x29, x30, [sp], #48");                     // release the transfer frame
            emitter.label("__rt_hash_entry_bind_reference_done");
            emitter.instruction("ret");                                         // return without a value
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp QWORD PTR [rdi + 16], {REFERENCE_CELL_VALUE_TAG}")); // is the entry already a reference marker?
            emitter.instruction("je __rt_hash_entry_bind_reference_bound_x");   // a promoted entry is ready for the swap
            emitter.instruction("push rbp");                                    // save the frame across the promotion call
            emitter.instruction("mov rbp, rsp");                                // establish the promotion frame
            emitter.instruction("sub rsp, 16");                                 // reserve slots for the entry address and cell
            emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                // save the entry value address
            emitter.instruction("mov QWORD PTR [rbp - 16], rsi");               // save the cell that must replace the promoted one
            emitter.instruction("call __rt_hash_entry_make_reference");         // allocate the entry's temporary reference cell
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                // reload the entry value address
            emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");               // reload the caller's live cell
            emitter.instruction("add rsp, 16");                                 // release the promotion slots
            emitter.instruction("pop rbp");                                     // restore the caller frame
            emitter.label("__rt_hash_entry_bind_reference_bound_x");
            emitter.instruction("mov rax, QWORD PTR [rdi]");                    // read the cell the entry currently owns
            emitter.instruction("cmp rax, rsi");                                // is the entry already bound to this cell?
            emitter.instruction("je __rt_hash_entry_bind_reference_done_x");    // the idempotent path changes no reference counts
            emitter.instruction("push rbp");                                    // save the frame across the reference-count transfer
            emitter.instruction("mov rbp, rsp");                                // establish the transfer frame
            emitter.instruction("sub rsp, 32");                                 // reserve slots for both cells and the entry
            emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                // save the entry value address
            emitter.instruction("mov QWORD PTR [rbp - 16], rsi");               // save the replacement cell
            emitter.instruction("mov QWORD PTR [rbp - 24], rax");               // save the cell the entry is giving up
            emitter.instruction("mov rax, rsi");                                // pass the replacement cell in the private incref register
            emitter.instruction("call __rt_incref");                            // the entry takes its own reference
            emitter.instruction("mov rax, QWORD PTR [rbp - 24]");               // load the old cell in the private release register
            emitter.instruction("call __rt_reference_cell_release");            // release the entry's share of the old cell
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                // reload the entry value address
            emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");               // reload the replacement cell
            emitter.instruction("mov QWORD PTR [rdi], rsi");                    // publish the shared cell as the entry payload
            emitter.instruction("add rsp, 32");                                 // release the transfer slots
            emitter.instruction("pop rbp");                                     // restore the caller frame
            emitter.label("__rt_hash_entry_bind_reference_done_x");
            emitter.instruction("ret");                                         // return without a value
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
fn emit_entry_deref<E: AssemblyEmitter + TargetAbi>(emitter: &mut E) {
    emitter.blank();
    emitter.comment("--- runtime: hash_entry_deref ---");
    emitter.label_global("__rt_hash_entry_deref");
    match emitter.arch() {
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
pub fn emit_inline_entry_deref<E: AssemblyEmitter + TargetAbi>(
    emitter: &mut E,
    done_label: &str,
    tag: &str,
    lo: &str,
    hi: &str,
) {
    match emitter.arch() {
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

