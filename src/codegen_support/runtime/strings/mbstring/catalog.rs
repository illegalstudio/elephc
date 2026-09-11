//! Purpose:
//! Owns the request-local native encoding catalog and its identity-preserving array returns.
//!
//! Called from:
//! - V2 mbstring result materialization, request entry, and native/web cleanup.
//!
//! Key details:
//! - The cache owns one reference; every returned array owns another and uses ordinary COW.
//! - Its root reference keeps cached strings reachable during cycle collection.
//! - Cleanup drains deferred capture owners, then clears catalog and string metadata before web arena reset.

use super::*;

/// Emits catalog materialization, cache/identity release, and combined engine/native request reset.
pub(super) fn emit(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    let result = if arm { "x0" } else { "rax" };
    let cached = if arm { "x9" } else { "r10" };
    emitter.label_global("__rt_mbstring_catalog");
    enter(emitter);
    abi::emit_load_symbol_to_reg(emitter, cached, "_mbstring_catalog_array", 0);
    if arm {
        emitter.instruction("cbnz x9, __rt_mbstring_catalog_cached");           // reuse the same payload identity for an initialized request
    } else {
        emitter.instruction("test r10, r10");                                   // test the current request's optional cached array
        emitter.instruction("jnz __rt_mbstring_catalog_cached");                // retain the existing catalog without materializing a duplicate
    }
    abi::emit_call_label(emitter, "__rt_mbstring_string_array");
    if arm {
        emitter.instruction("cbz x0, __rt_mbstring_catalog_done");              // preserve failure without publishing a partial array
    } else {
        emitter.instruction("test rax, rax");                                   // check whether packed catalog materialization succeeded
        emitter.instruction("jz __rt_mbstring_catalog_done");                   // keep failed arrays out of request state
    }
    abi::emit_store_reg_to_symbol(emitter, result, "_mbstring_catalog_array", 0);
    emitter.instruction(if arm { "b __rt_mbstring_catalog_retain" } else { "jmp __rt_mbstring_catalog_retain" }); // reserve the freshly acquired owner for the cache
    emitter.label("__rt_mbstring_catalog_cached");
    emitter.instruction(&format!("mov {result}, {cached}"));                    // return the exact cached payload identity
    emitter.label("__rt_mbstring_catalog_retain");
    emitter.instruction(if arm { "str x0, [sp, #16]" } else { "mov QWORD PTR [rsp], rax" }); // preserve the array pointer across ownership acquisition
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.instruction(if arm { "ldr x0, [sp, #16]" } else { "mov rax, QWORD PTR [rsp]" }); // transfer an independent caller reference
    emitter.label("__rt_mbstring_catalog_done");
    leave(emitter);

    emitter.label_global("__rt_mbstring_release_catalog");
    enter(emitter);
    abi::emit_call_label(emitter, "__rt_mbstring_release_deferred_captures");
    emitter.instruction(if arm { "str x0, [sp, #16]" } else { "mov QWORD PTR [rsp], rax" }); // retain pending cleanup failure while retiring native caches
    abi::emit_load_symbol_to_reg(emitter, result, "_mbstring_catalog_array", 0);
    abi::emit_store_zero_to_symbol(emitter, "_mbstring_catalog_array", 0);
    abi::emit_call_label(emitter, "__rt_decref_any");
    abi::emit_call_label(emitter, "__rt_mbstring_ini_reset");
    emitter.instruction(if arm { "ldr x0, [sp, #16]" } else { "mov rax, QWORD PTR [rsp]" }); // recover whether a deferred destructor left an exception
    if arm {
        emitter.instruction("ldp x29, x30, [sp], #32");                         // restore linkage before propagating request-cleanup failure
        emitter.instruction("cbz x0, __rt_mbstring_release_catalog_done");      // keep conditional relocation local for every Apple target
        emitter.instruction("b __rt_throw_current");                            // throw only after queued values and metadata owners were consumed
    } else {
        emitter.instruction("leave");                                           // restore the lifecycle caller before possible exception propagation
        emitter.instruction("test rax, rax");                                   // inspect the protected drain's pending flag
        emitter.instruction("jnz __rt_throw_current");                          // propagate failure after all remaining catalog cleanup
    }
    emitter.label("__rt_mbstring_release_catalog_done");
    emitter.instruction("ret");                                                 // return after complete request-owned value cleanup

    emitter.label_global("__rt_mbstring_request_reset");
    enter(emitter);
    abi::emit_call_label(emitter, "__rt_mbstring_release_catalog");
    emitter.bl_c("elephc_mbstring_reset_v1");
    leave(emitter);
}

/// Reserves an aligned frame with one preserved result slot for both native architectures.
fn enter(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("stp x29, x30, [sp, #-32]!");                       // preserve linkage and reserve a cache-result slot
        emitter.instruction("mov x29, sp");                                     // establish the aligned catalog helper frame
    } else {
        emitter.instruction("push rbp");                                        // preserve caller linkage and align helper calls
        emitter.instruction("mov rbp, rsp");                                    // establish a stable catalog helper frame
        emitter.instruction("sub rsp, 16");                                     // reserve the preserved array pointer and padding
    }
}

/// Restores native linkage without changing the owned result pointer.
fn leave(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("ldp x29, x30, [sp], #32");                         // release local ownership storage and restore linkage
    } else {
        emitter.instruction("leave");                                           // release local storage and restore the caller frame
    }
    emitter.instruction("ret");                                                 // return after all native ownership work has completed
}
