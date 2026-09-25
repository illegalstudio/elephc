//! Purpose:
//! Exposes persistent reference storage and detached value copies through the eval C ABI.
//!
//! Called from:
//! - Eval bridge emission for the active target architecture.
//!
//! Key details:
//! - References own GC-traced Mixed values independently of interpreter scopes.
//! - Replacement returns the previous owner so Rust controls when cleanup occurs.

use super::*;

/// Emits narrow C adapters over the target-aware shared reference runtime helpers.
pub(super) fn emit(emitter: &mut Emitter) {
    for (name, helper) in [
        ("__elephc_eval_value_reference_new", "__rt_reference_new"),
        ("__elephc_eval_value_is_reference", "__rt_reference_is"),
        ("__elephc_eval_value_copy", "__rt_mixed_clone"),
    ] {
        label_c_global(emitter, name);
        if emitter.target.arch == Arch::AArch64 {
            emitter.instruction(&format!("b {helper}"));                        // tail-call with the existing AArch64 boxed argument
        } else {
            emitter.instruction("mov rax, rdi");                                // adapt the borrowed C argument to the native boxed-value convention
            emitter.instruction(&format!("jmp {helper}"));                      // transfer directly to the shared runtime implementation
        }
    }
    label_c_global(emitter, "__elephc_eval_value_reference_is_shared");
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("cbz x0, __elephc_eval_reference_unshared");        // null cannot own a PHP reference
        emitter.instruction("ldr x9, [x0]");                                    // inspect the boxed runtime tag
        emitter.instruction("cmp x9, #7");                                      // persistent references use nested Mixed storage
        emitter.instruction("b.ne __elephc_eval_reference_unshared");           // ordinary boxes have no writable reference
        emitter.instruction("ldr x9, [x0, #16]");                               // inspect the persistent reference flag
        emitter.instruction("cmp x9, #1");                                      // flag one identifies a PHP reference
        emitter.instruction("b.ne __elephc_eval_reference_unshared");           // unrelated nested boxes remain ordinary
        emitter.instruction("ldr w9, [x0, #-12]");                              // read the wrapper's physical owner count
        emitter.instruction("ldr x10, [x0, #-8]");                              // inspect an artificial collector pin
        emitter.instruction("ubfx x10, x10, #18, #1");                          // isolate the collector-only owner bit
        emitter.instruction("sub w9, w9, w10");                                 // PHP sharing excludes collector pins
        emitter.instruction("cmp w9, #1");                                      // one owner separates during array COW
        emitter.instruction("cset x0, hi");                                     // another owner keeps the reference shared
        emitter.instruction("ret");                                             // return the sharing predicate
        emitter.label("__elephc_eval_reference_unshared");
        emitter.instruction("mov x0, #0");                                      // report a singleton or ordinary value
        emitter.instruction("ret");                                             // leave the borrowed wrapper untouched
    } else {
        emitter.instruction("test rdi, rdi");                                   // null cannot own a PHP reference
        emitter.instruction("jz __elephc_eval_reference_unshared");             // skip an absent boxed value
        emitter.instruction("cmp QWORD PTR [rdi], 7");                          // persistent references use nested Mixed storage
        emitter.instruction("jne __elephc_eval_reference_unshared");            // ordinary boxes have no writable reference
        emitter.instruction("cmp QWORD PTR [rdi + 16], 1");                     // inspect the persistent reference flag
        emitter.instruction("jne __elephc_eval_reference_unshared");            // unrelated nested boxes remain ordinary
        emitter.instruction("mov eax, DWORD PTR [rdi - 12]");                   // read the wrapper's physical owner count
        emitter.instruction("mov r10, QWORD PTR [rdi - 8]");                    // inspect an artificial collector pin
        emitter.instruction("shr r10, 18");                                     // position the collector-only owner bit
        emitter.instruction("and r10d, 1");                                     // isolate the collector pin
        emitter.instruction("sub eax, r10d");                                   // PHP sharing excludes collector pins
        emitter.instruction("cmp eax, 1");                                      // one owner separates during array COW
        emitter.instruction("seta al");                                         // another owner keeps the reference shared
        emitter.instruction("movzx eax, al");                                   // normalize the sharing predicate
        emitter.instruction("ret");                                             // leave the borrowed wrapper untouched
        emitter.label("__elephc_eval_reference_unshared");
        emitter.instruction("xor eax, eax");                                    // report a singleton or ordinary value
        emitter.instruction("ret");                                             // return the sharing predicate
    }
    label_c_global(emitter, "__elephc_eval_value_reference_replace");
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("b __rt_reference_replace");                        // return the previous owned value using the unchanged two-argument convention
    } else {
        emitter.instruction("mov rax, rdi");                                    // pass the persistent reference identity through the internal value register
        emitter.instruction("mov rdi, rsi");                                    // pass the borrowed replacement value through the internal second argument
        emitter.instruction("jmp __rt_reference_replace");                      // transfer the previous owner back to Rust for explicit cleanup
    }
}
