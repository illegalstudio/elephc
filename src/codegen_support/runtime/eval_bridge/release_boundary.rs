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

use super::{abi, Emitter};

const FRAME: usize = 64;
const VALUE: usize = 8;
const OUTPUT: usize = 16;
const THROWN: usize = 24;
const CAUGHT: usize = 32;

/// Emits bounded value release, preserving and chaining any exception already owned by the slot.
pub(super) fn emit(emitter: &mut Emitter) {
    // Rust enters through the target C ABI; the adapter preserves every Win64
    // nonvolatile register and presents the body with elephc's internal helper
    // convention before it invokes nested runtime cleanup helpers.
    abi::emit_c_callback_entry_with_arity(emitter, "__elephc_eval_value_release_v3", 2);
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::emit_frame_prologue(emitter, FRAME);
    abi::store_at_offset(emitter, abi::runtime_helper_int_arg_reg(emitter, 0), VALUE);
    abi::store_at_offset(emitter, abi::runtime_helper_int_arg_reg(emitter, 1), OUTPUT);
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
        for name in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
            "windows-x86_64",
        ] {
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

    /// The Windows callback adapter preserves Win64 state and translates
    /// RCX/RDX into the runtime's internal RDI/RSI argument convention.
    #[test]
    fn windows_release_boundary_preserves_native_arguments() {
        let target = Target::parse("windows-x86_64").unwrap();
        let mut emitter = Emitter::new(target);
        emit(&mut emitter);
        let asm = emitter.output();
        let cleanup = asm.find("call __rt_cleanup_invoke").unwrap();
        let entry = &asm[..cleanup];

        assert!(entry.contains("__elephc_eval_value_release_v3:"));
        assert!(entry.contains("mov rdi, rcx"));
        assert!(entry.contains("mov rsi, rdx"));
        assert!(entry.contains("mov QWORD PTR [rsp + 0], r10"));
        assert!(entry.contains("mov QWORD PTR [rsp + 8], r11"));
        assert!(entry.contains("movdqu XMMWORD PTR [rsp + 16], xmm6"));
        assert!(entry.contains("movdqu XMMWORD PTR [rsp + 160], xmm15"));
        assert!(entry.contains("mov QWORD PTR [rbp - 8], rdi"));
        assert!(entry.contains("mov QWORD PTR [rbp - 16], rsi"));
        assert!(!entry.contains("mov QWORD PTR [rbp - 8], rcx"));
        assert!(!entry.contains("mov QWORD PTR [rbp - 16], rdx"));
        assert!(entry.contains("mov rsi, rax"));
        assert!(entry.contains("lea rdi, [rip + __rt_decref_mixed]"));
        assert!(entry.contains("lea rdx, [rbp - 24]"));
    }
}
