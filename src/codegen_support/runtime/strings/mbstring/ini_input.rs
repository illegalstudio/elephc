//! Purpose:
//! Adds borrowed INI identity metadata to strings retained by the shared invocation coordinator.
//!
//! Called from:
//! - The SharedIni operation's description callback in the ordinary version-three host table.
//!
//! Key details:
//! - The ordinary classifier owns type and object metadata; only concrete strings gain identity tokens.
//! - Retained host copies own their origins and leases until final coordinator cleanup.
//! - This callback never executes PHP or holds a request-state borrow.

use super::*;
use elephc_builtin_contract::mbstring_abi::coercion::INPUT_INI_IDENTITY;

/// Describes a value normally, then resolves an exact native string origin without copying its bytes.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.label_global("__rt_mbstring_ini_input");
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("stp x29, x30, [sp, #-32]!");                       // preserve C linkage and one stable descriptor pointer
        emitter.instruction("str x2, [sp, #16]");                               // retain caller output storage through ordinary classification
        abi::emit_call_label(emitter, "__rt_mbstring_input");
        emitter.instruction("cbnz x0, __rt_mbstring_ini_input_done");           // preserve classification failure and its established ownership rules
        emitter.instruction("ldr x9, [sp, #16]");                               // recover the completed concrete descriptor
        emitter.instruction("ldr x10, [x9]");                                   // inspect its actual PHP value kind
        emitter.instruction("cmp x10, #1");                                     // only concrete strings have INI string identity
        emitter.instruction("b.ne __rt_mbstring_ini_input_done");               // retain successful scalar and object descriptions unchanged
        emitter.instruction("ldp x0, x1, [x9, #16]");                           // pass the retained native byte range to lazy identity resolution
        emitter.bl_c("elephc_mbstring_native_string_resolve_v1");
        emitter.instruction("cbz x0, __rt_mbstring_ini_input_failed");          // reject a missing retained origin without inventing an identity
        emitter.instruction("ldr x9, [sp, #16]");                               // restore output storage after the Rust registry callback
        emitter.instruction("str x0, [x9, #8]");                                // borrow the token owned by the retained native argument
        emitter.instruction(&format!("mov x10, #{INPUT_INI_IDENTITY}"));        // declare the string-specific identity payload contract
        emitter.instruction("str x10, [x9, #32]");                              // publish the identity capability after successful resolution
        emitter.instruction("mov x0, #0");                                      // report a complete description with borrowed identity ownership
        emitter.instruction("b __rt_mbstring_ini_input_done");                  // share normal C frame teardown
        emitter.label("__rt_mbstring_ini_input_failed");
        emitter.instruction("mov x0, #1");                                      // return internal failure for an invalid native origin
        emitter.label("__rt_mbstring_ini_input_done");
        emitter.instruction("ldp x29, x30, [sp], #32");                         // restore caller linkage and discard the descriptor spill
    } else {
        emitter.instruction("push rbp");                                        // preserve C linkage and align the callback frame
        emitter.instruction("mov rbp, rsp");                                    // establish stable C frame teardown
        emitter.instruction("sub rsp, 16");                                     // reserve an aligned descriptor-pointer spill
        emitter.instruction("mov QWORD PTR [rsp], rdx");                        // retain caller output through ordinary classification
        abi::emit_call_label(emitter, "__rt_mbstring_input");
        emitter.instruction("test eax, eax");                                   // inspect the original classifier's non-unwinding status
        emitter.instruction("jnz __rt_mbstring_ini_input_done");                // preserve its failure status and metadata ownership
        emitter.instruction("mov r10, QWORD PTR [rsp]");                        // recover the complete concrete-value descriptor
        emitter.instruction("cmp QWORD PTR [r10], 1");                          // only concrete strings carry INI identity tokens
        emitter.instruction("jne __rt_mbstring_ini_input_done");                // keep ordinary scalar and object metadata unchanged
        emitter.instruction("mov rdi, QWORD PTR [r10 + 16]");                   // pass the retained native string pointer to lazy resolution
        emitter.instruction("mov rsi, QWORD PTR [r10 + 24]");                   // resolve only its complete logical byte range
        emitter.bl_c("elephc_mbstring_native_string_resolve_v1");
        emitter.instruction("test rax, rax");                                   // require a live identity from the retained native origin
        emitter.instruction("jz __rt_mbstring_ini_input_failed");               // reject missing ownership without manufacturing a string identity
        emitter.instruction("mov r10, QWORD PTR [rsp]");                        // recover writable metadata after the registry callback
        emitter.instruction("mov QWORD PTR [r10 + 8], rax");                    // borrow the identity owned by the retained native argument
        emitter.instruction(&format!("mov QWORD PTR [r10 + 32], {INPUT_INI_IDENTITY}")); // publish the kind-specific identity capability
        emitter.instruction("xor eax, eax");                                    // report successful metadata with no additional owner
        emitter.instruction("jmp __rt_mbstring_ini_input_done");                // share callback teardown after successful binding
        emitter.label("__rt_mbstring_ini_input_failed");
        emitter.instruction("mov eax, 1");                                      // expose invalid native origin metadata as internal failure
        emitter.label("__rt_mbstring_ini_input_done");
        emitter.instruction("leave");                                           // release the spill and restore the original C frame
    }
    emitter.instruction("ret");                                                 // return after every Rust frame has completed
}
