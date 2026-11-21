//! Purpose:
//! Contains scoped static setter exceptions and selects checked canonical publication.
//!
//! Called from:
//! - User-assembly eval static setter emission for every supported target.
//!
//! Key details:
//! - The versioned entry borrows seven scoped setter arguments and returns a boxed Throwable.
//! - Existing native callers retain the original boolean setter ABI.
//! - Checked slot bodies share the same runtime publisher as direct EIR assignments.

use super::*;

const FRAME: usize = 160;
const OUTPUT: usize = 8;
const PENDING: usize = 16;
const CAUGHT: usize = 24;
const RECORD: usize = 128;
const RESULT: usize = RECORD - 56;

/// Emits set(class, class_len, name, name_len, value, scope, scope_len, throwable_out).
pub(super) fn emit(module: &Module, emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let frame_pointer = abi::frame_pointer_reg(emitter);
    let invalid = "__rt_eval_static_set_v2_invalid";
    let done = "__rt_eval_static_set_v2_done";
    label_c_global(module, emitter, "__elephc_eval_value_static_property_set_v2");
    abi::emit_frame_prologue(emitter, FRAME);
    for index in 0..8 {
        let register = if emitter.target.arch == Arch::AArch64 || index < 6 {
            abi::int_arg_reg_name(emitter.target, index)
        } else {
            abi::emit_load_from_address(emitter, scratch, frame_pointer, 16 + (index - 6) * 8);
            scratch
        };
        abi::store_at_offset(emitter, register, if index == 7 { OUTPUT } else { RECORD - index * 8 });
    }
    abi::load_at_offset(emitter, result, OUTPUT);
    abi::emit_branch_if_int_result_zero(emitter, invalid);
    abi::emit_store_zero_to_address(emitter, result, 0);
    abi::emit_store_zero_to_local_slot(emitter, PENDING);
    abi::emit_store_zero_to_local_slot(emitter, RESULT);
    abi::emit_local_symbol_address(emitter, abi::int_arg_reg_name(emitter.target, 0), "__rt_eval_static_set_v2_body");
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 1), RECORD);
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 2), PENDING);
    abi::emit_call_label(emitter, "__rt_cleanup_invoke");
    abi::store_at_offset(emitter, result, CAUGHT);
    abi::load_at_offset(emitter, result, PENDING);
    let success = "__rt_eval_static_set_v2_result";
    abi::emit_branch_if_int_result_zero(emitter, success);
    abi::emit_call_label(emitter, "__rt_throwable_box_owned");
    abi::load_at_offset(emitter, scratch, OUTPUT);
    abi::emit_store_to_address(emitter, result, scratch, 0);
    abi::emit_load_int_immediate(emitter, result, 2);
    abi::emit_jump(emitter, done);
    emitter.label(success);
    abi::load_at_offset(emitter, result, CAUGHT);
    abi::emit_branch_if_int_result_nonzero(emitter, invalid);
    abi::load_at_offset(emitter, result, RESULT);
    abi::emit_jump(emitter, done);
    emitter.label(invalid);
    abi::emit_load_int_immediate(emitter, result, 3);
    emitter.label(done);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
    emit_body(emitter);
}

/// Calls the legacy scoped dispatcher only beneath the native containing frame.
fn emit_body(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let record = abi::secondary_scratch_reg(emitter);
    emitter.label("__rt_eval_static_set_v2_body");
    abi::emit_frame_prologue(emitter, 32);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
    abi::load_at_offset(emitter, record, 8);
    if emitter.target.arch == Arch::X86_64 {
        abi::emit_reserve_temporary_stack(emitter, 16);
        abi::emit_load_from_address(emitter, result, record, 48);
        abi::emit_store_to_sp(emitter, result, 0);
    }
    let registers = if emitter.target.arch == Arch::AArch64 { 7 } else { 6 };
    for index in 0..registers {
        abi::emit_load_from_address(emitter, abi::int_arg_reg_name(emitter.target, index), record, index * 8);
    }
    let setter = emitter.target.extern_symbol("__elephc_eval_value_static_property_set");
    abi::emit_call_label(emitter, &setter);
    if emitter.target.arch == Arch::X86_64 { abi::emit_release_temporary_stack(emitter, 16); }
    abi::load_at_offset(emitter, record, 8);
    abi::emit_store_to_address(emitter, result, record, 56);
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);
}

/// Emits a checked slot body and returns whether the legacy typed store must be skipped.
pub(super) fn emit_checked_slot(module: &Module, emitter: &mut Emitter,
    slot: &EvalStaticPropertySlot, fail: &str,
) -> bool {
    let Some(descriptor) = crate::codegen::runtime_metadata::checked_boxed_static_descriptor(
        module, &slot.declaring_class, &slot.property) else { return false; };
    let result = abi::int_result_reg(emitter);
    let prefix = slot_body_label_raw(slot, "checked_set");
    let done = format!("{prefix}_done");
    let missing = format!("{prefix}_missing");
    abi::emit_reserve_temporary_stack(emitter, 16);
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::emit_store_to_sp(emitter, result, 0);
    let value_offset = if emitter.target.arch == Arch::AArch64 { 32 } else { 40 };
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 2), value_offset);
    abi::emit_symbol_address(emitter, abi::int_arg_reg_name(emitter.target, 0), &slot.symbol);
    abi::emit_symbol_address(emitter, abi::int_arg_reg_name(emitter.target, 1), &descriptor);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 3), 0);
    abi::emit_temporary_stack_address(emitter, abi::int_arg_reg_name(emitter.target, 4), 0);
    let store = emitter.target.extern_symbol("__elephc_eval_value_static_store_v1");
    abi::emit_call_label(emitter, &store);
    abi::emit_branch_if_int_result_zero(emitter, &done);
    abi::emit_load_temporary_stack_slot(emitter, result, 0);
    abi::emit_branch_if_int_result_zero(emitter, &missing);
    abi::emit_call_label(emitter, "__rt_throwable_take_boxed");
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    abi::emit_release_temporary_stack(emitter, 16);
    abi::emit_jump(emitter, "__rt_throw_current");
    emitter.label(&missing);
    abi::emit_release_temporary_stack(emitter, 16);
    abi::emit_jump(emitter, fail);
    emitter.label(&done);
    abi::emit_release_temporary_stack(emitter, 16);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, process::{Command, Stdio}};

    /// Both capability paths assemble with the scoped setter's register and stack arguments intact.
    #[test]
    #[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
    fn protected_static_setters_assemble_on_every_target() {
        let tokens = crate::lexer::tokenize("<?php class BoundaryStatic { public static float $value = 1.0; }").unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        let checked = crate::types::check(&program).unwrap();
        for (name, triple) in [("linux-x86_64", "x86_64-linux-gnu"),
            ("linux-aarch64", "aarch64-linux-gnu"), ("macos-aarch64", "arm64-apple-macos11"),
            ("ios-arm64", "arm64-apple-ios13"), ("ios-sim-arm64", "arm64-apple-ios13-simulator")] {
            for selected in [false, true] {
                let target = crate::codegen_support::platform::Target::parse(name).unwrap();
                let mut module = Module::new(target);
                module.semantic_profile = elephc_semantics::SemanticProfile::PERMISSIVE
                    .with(elephc_semantics::StrictFeature::Casts, selected);
                module.class_infos.insert("BoundaryStatic".into(), checked.classes["BoundaryStatic"].clone());
                let mut emitter = Emitter::new(target);
                emit(&module, &mut emitter);
                emitter.label("__test_static_set_slot");
                let slots = collect_eval_static_property_slots(&module);
                assert_eq!(emit_checked_slot(&module, &mut emitter, &slots[0], "__test_static_set_miss"), selected);
                emitter.label("__test_static_set_miss");
                abi::emit_return(&mut emitter);
                let syntax = if target.arch == Arch::X86_64 { ".intel_syntax noprefix\n" } else { "" };
                let assembly = format!("{syntax}.text\n{}", emitter.output());
                assert!(assembly.contains("__rt_cleanup_invoke"), "{name}");
                assert_eq!(assembly.contains("__elephc_eval_value_static_store_v1"), selected, "{name}");
                if target.arch == Arch::X86_64 {
                    assert!(assembly.contains("[rbp + 16]") && assembly.contains("[rbp + 24]"),
                        "{name}: scope length and Throwable output arrive on the caller stack");
                }
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
