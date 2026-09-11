//! Purpose:
//! Initializes a retained native capture reference with PHP's typed or untyped publication order.
//!
//! Called from:
//! - Capture host initialization adapters after caller-lvalue resolution and typed-reference checks.
//!
//! Key details:
//! - The C4 inputs are unused context, borrowed persistent reference, initialization mode, and result.
//! - The writer retains reference identity without retaining or cloning the previous PHP value.
//! - Untyped reentrant assignments transfer a separate owner for the host's request-lifetime handling.
//! - This primitive does not perform type validation, request shutdown, or V4 host registration.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use elephc_builtin_contract::mbstring_abi::invoke::{
    CAPTURE_REFERENCE_TYPED, CAPTURE_REFERENCE_UNTYPED, MbCaptureReferenceInitV1,
};

#[cfg(test)]
mod tests;

const NAME: &str = "__rt_mbstring_capture_reference_begin";

/// Publishes a fresh hash, contains old-value destruction, and returns success, invalid input, or pending throw.
pub(super) fn emit(emitter: &mut Emitter) {
    assert_eq!(std::mem::size_of::<MbCaptureReferenceInitV1>(), 24);
    assert_eq!(std::mem::offset_of!(MbCaptureReferenceInitV1, discarded), 16);
    assert_eq!((CAPTURE_REFERENCE_UNTYPED, CAPTURE_REFERENCE_TYPED), (0, 1));
    let arm = emitter.target.arch == Arch::AArch64;
    let invalid = format!("{NAME}_invalid");
    let untyped = format!("{NAME}_untyped");
    let release = format!("{NAME}_release");
    let ready = format!("{NAME}_ready");
    emitter.label_global(NAME);
    if arm {
        emitter.instruction(&format!("cbz x3, {invalid}"));                     // reject missing writable result storage before touching the reference
        emitter.instruction("stp xzr, xzr, [x3]");                              // initialize readiness and writer ownership on every nonnull output
        emitter.instruction("str xzr, [x3, #16]");                              // initialize separate deferred ownership before validating inputs
        emitter.instruction("cmp x2, #1");                                      // accept only untyped or already-type-checked initialization
        emitter.instruction(&format!("b.hi {invalid}"));                        // reject unknown publication modes without changing the caller
        emitter.instruction(&format!("cbz x1, {invalid}"));                     // require a live reference identity
        emitter.instruction("ldr x9, [x1]");                                    // inspect the outer wrapper without dereferencing its PHP value
        emitter.instruction("cmp x9, #7");                                      // persistent references use the nested Mixed tag
        emitter.instruction(&format!("b.ne {invalid}"));                        // reject ordinary boxed arguments
        emitter.instruction("ldr x9, [x1, #16]");                               // inspect the persistent-reference discriminator
        emitter.instruction("cmp x9, #1");                                      // exclude detached nested boxes from writable reference identities
        emitter.instruction(&format!("b.ne {invalid}"));                        // preserve malformed inputs without allocating a writer
        emitter.instruction("sub sp, sp, #80");                                 // reserve stable inputs and owners across reentrant old-value destruction
        emitter.instruction("stp x29, x30, [sp, #64]");                         // preserve caller linkage across native allocation and protected cleanup
        emitter.instruction("stp x1, x2, [sp]");                                // preserve reference identity and publication mode
        emitter.instruction("str x3, [sp, #16]");                               // preserve the caller's larger initialization result
        emitter.instruction("str xzr, [sp, #48]");                              // begin with no pending cleanup exception
        emitter.instruction("mov x0, x1");                                      // retain only the reference wrapper for the returned writer
    } else {
        emitter.instruction("test rcx, rcx");                                   // require writable result storage before touching the caller
        emitter.instruction(&format!("jz {invalid}"));                          // reject absent result storage
        for offset in [0, 8, 16] {
            emitter.instruction(&format!("mov QWORD PTR [rcx + {offset}], 0")); // clear readiness and both output owners before input validation
        }
        emitter.instruction("cmp rdx, 1");                                      // accept only untyped or already-type-checked initialization
        emitter.instruction(&format!("ja {invalid}"));                          // reject unknown publication modes without mutation
        emitter.instruction("test rsi, rsi");                                   // require a persistent reference address
        emitter.instruction(&format!("jz {invalid}"));                          // reject a missing reference
        emitter.instruction("cmp QWORD PTR [rsi], 7");                          // require a nested Mixed wrapper
        emitter.instruction(&format!("jne {invalid}"));                         // reject ordinary boxed PHP values
        emitter.instruction("cmp QWORD PTR [rsi + 16], 1");                     // require persistent reference identity
        emitter.instruction(&format!("jne {invalid}"));                         // reject detached nested boxes
        emitter.instruction("push rbp");                                        // preserve linkage and align nested calls
        emitter.instruction("mov rbp, rsp");                                    // establish the initialization frame
        emitter.instruction("sub rsp, 64");                                     // reserve inputs, fresh ownership, and protected cleanup state
        emitter.instruction("mov QWORD PTR [rsp], rsi");                        // retain the borrowed reference address
        emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                    // retain the validated publication mode
        emitter.instruction("mov QWORD PTR [rsp + 16], rcx");                   // retain the larger output buffer address
        emitter.instruction("mov QWORD PTR [rsp + 48], 0");                     // begin without a pending cleanup exception
        emitter.instruction("mov rax, rsi");                                    // retain only the reference wrapper for the writer
    }
    abi::emit_call_label(emitter, "__rt_incref");
    if arm {
        emitter.instruction("ldr x9, [sp, #16]");                               // recover the result before allocation can execute further runtime work
        emitter.instruction("str x0, [x9, #8]");                                // publish the writer owner before destructive initialization
        emitter.instruction("mov x0, #16");                                     // allocate initial capacity for ordered numeric and named captures
        emitter.instruction("mov x1, #7");                                      // use heterogeneous PHP hash metadata
    } else {
        emitter.instruction("mov r10, QWORD PTR [rsp + 16]");                   // recover output ownership storage
        emitter.instruction("mov QWORD PTR [r10 + 8], rax");                    // publish the writer owner before replacing the previous value
        emitter.instruction("mov edi, 16");                                     // allocate initial capacity for ordered captures
        emitter.instruction("mov esi, 7");                                      // select heterogeneous PHP hash metadata
    }
    abi::emit_call_label(emitter, "__rt_hash_new");
    if arm {
        emitter.instruction("str x0, [sp, #40]");                               // retain the newly allocated hash owner while boxing it
        emitter.instruction("mov x1, x0");                                      // supply the new hash as the object-independent PHP array payload
        emitter.instruction("mov x0, #5");                                      // box the associative-array PHP value
        emitter.instruction("mov x2, #0");                                      // associative hashes have no high payload word
    } else {
        emitter.instruction("mov QWORD PTR [rsp + 40], rax");                   // preserve the initial hash owner across boxing
        emitter.instruction("mov rdi, rax");                                    // supply the new hash payload
        emitter.instruction("mov eax, 5");                                      // select associative-array boxing
        emitter.instruction("xor esi, esi");                                    // associative hashes have no high payload word
    }
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    emitter.instruction(if arm { "str x0, [sp, #24]" } else { "mov QWORD PTR [rsp + 24], rax" }); // hold the fresh boxed array until its publication point
    emitter.instruction(if arm { "ldr x0, [sp, #40]" } else { "mov rax, QWORD PTR [rsp + 40]" }); // consume the temporary hash owner acquired before boxing
    abi::emit_call_label(emitter, "__rt_decref_hash");
    if arm {
        emitter.instruction("ldr x9, [sp]");                                    // recover the retained reference identity
        emitter.instruction("ldr x10, [x9, #8]");                               // transfer the old child owner out of the reference
        emitter.instruction("str x10, [sp, #32]");                              // retain the old value only until its protected release
        emitter.instruction("ldr x10, [sp, #8]");                               // inspect typed versus untyped publication order
        emitter.instruction(&format!("cbz x10, {untyped}"));                    // untyped destruction must observe null first
        emitter.instruction("ldr x10, [sp, #24]");                              // transfer the new boxed array before typed old-value destruction
        emitter.instruction("str x10, [x9, #8]");                               // publish the typed reference's current array
        emitter.instruction(&format!("b {release}"));                           // release the old owner after publication
        emitter.label(&untyped);
        emitter.instruction("str xzr, [x9, #8]");                               // expose PHP null while the untyped old value is destroyed
    } else {
        emitter.instruction("mov r10, QWORD PTR [rsp]");                        // recover the reference whose identity is retained
        emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                    // transfer the old boxed child owner
        emitter.instruction("mov QWORD PTR [rsp + 32], r11");                   // retain the previous value only until protected destruction
        emitter.instruction("cmp QWORD PTR [rsp + 8], 0");                      // choose the PHP publication order
        emitter.instruction(&format!("je {untyped}"));                          // untyped destruction observes null first
        emitter.instruction("mov r11, QWORD PTR [rsp + 24]");                   // transfer the fresh boxed array into the typed reference
        emitter.instruction("mov QWORD PTR [r10 + 8], r11");                    // publish before typed old-value cleanup
        emitter.instruction(&format!("jmp {release}"));                         // preserve the typed publication during callbacks
        emitter.label(&untyped);
        emitter.instruction("mov QWORD PTR [r10 + 8], 0");                      // expose PHP null during untyped old-value destruction
    }
    emitter.label(&release);
    abi::emit_symbol_address(emitter, if arm { "x0" } else { "rdi" }, "__rt_decref_any");
    emitter.instruction(if arm { "ldr x1, [sp, #32]" } else { "mov rsi, QWORD PTR [rsp + 32]" }); // consume the old boxed value behind the exception boundary
    emitter.instruction(if arm { "add x2, sp, #48" } else { "lea rdx, [rsp + 48]" }); // preserve pending status without abandoning initialization
    abi::emit_call_label(emitter, "__rt_cleanup_call");
    if arm {
        emitter.instruction("ldr x9, [sp, #8]");                                // typed callbacks already observe the published array
        emitter.instruction(&format!("cbnz x9, {ready}"));                      // preserve a typed destructor's later reassignment
        emitter.instruction("ldr x9, [sp]");                                    // resolve the same retained reference after untyped destruction
        emitter.instruction("ldr x10, [x9, #8]");                               // take any value installed by the destructor
        emitter.instruction("ldr x11, [sp, #16]");                              // recover the output's separate deferred-owner field
        emitter.instruction("str x10, [x11, #16]");                             // transfer overwritten ownership for later request-lifetime handling
        emitter.instruction("ldr x10, [sp, #24]");                              // recover the unpublished fresh capture array
        emitter.instruction("str x10, [x9, #8]");                               // complete untyped initialization after destructive callbacks
    } else {
        emitter.instruction("cmp QWORD PTR [rsp + 8], 0");                      // typed publication is already complete
        emitter.instruction(&format!("jne {ready}"));                           // preserve any typed callback reassignment
        emitter.instruction("mov r10, QWORD PTR [rsp]");                        // recover the retained reference after untyped destruction
        emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                    // take ownership of a destructor-installed value
        emitter.instruction("mov rax, QWORD PTR [rsp + 16]");                   // recover the output's separate deferred-owner field
        emitter.instruction("mov QWORD PTR [rax + 16], r11");                   // transfer overwritten ownership instead of destroying it too early
        emitter.instruction("mov r11, QWORD PTR [rsp + 24]");                   // recover the fresh unpublished array
        emitter.instruction("mov QWORD PTR [r10 + 8], r11");                    // complete untyped initialization after callbacks finish
    }
    emitter.label(&ready);
    if arm {
        emitter.instruction("ldr x9, [sp, #16]");                               // recover completion metadata
        emitter.instruction("mov x10, #1");                                     // allow matching even if old-value destruction left a throwable pending
        emitter.instruction("str x10, [x9]");                                   // publish readiness independently of exception status
        emitter.instruction("ldr x0, [sp, #48]");                               // return the pending cleanup flag
        emitter.instruction("lsl x0, x0, #1");                                  // convert the flag to the shared pending status
        emitter.instruction("ldp x29, x30, [sp, #64]");                         // restore caller linkage after ownership publication
        emitter.instruction("add sp, sp, #80");                                 // release only temporary initialization storage
    } else {
        emitter.instruction("mov r10, QWORD PTR [rsp + 16]");                   // recover completion metadata
        emitter.instruction("mov QWORD PTR [r10], 1");                          // publish readiness independently of pending status
        emitter.instruction("mov rax, QWORD PTR [rsp + 48]");                   // return the contained cleanup flag
        emitter.instruction("shl eax, 1");                                      // convert the flag to the shared pending status
        emitter.instruction("leave");                                           // restore caller linkage after transferring both output owners
    }
    emitter.instruction("ret");                                                 // return success or pending throwable with a ready writer
    emitter.label(&invalid);
    emitter.instruction(if arm { "mov x0, #1" } else { "mov eax, 1" });         // reject malformed references or modes without allocating ownership
    emitter.instruction("ret");                                                 // return before entering the initialization frame
}
