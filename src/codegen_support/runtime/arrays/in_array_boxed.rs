//! Purpose:
//! Searches packed and associative PHP arrays using shared Mixed comparison semantics.
//!
//! Called from:
//! - The typed InArray backend for boxed or non-scalar operands.
//!
//! Key details:
//! - Input cells and element payloads remain borrowed, including during nested comparisons.
//! - The logical iterator does not mutate the array's PHP internal cursor.
//! - A stack cell describes each element without heap allocation or reference-count changes.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

const FRAME_SIZE: usize = 80;
const NEEDLE: usize = 0;
const STRICT: usize = 8;
const CURSOR: usize = 16;
const PAYLOAD: usize = 24;
const CELL: usize = 32;

/// Borrows needle/haystack cells in ABI args 0/1 and strictness in arg 2; returns bool or -1 on bad input.
pub fn emit_in_array_boxed(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_in_array_boxed");
    abi::emit_frame_prologue(emitter, FRAME_SIZE);
    abi::emit_store_to_sp(emitter, abi::int_arg_reg_name(emitter.target, 0), NEEDLE);
    abi::emit_store_to_sp(emitter, abi::int_arg_reg_name(emitter.target, 2), STRICT);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("mov x0, x1"),                     // unbox the borrowed haystack, not the saved needle
        Arch::X86_64 => emitter.instruction("mov rax, rsi"),                    // use the internal Mixed-unbox input register
    }
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub x9, x0, #4");                              // fold valid packed/hash tags into zero and one
            emitter.instruction("cmp x9, #1");                                  // only PHP arrays may be traversed
            emitter.instruction("b.hi __rt_in_array_boxed_invalid");            // reject scalar, object and null payloads before dereference
            abi::emit_store_to_sp(emitter, "x1", PAYLOAD);
        }
        Arch::X86_64 => {
            emitter.instruction("lea r10, [rax - 4]");                          // fold valid packed/hash tags into zero and one
            emitter.instruction("cmp r10, 1");                                  // only PHP arrays may be traversed
            emitter.instruction("ja __rt_in_array_boxed_invalid");              // reject scalar, object and null payloads before dereference
            abi::emit_store_to_sp(emitter, "rdi", PAYLOAD);
        }
    }
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::emit_store_to_sp(emitter, result, CURSOR);

    emitter.label("__rt_in_array_boxed_loop");
    abi::emit_load_temporary_stack_slot(emitter, abi::int_arg_reg_name(emitter.target, 0), PAYLOAD);
    abi::emit_load_temporary_stack_slot(emitter, abi::int_arg_reg_name(emitter.target, 1), CURSOR);
    abi::emit_call_label(emitter, "__rt_array_iter_next");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmn x0, #1");                                  // minus one signals iterator exhaustion, not a live element
            emitter.instruction("b.eq __rt_in_array_boxed_false");              // a completed scan found no equal value
            abi::emit_store_to_sp(emitter, "x3", CELL);
            abi::emit_store_to_sp(emitter, "x4", CELL + 8);
            abi::emit_store_to_sp(emitter, "x5", CELL + 16);
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, -1");                                 // minus one signals iterator exhaustion, not a live element
            emitter.instruction("je __rt_in_array_boxed_false");                // a completed scan found no equal value
            abi::emit_store_to_sp(emitter, "r8", CELL);
            abi::emit_store_to_sp(emitter, "r9", CELL + 8);
            abi::emit_store_to_sp(emitter, "r10", CELL + 16);
        }
    }
    abi::emit_store_to_sp(emitter, result, CURSOR);
    abi::emit_load_temporary_stack_slot(emitter, result, STRICT);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_in_array_boxed_loose");
    load_comparison_operands(emitter);
    abi::emit_call_label(emitter, "__rt_mixed_strict_eq");
    abi::emit_jump(emitter, "__rt_in_array_boxed_compared");
    emitter.label("__rt_in_array_boxed_loose");
    load_comparison_operands(emitter);
    abi::emit_call_label(emitter, "__rt_mixed_loose_eq");
    emitter.label("__rt_in_array_boxed_compared");
    abi::emit_branch_if_int_result_zero(emitter, "__rt_in_array_boxed_loop");
    abi::emit_jump(emitter, "__rt_in_array_boxed_return");

    emitter.label("__rt_in_array_boxed_invalid");
    abi::emit_load_int_immediate(emitter, result, -1);
    abi::emit_jump(emitter, "__rt_in_array_boxed_return");
    emitter.label("__rt_in_array_boxed_false");
    abi::emit_load_int_immediate(emitter, result, 0);
    emitter.label("__rt_in_array_boxed_return");
    abi::emit_frame_restore(emitter, FRAME_SIZE);
    abi::emit_return(emitter);
}

/// Passes two borrowed cells to the shared comparison helpers' platform ABI.
fn load_comparison_operands(emitter: &mut Emitter) {
    abi::emit_load_temporary_stack_slot(emitter, abi::int_arg_reg_name(emitter.target, 0), NEEDLE);
    abi::emit_temporary_stack_address(emitter, abi::int_arg_reg_name(emitter.target, 1), CELL);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// All supported targets borrow entries through the logical iterator without allocating owners.
    #[test]
    fn boxed_membership_is_allocation_free_on_every_target() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            emit_in_array_boxed(&mut emitter);
            let asm = emitter.output();
            for helper in ["__rt_array_iter_next", "__rt_mixed_strict_eq", "__rt_mixed_loose_eq"] {
                assert!(asm.contains(helper), "{target}: {helper}");
            }
            for forbidden in ["__rt_alloc", "__rt_incref", "__rt_decref", "__rt_mixed_from_value"] {
                assert!(!asm.contains(forbidden), "{target}: {forbidden}");
            }
            let mut comparison = Emitter::new(Target::parse(target).unwrap());
            super::super::emit_mixed_strict_eq(&mut comparison);
            let asm = comparison.output();
            if target == "linux-x86_64" {
                assert!(asm.contains("ucomisd xmm0, xmm1"), "{target}");
                assert!(asm.contains("jp __rt_mixed_strict_eq_false"), "{target}");
            } else {
                assert!(asm.contains("fcmp d0, d1"), "{target}");
                assert!(asm.contains("cset x0, eq"), "{target}");
            }
        }
    }
}
