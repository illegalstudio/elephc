//! Purpose:
//! Publishes absent imported cells before retiring prior native scope owners.
//!
//! Called from:
//! - The native scope-set emitter at existing-entry and insertion boundaries.
//!
//! Key details:
//! - A null cell pointer denotes absence; PHP null has a nonzero boxed cell.
//! - Existing scope-set frame registers and stack slots retain their established ABI.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Detaches an absent native import before any old owned value can invoke PHP code.
pub(super) fn emit_clear_existing(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cbnz x22, __elephc_eval_scope_set_present");   // ordinary nonzero cells retain checked assignment
            emitter.instruction("ldr x0, [x25, #24]");                          // preserve the previous cell for retirement
            emitter.instruction("ldr x9, [x25, #32]");                          // preserve previous ownership flags
            emitter.instruction("str xzr, [x25, #24]");                         // publish the missing cell before invoking destructors
            emitter.instruction("mov x10, #4");                                 // retain dirty state without the present bit
            emitter.instruction("str x10, [x25, #32]");                         // make absence visible to reentrant scope reads
            emitter.instruction("tst x9, #16");                                 // retire only a previous owned scope lease
            emitter.instruction("b.eq __elephc_eval_scope_set_absent_done");    // borrowed old cells remain owned by their source
            emitter.instruction("cbz x0, __elephc_eval_scope_set_absent_done"); // an already absent cell owns no payload
            emitter.instruction("bl __rt_decref_mixed");                        // retire the displaced owner after publication
            emitter.label("__elephc_eval_scope_set_absent_done");
            emitter.instruction("mov x0, #0");                                  // report successful import of an absent binding
            emitter.instruction("b __elephc_eval_scope_set_done");              // restore the established scope-set frame
        }
        Arch::X86_64 => {
            emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                 // test the saved incoming native cell
            emitter.instruction("jne __elephc_eval_scope_set_present");         // ordinary nonzero cells retain checked assignment
            emitter.instruction("mov rax, QWORD PTR [r9 + 24]");                // preserve the previous cell for retirement
            emitter.instruction("mov r10, QWORD PTR [r9 + 32]");                // preserve previous ownership flags
            emitter.instruction("mov QWORD PTR [r9 + 24], 0");                  // publish the missing cell before invoking destructors
            emitter.instruction("mov QWORD PTR [r9 + 32], 4");                  // make dirty absence visible to reentrant scope reads
            emitter.instruction("test r10, 16");                                // retire only a previous owned scope lease
            emitter.instruction("jz __elephc_eval_scope_set_absent_done");      // borrowed old cells remain owned by their source
            emitter.instruction("test rax, rax");                               // test whether the displaced owner contains a cell
            emitter.instruction("jz __elephc_eval_scope_set_absent_done");      // an already absent cell owns no payload
            emitter.instruction("call __rt_decref_mixed");                      // retire the displaced owner after publication
            emitter.label("__elephc_eval_scope_set_absent_done");
            emitter.instruction("xor eax, eax");                                // report successful import of an absent binding
            emitter.instruction("jmp __elephc_eval_scope_set_done");            // restore the established scope-set frame
        }
    }
    emitter.label("__elephc_eval_scope_set_present");
}

/// Avoids allocating a named entry for a first import that has no visible binding.
pub(super) fn emit_skip_absent_insert(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cbz x22, __elephc_eval_scope_set_absent_done");// an absent first import needs no scope entry
        }
        Arch::X86_64 => {
            emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                 // test the incoming cell before allocating an entry
            emitter.instruction("je __elephc_eval_scope_set_absent_done");      // an absent first import needs no scope entry
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Both ABI implementations publish absence before an old owner can invoke PHP code.
    #[test]
    fn absent_scope_import_publication_precedes_release_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_clear_existing(&mut emitter);
            emit_skip_absent_insert(&mut emitter);
            let asm = emitter.output();
            let publication = if name == "linux-x86_64" { "mov QWORD PTR [r9 + 32], 4" }
                else { "str x10, [x25, #32]" };
            assert!(asm.find(publication).unwrap() < asm.find("__rt_decref_mixed").unwrap(), "{name}: {asm}");
            assert!(asm.contains("__elephc_eval_scope_set_absent_done"), "{name}: {asm}");
        }
    }
}
