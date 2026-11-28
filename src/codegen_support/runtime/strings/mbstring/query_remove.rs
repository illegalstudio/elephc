//! Purpose:
//! Removes an mb_parse_str root entry before executing its destructor-visible cleanup.
//!
//! Called from:
//! - The mbstring runtime emitter and focused native query storage tests.
//!
//! Key details:
//! - A lifetime pin preserves the selected hash without separating exposed root copies.
//! - Unlinking completes before callbacks; reentrant replacement entries remain intact.
//! - Guard claims avoid releasing a value already borrowed by an active construction destructor.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

#[cfg(test)]
mod tests;

const NAME: &str = "__rt_mbstring_query_hash_remove";

/// Emits C3 context/hash/normalized-key removal, returning zero or a contained pending status two.
/// The caller borrows a valid hash and integer/string descriptor through the complete callback.
pub(super) fn emit(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    emitter.label_global(NAME);
    // -- preserve the selected root and normalized borrowed key --
    if arm {
        emitter.instruction("sub sp, sp, #96");                                 // retain selected storage, removed ownership, pending state, and linkage
        emitter.instruction("stp x29, x30, [sp, #80]");                         // preserve the caller across destructor callbacks
        emitter.instruction("add x29, sp, #80");                                // establish the removal frame
        emitter.instruction("str x1, [sp]");                                    // retain exactly the selected root hash
        emitter.instruction("ldr x9, [x2]");                                    // inspect the normalized key's integer or string tag
        emitter.instruction("ldp x10, x11, [x2, #8]");                          // borrow the key payload and binary length
        emitter.instruction("cmp x9, #0");                                      // integer keys use an inline signed payload
        emitter.instruction("mov x12, #-1");                                    // prepare the integer-key length sentinel
        emitter.instruction("csel x11, x12, x11, eq");                          // preserve normalized integer and binary-string identity
        emitter.instruction("stp x10, x11, [sp, #8]");                          // keep query-owned key bytes stable across lookup
        emitter.instruction("str xzr, [sp, #48]");                              // initialize the contained pending-exception flag
        emitter.instruction("mov x0, x1");                                      // pin the selected hash without introducing a PHP value owner
    } else {
        emitter.instruction("push rbp");                                        // preserve linkage and align nested native calls
        emitter.instruction("mov rbp, rsp");                                    // establish the removal frame
        emitter.instruction("sub rsp, 80");                                     // retain selected storage, removed owners, and pending state
        emitter.instruction("mov QWORD PTR [rsp], rsi");                        // retain exactly the selected root hash
        emitter.instruction("mov r10, QWORD PTR [rdx + 8]");                    // borrow the normalized key payload
        emitter.instruction("mov QWORD PTR [rsp + 8], r10");                    // retain query-owned key bytes or the signed integer
        emitter.instruction("mov r10, QWORD PTR [rdx + 16]");                   // borrow the binary string length
        emitter.instruction("mov r11, -1");                                     // prepare the integer-key sentinel
        emitter.instruction("cmp QWORD PTR [rdx], 0");                          // distinguish normalized integers from strings
        emitter.instruction("cmove r10, r11");                                  // retain exact integer or binary-string identity
        emitter.instruction("mov QWORD PTR [rsp + 16], r10");                   // preserve normalized key metadata
        emitter.instruction("mov QWORD PTR [rsp + 48], 0");                     // initialize the contained pending flag
        emitter.instruction("mov rdi, rsi");                                    // pin the selected hash without ordinary COW separation
    }
    // -- detach ownership and unlink before any PHP-visible cleanup --
    abi::emit_call_label(emitter, "__rt_hash_pin");
    key_call(emitter, "__rt_hash_get");
    emitter.instruction(if arm { "cmp x0, #0" } else { "test rax, rax" });      // a missing key has no removal owners
    branch(emitter, "eq", "je", "finish");
    emitter.instruction(if arm { "str x4, [sp, #64]" } else { "mov QWORD PTR [rsp + 64], r8" }); // preserve the borrowed entry across a non-PHP guard claim
    key_call(emitter, "__rt_hash_write_guard_claim");
    emitter.instruction(if arm { "str x0, [sp, #56]" } else { "mov QWORD PTR [rsp + 56], rax" }); // remember whether this deletion owns the old value release
    if arm {
        emitter.instruction("ldr x4, [sp, #64]");                               // recover the stable entry before any PHP callbacks
        elephc_memoir::hash_layout::emit_entry_value(emitter, "x4", "x10", "x9", "x11");
        emitter.instruction("str x9, [sp, #24]");                               // retain the value owner until protected cleanup
        emitter.instruction("str x10, [sp, #32]");                              // retain its release convention after tombstoning
        elephc_memoir::hash_layout::emit_entry_key(emitter, "x4", "x9", "x10");
        emitter.instruction("cmn x10, #1");                                     // integer keys own no separate bytes
        emitter.instruction("csel x9, xzr, x9, eq");                            // use a null owner for inline integer keys
        emitter.instruction("str x9, [sp, #40]");                               // detach key ownership before callbacks may reuse the slot
    } else {
        emitter.instruction("mov r8, QWORD PTR [rsp + 64]");                    // recover the stable entry before any PHP callbacks
        elephc_memoir::hash_layout::emit_entry_value(emitter, "r8", "r10", "r11", "rax");
        emitter.instruction("mov QWORD PTR [rsp + 24], r11");                   // retain its owner for protected cleanup
        emitter.instruction("mov QWORD PTR [rsp + 32], r10");                   // retain the release convention after tombstoning
        elephc_memoir::hash_layout::emit_entry_key(emitter, "r8", "r10", "r11");
        emitter.instruction("xor eax, eax");                                    // prepare no owner for an inline integer key
        emitter.instruction("cmp r11, -1");                                     // classify the owned key representation
        emitter.instruction("cmove r10, rax");                                  // string keys keep their separate allocation owner
        emitter.instruction("mov QWORD PTR [rsp + 40], r10");                   // detach key ownership before callbacks may reuse the slot
    }
    elephc_memoir::values::hash_unset::emit_unlink_pinned_entry(emitter, NAME);
    // -- release detached owners and the final pin behind the exception boundary --
    emitter.instruction(if arm { "ldr x9, [sp, #40]" } else { "mov r10, QWORD PTR [rsp + 40]" }); // inspect the detached string-key owner
    emitter.instruction(if arm { "cmp x9, #0" } else { "test r10, r10" });      // integer keys need no release
    branch(emitter, "eq", "je", "value");
    cleanup(emitter, "__rt_decref_any", 40);
    emitter.label(&format!("{NAME}_value"));
    emitter.instruction(if arm { "ldr x9, [sp, #56]" } else { "mov r10, QWORD PTR [rsp + 56]" }); // inspect the old value's guard ownership
    emitter.instruction(if arm { "cmp x9, #0" } else { "test r10, r10" });      // an outer destruction already consumes a borrowed entry
    branch(emitter, "eq", "je", "finish");
    emitter.instruction(if arm { "ldr x9, [sp, #32]" } else { "mov r10, QWORD PTR [rsp + 32]" }); // classify the detached payload after removal is visible
    for (tag, ac, xc, phase) in [(1, "eq", "je", "release"), (8, "eq", "je", "finish"),
        (10, "eq", "je", "callable"), (4, "lo", "jb", "finish")] {
        emitter.instruction(&if arm { format!("cmp x9, #{tag}") } else { format!("cmp r10, {tag}") }); // distinguish owned strings, nulls, callables, and scalars
        branch(emitter, ac, xc, phase);
    }
    emitter.label(&format!("{NAME}_release"));
    cleanup(emitter, "__rt_decref_any", 24);
    jump(emitter, "finish");
    emitter.label(&format!("{NAME}_callable"));
    cleanup(emitter, "__rt_callable_descriptor_release", 24);
    emitter.label(&format!("{NAME}_finish"));
    cleanup(emitter, "__rt_hash_unpin", 0);
    if arm {
        emitter.instruction("ldr x0, [sp, #48]");                               // retain any throwable produced after the completed removal
        emitter.instruction("lsl x0, x0, #1");                                  // map the pending flag to shared callback status two
        emitter.instruction("ldp x29, x30, [sp, #80]");                         // restore caller linkage after all detached owners are retired
        emitter.instruction("add sp, sp, #96");                                 // release the removal frame
    } else {
        emitter.instruction("mov rax, QWORD PTR [rsp + 48]");                   // retain any throwable produced after completed mutation
        emitter.instruction("shl eax, 1");                                      // map the pending flag to shared callback status two
        emitter.instruction("leave");                                           // restore the caller and retire removal storage
    }
    emitter.instruction("ret");                                                 // return without touching any entry created by a reentrant destructor
}

/// Calls a non-PHP lookup or guard claim using stable normalized query key bytes.
fn key_call(emitter: &mut Emitter, symbol: &str) {
    let arm = emitter.target.arch == Arch::AArch64;
    for (offset, a, x) in [(0, "x0", "rdi"), (8, "x1", "rsi"), (16, "x2", "rdx")] {
        emitter.instruction(&if arm { format!("ldr {a}, [sp, #{offset}]") } else { format!("mov {x}, QWORD PTR [rsp + {offset}]") }); // restore one borrowed lookup argument
    }
    abi::emit_call_label(emitter, symbol);
}

/// Retires one detached owner behind the shared native exception boundary.
fn cleanup(emitter: &mut Emitter, symbol: &str, offset: usize) {
    let arm = emitter.target.arch == Arch::AArch64;
    emitter.instruction(&if arm { format!("ldr x1, [sp, #{offset}]") } else { format!("mov rsi, QWORD PTR [rsp + {offset}]") }); // recover the detached value or selected lifetime pin
    abi::emit_symbol_address(emitter, if arm { "x0" } else { "rdi" }, symbol);
    emitter.instruction(if arm { "add x2, sp, #48" } else { "lea rdx, [rsp + 48]" }); // accumulate pending state without skipping completed cleanup
    abi::emit_call_label(emitter, "__rt_cleanup_call");
}

/// Selects a removal phase with the active target's condition mnemonic.
fn branch(emitter: &mut Emitter, arm: &str, x86: &str, phase: &str) {
    let op = if emitter.target.arch == Arch::AArch64 { format!("b.{arm}") } else { x86.to_owned() };
    emitter.instruction(&format!("{op} {NAME}_{phase}"));                       // preserve the selected ownership or linkage decision
}

/// Transfers control within one pinned removal operation.
fn jump(emitter: &mut Emitter, phase: &str) {
    let op = if emitter.target.arch == Arch::AArch64 { "b" } else { "jmp" };
    emitter.instruction(&format!("{op} {NAME}_{phase}"));                       // keep pending state and detached owners in the same frame
}
