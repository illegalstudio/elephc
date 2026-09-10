//! Purpose:
//! Tracks hash entry owners transferred to a protected construction-time release.
//!
//! Called from:
//! - Managed runtime emission, capture construction, and hash set/unset/conversion helpers.
//!
//! Key details:
//! - Records borrow the stable hash and key while the releasing caller retains their lifetime.
//! - A matching mutation invalidates the record and skips the already-transferred old owner.
//! - Ownership inspection leaves records unchanged when an existing Mixed slot needs no rewrite.
//! - Records are caller-owned; unlinking tolerates a newer guard above the completed scope.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

const TOP: &str = "_hash_write_guard_top";

/// Emits five-word caller records, read-only ownership inspection, and claims for normalized hash keys.
pub fn emit_hash_write_guards(emitter: &mut Emitter) {
    push_pop(emitter);
    claim(emitter);
}

/// Links records containing next/hash/key-low/key-high/changed and unlinks an exact record.
fn push_pop(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    emitter.label_global("__rt_hash_write_guard_push");
    if arm {
        abi::emit_load_symbol_to_reg(emitter, "x9", TOP, 0);
        emitter.instruction("stp x9, x1, [x0]");                                // retain the preceding guard and selected hash identity
        emitter.instruction("stp x2, x3, [x0, #16]");                           // borrow the selected normalized key for callback duration
        emitter.instruction("str xzr, [x0, #32]");                              // mark the current entry as borrowing its transferred owner
        abi::emit_store_reg_to_symbol(emitter, "x0", TOP, 0);
    } else {
        abi::emit_load_symbol_to_reg(emitter, "r10", TOP, 0);
        emitter.instruction("mov QWORD PTR [rdi], r10");                        // retain the preceding active guard
        emitter.instruction("mov QWORD PTR [rdi + 8], rsi");                    // retain the selected stable hash identity
        emitter.instruction("mov QWORD PTR [rdi + 16], rdx");                   // borrow the key payload until the callback returns
        emitter.instruction("mov QWORD PTR [rdi + 24], rcx");                   // retain integer identity or binary string length
        emitter.instruction("mov QWORD PTR [rdi + 32], 0");                     // mark the entry's old owner as already transferred
        abi::emit_store_reg_to_symbol(emitter, "rdi", TOP, 0);
    }
    emitter.instruction("ret");                                                 // leave ownership with the releasing caller
    emitter.label_global("__rt_hash_write_guard_pop");
    abi::emit_symbol_address(emitter, if arm { "x9" } else { "r10" }, TOP);
    emitter.label("__rt_hash_write_guard_pop_loop");
    emitter.instruction(if arm { "ldr x10, [x9]" } else { "mov r11, QWORD PTR [r10]" }); // inspect the record reached through the current link
    emitter.instruction(if arm { "cmp x10, #0" } else { "test r11, r11" });     // recognize an already-unlinked record
    emitter.instruction(if arm { "b.eq __rt_hash_write_guard_pop_done" } else { "je __rt_hash_write_guard_pop_done" }); // preserve newer guards when this scope is absent
    emitter.instruction(if arm { "cmp x10, x0" } else { "cmp r11, rdi" });      // find the exact completing construction scope
    emitter.instruction(if arm { "b.eq __rt_hash_write_guard_pop_found" } else { "je __rt_hash_write_guard_pop_found" }); // unlink only the requested record
    emitter.instruction(if arm { "mov x9, x10" } else { "mov r10, r11" });      // follow the preceding-record field as the next link
    emitter.instruction(if arm { "b __rt_hash_write_guard_pop_loop" } else { "jmp __rt_hash_write_guard_pop_loop" }); // scan older scopes without changing their state
    emitter.label("__rt_hash_write_guard_pop_found");
    emitter.instruction(if arm { "ldr x10, [x0]" } else { "mov r11, QWORD PTR [rdi]" }); // recover the completing scope's predecessor
    emitter.instruction(if arm { "str x10, [x9]" } else { "mov QWORD PTR [r10], r11" }); // preserve the chain around the removed record
    emitter.label("__rt_hash_write_guard_pop_done");
    emitter.instruction(if arm { "ldr x0, [x0, #32]" } else { "mov rax, QWORD PTR [rdi + 32]" }); // report whether a callback changed ownership of this key
    emitter.instruction("ret");                                                 // let the caller either finish or consume a newly installed owner
}

/// Inspects or claims an entry owner, returning zero when protected release already owns the payload.
fn claim(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    emitter.label_global("__rt_hash_write_guard_claim");
    emitter.instruction(if arm { "mov x12, #1" } else { "mov r9d, 1" });        // mark matching records after inspecting their previous ownership
    emitter.instruction(if arm { "b __rt_hash_write_guard_inspect" } else { "jmp __rt_hash_write_guard_inspect" }); // share key matching between claims and read-only inspections
    emitter.label_global("__rt_hash_write_guard_owns");
    emitter.instruction(if arm { "mov x12, #0" } else { "xor r9d, r9d" });      // inspect ownership without invalidating an unchanged slot
    emitter.label("__rt_hash_write_guard_inspect");
    abi::emit_load_symbol_to_reg(emitter, if arm { "x9" } else { "r10" }, TOP, 0);
    emitter.instruction(if arm { "cmp x9, #0" } else { "test r10, r10" });      // ordinary writes usually have no active construction scopes
    emitter.instruction(if arm { "b.eq __rt_hash_write_guard_claim_ordinary" } else { "je __rt_hash_write_guard_claim_ordinary" }); // return an ordinary owner without allocating a frame
    if arm {
        emitter.instruction("sub sp, sp, #64");                                 // reserve key identity, iteration state, and linkage
        emitter.instruction("stp x29, x30, [sp, #48]");                         // preserve the mutator across key equality calls
        emitter.instruction("add x29, sp, #48");                                // establish an aligned guard scan frame
        emitter.instruction("stp x0, x1, [sp]");                                // retain hash identity and incoming key payload
        emitter.instruction("stp x2, x9, [sp, #16]");                           // retain normalized key length and the first record
        emitter.instruction("mov x10, #1");                                     // start with the assumption that the entry owns its value
        emitter.instruction("str x10, [sp, #32]");                              // retain ownership across each matching scope
        emitter.instruction("str x12, [sp, #40]");                              // preserve whether this lookup also claims matching owners
    } else {
        emitter.instruction("push rbp");                                        // preserve linkage and align key-comparison calls
        emitter.instruction("mov rbp, rsp");                                    // establish a stable guard scan frame
        emitter.instruction("sub rsp, 48");                                     // retain the input identity and scan state
        emitter.instruction("mov QWORD PTR [rsp], rdi");                        // retain the mutating hash identity
        emitter.instruction("mov QWORD PTR [rsp + 8], rsi");                    // retain the incoming normalized key payload
        emitter.instruction("mov QWORD PTR [rsp + 16], rdx");                   // retain the key high word
        emitter.instruction("mov QWORD PTR [rsp + 24], r10");                   // start with the newest active scope
        emitter.instruction("mov QWORD PTR [rsp + 32], 1");                     // assume ordinary ownership until a borrowing scope matches
        emitter.instruction("mov QWORD PTR [rsp + 40], r9");                    // preserve whether this lookup also claims matching owners
    }
    emitter.label("__rt_hash_write_guard_claim_loop");
    emitter.instruction(if arm { "ldr x9, [sp, #24]" } else { "mov r10, QWORD PTR [rsp + 24]" }); // recover the current scope after any comparison
    emitter.instruction(if arm { "cmp x9, #0" } else { "test r10, r10" });      // recognize the end of active scopes
    emitter.instruction(if arm { "b.eq __rt_hash_write_guard_claim_done" } else { "je __rt_hash_write_guard_claim_done" }); // return the accumulated ownership decision
    emitter.instruction(if arm { "ldr x10, [x9, #8]" } else { "mov r11, QWORD PTR [r10 + 8]" }); // read the scope's stable hash identity
    emitter.instruction(if arm { "ldr x11, [sp]" } else { "cmp r11, QWORD PTR [rsp]" }); // compare the actual mutating container with the scoped container
    if arm { emitter.instruction("cmp x10, x11"); }                             // distinguish a COW clone from the original scoped hash
    emitter.instruction(if arm { "b.ne __rt_hash_write_guard_claim_next" } else { "jne __rt_hash_write_guard_claim_next" }); // a different container has independent entry ownership
    if arm {
        emitter.instruction("ldp x1, x2, [sp, #8]");                            // pass the mutator's normalized key
        emitter.instruction("ldp x3, x4, [x9, #16]");                           // pass the scoped key without copying bytes
    } else {
        emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");                    // pass the mutator's key payload
        emitter.instruction("mov rsi, QWORD PTR [rsp + 16]");                   // pass its normalized high word
        emitter.instruction("mov rdx, QWORD PTR [r10 + 16]");                   // borrow the scoped key payload
        emitter.instruction("mov rcx, QWORD PTR [r10 + 24]");                   // borrow the scoped normalized high word
    }
    abi::emit_call_label(emitter, "__rt_hash_key_eq");
    emitter.instruction(if arm { "cmp x0, #0" } else { "test rax, rax" });      // recognize an unrelated key in the same hash
    emitter.instruction(if arm { "b.eq __rt_hash_write_guard_claim_next" } else { "je __rt_hash_write_guard_claim_next" }); // leave other entries' ownership unchanged
    if arm {
        emitter.instruction("ldr x9, [sp, #24]");                               // recover the matching scope after equality
        emitter.instruction("ldr x10, [x9, #32]");                              // zero means its current value is already being released
        emitter.instruction("ldr x11, [sp, #32]");                              // recover the accumulated ownership decision
        emitter.instruction("and x11, x11, x10");                               // reject a second release of any borrowed old owner
        emitter.instruction("str x11, [sp, #32]");                              // retain that decision while scanning outer scopes
        emitter.instruction("ldr x12, [sp, #40]");                              // read whether ownership inspection should mutate the record
        emitter.instruction("cbz x12, __rt_hash_write_guard_claim_next");       // leave ownership unchanged during a read-only probe
        emitter.instruction("mov x10, #1");                                     // subsequent writes own whatever this mutation publishes
        emitter.instruction("str x10, [x9, #32]");                              // tell the releasing caller to inspect the new entry owner
    } else {
        emitter.instruction("mov r10, QWORD PTR [rsp + 24]");                   // recover the matching scope after equality
        emitter.instruction("mov r11, QWORD PTR [r10 + 32]");                   // zero identifies an already-transferred old owner
        emitter.instruction("and QWORD PTR [rsp + 32], r11");                   // suppress duplicate release of that borrowed value
        emitter.instruction("cmp QWORD PTR [rsp + 40], 0");                     // read whether ownership inspection should mutate the record
        emitter.instruction("je __rt_hash_write_guard_claim_next");             // leave ownership unchanged during a read-only probe
        emitter.instruction("mov QWORD PTR [r10 + 32], 1");                     // notify the releasing caller that this key is being replaced or removed
    }
    emitter.label("__rt_hash_write_guard_claim_next");
    emitter.instruction(if arm { "ldr x9, [sp, #24]" } else { "mov r10, QWORD PTR [rsp + 24]" }); // recover the current record without retaining comparison scratch registers
    emitter.instruction(if arm { "ldr x9, [x9]" } else { "mov r10, QWORD PTR [r10]" }); // follow the preceding active scope
    emitter.instruction(if arm { "str x9, [sp, #24]" } else { "mov QWORD PTR [rsp + 24], r10" }); // retain the next record across comparison
    emitter.instruction(if arm { "b __rt_hash_write_guard_claim_loop" } else { "jmp __rt_hash_write_guard_claim_loop" }); // consider every matching outer scope
    emitter.label("__rt_hash_write_guard_claim_done");
    if arm {
        emitter.instruction("ldr x0, [sp, #32]");                               // return whether this mutation owns the previous value
        emitter.instruction("ldp x29, x30, [sp, #48]");                         // restore caller linkage after the scan
        emitter.instruction("add sp, sp, #64");                                 // release local scan state
    } else {
        emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                   // return whether an ordinary previous owner remains
        emitter.instruction("add rsp, 48");                                     // release local scan state
        emitter.instruction("pop rbp");                                         // restore caller linkage
    }
    emitter.instruction("ret");                                                 // return the owner-claim result
    emitter.label("__rt_hash_write_guard_claim_ordinary");
    emitter.instruction(if arm { "mov x0, #1" } else { "mov eax, 1" });         // report ordinary ownership when no scope is active
    emitter.instruction("ret");                                                 // avoid key comparisons on the ordinary mutation path
}
