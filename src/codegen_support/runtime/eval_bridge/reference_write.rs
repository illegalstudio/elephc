//! Purpose:
//! Contains native declaration validation before eval replaces a persistent reference child.
//!
//! Called from:
//! - Eval bridge emission and the versioned RuntimeValueOps replacement hook.
//!
//! Key details:
//! - Borrowed inputs stay unchanged; success transfers the displaced owner to Rust.
//! - The native publisher roots conversion snapshots and contains every PHP Throwable.
//! - Builds without native validation retain the ordinary reference replacement contract.

use super::*;

const FRAME: usize = 176;
const REFERENCE: usize = 8;
const VALUE: usize = 16;
const FLAGS: usize = 24;
const PREVIOUS: usize = 32;
const THROWABLE: usize = 40;
const STATUS: usize = 48;
const CONTEXT: usize = 72;
const OUTCOME: usize = 152;

/// Emits replace(reference, borrowed_rhs, caller_flags, previous_out, throwable_out).
pub(super) fn emit(emitter: &mut Emitter, native_validation: bool) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let invalid = "__rt_eval_reference_replace_invalid";
    let done = "__rt_eval_reference_replace_done";
    label_c_global(emitter, "__elephc_eval_value_reference_replace_v2");
    abi::emit_frame_prologue(emitter, FRAME);
    for (index, offset) in [REFERENCE, VALUE, FLAGS, PREVIOUS, THROWABLE].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    for offset in [PREVIOUS, THROWABLE] {
        abi::load_at_offset(emitter, result, offset);
        abi::emit_branch_if_int_result_zero(emitter, invalid);
        abi::emit_store_zero_to_address(emitter, result, 0);
    }
    if native_validation {
        abi::load_at_offset(emitter, result, FLAGS);
        abi::store_at_offset(emitter, result, CONTEXT);
        abi::emit_store_zero_to_local_slot(emitter, CONTEXT - 8);
        abi::load_at_offset(emitter, result, VALUE);
        abi::emit_call_label(emitter, "__rt_mixed_clone");
        abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 2), result);
        abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 0), CONTEXT);
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), REFERENCE);
        abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 3), OUTCOME);
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 4), PREVIOUS);
        abi::emit_call_label(emitter, "__rt_reference_publish_native_deferred");
        abi::store_at_offset(emitter, result, STATUS);
        abi::load_at_offset(emitter, result, CONTEXT - 8);
        let returned = "__rt_eval_reference_replace_return_status";
        abi::emit_branch_if_int_result_zero(emitter, returned);
        abi::emit_call_label(emitter, "__rt_throwable_box_owned");
        abi::load_at_offset(emitter, scratch, THROWABLE);
        abi::emit_store_to_address(emitter, result, scratch, 0);
        emitter.label(returned);
        abi::load_at_offset(emitter, result, STATUS);
    } else {
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), REFERENCE);
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), VALUE);
        let replace = emitter.target.extern_symbol("__elephc_eval_value_reference_replace");
        abi::emit_call_label(emitter, &replace);
        abi::load_at_offset(emitter, scratch, PREVIOUS);
        abi::emit_store_to_address(emitter, result, scratch, 0);
        abi::emit_load_int_immediate(emitter, result, 0);
    }
    abi::emit_jump(emitter, done);
    emitter.label(invalid);
    abi::emit_load_int_immediate(emitter, result, 1);
    emitter.label(done);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, process::{Command, Stdio}};

    /// Every target exports a containing boundary and real assemblers accept both capability paths.
    #[test]
    #[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
    fn eval_reference_replacement_assembles_on_every_target() {
        for (name, triple) in [("linux-x86_64", "x86_64-linux-gnu"),
            ("linux-aarch64", "aarch64-linux-gnu"), ("macos-aarch64", "arm64-apple-macos11"),
            ("ios-arm64", "arm64-apple-ios13"), ("ios-sim-arm64", "arm64-apple-ios13-simulator")] {
            for selected in [false, true] {
                let target = crate::codegen_support::platform::Target::parse(name).unwrap();
                let mut emitter = Emitter::new(target);
                emit(&mut emitter, selected);
                super::super::static_write::emit(&mut emitter, selected);
                if selected {
                    crate::codegen_support::runtime::reference_cells::emit_validation_publish(&mut emitter);
                    crate::codegen_support::runtime::reference_cells::emit_validation_commit(&mut emitter);
                }
                let syntax = if target.arch == Arch::X86_64 { ".intel_syntax noprefix\n" } else { "" };
                let assembly = format!("{syntax}.text\n{}", emitter.output());
                assert_eq!(assembly.contains("__rt_reference_publish_native_deferred"), selected, "{name}");
                assert!(!assembly.contains("__rt_throw_current"), "{name}: no PHP exit crosses Rust");
                let mut process = Command::new("clang").args(["-target", triple, "-x", "assembler",
                    "-c", "-o", "/dev/null", "-"]).stdin(Stdio::piped()).stderr(Stdio::piped())
                    .spawn().expect("clang is required for the explicit cross-target assembly test");
                process.stdin.take().unwrap().write_all(assembly.as_bytes()).unwrap();
                let output = process.wait_with_output().unwrap();
                assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
            }
        }
    }
}
