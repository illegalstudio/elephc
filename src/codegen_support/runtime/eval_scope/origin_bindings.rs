//! Purpose:
//! Publishes fresh eval bindings through the existing canonical native reference coordinator.
//!
//! Called from:
//! - Checked scope stores, scope destruction and interpreter unset through contained C exports.
//!
//! Key details:
//! - Producer descriptors apply only to fresh names; native snapshots record open imports.
//! - A stable registry slot owns one managed reference cell until unset or activation retirement.
//! - Cell retirement contains all PHP destructors before returning to a Rust caller.

use crate::codegen::{abi, emit::Emitter};

const FRAME: usize = 96;
const SCOPE: usize = 8;
const NAME: usize = 16;
const LEN: usize = 24;
const VALUE: usize = 32;
const FLAGS: usize = 40;
const DESCRIPTOR: usize = 48;
const BINDING: usize = 56;
const CELL: usize = 64;
const THROWN: usize = 72;

/// Emits profile-selected publication and contained lifetime adapters for both scope runtimes.
pub(super) fn emit(emitter: &mut Emitter, enabled: bool) {
    emitter.label_global(&emitter.target.extern_symbol("__elephc_eval_scope_visit_origins_v1"));
    if enabled {
        abi::emit_jump(emitter, &emitter.target.extern_symbol("__elephc_reference_scope_visit_v1"));
    } else { abi::emit_return(emitter); }
    emit_set(emitter, enabled);
    emit_retire(emitter, enabled, false);
    emit_retire(emitter, enabled, true);
    if enabled {
        emitter.label_global("__rt_eval_scope_origin_release_cell");
        abi::emit_reg_move(emitter, abi::int_result_reg(emitter), abi::int_arg_reg_name(emitter.target, 0));
        abi::emit_jump(emitter, "__rt_reference_cell_release");
    }
}

/// Keeps the five ordinary scope-set arguments and a sixth immutable producer descriptor.
fn emit_set(emitter: &mut Emitter, enabled: bool) {
    emitter.label_global("__rt_eval_scope_set_with_contract");
    if !enabled {
        abi::emit_jump(emitter, &emitter.target.extern_symbol("__elephc_eval_scope_set"));
        return;
    }
    let result = abi::int_result_reg(emitter);
    let pointer = abi::secondary_scratch_reg(emitter);
    let publish = "__rt_eval_scope_set_with_contract_publish";
    abi::emit_frame_prologue(emitter, FRAME);
    for (index, offset) in [SCOPE, NAME, LEN, VALUE, FLAGS, DESCRIPTOR].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    // Snapshot installation imports a pre-existing contract, including explicit mixed.
    abi::load_at_offset(emitter, result, FLAGS);
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            emitter.instruction("tbnz x0, #7, __rt_eval_scope_set_with_contract_origin"); // PHP assignment may establish an inferred origin
        }
        crate::codegen::platform::Arch::X86_64 => {
            emitter.instruction("test rax, 128");                               // native snapshots never establish a producer contract
            emitter.instruction("jnz __rt_eval_scope_set_with_contract_origin"); // only PHP assignment selects its inferred producer
        }
    }
    abi::emit_store_zero_to_local_slot(emitter, DESCRIPTOR);
    emitter.label("__rt_eval_scope_set_with_contract_origin");
    for (index, offset) in [SCOPE, NAME, LEN, DESCRIPTOR].into_iter().enumerate() {
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::emit_call_label(emitter, &emitter.target.extern_symbol("__elephc_reference_scope_binding_v1"));
    abi::emit_branch_if_int_result_zero(emitter, publish);
    abi::store_at_offset(emitter, result, BINDING);
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::emit_branch_if_int_result_nonzero(emitter, publish);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 0), 7);
    abi::emit_call_label(emitter, "__rt_reference_cell_new");
    abi::store_at_offset(emitter, result, CELL);
    abi::load_at_offset(emitter, pointer, VALUE);
    abi::emit_store_to_address(emitter, pointer, result, 0);
    abi::emit_call_label(emitter, "__rt_reference_cell_promote_mixed");
    abi::load_at_offset(emitter, pointer, BINDING);
    abi::load_at_offset(emitter, result, CELL);
    abi::emit_store_to_address(emitter, result, pointer, 0);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), BINDING);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), CELL);
    abi::emit_load_from_address(emitter, abi::int_arg_reg_name(emitter.target, 2), pointer, 8);
    abi::emit_call_label(emitter, &emitter.target.extern_symbol("__elephc_reference_sources_bind_v1"));
    abi::emit_branch_if_int_result_zero(emitter, "__rt_reference_sources_failure");
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), CELL);
    abi::emit_call_label(emitter, "__rt_reference_source_associate_native");
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::store_at_offset(emitter, result, VALUE);
    emitter.label(publish);
    for (index, offset) in [SCOPE, NAME, LEN, VALUE, FLAGS].into_iter().enumerate() {
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::emit_call_label(emitter, &emitter.target.extern_symbol("__elephc_eval_scope_set"));
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

/// Detaches before contained retirement and returns an independently owned boxed Throwable.
fn emit_retire(emitter: &mut Emitter, enabled: bool, whole_scope: bool) {
    let entry = if whole_scope { "__elephc_eval_scope_origins_release_v1" }
        else { "__elephc_eval_scope_origin_unset_v1" };
    emitter.label_global(&emitter.target.extern_symbol(entry));
    let result = abi::int_result_reg(emitter);
    if !enabled {
        abi::emit_load_int_immediate(emitter, result, 0);
        abi::emit_return(emitter);
        return;
    }
    let next = format!("{entry}_next");
    let done = format!("{entry}_done");
    let returned = format!("{entry}_returned");
    abi::emit_frame_prologue(emitter, FRAME);
    for (index, offset) in [SCOPE, NAME, LEN].into_iter().enumerate().take(if whole_scope { 1 } else { 3 }) {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::emit_store_zero_to_local_slot(emitter, THROWN);
    emitter.label(&next);
    for (index, offset) in [SCOPE, NAME, LEN].into_iter().enumerate().take(if whole_scope { 1 } else { 3 }) {
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    let operation = if whole_scope { "__elephc_reference_scope_pop_v1" } else { "__elephc_reference_scope_take_v1" };
    abi::emit_call_label(emitter, &emitter.target.extern_symbol(operation));
    abi::emit_branch_if_int_result_zero(emitter, &done);
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 1), result);
    abi::emit_symbol_address(emitter, abi::int_arg_reg_name(emitter.target, 0), "__rt_eval_scope_origin_release_cell");
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 2), THROWN);
    abi::emit_call_label(emitter, "__rt_cleanup_invoke");
    if whole_scope { abi::emit_jump(emitter, &next); }
    emitter.label(&done);
    abi::load_at_offset(emitter, result, THROWN);
    abi::emit_branch_if_int_result_zero(emitter, &returned);
    crate::codegen_support::value_boxing::emit_box_current_owned_value_as_mixed(emitter,
        &crate::types::PhpType::Object(String::new()));
    emitter.label(&returned);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use crate::codegen::{emit::Emitter, platform::Target};
    use std::{io::Write, process::{Command, Stdio}};

    /// Real assemblers validate checked producer and contained retirement helpers on every target.
    #[test]
    #[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
    fn scope_origin_bindings_assemble_on_every_supported_target() {
        for (name, triple) in [("linux-x86_64", "x86_64-linux-gnu"),
            ("linux-aarch64", "aarch64-linux-gnu"), ("macos-aarch64", "arm64-apple-macos11"),
            ("ios-arm64", "arm64-apple-ios13"), ("ios-sim-arm64", "arm64-apple-ios13-simulator")] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            super::emit(&mut emitter, true);
            let syntax = if name == "linux-x86_64" { ".intel_syntax noprefix\n" } else { "" };
            let assembly = format!("{syntax}.text\n{}", emitter.output());
            let mut process = Command::new("clang").args(["-target", triple, "-x", "assembler",
                "-c", "-o", "/dev/null", "-"]).stdin(Stdio::piped()).stderr(Stdio::piped())
                .spawn().expect("clang is required for this explicit target check");
            process.stdin.take().unwrap().write_all(assembly.as_bytes()).unwrap();
            let output = process.wait_with_output().unwrap();
            assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}
