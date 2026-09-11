//! Purpose:
//! Stores mbregex captures and owned query arrays in a selected native associative array.
//!
//! Called from:
//! - The mbstring runtime emitter and focused native construction tests.
//!
//! Key details:
//! - Lifetime pins preserve the selected array without ordinary copy-on-write separation.
//! - Destruction is protected and the selected key is looked up again after callbacks.
//! - Owner guards coordinate same-key set/unset and nested capture replacement during destruction.
//! - Reference adapters select or promote each destination before entering this hash-only writer.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

#[cfg(test)]
pub(super) mod tests;

const NAME: &str = "__rt_mbstring_capture_hash_store";

/// Emits a C store callback with unused context, selected hash, key descriptor, and value descriptor.
/// Descriptors remain borrowed; string bytes are persisted. Guards track set/unset and nested
/// capture writes to the selected key. A replacement owner is consumed before the final capture
/// is installed. Returns zero or a pending-throwable status after completing the write.
/// The caller validates nonnull descriptors, integer/string keys, and string/false values.
/// The query-only alias accepts tag five and transfers one owned native hash payload;
/// its descriptor and key bytes stay borrowed. Both entries share guarded replacement.
pub(super) fn emit(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    emitter.label_global("__rt_mbstring_query_hash_store_array");
    emitter.label_global(NAME);
    if arm {
        emitter.instruction("sub sp, sp, #128");                                // reserve capture ownership, pending state, and linkage
        emitter.instruction("stp x29, x30, [sp, #112]");                        // retain the caller across native callbacks
        emitter.instruction("add x29, sp, #112");                               // establish an aligned construction frame
        emitter.instruction("str x1, [sp]");                                    // retain the selected stable hash header
        emitter.instruction("ldp x9, x10, [x2]");                               // borrow the normalized key tag and payload
        emitter.instruction("ldr x11, [x2, #16]");                              // borrow the binary string key length
        emitter.instruction("cmp x9, #0");                                      // distinguish integer keys from named captures
        emitter.instruction("mov x12, #-1");                                    // prepare the runtime integer-key sentinel
        emitter.instruction("csel x11, x12, x11, eq");                          // preserve exact integer and binary string key identities
        emitter.instruction("stp x10, x11, [sp, #8]");                          // retain key bytes or integer payload across callbacks
        emitter.instruction("ldp x9, x10, [x3]");                               // borrow the capture value tag and payload
        emitter.instruction("ldr x11, [x3, #16]");                              // borrow the captured byte length
        emitter.instruction("stp x10, x11, [sp, #24]");                         // retain the borrowed value before acquiring ownership
        emitter.instruction("str x9, [sp, #40]");                               // retain the string or false value tag
        emitter.instruction("str xzr, [sp, #48]");                              // initialize this write's pending-exception flag
        emitter.instruction("mov x0, x1");                                      // pin exactly the array selected before any destructor
    } else {
        emitter.instruction("push rbp");                                        // preserve linkage and align the nested native calls
        emitter.instruction("mov rbp, rsp");                                    // establish the construction frame
        emitter.instruction("sub rsp, 112");                                    // reserve capture ownership and pending state
        emitter.instruction("mov QWORD PTR [rsp], rsi");                        // retain the selected stable hash header
        emitter.instruction("mov r10, QWORD PTR [rdx + 8]");                    // borrow the integer or binary key payload
        emitter.instruction("mov QWORD PTR [rsp + 8], r10");                    // retain the key across value persistence
        emitter.instruction("mov r10, QWORD PTR [rdx + 16]");                   // borrow the binary string key length
        emitter.instruction("mov r11, -1");                                     // prepare the runtime integer-key sentinel
        emitter.instruction("cmp QWORD PTR [rdx], 0");                          // recognize a normalized integer key
        emitter.instruction("cmove r10, r11");                                  // keep integer keys distinct from numeric strings
        emitter.instruction("mov QWORD PTR [rsp + 16], r10");                   // retain the normalized key high word
        emitter.instruction("mov r10, QWORD PTR [rcx + 8]");                    // borrow the capture value payload
        emitter.instruction("mov QWORD PTR [rsp + 24], r10");                   // retain the value before ownership acquisition
        emitter.instruction("mov r10, QWORD PTR [rcx + 16]");                   // borrow the captured byte length
        emitter.instruction("mov QWORD PTR [rsp + 32], r10");                   // retain the exact string length
        emitter.instruction("mov r10, QWORD PTR [rcx]");                        // borrow the string or false value tag
        emitter.instruction("mov QWORD PTR [rsp + 40], r10");                   // retain the per-entry tag across callbacks
        emitter.instruction("mov QWORD PTR [rsp + 48], 0");                     // initialize this write's pending-exception flag
        emitter.instruction("mov rdi, rsi");                                    // pin exactly the array selected before any destructor
    }
    abi::emit_call_label(emitter, "__rt_hash_pin");
    emitter.instruction(if arm { "ldr x9, [sp, #40]" } else { "mov r10, QWORD PTR [rsp + 40]" }); // inspect the captured value type
    emitter.instruction(if arm { "cmp x9, #1" } else { "cmp r10, 1" });         // only captured strings need new byte storage
    branch(emitter, "ne", "jne", "value_ready");
    persist(emitter, 24);
    emitter.label(&format!("{NAME}_value_ready"));
    emitter.label(&format!("{NAME}_value_lookup"));
    key_call(emitter, "__rt_hash_write_guard_claim");
    emitter.instruction(if arm { "str x0, [sp, #56]" } else { "mov QWORD PTR [rsp + 56], rax" }); // retain whether this entry has an ordinary owner to release
    lookup(emitter);
    emitter.instruction(if arm { "cmp x0, #0" } else { "test rax, rax" });      // recognize a key that has no previous value owner
    branch(emitter, "eq", "je", "insert");
    emitter.instruction(if arm { "ldr x9, [sp, #56]" } else { "mov r10, QWORD PTR [rsp + 56]" }); // recover the entry owner claim after lookup
    emitter.instruction(if arm { "cmp x9, #0" } else { "test r10, r10" });      // an outer release already owns a borrowed previous value
    branch(emitter, "eq", "je", "write");
    emitter.instruction(if arm { "cmp x3, #1" } else { "cmp rcx, 1" });         // recognize an overwritten string owner
    branch(emitter, "eq", "je", "release_any");
    emitter.instruction(if arm { "cmp x3, #8" } else { "cmp rcx, 8" });         // null owns no native allocation
    branch(emitter, "eq", "je", "write");
    emitter.instruction(if arm { "cmp x3, #10" } else { "cmp rcx, 10" });       // callable descriptors use their dedicated release convention
    branch(emitter, "eq", "je", "release_callable");
    emitter.instruction(if arm { "cmp x3, #4" } else { "cmp rcx, 4" });         // arrays, objects, and Mixed cells own heap storage
    branch(emitter, "lo", "jb", "write");
    emitter.label(&format!("{NAME}_release_any"));
    prepare_release(emitter, "__rt_decref_any");
    jump(emitter, "release_guard");
    emitter.label(&format!("{NAME}_release_callable"));
    prepare_release(emitter, "__rt_callable_descriptor_release");
    emitter.label(&format!("{NAME}_release_guard"));
    guarded_release(emitter);
    emitter.instruction(if arm { "cmp x0, #0" } else { "test rax, rax" });      // a changed key now holds a new owner or no entry
    branch(emitter, "ne", "jne", "value_lookup");
    emitter.label(&format!("{NAME}_relookup"));
    lookup(emitter);
    emitter.instruction(if arm { "cmp x0, #0" } else { "test rax, rax" });      // recover a key after a callback changed the entry allocation
    branch(emitter, "eq", "je", "insert");
    emitter.label(&format!("{NAME}_write"));
    if arm {
        emitter.instruction("ldp x9, x10, [sp, #24]");                          // recover the new owned capture payload
        emitter.instruction("stp x9, x10, [x4, #24]");                          // complete the selected array's current entry after destruction
        emitter.instruction("ldr x9, [sp, #40]");                               // recover the capture's concrete string or bool tag
        emitter.instruction("str x9, [x4, #40]");                               // publish the final value type without changing key identity
    } else {
        for offset in [24, 32, 40] {
            emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {offset}]")); // recover one owned capture word
            emitter.instruction(&format!("mov QWORD PTR [r8 + {offset}], r10")); // finish the selected entry without ordinary COW
        }
    }
    jump(emitter, "finish");
    emitter.label(&format!("{NAME}_insert"));
    insert(emitter);
    emitter.label(&format!("{NAME}_finish"));
    emitter.instruction(if arm { "ldr x0, [sp]" } else { "mov rdi, QWORD PTR [rsp]" }); // retire the selected array's internal lifetime root
    cleanup(emitter, "__rt_hash_unpin");
    if arm {
        emitter.instruction("ldr x0, [sp, #48]");                               // report whether any protected release left a throwable pending
        emitter.instruction("lsl x0, x0, #1");                                  // map the pending flag to the shared status value two
        emitter.instruction("ldp x29, x30, [sp, #112]");                        // restore caller linkage after all owners have transferred
        emitter.instruction("add sp, sp, #128");                                // release construction-local storage
    } else {
        emitter.instruction("mov rax, QWORD PTR [rsp + 48]");                   // report whether any protected release left a throwable pending
        emitter.instruction("shl eax, 1");                                      // map the pending flag to the shared status value two
        emitter.instruction("add rsp, 112");                                    // release construction-local storage
        emitter.instruction("pop rbp");                                         // restore caller linkage after every callback returned
    }
    emitter.instruction("ret");                                                 // return after completing the write even when destruction threw
}

/// Spills the old payload and its release operation before linking a borrowing scope.
fn prepare_release(emitter: &mut Emitter, symbol: &str) {
    let arm = emitter.target.arch == Arch::AArch64;
    emitter.instruction(if arm { "str x1, [sp, #96]" } else { "mov QWORD PTR [rsp + 96], rdi" }); // transfer the previous entry owner to this protected release
    abi::emit_symbol_address(emitter, if arm { "x9" } else { "r10" }, symbol);
    emitter.instruction(if arm { "str x9, [sp, #104]" } else { "mov QWORD PTR [rsp + 104], r10" }); // retain the type-specific release operation
}

/// Keeps an observable old entry borrowed while its owner is released behind the exception boundary.
fn guarded_release(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    emitter.instruction(if arm { "add x0, sp, #56" } else { "lea rdi, [rsp + 56]" }); // supply the caller-owned five-word scope record
    emitter.instruction(if arm { "ldr x1, [sp]" } else { "mov rsi, QWORD PTR [rsp]" }); // retain the selected stable array identity
    emitter.instruction(if arm { "ldr x2, [sp, #8]" } else { "mov rdx, QWORD PTR [rsp + 8]" }); // borrow the current normalized key payload
    emitter.instruction(if arm { "ldr x3, [sp, #16]" } else { "mov rcx, QWORD PTR [rsp + 16]" }); // retain integer identity or exact string length
    abi::emit_call_label(emitter, "__rt_hash_write_guard_push");
    emitter.instruction(if arm { "ldr x0, [sp, #104]" } else { "mov rdi, QWORD PTR [rsp + 104]" }); // recover the protected release operation
    emitter.instruction(if arm { "ldr x1, [sp, #96]" } else { "mov rsi, QWORD PTR [rsp + 96]" }); // release the previous owner exactly once
    emitter.instruction(if arm { "add x2, sp, #48" } else { "lea rdx, [rsp + 48]" }); // preserve pending state across recursive PHP callbacks
    abi::emit_call_label(emitter, "__rt_cleanup_call");
    emitter.instruction(if arm { "add x0, sp, #56" } else { "lea rdi, [rsp + 56]" }); // end this scope only after the protected release returned
    abi::emit_call_label(emitter, "__rt_hash_write_guard_pop");
}

/// Borrows the selected hash entry using stable key bytes from this write's frame.
fn lookup(emitter: &mut Emitter) {
    key_call(emitter, "__rt_hash_get");
}

/// Calls a nonmutating lookup or an ownership claim with the retained stable hash/key identity.
fn key_call(emitter: &mut Emitter, symbol: &str) {
    let arm = emitter.target.arch == Arch::AArch64;
    emitter.instruction(if arm { "ldr x0, [sp]" } else { "mov rdi, QWORD PTR [rsp]" }); // resolve the array selected for this write
    emitter.instruction(if arm { "ldr x1, [sp, #8]" } else { "mov rsi, QWORD PTR [rsp + 8]" }); // recover the exact key payload
    emitter.instruction(if arm { "ldr x2, [sp, #16]" } else { "mov rdx, QWORD PTR [rsp + 16]" }); // recover the normalized key high word
    abi::emit_call_label(emitter, symbol);
}

/// Persists a borrowed binary string pair and records the acquired native owner.
fn persist(emitter: &mut Emitter, offset: usize) {
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction(&format!("ldp x1, x2, [sp, #{offset}]"));           // borrow the exact binary byte range
    } else {
        emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {offset}]"));   // borrow the binary byte pointer
        emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", offset + 8)); // borrow the exact byte length
    }
    abi::emit_call_label(emitter, "__rt_str_persist");
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction(&format!("stp x1, x2, [sp, #{offset}]"));           // retain the acquired string owner until entry transfer
    } else {
        emitter.instruction(&format!("mov QWORD PTR [rsp + {offset}], rax"));   // retain the acquired string owner
        emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdx", offset + 8)); // retain its binary byte length
    }
}

/// Allocates room and transfers a new key/value pair without separating construction aliases.
fn insert(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    emitter.instruction(if arm { "ldr x0, [sp]" } else { "mov rdi, QWORD PTR [rsp]" }); // reload the stable destination header
    emitter.instruction(if arm { "ldr x9, [x0]" } else { "mov r10, QWORD PTR [rdi]" }); // read the current count after any destructor growth
    emitter.instruction(if arm { "lsl x9, x9, #2" } else { "shl r10, 2" });     // scale occupied slots for the load-factor comparison
    emitter.instruction(if arm { "ldr x10, [x0, #8]" } else { "mov r11, QWORD PTR [rdi + 8]" }); // read the current capacity
    emitter.instruction(if arm { "add x10, x10, x10, lsl #1" } else { "imul r11, r11, 3" }); // calculate the three-quarter capacity threshold
    emitter.instruction(if arm { "cmp x9, x10" } else { "cmp r10, r11" });      // test whether insertion needs more storage
    branch(emitter, "lo", "jb", "capacity_ready");
    abi::emit_call_label(emitter, "__rt_hash_grow_owned");
    emitter.label(&format!("{NAME}_capacity_ready"));
    emitter.instruction(if arm { "ldr x9, [sp, #16]" } else { "mov r10, QWORD PTR [rsp + 16]" }); // inspect normalized key identity
    emitter.instruction(if arm { "cmn x9, #1" } else { "cmp r10, -1" });        // integer keys need no byte owner
    branch(emitter, "eq", "je", "key_ready");
    persist(emitter, 8);
    emitter.label(&format!("{NAME}_key_ready"));
    for (offset, a, x) in [(0, "x0", "rdi"), (8, "x1", "rsi"), (16, "x2", "rdx"),
        (24, "x3", "rcx"), (32, "x4", "r8"), (40, "x5", "r9")] {
        let instruction = if arm { format!("ldr {a}, [sp, #{offset}]") }
            else { format!("mov {x}, QWORD PTR [rsp + {offset}]") };
        emitter.instruction(&instruction);                                      // transfer the selected hash and fully owned key/value pair
    }
    abi::emit_call_label(emitter, "__rt_hash_insert_owned");
}

/// Runs one potentially throwing release while preserving this write's pending flag.
fn cleanup(emitter: &mut Emitter, symbol: &str) {
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("mov x1, x0");                                      // pass the unary release value through the protected C boundary
        abi::emit_symbol_address(emitter, "x0", symbol);
        emitter.instruction("add x2, sp, #48");                                 // keep pending state in the construction frame
    } else {
        emitter.instruction("mov rsi, rdi");                                    // pass the selected hash through the cleanup callback's C input
        abi::emit_symbol_address(emitter, "rdi", symbol);
        emitter.instruction("lea rdx, [rsp + 48]");                             // keep pending state in the construction frame
    }
    abi::emit_call_label(emitter, "__rt_cleanup_call");
}

/// Branches to a local construction phase using the selected target's condition mnemonics.
fn branch(emitter: &mut Emitter, arm: &str, x86: &str, label: &str) {
    let instruction = if emitter.target.arch == Arch::AArch64 { format!("b.{arm}") } else { x86.to_owned() };
    emitter.instruction(&format!("{instruction} {NAME}_{label}"));              // select the next construction phase from the current comparison
}

/// Transfers control to the named phase without crossing another ownership boundary.
fn jump(emitter: &mut Emitter, label: &str) {
    let instruction = if emitter.target.arch == Arch::AArch64 { "b" } else { "jmp" };
    emitter.instruction(&format!("{instruction} {NAME}_{label}"));              // continue the same capture write with its retained owners
}
