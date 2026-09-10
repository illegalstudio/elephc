//! Purpose:
//! Emits cleanup callbacks and normal-path removal for owned-value exception guards.
//!
//! Called from:
//! - The common exception runtime emitter and typed EIR ownership calls.
//!
//! Key details:
//! - Guards use the existing three-word activation prefix followed by one heap owner.
//! - Removing any active guard preserves newer guards, including reordered named arguments.
//! - Cleanup clears the transferred owner before invoking potentially reentrant destruction.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::try_handlers::EXCEPTION_GUARD_OWNER_OFFSET;

/// Emits target-aware removal and cleanup entries for stack-local ownership guards.
pub(super) fn emit(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::AArch64 { aarch64(emitter); } else { x86_64(emitter); }
    super::emit_protected(emitter, "__rt_exception_release_owned", release_body);
    super::emit_protected(emitter, "__rt_exception_release_callable", release_callable_body);
}

/// Unlinks an AArch64 guard from the activation chain and transfers exceptional owners to the GC.
fn aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_exception_guard_owned");
    abi::emit_symbol_address(emitter, "x9", "_exc_call_frame_top");
    emitter.instruction("cmp x1, #0");                                          // zero insertion tokens start a new parameter-ordered group at the chain head
    emitter.instruction("csel x9, x9, x1, eq");                                 // otherwise append after the preceding parameter's guard
    emitter.instruction("ldr x10, [x9]");                                       // preserve the activation previously reached through this insertion link
    emitter.instruction("str x10, [x0]");                                       // connect the new record to its predecessor in unwind order
    emitter.instruction("str x0, [x0, #16]");                                   // pass this guard record as callback data during unwinding
    emitter.instruction("str x0, [x9]");                                        // publish the completed guard without disturbing newer parameters
    emitter.instruction("ret");                                                 // return the guard address as its normal-path removal token
    emitter.label_global("__rt_exception_unguard_owned");
    abi::emit_symbol_address(emitter, "x9", "_exc_call_frame_top");
    emitter.label("__rt_exception_unguard_owned_loop");
    emitter.instruction("ldr x10, [x9]");                                       // inspect the activation referenced by the current link
    emitter.instruction("cbz x10, __rt_exception_unguard_owned_done");          // stop defensively when this guard has already been removed
    emitter.instruction("cmp x10, x0");                                         // recognize the guard whose owner returns to normal EIR cleanup
    emitter.instruction("b.eq __rt_exception_unguard_owned_found");             // detach exactly this guard while preserving later registrations
    emitter.instruction("mov x9, x10");                                         // continue through the preceding-activation link
    emitter.instruction("b __rt_exception_unguard_owned_loop");                 // search older activations without touching their ownership
    emitter.label("__rt_exception_unguard_owned_found");
    emitter.instruction("ldr x10, [x0]");                                       // recover the guarded value's previous activation
    emitter.instruction("str x10, [x9]");                                       // connect the newer activation directly to its predecessor
    emitter.instruction(&format!("str xzr, [x0, #{EXCEPTION_GUARD_OWNER_OFFSET}]")); // clear exceptional ownership before ordinary EIR release
    emitter.label("__rt_exception_unguard_owned_done");
    emitter.instruction("ret");                                                 // leave value ownership unchanged on normal completion

}

/// Applies the identical guard-chain operations with SysV arguments and the native GC convention.
fn x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_exception_guard_owned");
    abi::emit_symbol_address(emitter, "r10", "_exc_call_frame_top");
    emitter.instruction("test rsi, rsi");                                       // distinguish a new call group from insertion after an earlier parameter
    emitter.instruction("cmovnz r10, rsi");                                     // preserve parameter-order cleanup independently of source evaluation order
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // retain the activation previously referenced by the insertion link
    emitter.instruction("mov QWORD PTR [rdi], r11");                            // connect the new guard to its next cleanup activation
    emitter.instruction("mov QWORD PTR [rdi + 16], rdi");                       // provide this record as the callback's opaque data
    emitter.instruction("mov QWORD PTR [r10], rdi");                            // publish the completed guard in PHP parameter order
    emitter.instruction("mov rax, rdi");                                        // return the stack-local guard token through the C result register
    emitter.instruction("ret");                                                 // retain transferred exceptional ownership until unguarding or unwinding
    emitter.label_global("__rt_exception_unguard_owned");
    abi::emit_symbol_address(emitter, "r10", "_exc_call_frame_top");
    emitter.label("__rt_exception_unguard_owned_loop");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // inspect the next activation without changing the guard token
    emitter.instruction("test r11, r11");                                       // recognize the end of the activation chain
    emitter.instruction("jz __rt_exception_unguard_owned_done");                // permit an already-removed guard without touching unrelated records
    emitter.instruction("cmp r11, rdi");                                        // identify the exact guard consumed by normal EIR cleanup
    emitter.instruction("je __rt_exception_unguard_owned_found");               // preserve newer registrations when arguments were reordered
    emitter.instruction("mov r10, r11");                                        // select this activation's previous-record link
    emitter.instruction("jmp __rt_exception_unguard_owned_loop");               // continue searching older activations
    emitter.label("__rt_exception_unguard_owned_found");
    emitter.instruction("mov r11, QWORD PTR [rdi]");                            // recover the activation that preceded the removed guard
    emitter.instruction("mov QWORD PTR [r10], r11");                            // splice out this guard without consuming its PHP value
    emitter.instruction(&format!("mov QWORD PTR [rdi + {EXCEPTION_GUARD_OWNER_OFFSET}], 0")); // return ownership exclusively to normal argument cleanup
    emitter.label("__rt_exception_unguard_owned_done");
    emitter.instruction("ret");                                                 // complete guard removal without retaining or releasing a heap value

}

/// Consumes the guard's owner while the shared protected callback restores handler and GC state.
fn release_body(emitter: &mut Emitter) {
    emit_release_body(emitter, "__rt_decref_any");
}

/// Releases callable descriptors and their captures through the dedicated ownership protocol.
fn release_callable_body(emitter: &mut Emitter) {
    emit_release_body(emitter, "__rt_callable_descriptor_release");
}

/// Clears transferred guard ownership before calling its type-specific native release helper.
fn emit_release_body(emitter: &mut Emitter, symbol: &str) {
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("ldr x9, [sp, #224]");                              // recover the cleanup record passed as the first C argument
        emitter.instruction(&format!("ldr x0, [x9, #{EXCEPTION_GUARD_OWNER_OFFSET}]")); // transfer the guarded owner to the native GC convention
        emitter.instruction(&format!("str xzr, [x9, #{EXCEPTION_GUARD_OWNER_OFFSET}]")); // clear ownership before a destructor reenters PHP
    } else {
        emitter.instruction("mov r10, QWORD PTR [rsp + 224]");                  // recover the callback's guard-record argument
        emitter.instruction(&format!("mov rax, QWORD PTR [r10 + {EXCEPTION_GUARD_OWNER_OFFSET}]")); // pass the owned heap value through the native GC convention
        emitter.instruction(&format!("mov QWORD PTR [r10 + {EXCEPTION_GUARD_OWNER_OFFSET}], 0")); // prevent duplicate release during nested exception propagation
    }
    abi::emit_call_label(emitter, symbol);
}
