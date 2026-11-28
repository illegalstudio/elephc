//! Purpose:
//! Shares checked reference publication between scope-only eval AOT and the interpreter bridge.
//!
//! Called from:
//! - Both target implementations of the core eval scope-set helper.
//!
//! Key details:
//! - Snapshot installation never consumes assignment ownership or changes a reference child.
//! - Publication and destructor failures return through native containment before propagation.

use crate::codegen::{abi, emit::Emitter};

const FRAME: usize = 64;
const REFERENCE: usize = 8;
const VALUE: usize = 16;
const FLAGS: usize = 24;
const PREVIOUS: usize = 32;
const THROWN: usize = 40;
const STATUS: usize = 48;

/// Returns zero for ordinary replacement, one for successful reference assignment, or fatal status two.
pub(super) fn emit(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let ordinary = "__rt_eval_scope_assign_reference_ordinary";
    let handled = "__rt_eval_scope_assign_reference_handled";
    let done = "__rt_eval_scope_assign_reference_done";
    let thrown = "__rt_eval_scope_assign_reference_throw";
    emitter.label_global("__rt_eval_scope_assign_reference");
    abi::emit_frame_prologue(emitter, FRAME);
    for (index, offset) in [REFERENCE, VALUE, FLAGS].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    branch_if_flag_clear(emitter, 7, ordinary);
    abi::load_at_offset(emitter, result, REFERENCE);
    abi::emit_call_label(emitter, "__rt_reference_is");
    abi::emit_branch_if_int_result_zero(emitter, ordinary);
    for offset in [PREVIOUS, THROWN] { abi::emit_store_zero_to_local_slot(emitter, offset); }
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), REFERENCE);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), VALUE);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 2), 0);
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 3), PREVIOUS);
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 4), THROWN);
    abi::emit_call_label(emitter, &emitter.target.extern_symbol("__elephc_eval_value_reference_replace_v2"));
    abi::store_at_offset(emitter, result, STATUS);
    release(emitter, PREVIOUS);
    let input_released = "__rt_eval_scope_assign_reference_input_released";
    branch_if_flag_clear(emitter, 4, input_released);
    release(emitter, VALUE);
    emitter.label(input_released);
    abi::load_at_offset(emitter, result, THROWN);
    abi::emit_branch_if_int_result_nonzero(emitter, thrown);
    abi::load_at_offset(emitter, result, STATUS);
    abi::emit_branch_if_int_result_zero(emitter, handled);
    abi::emit_load_int_immediate(emitter, result, 2);
    abi::emit_jump(emitter, done);
    emitter.label(handled);
    abi::emit_load_int_immediate(emitter, result, 1);
    abi::emit_jump(emitter, done);
    emitter.label(ordinary);
    abi::emit_load_int_immediate(emitter, result, 0);
    emitter.label(done);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
    emitter.label(thrown);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_jump(emitter, "__rt_throw_boxed_destructor_exception");
}

/// Retires one owner while accumulating destructor failures with an already pending boxed Throwable.
fn release(emitter: &mut Emitter, offset: usize) {
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), offset);
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 1), THROWN);
    abi::emit_call_label(emitter, &emitter.target.extern_symbol("__elephc_eval_value_release_v3"));
}

/// Tests one scope-set input flag through the target's native integer argument width.
fn branch_if_flag_clear(emitter: &mut Emitter, bit: u8, label: &str) {
    let result = abi::int_result_reg(emitter);
    abi::load_at_offset(emitter, result, FLAGS);
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            emitter.instruction(&format!("tbz {result}, #{bit}, {label}"));     // bypass publication when the requested scope-set flag is absent
        }
        crate::codegen::platform::Arch::X86_64 => {
            emitter.instruction(&format!("test {result}, {}", 1_u64 << bit));   // inspect the scope-set input flag without changing the value owner
            emitter.instruction(&format!("jz {label}"));                        // bypass publication when the requested scope-set flag is absent
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::codegen::{emit::Emitter, platform::Target};
    use std::{io::Write, process::{Command, Stdio}};

    /// Real target assemblers verify the scope core and its checked publication ABI on every object format.
    #[test]
    #[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
    fn scope_only_checked_assignment_assembles_on_every_target() {
        for (name, triple) in [("linux-x86_64", "x86_64-linux-gnu"),
            ("linux-aarch64", "aarch64-linux-gnu"), ("macos-aarch64", "arm64-apple-macos11"),
            ("ios-arm64", "arm64-apple-ios13"), ("ios-sim-arm64", "arm64-apple-ios13-simulator")] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            super::super::emit_eval_scope_runtime(&mut emitter);
            let syntax = if target.arch == crate::codegen::platform::Arch::X86_64 { ".intel_syntax noprefix\n" } else { "" };
            let assembly = format!("{syntax}.text\n{}", emitter.output());
            let mut process = Command::new("clang").args(["-target", triple, "-x", "assembler", "-c", "-o", "/dev/null", "-"])
                .stdin(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("clang is required for this explicit target check");
            process.stdin.take().unwrap().write_all(assembly.as_bytes()).unwrap();
            let output = process.wait_with_output().unwrap();
            assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}
