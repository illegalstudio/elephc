//! Purpose:
//! Resolves the live reference behind each associative mbregex capture write.
//!
//! Called from:
//! - Capture-output host adapters and focused native reference-retargeting tests.
//!
//! Key details:
//! - The caller owns a persistent reference cell for the complete capture operation.
//! - Each write selects its current hash; the hash store pins only that selected destination.
//! - Invalid reference markers and non-hash destinations return the fatal host status.
//! - Unique indexed destinations are promoted through their existing boxed value identity.
//! - Shared indexed payloads still require an identity-preserving representation adapter.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits graph filling and per-entry live-reference selection for associative capture output.
pub(super) fn emit(emitter: &mut Emitter) {
    emit_store(emitter);
    emit_fill(emitter);
}

/// Emits the C4 store callback accepting context, borrowed writer reference, key, and value descriptors.
pub(super) fn emit_store(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    emitter.label_global("__rt_mbstring_capture_reference_store");
    if arm {
        emitter.instruction("cbz x1, __rt_mbstring_capture_reference_invalid"); // reject an absent writer before inspecting its marker
        emitter.instruction("ldr x9, [x1]");                                    // inspect the persistent reference tag
        emitter.instruction("cmp x9, #7");                                      // require a nested reference cell
        emitter.instruction("b.ne __rt_mbstring_capture_reference_invalid");    // ordinary values are not writable reference identities
        emitter.instruction("ldr x9, [x1, #16]");                               // inspect the persistent reference discriminator
        emitter.instruction("cmp x9, #1");                                      // distinguish reference cells from ordinary nested boxes
        emitter.instruction("b.ne __rt_mbstring_capture_reference_invalid");    // reject a detached value rather than mutating it
        emitter.instruction("sub sp, sp, #48");                                 // preserve the caller inputs across current-value resolution
        emitter.instruction("stp x29, x30, [sp, #32]");                         // retain caller linkage for the protected store
        emitter.instruction("str x0, [sp]");                                    // retain the opaque host context
        emitter.instruction("stp x2, x3, [sp, #8]");                            // retain borrowed key and capture descriptors
        emitter.instruction("mov x0, x1");                                      // dereference the writer afresh for this entry
        emitter.instruction("bl __rt_mbstring_capture_destination");            // select a current hash or promote a unique indexed payload
        emitter.instruction("cbz x0, __rt_mbstring_capture_reference_failed");  // leave unsupported shared indexed storage untouched
        emitter.instruction("mov x1, x0");                                      // pass the selected stable hash to the construction store
        emitter.instruction("ldr x0, [sp]");                                    // restore context beside the selected hash in x1
        emitter.instruction("ldp x2, x3, [sp, #8]");                            // restore the validated borrowed descriptors
        emitter.instruction("bl __rt_mbstring_capture_hash_store");             // complete this selected write before following later retargeting
        emitter.instruction("b __rt_mbstring_capture_reference_done");          // preserve the successful or pending status
        emitter.label("__rt_mbstring_capture_reference_failed");
        emitter.instruction("mov x0, #1");                                      // report an unsupported destination to the host coordinator
        emitter.label("__rt_mbstring_capture_reference_done");
        emitter.instruction("ldp x29, x30, [sp, #32]");                         // restore linkage after the selected write finishes
        emitter.instruction("add sp, sp, #48");                                 // release input storage without retaining a stale hash
        emitter.instruction("ret");                                             // return the contained operation status
    } else {
        emitter.instruction("test rsi, rsi");                                   // reject an absent writer before inspecting its marker
        emitter.instruction("jz __rt_mbstring_capture_reference_invalid");      // preserve a missing output reference without dereferencing it
        emitter.instruction("cmp QWORD PTR [rsi], 7");                          // require a nested reference cell
        emitter.instruction("jne __rt_mbstring_capture_reference_invalid");     // ordinary values are not writable reference identities
        emitter.instruction("cmp QWORD PTR [rsi + 16], 1");                     // require the persistent reference discriminator
        emitter.instruction("jne __rt_mbstring_capture_reference_invalid");     // reject a detached nested value
        emitter.instruction("push rbp");                                        // preserve linkage and align nested calls
        emitter.instruction("mov rbp, rsp");                                    // establish a stable callback frame
        emitter.instruction("sub rsp, 32");                                     // preserve inputs across current-value resolution
        emitter.instruction("mov QWORD PTR [rsp], rdi");                        // retain the opaque host context
        emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                    // retain the borrowed key descriptor
        emitter.instruction("mov QWORD PTR [rsp + 16], rcx");                   // retain the borrowed capture descriptor
        emitter.instruction("mov rax, rsi");                                    // dereference the writer afresh for this entry
        emitter.instruction("call __rt_mbstring_capture_destination");          // select a current hash or promote a unique indexed payload
        emitter.instruction("test rax, rax");                                   // distinguish a selected hash from unsupported storage
        emitter.instruction("jz __rt_mbstring_capture_reference_failed");       // preserve unsupported shared indexed payloads
        emitter.instruction("mov rsi, rax");                                    // supply the selected current hash as the C store receiver
        emitter.instruction("mov rdi, QWORD PTR [rsp]");                        // restore the host context
        emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                    // restore the validated key descriptor
        emitter.instruction("mov rcx, QWORD PTR [rsp + 16]");                   // restore the borrowed capture descriptor
        emitter.instruction("call __rt_mbstring_capture_hash_store");           // complete this selected write even if a destructor retargets the reference
        emitter.instruction("jmp __rt_mbstring_capture_reference_done");        // preserve the successful or pending status
        emitter.label("__rt_mbstring_capture_reference_failed");
        emitter.instruction("mov eax, 1");                                      // report an unsupported destination to the host coordinator
        emitter.label("__rt_mbstring_capture_reference_done");
        emitter.instruction("leave");                                           // release input storage and restore caller linkage
        emitter.instruction("ret");                                             // return the contained operation status
    }
    emitter.label("__rt_mbstring_capture_reference_invalid");
    emitter.instruction(if arm { "mov x0, #1" } else { "mov eax, 1" });         // reject malformed reference identities without mutation
    emitter.instruction("ret");                                                 // return before allocating a callback frame
}

/// Passes a borrowed graph and retained reference to the shared validated capture insertion engine.
fn emit_fill(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    emitter.label_global("__rt_mbstring_capture_reference_fill");
    if arm {
        emitter.instruction("stp x29, x30, [sp, #-16]!");                       // preserve linkage while the shared engine invokes native stores
    } else {
        emitter.instruction("sub rsp, 8");                                      // align the five-argument C call without disturbing incoming arguments
    }
    abi::emit_symbol_address(emitter, if arm { "x4" } else { "r8" }, "__rt_mbstring_capture_reference_store");
    emitter.bl_c("elephc_mbstring_capture_apply_v1");
    if arm {
        emitter.instruction("ldp x29, x30, [sp], #16");                         // restore linkage after all ordered writes have completed
    } else {
        emitter.instruction("add rsp, 8");                                      // restore caller alignment while preserving the shared status
    }
    emitter.instruction("ret");                                                 // return success, fatal failure, or the pending throwable status
}
