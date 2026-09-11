//! Purpose:
//! Resolves native capture destinations and promotes uniquely owned indexed payloads.
//!
//! Called from:
//! - The per-entry mbregex capture-reference store before selecting its destination hash.
//!
//! Key details:
//! - Promotion updates the existing terminal Mixed cell, preserving reference identity.
//! - Shared indexed payloads remain unsupported because copying would hide writes from aliases.
//! - Conversion retains every element before releasing the old indexed owner.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

#[cfg(test)]
mod tests;

const NAME: &str = "__rt_mbstring_capture_destination";

/// Returns a borrowed destination hash, or zero without mutation for unsupported storage.
pub(super) fn emit(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    emitter.label_global(NAME);
    emitter.label(&format!("{NAME}_follow"));
    if arm {
        emitter.instruction(&format!("cbz x0, {NAME}_invalid"));                // reject null while following nested reference values
        emitter.instruction("ldr x9, [x0]");                                    // inspect the current boxed PHP representation
        emitter.instruction("cmp x9, #7");                                      // nested Mixed cells retain their child at offset eight
        emitter.instruction(&format!("b.ne {NAME}_value"));                     // inspect the terminal value without copying its owner
        emitter.instruction("ldr x0, [x0, #8]");                                // follow the current reference value
        emitter.instruction(&format!("b {NAME}_follow"));                       // resolve nested aliases before selecting an array
    } else {
        emitter.instruction("test rax, rax");                                   // reject null while following nested reference values
        emitter.instruction(&format!("jz {NAME}_invalid"));                     // avoid dereferencing an absent current value
        emitter.instruction("cmp QWORD PTR [rax], 7");                          // nested Mixed cells retain their child at offset eight
        emitter.instruction(&format!("jne {NAME}_value"));                      // inspect the terminal PHP representation
        emitter.instruction("mov rax, QWORD PTR [rax + 8]");                    // follow the live reference child
        emitter.instruction(&format!("jmp {NAME}_follow"));                     // resolve nested aliases before selecting an array
    }
    emitter.label(&format!("{NAME}_value"));
    emitter.instruction(if arm { "cmp x9, #5" } else { "cmp QWORD PTR [rax], 5" }); // existing hash destinations already have stable construction storage
    branch(emitter, "eq", "je", "hash");
    emitter.instruction(if arm { "cmp x9, #4" } else { "cmp QWORD PTR [rax], 4" }); // indexed arrays require a representation change before heterogeneous writes
    branch(emitter, "ne", "jne", "invalid");
    if arm {
        emitter.instruction("sub sp, sp, #48");                                 // retain the terminal cell and both container owners across conversion
        emitter.instruction("stp x29, x30, [sp, #32]");                         // preserve linkage for allocation and release helpers
        emitter.instruction("str x0, [sp]");                                    // keep the exact cell whose representation will change
        emitter.instruction("ldr x0, [x0, #8]");                                // borrow its current indexed payload
        emitter.instruction("str x0, [sp, #8]");                                // retain the old payload address until ownership transfers
    } else {
        emitter.instruction("push rbp");                                        // preserve linkage and align nested helper calls
        emitter.instruction("mov rbp, rsp");                                    // establish the conversion frame
        emitter.instruction("sub rsp, 32");                                     // retain the terminal cell and both container owners
        emitter.instruction("mov QWORD PTR [rsp], rax");                        // keep the exact terminal cell for in-place publication
        emitter.instruction("mov rax, QWORD PTR [rax + 8]");                    // borrow its current indexed payload
        emitter.instruction("mov QWORD PTR [rsp + 8], rax");                    // preserve the old payload until conversion completes
    }
    abi::emit_call_label(emitter, "__rt_heap_kind");
    emitter.instruction(if arm { "cmp x0, #2" } else { "cmp eax, 2" });         // require a real indexed allocation before inspecting its ownership
    branch(emitter, "ne", "jne", "failed");
    if arm {
        emitter.instruction("ldr x0, [sp, #8]");                                // recover the verified indexed allocation
        emitter.instruction("ldr w9, [x0, #-12]");                              // read the number of distinct payload owners
        emitter.instruction("cmp w9, #1");                                      // promotion can preserve all observers only for a unique payload
    } else {
        emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");                    // adapt the verified allocation to the indexed conversion ABI
        emitter.instruction("cmp DWORD PTR [rdi - 12], 1");                     // shared payloads need an identity-preserving representation adapter
    }
    branch(emitter, "ne", "jne", "failed");
    abi::emit_call_label(emitter, "__rt_array_to_hash");
    if arm {
        emitter.instruction("str x0, [sp, #16]");                               // preserve the new owned hash across old-container release
        emitter.instruction("mov x9, #7");                                      // subsequent captures can mix strings, false, and preserved elements
        emitter.instruction("str x9, [x0, #16]");                               // declare heterogeneous hash payloads
        emitter.instruction("ldr x9, [sp]");                                    // recover the existing terminal Mixed cell
        emitter.instruction("str x0, [x9, #8]");                                // transfer the new hash owner before releasing the indexed payload
        emitter.instruction("mov x10, #5");                                     // publish the boxed associative-array representation
        emitter.instruction("str x10, [x9]");                                   // preserve every alias to this same cell
        emitter.instruction("str xzr, [x9, #16]");                              // clear the indexed representation's unused high payload
        emitter.instruction("ldr x0, [sp, #8]");                                // consume the old container after all children acquired new owners
    } else {
        emitter.instruction("mov QWORD PTR [rsp + 16], rax");                   // preserve the new owned hash across old-container release
        emitter.instruction("mov QWORD PTR [rax + 16], 7");                     // subsequent captures use heterogeneous entry tags
        emitter.instruction("mov r10, QWORD PTR [rsp]");                        // recover the existing terminal Mixed cell
        emitter.instruction("mov QWORD PTR [r10 + 8], rax");                    // transfer the new hash owner before releasing the old payload
        emitter.instruction("mov QWORD PTR [r10], 5");                          // publish the associative-array tag through the existing cell identity
        emitter.instruction("mov QWORD PTR [r10 + 16], 0");                     // clear the unused high payload
        emitter.instruction("mov rax, QWORD PTR [rsp + 8]");                    // consume the old indexed owner after every child was retained
    }
    abi::emit_call_label(emitter, "__rt_decref_any");
    emitter.instruction(if arm { "ldr x0, [sp, #16]" } else { "mov rax, QWORD PTR [rsp + 16]" }); // borrow the published hash for the current capture write
    jump(emitter, "done");
    emitter.label(&format!("{NAME}_failed"));
    emitter.instruction(if arm { "mov x0, #0" } else { "xor eax, eax" });       // return unsupported without changing the original cell or payload
    emitter.label(&format!("{NAME}_done"));
    if arm {
        emitter.instruction("ldp x29, x30, [sp, #32]");                         // restore linkage after conversion or an unchanged failure
        emitter.instruction("add sp, sp, #48");                                 // retire only local conversion storage
    } else {
        emitter.instruction("leave");                                           // restore the caller while preserving the selected hash or zero
    }
    emitter.instruction("ret");                                                 // return the borrowed destination without an extra owner
    emitter.label(&format!("{NAME}_hash"));
    emitter.instruction(if arm { "ldr x0, [x0, #8]" } else { "mov rax, QWORD PTR [rax + 8]" }); // borrow the existing stable hash identity
    emitter.instruction("ret");                                                 // no conversion or allocation is needed for associative destinations
    emitter.label(&format!("{NAME}_invalid"));
    emitter.instruction(if arm { "mov x0, #0" } else { "xor eax, eax" });       // unsupported values remain untouched
    emitter.instruction("ret");                                                 // reject invalid storage before entering the conversion frame
}

/// Branches to a destination phase using the active target's condition mnemonic.
fn branch(emitter: &mut Emitter, arm: &str, x86: &str, label: &str) {
    let op = if emitter.target.arch == Arch::AArch64 { format!("b.{arm}") } else { x86.to_owned() };
    emitter.instruction(&format!("{op} {NAME}_{label}"));                       // preserve validation before any representation change
}

/// Jumps to the shared conversion epilogue without changing the selected result.
fn jump(emitter: &mut Emitter, label: &str) {
    let op = if emitter.target.arch == Arch::AArch64 { "b" } else { "jmp" };
    emitter.instruction(&format!("{op} {NAME}_{label}"));                       // share frame teardown for successful and unsupported destinations
}
