//! Purpose:
//! Emits `__rt_float_compare_slots` / `__rt_float_compare_slots_desc`: comparator
//! callbacks that order two raw `float` array slots by PHP's own rules.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Written to `__rt_usort`'s callback ABI: `(a, b)` in the first two argument
//!   registers and ordering in the result register. A float array's slots are
//!   pointer-sized, so the existing slot permuter moves them unchanged.
//! - The ordering is `__rt_php_compare` with both operands tagged as floats, which
//!   is what `<` and `<=>` already use and what the boxed-`Mixed` comparator calls.
//!   A local `fcmp` would be a second answer to a question the runtime already
//!   answers, and the two would drift over `NAN` and over `-0.0 == 0.0`.
//! - Unlike the `Mixed` comparator there is no validation pass: every slot of an
//!   `array<float>` is a float by construction, so there is no tag to reject.
//! - The descending variant negates the result rather than swapping the operands.
//!   Swapping would reverse the order of EQUAL elements too, and PHP's `rsort` is
//!   not specified to do that.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// The runtime value tag `__rt_php_compare` reads as "this payload is a float".
const FLOAT_TAG: i64 = 2;

/// Emits both float-slot comparator callbacks for the current target.
pub fn emit_float_compare_slots(emitter: &mut Emitter) {
    emit_one(emitter, "__rt_float_compare_slots", false);
    emit_one(emitter, "__rt_float_compare_slots_desc", true);
}

/// Emits one target-specific float-slot comparator in the requested direction.
fn emit_one(emitter: &mut Emitter, label: &str, descending: bool) {
    emitter.blank();
    emitter.comment(&format!("--- runtime: {label} ---"));
    emitter.label_global(label);

    match emitter.target.arch {
        Arch::AArch64 => {
            // `__rt_php_compare` takes (tag, lo, hi) twice; the slot payload is the low word
            // and a float carries no high word. b moves first, because a still occupies the
            // register b's tag is about to take.
            emitter.instruction("mov x4, x1");                                  // right payload = b's float bits
            emitter.instruction("mov x1, x0");                                  // left payload = a's float bits
            emitter.instruction(&format!("mov x0, #{}", FLOAT_TAG));            // left operand is a float
            emitter.instruction("mov x2, xzr");                                 // floats carry no high word
            emitter.instruction(&format!("mov x3, #{}", FLOAT_TAG));            // right operand is a float
            emitter.instruction("mov x5, xzr");                                 // floats carry no high word
            if descending {
                emitter.instruction("sub sp, sp, #16");                         // reserve the saved frame record
                emitter.instruction("stp x29, x30, [sp]");                      // save it across the nested call
                emitter.instruction("mov x29, sp");                             // establish a stable comparator frame
                abi::emit_call_label(emitter, "__rt_php_compare");              // ordering as x0 = -1, 0, or 1
                emitter.instruction("neg x0, x0");                              // reverse the order, not the equal elements
                emitter.instruction("ldp x29, x30, [sp]");                      // restore the caller frame record
                emitter.instruction("add sp, sp, #16");                         // release the comparator frame
                emitter.instruction("ret");                                     // return the PHP ordering result
            } else {
                emitter.instruction("b __rt_php_compare");                      // tail-call: its answer is ours
            }
        }
        Arch::X86_64 => {
            emitter.instruction("mov r8, rsi");                                 // right payload = b's float bits
            emitter.instruction("mov rsi, rdi");                                // left payload = a's float bits
            emitter.instruction(&format!("mov edi, {}", FLOAT_TAG));            // left operand is a float
            emitter.instruction("xor edx, edx");                                // floats carry no high word
            emitter.instruction(&format!("mov ecx, {}", FLOAT_TAG));            // right operand is a float
            emitter.instruction("xor r9d, r9d");                                // floats carry no high word
            if descending {
                emitter.instruction("push rbp");                                // preserve the caller frame pointer
                emitter.instruction("mov rbp, rsp");                            // establish a stable comparator frame
                abi::emit_call_label(emitter, "__rt_php_compare");              // ordering as rax = -1, 0, or 1
                emitter.instruction("neg rax");                                 // reverse the order, not the equal elements
                emitter.instruction("pop rbp");                                 // restore the caller frame pointer
                emitter.instruction("ret");                                     // return the PHP ordering result
            } else {
                emitter.instruction("jmp __rt_php_compare");                    // tail-call: its answer is ours
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Emits both comparators for one target and returns the assembly.
    fn assembly_for(target: Target) -> String {
        let mut emitter = Emitter::new(target);
        emit_float_compare_slots(&mut emitter);
        emitter.output()
    }

    /// The ordering comes from the shared comparator, not from a local `fcmp`.
    ///
    /// A hand-rolled float compare would have to decide `NAN` and `-0.0` for itself, and would
    /// then disagree with `<` and `<=>` on the same two values.
    #[test]
    fn float_slot_comparators_defer_to_the_shared_ordering() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let asm = assembly_for(target);
            assert!(
                asm.contains("__rt_php_compare"),
                "{:?} must defer to the shared comparator: {}",
                target,
                asm
            );
            assert!(
                !asm.contains("fcmp") && !asm.contains("ucomisd"),
                "{:?} must not compare floats locally: {}",
                target,
                asm
            );
        }
    }

    /// Descending negates the result; it does not swap the operands.
    ///
    /// Swapping would reverse the order of EQUAL elements too, which `rsort` is not specified
    /// to do — and the two are indistinguishable on any fixture whose elements are distinct.
    #[test]
    fn descending_negates_rather_than_swapping() {
        let arm = assembly_for(Target::new(Platform::MacOS, Arch::AArch64));
        assert!(arm.contains("neg x0, x0"));
        let x86 = assembly_for(Target::new(Platform::Linux, Arch::X86_64));
        assert!(x86.contains("neg rax"));
    }
}
