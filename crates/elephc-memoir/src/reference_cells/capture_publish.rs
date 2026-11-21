//! Purpose:
//! Publishes a prepared capture array into a live native reference child slot.
//!
//! Called from:
//! - mbstring capture initialization after reference validation and result allocation.
//!
//! Key details:
//! - Typed publication installs the array before cleanup and honors reentrant replacement.
//! - Untyped publication exposes null during cleanup and transfers any replacement to the caller.

use crate::{Arch, Emitter, abi};

/// Installs the staged array and records pending cleanup without unwinding through the host.
/// The adapter provides the result offset for an independently owned displaced value.
pub fn emit(
    emitter: &mut impl Emitter, untyped: &str, release: &str, ready: &str, discarded_offset: usize,
) {
    let arm = emitter.arch() == Arch::AArch64;
    if arm {
        emitter.instruction("ldr x9, [sp, #56]");                               // recover the writable child slot
        emitter.instruction("ldr x10, [x9]");                                   // transfer the old child owner out of the reference
        emitter.instruction("str x10, [sp, #32]");                              // retain the old value only until its protected release
        emitter.instruction("ldr x10, [sp, #8]");                               // inspect typed versus untyped publication order
        emitter.instruction(&format!("cbz x10, {untyped}"));                    // untyped destruction must observe null first
        emitter.instruction("ldr x10, [sp, #24]");                              // transfer the new boxed array before typed old-value destruction
        emitter.instruction("str x10, [x9]");                                   // publish the typed reference's current array
        emitter.instruction(&format!("b {release}"));                           // release the old owner after publication
        emitter.label(untyped);
        emitter.instruction("str xzr, [x9]");                                   // expose PHP null while the untyped old value is destroyed
    } else {
        emitter.instruction("mov r10, QWORD PTR [rsp + 56]");                   // recover the writable child slot
        emitter.instruction("mov r11, QWORD PTR [r10]");                        // transfer the old boxed child owner
        emitter.instruction("mov QWORD PTR [rsp + 32], r11");                   // retain the previous value only until protected destruction
        emitter.instruction("cmp QWORD PTR [rsp + 8], 0");                      // choose the PHP publication order
        emitter.instruction(&format!("je {untyped}"));                          // untyped destruction observes null first
        emitter.instruction("mov r11, QWORD PTR [rsp + 24]");                   // transfer the fresh boxed array into the typed reference
        emitter.instruction("mov QWORD PTR [r10], r11");                        // publish before typed old-value cleanup
        emitter.instruction(&format!("jmp {release}"));                         // preserve the typed publication during callbacks
        emitter.label(untyped);
        emitter.instruction("mov QWORD PTR [r10], 0");                          // expose PHP null during untyped old-value destruction
    }
    emitter.label(release);
    abi::emit_symbol_address(emitter, if arm { "x0" } else { "rdi" }, "__rt_decref_any");
    emitter.instruction(if arm { "ldr x1, [sp, #32]" } else { "mov rsi, QWORD PTR [rsp + 32]" }); // consume the old boxed value behind the exception boundary
    emitter.instruction(if arm { "add x2, sp, #48" } else { "lea rdx, [rsp + 48]" }); // preserve pending status without abandoning initialization
    abi::emit_call_label(emitter, "__rt_cleanup_call");
    if arm {
        emitter.instruction("ldr x9, [sp, #8]");                                // typed callbacks already observe the published array
        emitter.instruction(&format!("cbnz x9, {ready}"));                      // preserve a typed destructor's later reassignment
        emitter.instruction("ldr x9, [sp, #56]");                               // resolve the same writable child slot after untyped destruction
        emitter.instruction("ldr x10, [x9]");                                   // take any value installed by the destructor
        emitter.instruction("ldr x11, [sp, #16]");                              // recover the output's separate deferred-owner field
        emitter.instruction(&format!("str x10, [x11, #{discarded_offset}]"));   // transfer overwritten ownership for later request-lifetime handling
        emitter.instruction("ldr x10, [sp, #24]");                              // recover the unpublished fresh capture array
        emitter.instruction("str x10, [x9]");                                   // complete untyped initialization after destructive callbacks
    } else {
        emitter.instruction("cmp QWORD PTR [rsp + 8], 0");                      // typed publication is already complete
        emitter.instruction(&format!("jne {ready}"));                           // preserve any typed callback reassignment
        emitter.instruction("mov r10, QWORD PTR [rsp + 56]");                   // recover the writable child slot after untyped destruction
        emitter.instruction("mov r11, QWORD PTR [r10]");                        // take ownership of a destructor-installed value
        emitter.instruction("mov rax, QWORD PTR [rsp + 16]");                   // recover the output's separate deferred-owner field
        emitter.instruction(&format!("mov QWORD PTR [rax + {discarded_offset}], r11"));                   // transfer overwritten ownership instead of destroying it too early
        emitter.instruction("mov r11, QWORD PTR [rsp + 24]");                   // recover the fresh unpublished array
        emitter.instruction("mov QWORD PTR [r10], r11");                        // complete untyped initialization after callbacks finish
    }
}
