//! Purpose:
//! Adapts native query roots, append probes, and cursor release to the shared storage ABI.
//!
//! Called from:
//! - The native query registration callback table.
//!
//! Key details:
//! - A writer is a borrowed persistent reference, resolved afresh for each root acquisition.
//! - Non-array values are ignored; unsupported shared indexed promotion returns fatal status.
//! - Cursor release contains destructor exceptions before returning to Rust.

use super::{abi, Arch, Emitter};
use crate::codegen_support::sentinels;

/// Emits the three native adapters not already shared with capture and nested query storage.
pub(super) fn emit(emitter: &mut Emitter) {
    root(emitter);
    next(emitter);
    release(emitter);
}

/// Acquires one hash pin from a live reference, or returns an empty cursor for a non-array value.
fn root(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    emitter.label_global("__rt_mbstring_query_root");
    if arm {
        emitter.instruction("str xzr, [x2]");                                   // publish no cursor until a valid managed hash is selected
        emitter.instruction("cbz x1, __rt_mbstring_query_root_invalid");        // require a borrowed persistent writer reference
        emitter.instruction("ldr x9, [x1]");                                    // inspect the writer's boxed representation
        emitter.instruction("cmp x9, #7");                                      // require a nested reference cell
        emitter.instruction("b.ne __rt_mbstring_query_root_invalid");           // reject ordinary values as writer handles
        emitter.instruction("ldr x9, [x1, #16]");                               // inspect the persistent reference discriminator
        emitter.instruction("cmp x9, #1");                                      // distinguish PHP references from ordinary Mixed boxes
        emitter.instruction("b.ne __rt_mbstring_query_root_invalid");           // leave detached boxes untouched
        emitter.instruction("sub sp, sp, #32");                                 // preserve the cursor output and linkage across native helpers
        emitter.instruction("stp x29, x30, [sp, #16]");                         // retain caller linkage during resolution and pinning
        emitter.instruction("str x2, [sp]");                                    // retain the shared executor's cursor output
        emitter.instruction("mov x0, x1");                                      // resolve the writer's current value without copying it
    } else {
        emitter.instruction("mov QWORD PTR [rdx], 0");                          // publish no cursor before managed hash selection
        emitter.instruction("test rsi, rsi");                                   // require a borrowed writer handle
        emitter.instruction("jz __rt_mbstring_query_root_invalid");             // reject a missing persistent reference
        emitter.instruction("cmp QWORD PTR [rsi], 7");                          // require a nested reference cell
        emitter.instruction("jne __rt_mbstring_query_root_invalid");            // ordinary values cannot identify writable storage
        emitter.instruction("cmp QWORD PTR [rsi + 16], 1");                     // require the persistent reference discriminator
        emitter.instruction("jne __rt_mbstring_query_root_invalid");            // reject detached Mixed boxes without mutation
        emitter.instruction("sub rsp, 24");                                     // align native calls and preserve the cursor output
        emitter.instruction("mov QWORD PTR [rsp], rdx");                        // retain the executor's cursor output across helper calls
        emitter.instruction("mov rax, rsi");                                    // adapt the live writer to the private Mixed dereference ABI
    }
    abi::emit_call_label(emitter, "__rt_mixed_deref");
    emitter.instruction(if arm { "cmp x0, #0" } else { "test rax, rax" });      // a null terminal value produces no query writes
    branch(emitter, "eq", "je", "empty");
    emitter.instruction(if arm { "ldr x9, [x0]" } else { "mov r10, QWORD PTR [rax]" }); // inspect the current terminal representation
    emitter.instruction(if arm { "cmp x9, #4" } else { "cmp r10, 4" });         // indexed arrays may need in-place representation promotion
    branch(emitter, "eq", "je", "array");
    emitter.instruction(if arm { "cmp x9, #5" } else { "cmp r10, 5" });         // hashes already provide stable native query storage
    branch(emitter, "ne", "jne", "empty");
    emitter.label("__rt_mbstring_query_root_array");
    abi::emit_call_label(emitter, "__rt_mbstring_capture_destination");
    sentinels::emit_branch_if_null_container(emitter, if arm { "x0" } else { "rax" },
        if arm { "x9" } else { "r10" }, "__rt_mbstring_query_root_failed");
    if !arm { emitter.instruction("mov rdi, rax"); }                            // adapt the selected managed hash to the C pin ABI
    abi::emit_call_label(emitter, "__rt_hash_pin");
    if arm {
        emitter.instruction("ldr x9, [sp]");                                    // recover the independently published cursor slot
        emitter.instruction("str x0, [x9]");                                    // transfer one lifetime pin to the shared executor
    } else {
        emitter.instruction("mov r10, QWORD PTR [rsp]");                        // recover the cursor metadata destination
        emitter.instruction("mov QWORD PTR [r10], rax");                        // publish the protected hash without creating a PHP copy
    }
    emitter.label("__rt_mbstring_query_root_empty");
    emitter.instruction(if arm { "mov x0, #0" } else { "xor eax, eax" });       // return success for a pinned array or an ignored non-array
    emitter.instruction(if arm { "b __rt_mbstring_query_root_done" } else { "jmp __rt_mbstring_query_root_done" }); // share frame retirement without touching the cursor
    emitter.label("__rt_mbstring_query_root_failed");
    emitter.instruction(if arm { "mov x0, #1" } else { "mov eax, 1" });         // report unsupported promotion without pretending the field was applied
    emitter.label("__rt_mbstring_query_root_done");
    if arm {
        emitter.instruction("ldp x29, x30, [sp, #16]");                         // restore linkage after selection or unchanged failure
        emitter.instruction("add sp, sp, #32");                                 // retire the borrowed output slot
    } else {
        emitter.instruction("add rsp, 24");                                     // restore caller alignment without altering status
    }
    emitter.instruction("ret");                                                 // return with exactly the pin published in the output slot
    emitter.label("__rt_mbstring_query_root_invalid");
    emitter.instruction(if arm { "mov x0, #1" } else { "mov eax, 1" });         // reject malformed writer markers before allocating a frame
    emitter.instruction("ret");                                                 // leave the already initialized cursor empty
}

/// Adapts the non-mutating native append probe to the shared available/index output record.
fn next(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    emitter.label_global("__rt_mbstring_query_next");
    if arm {
        emitter.instruction("sub sp, sp, #32");                                 // preserve the append-result output and linkage
        emitter.instruction("stp x29, x30, [sp, #16]");                         // retain the caller across saturated-index lookup
        emitter.instruction("str x2, [sp]");                                    // keep the executor's append-result storage
        emitter.instruction("mov x0, x1");                                      // supply the current pinned hash to the append probe
    } else {
        emitter.instruction("sub rsp, 24");                                     // align helper calls while retaining result storage
        emitter.instruction("mov QWORD PTR [rsp], rdx");                        // preserve the shared append-result output
        emitter.instruction("mov rdi, rsi");                                    // supply the pinned cursor through the C argument register
    }
    abi::emit_call_label(emitter, "__rt_hash_try_next_index");
    if arm {
        emitter.instruction("ldr x9, [sp]");                                    // recover the shared output after the index probe
        emitter.instruction("stp x1, x0, [x9]");                                // publish availability followed by the signed next index
        emitter.instruction("mov x0, #0");                                      // append exhaustion is metadata rather than a callback failure
        emitter.instruction("ldp x29, x30, [sp, #16]");                         // restore caller linkage without mutating the hash
        emitter.instruction("add sp, sp, #32");                                 // release the output-preservation frame
    } else {
        emitter.instruction("mov r10, QWORD PTR [rsp]");                        // recover the executor's append-result storage
        emitter.instruction("mov QWORD PTR [r10], rdx");                        // publish whether the next index can be used
        emitter.instruction("mov QWORD PTR [r10 + 8], rax");                    // preserve the complete signed native index
        emitter.instruction("xor eax, eax");                                    // exhaustion still represents a successful non-mutating probe
        emitter.instruction("add rsp, 24");                                     // restore caller alignment after output publication
    }
    emitter.instruction("ret");                                                 // return the completed append availability result
}

/// Retires a cursor pin inside the protected PHP cleanup boundary and returns any pending status.
fn release(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    emitter.label_global("__rt_mbstring_query_release");
    if arm {
        emitter.instruction("sub sp, sp, #32");                                 // reserve pending state and caller linkage
        emitter.instruction("stp x29, x30, [sp, #16]");                         // preserve linkage if final unpin invokes PHP destructors
        emitter.instruction("str xzr, [sp]");                                   // begin without a new pending throwable
        emitter.instruction("mov x2, sp");                                      // provide writable pending metadata beside the cursor in x1
    } else {
        emitter.instruction("sub rsp, 24");                                     // align protected cleanup while preserving pending metadata
        emitter.instruction("mov QWORD PTR [rsp], 0");                          // initialize the cleanup status independently of ambient exceptions
        emitter.instruction("mov rdx, rsp");                                    // supply pending metadata beside the cursor in rsi
    }
    abi::emit_symbol_address(emitter, if arm { "x0" } else { "rdi" }, "__rt_hash_unpin");
    abi::emit_call_label(emitter, "__rt_cleanup_call");
    if arm {
        emitter.instruction("ldr x0, [sp]");                                    // preserve exceptions contained during final cursor release
        emitter.instruction("lsl x0, x0, #1");                                  // encode the shared callback's pending status two
        emitter.instruction("ldp x29, x30, [sp, #16]");                         // restore linkage after all native destruction has completed
        emitter.instruction("add sp, sp, #32");                                 // retire pending metadata after transferring the status
    } else {
        emitter.instruction("mov rax, QWORD PTR [rsp]");                        // load the protected cleanup result
        emitter.instruction("shl eax, 1");                                      // encode pending status two without unwinding through Rust
        emitter.instruction("add rsp, 24");                                     // restore caller alignment after protected release
    }
    emitter.instruction("ret");                                                 // return only after the cursor pin has been consumed
}

/// Branches to one root-selection phase using the active architecture's condition mnemonic.
fn branch(emitter: &mut Emitter, arm: &str, x86: &str, phase: &str) {
    let op = if emitter.target.arch == Arch::AArch64 { format!("b.{arm}") } else { x86.to_owned() };
    emitter.instruction(&format!("{op} __rt_mbstring_query_root_{phase}"));     // preserve root classification before promotion or pin publication
}
