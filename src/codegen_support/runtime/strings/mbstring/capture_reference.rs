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
use crate::codegen_support::sentinels::REFERENCE_CELL_HEAP_KIND;

/// Emits graph filling and per-entry live-reference selection for associative capture output.
pub(super) fn emit(emitter: &mut Emitter) {
    emit_reference_child_slot(emitter);
    emit_store(emitter);
    emit_fill(emitter);
}

/// Resolves native ref-cells and eval's persistent Mixed wrappers to one writable child slot.
pub(super) fn emit_reference_child_slot(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    let name = "__rt_mbstring_reference_child_slot";
    emitter.label_global(name);
    if arm {
        emitter.instruction(&format!("cbz x0, {name}_invalid"));             // reject a missing reference before reading its allocation header
        emitter.instruction("ldurb w9, [x0, #-8]");                          // inspect the managed heap kind of the outer owner
        emitter.instruction(&format!("cmp w9, #{REFERENCE_CELL_HEAP_KIND}")); // native aliases use a two-word ref-cell
        emitter.instruction(&format!("b.eq {name}_valid"));                    // the first word is already the writable child slot
        emitter.instruction("ldr x9, [x0]");                                  // eval references use a boxed Mixed wrapper
        emitter.instruction("cmp x9, #7");                                    // require the nested Mixed tag
        emitter.instruction(&format!("b.ne {name}_invalid"));                 // reject ordinary PHP values
        emitter.instruction("ldr x9, [x0, #16]");                             // inspect eval's persistent-reference marker
        emitter.instruction("cmp x9, #1");                                    // reject detached nested boxes
        emitter.instruction(&format!("b.ne {name}_invalid"));
        emitter.instruction("add x0, x0, #8");                                // return the wrapper's writable child slot
    } else {
        emitter.instruction("test rax, rax");                                 // reject a missing reference
        emitter.instruction(&format!("jz {name}_invalid"));
        emitter.instruction(&format!("cmp BYTE PTR [rax - 8], {REFERENCE_CELL_HEAP_KIND}")); // recognize native ref-cells
        emitter.instruction(&format!("je {name}_valid"));
        emitter.instruction("cmp QWORD PTR [rax], 7");                        // require an eval nested Mixed wrapper
        emitter.instruction(&format!("jne {name}_invalid"));
        emitter.instruction("cmp QWORD PTR [rax + 16], 1");                   // require a persistent eval reference
        emitter.instruction(&format!("jne {name}_invalid"));
        emitter.instruction("add rax, 8");                                    // return the wrapper's writable child slot
    }
    emitter.label(&format!("{name}_valid"));
    emitter.instruction("ret");
    emitter.label(&format!("{name}_invalid"));
    emitter.instruction(if arm { "mov x0, #0" } else { "xor eax, eax" });      // expose an invalid reference without touching caller storage
    emitter.instruction("ret");
}

/// Emits the C4 store callback accepting context, borrowed writer reference, key, and value descriptors.
pub(super) fn emit_store(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    emitter.label_global("__rt_mbstring_capture_reference_store");
    if arm {
        emitter.instruction("cbz x1, __rt_mbstring_capture_reference_invalid"); // reject an absent writer before inspecting its marker
        emitter.instruction("mov x10, x0");                                     // preserve the opaque host context across reference validation
        emitter.instruction("mov x0, x1");                                      // resolve either managed reference representation
        emitter.instruction("bl __rt_mbstring_reference_child_slot");           // return the writable child slot
        emitter.instruction("cbz x0, __rt_mbstring_capture_reference_invalid"); // reject malformed references
        emitter.instruction("sub sp, sp, #48");                                 // preserve the caller inputs across current-value resolution
        emitter.instruction("stp x29, x30, [sp, #32]");                         // retain caller linkage for the protected store
        emitter.instruction("str x10, [sp]");                                   // retain the opaque host context
        emitter.instruction("stp x2, x3, [sp, #8]");                            // retain borrowed key and capture descriptors
        emitter.instruction("ldr x0, [x0]");                                    // dereference the writer afresh for this entry
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
        emitter.instruction("mov rax, rsi");                                    // resolve either managed reference representation
        emitter.instruction("call __rt_mbstring_reference_child_slot");         // return the writable child slot
        emitter.instruction("test rax, rax");                                   // reject malformed references
        emitter.instruction("jz __rt_mbstring_capture_reference_invalid");
        emitter.instruction("push rbp");                                        // preserve linkage and align nested calls
        emitter.instruction("mov rbp, rsp");                                    // establish a stable callback frame
        emitter.instruction("sub rsp, 32");                                     // preserve inputs across current-value resolution
        emitter.instruction("mov QWORD PTR [rsp], rdi");                        // retain the opaque host context
        emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                    // retain the borrowed key descriptor
        emitter.instruction("mov QWORD PTR [rsp + 16], rcx");                   // retain the borrowed capture descriptor
        emitter.instruction("mov rax, QWORD PTR [rax]");                        // dereference the writer afresh for this entry
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
