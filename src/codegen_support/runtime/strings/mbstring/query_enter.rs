//! Purpose:
//! Selects a writable nested query array and returns a protected native cursor.
//!
//! Called from:
//! - The mbstring runtime emitter and native query registration storage tests.
//!
//! Key details:
//! - Unique nested hashes retain their identity; shared arrays or boxes require COW.
//! - Ordinary Mixed storage is transparent, but PHP reference values are replaced.
//! - Active construction borrows require a copy even when their physical owner count is one.
//! - The selected child is pinned before parent replacement can run PHP destructors.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch, sentinels};

#[cfg(test)]
mod tests;

const NAME: &str = "__rt_mbstring_query_hash_enter";

/// Emits C4 context/parent/normalized-key/output-child entry, returning zero or pending status two.
/// Valid inputs borrow a managed parent and an integer/string descriptor for the whole call.
/// The output receives one child lifetime pin even on pending; its caller must unpin it.
pub(super) fn emit(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    emitter.label_global(NAME);
    // -- preserve the selected parent, query-owned key, and cursor output --
    if arm {
        emitter.instruction("sub sp, sp, #96");                                 // reserve borrowed inputs, pending state, an owned descriptor, and linkage
        emitter.instruction("stp x29, x30, [sp, #80]");                         // preserve the caller across destruction and allocation
        emitter.instruction("add x29, sp, #80");                                // establish the protected child-selection frame
        emitter.instruction("stp x1, x2, [sp]");                                // keep the selected parent and normalized query key
        emitter.instruction("str x3, [sp, #16]");                               // retain the caller's cursor output slot
        emitter.instruction("str xzr, [x3]");                                   // publish no cursor until child protection is acquired
        emitter.instruction("str xzr, [sp, #24]");                              // begin without a newly pending exception
        emitter.instruction("mov x0, x1");                                      // pin the parent without creating an ordinary PHP copy
    } else {
        emitter.instruction("push rbp");                                        // preserve linkage and align nested native calls
        emitter.instruction("mov rbp, rsp");                                    // establish the child-selection frame
        emitter.instruction("sub rsp, 80");                                     // retain borrowed inputs, the new descriptor, and pending state
        emitter.instruction("mov QWORD PTR [rsp], rsi");                        // keep exactly the selected parent
        emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                    // retain the query-owned normalized descriptor
        emitter.instruction("mov QWORD PTR [rsp + 16], rcx");                   // keep the caller's child output slot
        emitter.instruction("mov QWORD PTR [rcx], 0");                          // initialize the independently published cursor metadata
        emitter.instruction("mov QWORD PTR [rsp + 24], 0");                     // initialize this operation's pending flag
        emitter.instruction("mov rdi, rsi");                                    // acquire a non-COW root for the selected parent
    }
    abi::emit_call_label(emitter, "__rt_hash_pin");
    key_call(emitter, "__rt_hash_write_guard_owns");
    emitter.instruction(if arm { "cmp x0, #0" } else { "test rax, rax" });      // distinguish an ordinary entry owner from an enclosing release borrow
    emitter.instruction(if arm { "cset x9, eq" } else { "sete al" });           // active borrows cannot become a uniquely owned query child
    if !arm { emitter.instruction("movzx eax, al"); }                           // normalize the enclosing-release predicate before spilling
    emitter.instruction(if arm { "str x9, [sp, #40]" } else { "mov QWORD PTR [rsp + 40], rax" }); // retain authoritative ownership across lookup
    key_call(emitter, "__rt_hash_get");
    emitter.instruction(if arm { "cmp x0, #0" } else { "test rax, rax" });      // a missing child needs a fresh empty array
    branch(emitter, "eq", "je", "fresh");
    select_existing(emitter);

    // -- acquire an owned replacement without exposing it during old-value destruction --
    emitter.label(&format!("{NAME}_clone"));
    emitter.instruction(if arm { "mov x0, x10" } else { "mov rdi, r10" });      // borrow the shared hash without consuming its entry owner
    abi::emit_call_label(emitter, "__rt_hash_clone_shallow");
    jump(emitter, "owned");
    emitter.label(&format!("{NAME}_indexed"));
    emitter.instruction(if arm { "mov x0, x10" } else { "mov rdi, r10" });      // preserve source owners while promoting dense storage
    abi::emit_call_label(emitter, "__rt_array_to_hash");
    jump(emitter, "owned");
    emitter.label(&format!("{NAME}_fresh"));
    emitter.instruction(if arm { "mov x0, #4" } else { "mov edi, 4" });         // start an empty nested query table with a small capacity
    emitter.instruction(if arm { "mov x1, #7" } else { "mov esi, 7" });         // nested query arrays accept heterogeneous values
    abi::emit_call_label(emitter, "__rt_hash_new");
    emitter.label(&format!("{NAME}_owned"));
    emitter.instruction(if arm { "str x0, [sp, #56]" } else { "mov QWORD PTR [rsp + 56], rax" }); // retain the replacement's ordinary owner before pinning
    if !arm { emitter.instruction("mov rdi, rax"); }                            // adapt the selected child to the C pin convention
    abi::emit_call_label(emitter, "__rt_hash_pin");
    if arm {
        emitter.instruction("mov x9, #5");                                      // identify an owned native associative-array payload
        emitter.instruction("str x9, [sp, #48]");                               // publish the replacement descriptor's concrete tag
        emitter.instruction("str xzr, [sp, #64]");                              // clear its unused high word
        emitter.instruction("mov x0, #0");                                      // the internal writer needs no host context
        emitter.instruction("ldp x1, x2, [sp]");                                // restore the selected parent and borrowed query key
        emitter.instruction("add x3, sp, #48");                                 // transfer only the owned array described by this local value
    } else {
        emitter.instruction("mov QWORD PTR [rsp + 48], 5");                     // identify the owned native array replacement
        emitter.instruction("mov QWORD PTR [rsp + 64], 0");                     // clear the descriptor's unused high word
        emitter.instruction("xor edi, edi");                                    // the guarded writer needs no host context
        emitter.instruction("mov rsi, QWORD PTR [rsp]");                        // select the same pinned parent after allocation
        emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                    // restore the normalized borrowed key
        emitter.instruction("lea rcx, [rsp + 48]");                             // transfer the descriptor's array owner into the parent
    }
    abi::emit_call_label(emitter, "__rt_mbstring_query_hash_store_array");
    emitter.instruction(if arm { "lsr x9, x0, #1" } else { "shr eax, 1" });     // preserve completed writes independently of pending status two
    emitter.instruction(if arm { "str x9, [sp, #24]" } else { "mov QWORD PTR [rsp + 24], rax" }); // keep pending state through final parent cleanup
    jump(emitter, "finish");

    // -- preserve an already unique child without changing parent storage or wrapper identity --
    emitter.label(&format!("{NAME}_unique"));
    emitter.instruction(if arm { "str x10, [sp, #56]" } else { "mov QWORD PTR [rsp + 56], r10" }); // retain the exact unique child selected through ordinary boxes
    emitter.instruction(if arm { "mov x0, x10" } else { "mov rdi, r10" });      // acquire cursor protection before releasing the parent pin
    abi::emit_call_label(emitter, "__rt_hash_pin");
    emitter.label(&format!("{NAME}_finish"));
    if arm {
        emitter.instruction("ldr x9, [sp, #16]");                               // recover the caller's child output slot
        emitter.instruction("ldr x10, [sp, #56]");                              // retain the live selected child regardless of parent retargeting
        emitter.instruction("str x10, [x9]");                                   // transfer one child pin to the registration cursor
        emitter.instruction("ldr x1, [sp]");                                    // retire only this operation's parent protection
        emitter.instruction("add x2, sp, #24");                                 // accumulate any exception from final parent release
    } else {
        emitter.instruction("mov r10, QWORD PTR [rsp + 16]");                   // recover the cursor metadata output slot
        emitter.instruction("mov r11, QWORD PTR [rsp + 56]");                   // retain the pinned child even if its parent was displaced
        emitter.instruction("mov QWORD PTR [r10], r11");                        // transfer the child lifetime pin to the caller
        emitter.instruction("mov rsi, QWORD PTR [rsp]");                        // release only this operation's parent pin
        emitter.instruction("lea rdx, [rsp + 24]");                             // retain pending state through final parent destruction
    }
    abi::emit_symbol_address(emitter, if arm { "x0" } else { "rdi" }, "__rt_hash_unpin");
    abi::emit_call_label(emitter, "__rt_cleanup_call");
    if arm {
        emitter.instruction("ldr x0, [sp, #24]");                               // return pending state after all local owners have transferred
        emitter.instruction("lsl x0, x0, #1");                                  // encode the shared callback's pending status two
        emitter.instruction("ldp x29, x30, [sp, #80]");                         // restore the caller with a protected child output
        emitter.instruction("add sp, sp, #96");                                 // release child-selection storage
    } else {
        emitter.instruction("mov rax, QWORD PTR [rsp + 24]");                   // preserve any pending throwable after completed entry
        emitter.instruction("shl eax, 1");                                      // encode the shared callback's pending status two
        emitter.instruction("leave");                                           // restore caller linkage after transferring cursor protection
    }
    emitter.instruction("ret");                                                 // return the status while the output holds one child lifetime pin
}

/// Distinguishes ordinary box sharing, persistent references, dense arrays, and unique nested hashes.
fn select_existing(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    if arm {
        emitter.instruction("mov x9, x3");                                      // inspect the concrete entry tag independently of lookup's result flag
        emitter.instruction("mov x10, x1");                                     // retain the entry's borrowed payload
        emitter.instruction("mov x11, x2");                                     // preserve its high word for persistent reference recognition
        emitter.instruction("ldr x12, [sp, #40]");                              // preserve any enclosing construction borrow while traversing wrappers
    } else {
        emitter.instruction("mov r9, rcx");                                     // inspect the entry's concrete runtime tag
        emitter.instruction("mov r10, rdi");                                    // retain its borrowed payload across representation checks
        emitter.instruction("mov r11, rsi");                                    // preserve the high payload word
        emitter.instruction("mov rdx, QWORD PTR [rsp + 40]");                   // retain whether an outer release already owns this entry
    }
    emitter.label(&format!("{NAME}_peel"));
    emitter.instruction(if arm { "cmp x9, #7" } else { "cmp r9, 7" });          // only internal Mixed wrappers need transparent traversal
    branch(emitter, "ne", "jne", "array");
    emitter.instruction(if arm { "cmp x11, #1" } else { "cmp r11, 1" });        // PHP references are scalar replacements for named query entry
    branch(emitter, "eq", "je", "fresh");
    sentinels::emit_branch_if_null_container(emitter, if arm { "x10" } else { "r10" }, if arm { "x13" } else { "r8" }, &format!("{NAME}_fresh"));
    if arm {
        emitter.instruction("ldr w13, [x10, #-12]");                            // count ordinary owners of the internal value wrapper
        emitter.instruction("cmp w13, #1");                                     // shared or dying boxes cannot expose a unique nested value
        emitter.instruction("cset x13, ne");                                    // remember whether this wrapper requires child separation
        emitter.instruction("orr x12, x12, x13");                               // preserve sharing observed anywhere in the wrapper chain
        emitter.instruction("ldr x9, [x10]");                                   // inspect the wrapped PHP tag before following its payload
        emitter.instruction("ldr x11, [x10, #16]");                             // preserve any persistent reference marker in the child
        emitter.instruction("ldr x10, [x10, #8]");                              // follow the borrowed child without mutating a shared box
    } else {
        emitter.instruction("cmp DWORD PTR [r10 - 12], 1");                     // distinguish one ordinary wrapper owner from shared or dying storage
        emitter.instruction("setne cl");                                        // record whether the current wrapper requires COW
        emitter.instruction("movzx ecx, cl");                                   // normalize the wrapper-sharing predicate
        emitter.instruction("or edx, ecx");                                     // preserve sharing found in any preceding wrapper
        emitter.instruction("mov r9, QWORD PTR [r10]");                         // inspect the wrapped PHP tag
        emitter.instruction("mov r11, QWORD PTR [r10 + 16]");                   // preserve a nested persistent reference marker
        emitter.instruction("mov r10, QWORD PTR [r10 + 8]");                    // follow internal storage without changing its shared identity
    }
    jump(emitter, "peel");
    emitter.label(&format!("{NAME}_array"));
    sentinels::emit_branch_if_null_container(emitter, if arm { "x10" } else { "r10" }, if arm { "x13" } else { "r8" }, &format!("{NAME}_fresh"));
    emitter.instruction(if arm { "cmp x9, #4" } else { "cmp r9, 4" });          // dense arrays must be promoted before heterogeneous query writes
    branch(emitter, "eq", "je", "indexed");
    emitter.instruction(if arm { "cmp x9, #5" } else { "cmp r9, 5" });          // only an existing associative array can retain its identity
    branch(emitter, "ne", "jne", "fresh");
    emitter.instruction(if arm { "cmp x12, #0" } else { "test edx, edx" });     // any shared ordinary wrapper requires a separate child value
    branch(emitter, "ne", "jne", "clone");
    if arm {
        emitter.instruction("ldr w13, [x10, #-12]");                            // inspect the child hash's physical owner count
        emitter.instruction("ldr x14, [x10, #48]");                             // exclude cursor pins from logical PHP ownership
        emitter.instruction("sub x13, x13, x14");                               // preserve unique identity while nested cursors keep the hash alive
        emitter.instruction("cmp x13, #1");                                     // shared or actively destroyed children require a value copy
    } else {
        emitter.instruction("mov ecx, DWORD PTR [r10 - 12]");                   // inspect the nested hash's physical reference count
        emitter.instruction("sub rcx, QWORD PTR [r10 + 48]");                   // remove cursor pins from the logical owner count
        emitter.instruction("cmp rcx, 1");                                      // preserve identity only for exactly one live PHP owner
    }
    branch(emitter, "eq", "je", "unique");
    jump(emitter, "clone");
}

/// Inspects an already normalized key without introducing another PHP owner or name rule.
fn key_call(emitter: &mut Emitter, symbol: &str) {
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("ldr x0, [sp]");                                    // recover the pinned parent for child selection
        emitter.instruction("ldr x9, [sp, #8]");                                // borrow the normalized query key descriptor
        emitter.instruction("ldp x1, x2, [x9, #8]");                            // preserve the key payload and binary length
        emitter.instruction("ldr x9, [x9]");                                    // identify integer versus string keys
        emitter.instruction("cmp x9, #0");                                      // normalized integers use an inline signed payload
        emitter.instruction("mov x10, #-1");                                    // prepare the runtime integer-key sentinel
        emitter.instruction("csel x2, x10, x2, eq");                            // preserve integer and binary-string identity
    } else {
        emitter.instruction("mov rdi, QWORD PTR [rsp]");                        // recover the selected parent without COW
        emitter.instruction("mov r11, QWORD PTR [rsp + 8]");                    // borrow the normalized query descriptor
        emitter.instruction("mov rsi, QWORD PTR [r11 + 8]");                    // restore the signed integer or binary string pointer
        emitter.instruction("mov rdx, QWORD PTR [r11 + 16]");                   // preserve the exact binary key length
        emitter.instruction("mov r10, -1");                                     // prepare the integer-key sentinel
        emitter.instruction("cmp QWORD PTR [r11], 0");                          // classify the already normalized key
        emitter.instruction("cmove rdx, r10");                                  // retain integer identity independently of string spelling
    }
    abi::emit_call_label(emitter, symbol);
}

/// Branches between representation or ownership phases with target-specific condition mnemonics.
fn branch(emitter: &mut Emitter, arm: &str, x86: &str, phase: &str) {
    let op = if emitter.target.arch == Arch::AArch64 { format!("b.{arm}") } else { x86.to_owned() };
    emitter.instruction(&format!("{op} {NAME}_{phase}"));                       // preserve the selected array representation or ownership decision
}

/// Transfers control between child-selection phases without changing local ownership.
fn jump(emitter: &mut Emitter, phase: &str) {
    let op = if emitter.target.arch == Arch::AArch64 { "b" } else { "jmp" };
    emitter.instruction(&format!("{op} {NAME}_{phase}"));                       // keep pending state and cursor ownership in the same frame
}
