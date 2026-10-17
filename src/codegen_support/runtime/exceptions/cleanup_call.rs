//! Purpose:
//! Invokes one cleanup callback while retaining any thrown exception for its owning cleanup frame.
//!
//! Called from:
//! - Deep object release, which must finish releasing siblings before propagating a destructor throw.
//!
//! Key details:
//! - The C arguments are a unary native entry, its payload, and an owned pending-Throwable slot.
//! - Both internal and C unary argument registers receive the payload on x86_64.
//! - The caller's handler and active exception are restored before ordinary cleanup resumes.
//! - The integer result reports a newly caught throw, independently of the pending owner slot.
//! - Validated scalar Mixed and empty typed-array releases keep the original dispatcher without a new handler.

mod scalar_mixed;
mod empty_array;

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE,
};

const FRAME: usize = TRY_HANDLER_SLOT_SIZE + 64;
const HANDLER: usize = FRAME - 16;
const ENTRY: usize = 8;
const PAYLOAD: usize = 16;
const OUTPUT: usize = 24;
const PREVIOUS: usize = 32;
const CAUGHT: usize = 40;

/// Emits the non-escaping unary cleanup boundary; the caller owns the pending exception slot.
pub fn emit_cleanup_invoke(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let arg0 = abi::int_arg_reg_name(emitter.target, 0);
    let older = match emitter.target.arch { Arch::AArch64 => "x1", Arch::X86_64 => "rdi" };
    let scratch = abi::secondary_scratch_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_cleanup_invoke");
    scalar_mixed::emit_fast_path(emitter);
    // -- save the callback and install a handler whose lifetime ends before returning --
    abi::emit_frame_prologue(emitter, FRAME);
    for (index, offset) in [ENTRY, PAYLOAD, OUTPUT].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::emit_load_symbol_to_reg(emitter, result, "_exc_value", 0);
    abi::store_at_offset(emitter, result, PREVIOUS);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::emit_store_zero_to_local_slot(emitter, CAUGHT);
    for (symbol, offset) in [
        ("_exc_handler_top", HANDLER),
        ("_exc_call_frame_top", HANDLER - 8),
        ("_rt_diag_suppression", HANDLER - TRY_HANDLER_DIAG_DEPTH_OFFSET),
    ] {
        abi::emit_load_symbol_to_reg(emitter, result, symbol, 0);
        abi::store_at_offset(emitter, result, offset);
    }
    abi::emit_frame_slot_address(emitter, result, HANDLER);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_handler_top", 0);
    abi::emit_frame_slot_address(emitter, arg0, HANDLER - TRY_HANDLER_JMP_BUF_OFFSET);
    emitter.bl_c("setjmp");                                                     // retain cleanup control when the unary callback throws
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_cleanup_invoke_caught");
    abi::load_at_offset(emitter, result, PAYLOAD);
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // support both internal and System V unary callback entries
    }
    abi::load_at_offset(emitter, scratch, ENTRY);
    abi::emit_call_reg(emitter, scratch);
    abi::emit_jump(emitter, "__rt_cleanup_invoke_return");

    // -- publish the newest exception before linking any older pending cleanup throw --
    emitter.label("__rt_cleanup_invoke_caught");
    abi::emit_load_int_immediate(emitter, result, 1);
    abi::store_at_offset(emitter, result, CAUGHT);
    abi::emit_load_symbol_to_reg(emitter, result, "_exc_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::load_at_offset(emitter, scratch, OUTPUT);
    abi::emit_load_from_address(emitter, older, scratch, 0);
    abi::emit_store_to_address(emitter, result, scratch, 0);
    abi::emit_call_label(emitter, "__rt_exception_chain");
    emitter.label("__rt_cleanup_invoke_return");
    for (symbol, offset) in [
        ("_exc_handler_top", HANDLER),
        ("_rt_diag_suppression", HANDLER - TRY_HANDLER_DIAG_DEPTH_OFFSET),
        ("_exc_value", PREVIOUS),
    ] {
        abi::load_at_offset(emitter, result, offset);
        abi::emit_store_reg_to_symbol(emitter, result, symbol, 0);
    }
    abi::load_at_offset(emitter, result, CAUGHT);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
    empty_array::emit_candidate(emitter, scalar_mixed::PROTECTED, scalar_mixed::DIRECT);
}

/// Passes one unary cleanup through the bounded helper, accumulating throws in a caller frame slot.
pub(crate) fn emit_guarded_cleanup_call(
    emitter: &mut Emitter,
    entry: &str,
    payload: &str,
    pending_offset: usize,
) {
    let arg0 = abi::int_arg_reg_name(emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(emitter.target, 1);
    let arg2 = abi::int_arg_reg_name(emitter.target, 2);
    abi::emit_reg_move(emitter, arg1, payload);
    abi::emit_symbol_address(emitter, arg0, entry);
    abi::emit_frame_slot_address(emitter, arg2, pending_offset);
    abi::emit_call_label(emitter, "__rt_cleanup_invoke");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every supported target captures callback exceptions and restores native state before returning.
    #[test]
    fn cleanup_invocation_is_bounded_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_cleanup_invoke(&mut emitter);
            let asm = emitter.output();
            let enter = asm.find(&target.extern_symbol("setjmp")).unwrap();
            let invoke = asm.find(if target.arch == Arch::AArch64 { "blr x10" } else { "call r10" }).unwrap();
            let caught = asm.find("__rt_cleanup_invoke_caught:").unwrap();
            let chain = asm.find("__rt_exception_chain").unwrap();
            assert!(enter < invoke && invoke < caught && caught < chain, "{name}");
            assert!(!asm.contains("__rt_throw_current"), "{name}");
            assert!(asm.matches("_exc_handler_top").count() >= 3, "{name}");
            let caught_flag = match target.arch {
                Arch::AArch64 => "ldur x0, [x29, #-40]",
                Arch::X86_64 => "mov rax, QWORD PTR [rbp - 40]",
            };
            assert!(asm.contains(caught_flag), "{name}: return a separate new-throw flag");
        }
    }

    /// Validated scalar cells bypass handlers with identical proof gates in direct and PIC code.
    #[test]
    fn scalar_mixed_cleanup_gate_covers_all_targets_and_pic_modes() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for pic in [false, true] {
                let target = Target::parse(name).unwrap();
                let mut emitter = Emitter::new(target);
                emitter.pic_data_refs = pic;
                emit_cleanup_invoke(&mut emitter);
                let asm = emitter.output();
                let (fast, protected) = asm.split_once("__rt_cleanup_invoke_protected:").unwrap();
                assert!(fast.contains("__rt_decref_any"), "{name}, pic={pic}");
                assert!(fast.contains("_heap_buf") && fast.contains("_heap_off"), "{name}, pic={pic}");
                assert!(!fast.contains("_exc_value") && !fast.contains("_exc_handler_top"), "{name}, pic={pic}");
                assert!(!fast.contains("setjmp"), "{name}, pic={pic}");
                let callback = if target.arch == Arch::AArch64 {
                    ("mov x9, x0", "blr x9")
                } else {
                    ("mov r11, rdi", "call r11")
                };
                assert!(fast.contains(callback.0) && fast.contains(callback.1),
                    "{name}, pic={pic}: invoke the authenticated callback pointer unchanged");
                assert!(protected.contains(&target.extern_symbol("setjmp")), "{name}, pic={pic}");
                assert!(protected.contains("__rt_cleanup_invoke_caught"), "{name}, pic={pic}");
                let (tag, branch) = if target.arch == Arch::AArch64 { ("cmp x10, #", "b.eq") }
                    else { ("cmp r10, ", "je") };
                for ty in [crate::types::PhpType::Int, crate::types::PhpType::Float,
                    crate::types::PhpType::Bool, crate::types::PhpType::Void] {
                    let value = crate::codegen_support::value_boxing::runtime_value_tag(&ty);
                    assert!(fast.contains(&format!("{tag}{value}")), "{name}, pic={pic}: {ty:?}");
                }
                assert_eq!(fast.matches(&format!("{branch} __rt_cleanup_invoke_direct")).count(), 4,
                    "{name}, pic={pic}: only four scalar tags qualify");
            }
        }
    }
    /// Typed empty arrays qualify by live storage and length, even with reserved capacity.
    #[test]
    fn empty_array_cleanup_gate_covers_all_targets_and_pic_modes() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for pic in [false, true] {
                let target = Target::parse(name).unwrap();
                let mut emitter = Emitter::new(target);
                emitter.pic_data_refs = pic;
                emit_cleanup_invoke(&mut emitter);
                let asm = emitter.output();
                let (_, probe) = asm.split_once(&format!("{}:", empty_array::CANDIDATE)).unwrap();
                assert!(probe.contains("__rt_decref_array"), "{name}, pic={pic}");
                assert!(probe.contains("_heap_buf") && probe.contains("_heap_off"), "{name}, pic={pic}");
                assert!(!probe.contains("_exc_value") && !probe.contains("setjmp"), "{name}, pic={pic}");
                assert!(probe.contains(scalar_mixed::PROTECTED) && probe.contains(scalar_mixed::DIRECT),
                    "{name}, pic={pic}");
                let required: &[&str] = if target.arch == Arch::AArch64 {
                    &["ldr w10, [x1, #-12]", "cmp w10, #24", "cmp x10, #2",
                        "ldr x10, [x1]", "cbnz x10, __rt_cleanup_invoke_protected"]
                } else {
                    &["cmp DWORD PTR [rsi - 12], 0", "cmp DWORD PTR [rsi - 16], 24",
                        "cmp r10d, 2", "cmp QWORD PTR [rsi], 0", "jne __rt_cleanup_invoke_protected"]
                };
                for instruction in required {
                    assert!(probe.contains(instruction), "{name}, pic={pic}: {instruction}");
                }
                assert!(!probe.contains("[x1, #8]") && !probe.contains("[rsi + 8]"),
                    "{name}, pic={pic}: reserved capacity is not logical child ownership");
            }
        }
    }

    /// Both ARM64 probes must retain the heap base until the upper bound rejects foreign pointers.
    #[test]
    fn cleanup_arm64_bounds_preserve_base_before_header_reads() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64"] {
            for pic in [false, true] {
                let target = Target::parse(name).unwrap();
                let mut emitter = Emitter::new(target);
                emitter.pic_data_refs = pic;
                emit_cleanup_invoke(&mut emitter);
                let asm = emitter.output();
                let scalar = asm.split_once("__rt_cleanup_invoke_direct:").unwrap().0;
                let array = asm.split_once(&format!("{}:", empty_array::CANDIDATE)).unwrap().1;

                // -- resolving the bump offset must not overwrite the live base in x9 --
                let mut bounds = Emitter::new(target);
                bounds.pic_data_refs = pic;
                abi::emit_symbol_address(&mut bounds, "x10", "_heap_off");
                bounds.instruction("ldr x10, [x10]");                           // read the bump offset through its own address register
                bounds.instruction("add x10, x9, x10");                         // combine the offset with the preserved heap base
                bounds.instruction("cmp x1, x10");                              // bound the payload before any header access
                bounds.instruction("b.hs __rt_cleanup_invoke_protected");        // foreign pointers must take the protected fallback
                bounds.instruction("ldr w10, [x1, #-12]");                      // only an in-range payload permits a refcount read
                let required = bounds.output();
                for (kind, probe) in [("scalar", scalar), ("empty array", array)] {
                    assert!(probe.contains(&required),
                        "{name}, pic={pic}, {kind}: preserve x9 and reject the heap end before reading the header\n{probe}");
                }
            }
        }
    }

}
