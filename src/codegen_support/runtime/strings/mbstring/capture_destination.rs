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

use crate::codegen_support::{emit::Emitter, platform::Arch};

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
    elephc_memoir::values::boxed_array::emit_promote_unique_indexed_payload(
        emitter, &format!("{NAME}_failed"), &format!("{NAME}_done"),
    );
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
