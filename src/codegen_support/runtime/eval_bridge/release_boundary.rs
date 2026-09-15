//! Purpose:
//! Contains destructor exceptions raised while Rust eval releases a native Mixed owner.
//!
//! Called from:
//! - The shared eval bridge emitter on every supported target.
//!
//! Key details:
//! - The versioned C ABI consumes one value and updates an owned, nullable Throwable accumulator.
//! - Cleanup installs its jump target below Rust and restores the enclosing native exception state.
//! - Its integer result distinguishes a new destructor throw from an earlier pending exception.

use super::{abi, label_c_global, Emitter};

const FRAME: usize = 64;
const VALUE: usize = 8;
const OUTPUT: usize = 16;
const THROWN: usize = 24;
const CAUGHT: usize = 32;

/// Emits bounded value release, preserving and chaining any exception already owned by the slot.
pub(super) fn emit(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_value_release_v3");
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::emit_frame_prologue(emitter, FRAME);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), VALUE);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), OUTPUT);
    abi::load_at_offset(emitter, scratch, OUTPUT);
    abi::emit_load_from_address(emitter, result, scratch, 0);
    abi::emit_store_zero_to_address(emitter, scratch, 0);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_eval_release_begin");
    abi::emit_call_label(emitter, "__rt_throwable_take_boxed");
    emitter.label("__rt_eval_release_begin");
    abi::store_at_offset(emitter, result, THROWN);
    abi::load_at_offset(emitter, result, VALUE);
    super::super::exceptions::emit_guarded_cleanup_call(emitter, "__rt_decref_mixed", result, THROWN);
    abi::store_at_offset(emitter, result, CAUGHT);
    abi::load_at_offset(emitter, result, THROWN);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_eval_release_return");
    abi::emit_call_label(emitter, "__rt_throwable_box_owned");
    abi::load_at_offset(emitter, scratch, OUTPUT);
    abi::emit_store_to_address(emitter, result, scratch, 0);
    emitter.label("__rt_eval_release_return");
    abi::load_at_offset(emitter, result, CAUGHT);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target contains the native release before exporting one boxed exception owner to Rust.
    #[test]
    fn eval_value_release_boundary_returns_throwables_without_native_escape() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains(&target.extern_symbol("__elephc_eval_value_release_v3")), "{name}");
            assert!(asm.find("__rt_cleanup_invoke").unwrap() < asm.find("__rt_throwable_box_owned").unwrap(), "{name}");
            assert!(asm.find("__rt_throwable_take_boxed").unwrap() < asm.find("__rt_cleanup_invoke").unwrap(), "{name}");
            // Materializing one address mentions the symbol twice on AArch64 (page and offset).
            // Count calls instead, proving release cannot bypass the containing native boundary.
            let call = if target.arch == super::super::Arch::AArch64 { "bl" } else { "call" };
            assert!(asm.contains("__rt_decref_mixed"), "{name}");
            assert_eq!(asm.lines().filter(|line| line.trim() == format!("{call} __rt_cleanup_invoke")).count(), 1, "{name}");
            assert!(!asm.lines().any(|line| line.trim() == format!("{call} __rt_decref_mixed")), "{name}");
            assert!(!asm.contains("__rt_throw_current"), "{name}");
            let caught_flag = match target.arch {
                super::super::Arch::AArch64 => "ldur x0, [x29, #-32]",
                super::super::Arch::X86_64 => "mov rax, QWORD PTR [rbp - 32]",
            };
            assert!(asm.contains(caught_flag), "{name}: preserve the new-throw status across boxing");
        }
    }
}
