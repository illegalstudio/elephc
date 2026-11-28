//! Purpose:
//! Validates and publishes native static reference rebinding from interpreted source.
//!
//! Called from:
//! - The scoped static binding selector and Magician's persistent reference adapter.
//!
//! Key details:
//! - A copied proposed source checks the new declaration without constraining reentrant writes.
//! - Publication replaces the binding, preserving aliases of its displaced reference.
//! - Staging cells stay rooted; all PHP cleanup exits are contained below Rust.

use super::*;

const FRAME: usize = 272;
const RECORD: usize = 8;
const REFERENCE: usize = 16;
const FLAGS: usize = 24;
const THROWABLE: usize = 32;
const CELL: usize = 40;
const PREVIOUS: usize = 48;
const STATUS: usize = 56;
const CONTEXT: usize = 80;
const OUTCOME: usize = 160;
const TEMPORARY: usize = 168;
const PIN: usize = 208;
const PROPOSED: usize = 248;
const _: () = assert!(PROPOSED + 16 <= FRAME);

/// Emits rebind(binding_descriptor_pair, borrowed_reference, caller_flags, throwable_out).
pub(super) fn emit(emitter: &mut Emitter, native_validation: bool) {
    let result = abi::int_result_reg(emitter);
    let invalid = "__rt_eval_static_rebind_invalid";
    let done = "__rt_eval_static_rebind_done";
    label_c_global(emitter, "__elephc_eval_value_static_rebind_v1");
    abi::emit_frame_prologue(emitter, FRAME);
    for (index, offset) in [RECORD, REFERENCE, FLAGS, THROWABLE].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::load_at_offset(emitter, result, THROWABLE);
    abi::emit_branch_if_int_result_zero(emitter, invalid);
    abi::emit_store_zero_to_address(emitter, result, 0);
    if native_validation { emit_checked(emitter); abi::emit_jump(emitter, done); }
    emitter.label(invalid);
    abi::emit_load_int_immediate(emitter, result, 1);
    emitter.label(done);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

/// Roots proposed storage while independently validating it and all existing declarations.
fn emit_checked(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let abort = "__rt_eval_static_rebind_abort";
    let released = "__rt_eval_static_rebind_released";
    abi::load_at_offset(emitter, result, FLAGS);
    abi::store_at_offset(emitter, result, CONTEXT);
    abi::emit_store_zero_to_local_slot(emitter, CONTEXT - 8);
    abi::load_at_offset(emitter, result, REFERENCE);
    abi::emit_call_label(emitter, "__rt_reference_is");
    abi::emit_branch_if_int_result_zero(emitter, "__rt_eval_static_rebind_invalid");
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 0), 7);
    abi::emit_call_label(emitter, "__rt_reference_cell_new");
    abi::store_at_offset(emitter, result, CELL);
    abi::store_at_offset(emitter, result, TEMPORARY);
    abi::load_at_offset(emitter, result, REFERENCE);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::load_at_offset(emitter, scratch, CELL);
    abi::emit_store_to_address(emitter, result, scratch, 0);
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 0), PIN);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), CELL);
    abi::emit_call_label(emitter, "__rt_reference_validation_pin_enter");
    abi::load_at_offset(emitter, result, CELL);
    abi::emit_call_label(emitter, "__rt_reference_cell_release");
    abi::emit_store_zero_to_local_slot(emitter, PROPOSED);
    abi::emit_frame_slot_address(emitter, result, TEMPORARY);
    abi::store_at_offset(emitter, result, PROPOSED - 8);
    abi::load_at_offset(emitter, result, CELL);
    abi::store_at_offset(emitter, result, PROPOSED - 16);
    load_record_word(emitter, result, 8);
    abi::store_at_offset(emitter, result, PROPOSED - 24);
    abi::load_at_offset(emitter, result, REFERENCE);
    abi::emit_call_label(emitter, "__rt_mixed_clone");
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 2), result);
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 0), CONTEXT);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), REFERENCE);
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 3), OUTCOME);
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 4), PROPOSED);
    abi::emit_call_label(emitter, "__rt_reference_publish_native_with_source");
    abi::store_at_offset(emitter, result, STATUS);
    abi::emit_branch_if_int_result_nonzero(emitter, abort);

    // Re-resolve current native storage after callbacks before detaching its actual owner.
    load_record_word(emitter, abi::int_arg_reg_name(emitter.target, 0), 0);
    load_record_word(emitter, abi::int_arg_reg_name(emitter.target, 1), 8);
    abi::emit_call_label(emitter, "__rt_reference_static_mixed_prepare_store");
    abi::store_at_offset(emitter, result, PREVIOUS);
    emit_bind(emitter);
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 0), PIN);
    abi::emit_call_label(emitter, "__rt_reference_validation_pin_take");
    load_record_word(emitter, scratch, 0);
    abi::emit_store_to_address(emitter, result, scratch, 0);
    abi::emit_store_zero_to_address(emitter, scratch, 8);
    abi::emit_load_int_immediate(emitter, result, 2);
    abi::emit_store_to_address(emitter, result, scratch, 16);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), PREVIOUS);
    emit_cell_cleanup(emitter);
    abi::emit_jump(emitter, released);

    emitter.label(abort);
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 0), PIN);
    abi::emit_call_label(emitter, "__rt_reference_validation_pin_take");
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 1), result);
    emit_cell_cleanup(emitter);
    emitter.label(released);
    abi::load_at_offset(emitter, result, CONTEXT - 8);
    let returned = "__rt_eval_static_rebind_returned";
    abi::emit_branch_if_int_result_zero(emitter, returned);
    abi::emit_call_label(emitter, "__rt_throwable_box_owned");
    abi::load_at_offset(emitter, scratch, THROWABLE);
    abi::emit_store_to_address(emitter, result, scratch, 0);
    abi::emit_load_int_immediate(emitter, result, 2);
    abi::store_at_offset(emitter, result, STATUS);
    emitter.label(returned);
    abi::load_at_offset(emitter, result, STATUS);
}

/// Loads an immutable user-module address without borrowing implicit scratch registers.
fn load_record_word(emitter: &mut Emitter, register: &str, offset: usize) {
    abi::load_at_offset(emitter, register, RECORD);
    abi::emit_load_from_address(emitter, register, register, offset);
}

/// Registers the proposed declaration and authenticates its current canonical wrapper.
fn emit_bind(emitter: &mut Emitter) {
    load_record_word(emitter, abi::int_arg_reg_name(emitter.target, 0), 0);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), CELL);
    load_record_word(emitter, abi::int_arg_reg_name(emitter.target, 2), 8);
    let entry = emitter.target.extern_symbol("__elephc_reference_sources_bind_v1");
    abi::emit_call_label(emitter, &entry);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_reference_sources_failure");
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), CELL);
    abi::emit_call_label(emitter, "__rt_reference_source_associate_native");
}

/// Consumes a managed cell through the existing protected cleanup accumulator.
fn emit_cell_cleanup(emitter: &mut Emitter) {
    abi::emit_symbol_address(emitter, abi::int_arg_reg_name(emitter.target, 0), "__rt_reference_cell_release");
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 2), CONTEXT - 8);
    abi::emit_call_label(emitter, "__rt_cleanup_invoke");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Scoped binding records and owning aliases agree with checked physical storage on every ABI.
    #[test]
    fn checked_static_reference_aliases_own_their_cell_on_every_target() {
        let tokens = crate::lexer::tokenize("<?php class RebindOwner { public static string $text = 'old'; } \
            $first =& RebindOwner::$text; echo $first;").unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            for feature in [elephc_semantics::StrictFeature::Casts, elephc_semantics::StrictFeature::Coercions] {
                let profile = elephc_semantics::SemanticProfile::PERMISSIVE.with(feature, true);
                let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
                let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
                crate::ir::validate_module(&module).unwrap();
                assert!(module.functions.iter().flat_map(|function| &function.instructions).any(|inst|
                    inst.op == crate::ir::Op::BindRefCellPtr
                        && matches!(inst.immediate, Some(crate::ir::Immediate::LocalSlotPair { .. }))),
                    "{name} {feature:?}: native static aliases need a cell owner");
                crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            }
        }
    }

    /// Real assemblers accept the protected rebind, sharing and destination transitions on every ABI.
    #[test]
    #[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
    fn static_reference_rebinding_assembles_on_every_supported_target() {
        use std::{io::Write, process::{Command, Stdio}};
        for (name, triple) in [("linux-x86_64", "x86_64-linux-gnu"),
            ("linux-aarch64", "aarch64-linux-gnu"), ("macos-aarch64", "arm64-apple-macos11"),
            ("ios-arm64", "arm64-apple-ios13"), ("ios-sim-arm64", "arm64-apple-ios13-simulator")] {
            for selected in [false, true] {
                let mut emitter = Emitter::new(Target::parse(name).unwrap());
                emit(&mut emitter, selected);
                super::super::static_write::emit(&mut emitter, selected);
                if selected {
                    super::super::super::reference_cells::emit_reference_cells(&mut emitter, true, true);
                    super::super::super::reference_cells::emit_validation_publish(&mut emitter);
                    super::super::super::reference_cells::emit_validation_commit(&mut emitter);
                }
                let syntax = if name == "linux-x86_64" { ".intel_syntax noprefix\n" } else { "" };
                let assembly = format!("{syntax}.text\n{}", emitter.output());
                assert_eq!(assembly.contains("__rt_reference_publish_native"), selected, "{name}");
                let mut process = Command::new("clang").args(["-target", triple, "-x", "assembler",
                    "-c", "-o", "/dev/null", "-"]).stdin(Stdio::piped()).stderr(Stdio::piped())
                    .spawn().expect("clang is required for explicit cross-target assembly validation");
                process.stdin.take().unwrap().write_all(assembly.as_bytes()).unwrap();
                let output = process.wait_with_output().unwrap();
                assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
            }
        }
    }
}
