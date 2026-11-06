//! Purpose:
//! Proves that one typed indexed-array release owns no PHP-visible child cleanup.
//!
//! Called from:
//! - super::emit_cleanup_invoke() after scalar callback authentication rejects an entry.
//!
//! Key details:
//! - Only the exact __rt_decref_array callback and a live managed array qualify.
//! - Logical length zero excludes child releases regardless of reserved capacity.
//! - Hash-promoted storage, nonempty arrays and other callbacks retain protection.
//! - The shared direct stub invokes the original callback and preserves GC semantics.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

pub(super) const CANDIDATE: &str = "__rt_cleanup_invoke_empty_array_candidate";

/// Emits a separate empty-array proof without changing the scalar or protected body.
pub(super) fn emit_candidate(emitter: &mut Emitter, protected: &str, direct: &str) {
    emitter.label(CANDIDATE);
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter, protected, direct),
        Arch::X86_64 => emit_x86(emitter, protected, direct),
    }
}

/// Preserves AAPCS64 inputs while authenticating an empty indexed array.
fn emit_aarch64(emitter: &mut Emitter, protected: &str, direct: &str) {
    // -- authenticate the typed array callback before borrowing a header --
    abi::emit_symbol_address(emitter, "x9", "__rt_decref_array");
    emitter.instruction("cmp x0, x9");                                          // only the original typed indexed-array dispatcher has this proof
    emitter.instruction(&format!("b.ne {protected}"));                          // arbitrary callbacks keep the protected path
    abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("add x10, x9, #16");                                    // the first payload follows the uniform heap header
    emitter.instruction("cmp x1, x10");                                         // reject null, literals, sentinels and foreign payloads
    emitter.instruction(&format!("b.lo {protected}"));                          // do not read a header below the managed payload range
    abi::emit_symbol_address(emitter, "x10", "_heap_off");
    emitter.instruction("ldr x10, [x10]");                                      // preserve the heap base in x9 while loading its bump offset
    emitter.instruction("add x10, x9, x10");                                    // derive the allocated heap end
    emitter.instruction("cmp x1, x10");                                         // require an address within managed storage
    emitter.instruction(&format!("b.hs {protected}"));                          // out-of-range values retain existing behavior
    emitter.instruction("ldr w10, [x1, #-12]");                                 // inspect the actual owner count without changing it
    emitter.instruction(&format!("cbz w10, {protected}"));                      // freed or collector-doomed storage is not admitted
    emitter.instruction("ldr w10, [x1, #-16]");                                 // borrow the allocated payload byte count
    emitter.instruction("cmp w10, #24");                                        // indexed arrays own a three-word header
    emitter.instruction(&format!("b.lo {protected}"));                          // short storage cannot qualify as an array
    emitter.instruction("ldr x10, [x1, #-8]");                                  // retain all storage and collector flags in the actual header
    emitter.instruction("and x10, x10, #0xff");                                 // isolate the concrete storage kind
    emitter.instruction("cmp x10, #2");                                         // only indexed arrays use this length layout
    emitter.instruction(&format!("b.ne {protected}"));                          // promoted hashes and all other kinds remain protected
    emitter.instruction("ldr x10, [x1]");                                       // length counts owned children; capacity may already reserve four slots
    emitter.instruction(&format!("cbnz x10, {protected}"));                     // any logical element may execute PHP cleanup
    emitter.instruction(&format!("b {direct}"));                                // invoke the authenticated dispatcher with its original GC behavior
}

/// Preserves System V inputs while authenticating an empty indexed array.
fn emit_x86(emitter: &mut Emitter, protected: &str, direct: &str) {
    // -- validate the exact typed callback and managed header --
    abi::emit_symbol_address(emitter, "r9", "__rt_decref_array");
    emitter.instruction("cmp rdi, r9");                                         // only the typed indexed-array dispatcher qualifies here
    emitter.instruction(&format!("jne {protected}"));                           // other callbacks retain their existing handler
    abi::emit_symbol_address(emitter, "r9", "_heap_buf");
    emitter.instruction("lea r10, [r9 + 16]");                                  // locate the first valid payload after the uniform header
    emitter.instruction("cmp rsi, r10");                                        // reject null, static and sentinel payloads
    emitter.instruction(&format!("jb {protected}"));                            // leave foreign values on the protected path
    abi::emit_load_symbol_to_reg(emitter, "r10", "_heap_off", 0);
    emitter.instruction("add r10, r9");                                         // derive the allocated heap end
    emitter.instruction("cmp rsi, r10");                                        // validate the payload before borrowing its header
    emitter.instruction(&format!("jae {protected}"));                           // out-of-range storage cannot qualify
    emitter.instruction("cmp DWORD PTR [rsi - 12], 0");                         // require a live real owner
    emitter.instruction(&format!("je {protected}"));                            // freed and collector-doomed blocks remain protected
    emitter.instruction("cmp DWORD PTR [rsi - 16], 24");                        // require the complete indexed-array header
    emitter.instruction(&format!("jb {protected}"));                            // reject malformed short storage
    emitter.instruction("mov r10, QWORD PTR [rsi - 8]");                        // preserve the full stamped kind word for validation
    emitter.instruction("mov r11, r10");                                        // retain the low kind bits while authenticating the marker
    emitter.instruction("shr r11, 32");                                         // isolate the x86 managed-heap marker
    emitter.instruction(&format!("cmp r11, {}", crate::codegen_support::sentinels::X86_64_HEAP_MAGIC_HI32)); // reject unstamped headers
    emitter.instruction(&format!("jne {protected}"));                           // only managed indexed arrays can qualify
    emitter.instruction("and r10d, 0xff");                                      // ignore transient flags and cached incoming counts
    emitter.instruction("cmp r10d, 2");                                         // require actual indexed storage rather than a promoted hash
    emitter.instruction(&format!("jne {protected}"));                           // other kinds retain the existing cleanup boundary
    emitter.instruction("cmp QWORD PTR [rsi], 0");                              // only logical length matters, not reserved capacity
    emitter.instruction(&format!("jne {protected}"));                           // nonempty arrays may own throwing child destructors
    emitter.instruction(&format!("jmp {direct}"));                              // preserve original refcounts, retirement hooks and sweep suppression
}
