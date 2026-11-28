//! Purpose:
//! Emits PHP's by-reference temporary warning for descriptor invocations.
//!
//! Called from:
//! - `crate::codegen::runtime_callable_invoker` while marshalling indexed or hash arguments.
//!
//! Key details:
//! - `call_user_func*` always passes by value, including reference cells and persistent
//!   wrappers. A callable with a by-reference parameter therefore warns once per such
//!   parameter and mutates only the invoker's temporary cell.
//! - The warning decision is runtime state because one cached invoker serves both
//!   `call_user_func*` and ordinary dynamic calls, which may pass a real variable reference.
//! - The callable name comes from the descriptor; the parameter name comes from the signature.

use super::*;
use crate::codegen_support::callable_descriptor::{
    CALLABLE_DESC_NAME_LENGTH_OFFSET, CALLABLE_DESC_NAME_OFFSET,
};

/// Warns when a `call_user_func*` argument targets a by-reference parameter.
pub(super) fn warn_indexed_ref_temporary(
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
    sig: &FunctionSig,
    index: usize,
) {
    let skip_label = ctx.next_label("invoker_byref_warning_skip");
    let warn_label = ctx.next_label("invoker_byref_warning");

    // -- only call_user_func-family invocations carry the private profile bit --
    let scope = abi::symbol_scratch_reg(emitter);
    let mask = abi::tertiary_scratch_reg(emitter);
    abi::load_at_offset(emitter, scope, INVOKER_SCOPE_OFFSET);
    abi::emit_load_int_immediate(emitter, mask, 3);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("and {scope}, {scope}, {mask}"));      // isolate the invoker-private profile bits
            emitter.instruction(&format!(
                "cmp {scope}, #{CALLABLE_PROFILE_WARN_BY_REF}"
            ));                                                                 // is the by-reference warning requested?
            emitter.instruction(&format!("b.ne {skip_label}"));                 // ordinary dynamic calls keep their implicit references
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("and {scope}, {mask}"));               // isolate the invoker-private profile bits
            emitter.instruction(&format!(
                "cmp {scope}, {CALLABLE_PROFILE_WARN_BY_REF}"
            ));                                                                 // is the by-reference warning requested?
            emitter.instruction(&format!("jne {skip_label}"));                  // ordinary dynamic calls keep their implicit references
        }
    }

    emitter.label(&warn_label);
    emit_by_reference_warning(emitter, ctx, data, sig, index, &skip_label);
    emitter.label(&skip_label);
}

/// Emits `<callable>(): Argument #N ($param) must be passed by reference, value given`.
fn emit_by_reference_warning(
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
    sig: &FunctionSig,
    index: usize,
    skip_label: &str,
) {
    let descriptor = abi::tertiary_scratch_reg(emitter);
    let name_len = abi::secondary_scratch_reg(emitter);
    let named_label = ctx.next_label("invoker_byref_named_callable");
    abi::load_at_offset(emitter, descriptor, INVOKER_DESCRIPTOR_OFFSET);
    abi::emit_load_from_address(
        emitter,
        name_len,
        descriptor,
        CALLABLE_DESC_NAME_LENGTH_OFFSET,
    );
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbnz {name_len}, {named_label}"));    // a named callable always publishes its PHP spelling
            emitter.instruction(&format!("b {skip_label}"));                    // unnamed closures fall back to their own call path
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {name_len}, {name_len}"));       // a named callable always publishes its PHP spelling
            emitter.instruction(&format!("jne {named_label}"));                 // continue when the descriptor carries a name
            emitter.instruction(&format!("jmp {skip_label}"));                  // unnamed closures fall back to their own call path
        }
    }
    emitter.label(&named_label);

    // -- preserve the global string-concat reader state across user warning handlers --
    let scratch = abi::symbol_scratch_reg(emitter);
    abi::emit_reserve_temporary_stack(emitter, 16);
    elephc_memoir::strings::concat_cursor::SavedConcatCursor::Stack(0)
        .emit_save(emitter, scratch);

    let warning_prefix = data.add_string(b"Warning: ");
    emit_static_fragment(emitter, &warning_prefix, false);
    emit_descriptor_name_fragment(emitter, descriptor);
    let argument_prefix = data.add_string(b"(): Argument #");
    emit_static_fragment(emitter, &argument_prefix, false);
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), index as i64 + 1);
    abi::emit_call_label(emitter, "__rt_itoa");
    if emitter.target.arch == Arch::X86_64 {
        abi::emit_reg_move(emitter, "rdi", "rax");
        abi::emit_reg_move(emitter, "rsi", "rdx");
    }
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    let parameter = sig
        .params
        .get(index)
        .map(|(name, _)| format!(" (${name}) must be passed by reference, value given"))
        .unwrap_or_else(|| " must be passed by reference, value given".to_string());
    let suffix = data.add_string(parameter.as_bytes());
    emit_static_fragment(emitter, &suffix, false);
    // The shared dispatcher appends the call site's file and line to a completed warning,
    // so this message stops at its own newline and lets that writer add the location once.
    let newline = data.add_string(b"\n");
    emit_static_fragment(emitter, &newline, true);

    elephc_memoir::strings::concat_cursor::SavedConcatCursor::Stack(0)
        .emit_restore(emitter, scratch);
    abi::emit_release_temporary_stack(emitter, 16);
}

/// Emits the descriptor's PHP callable name as a diagnostic fragment.
fn emit_descriptor_name_fragment(emitter: &mut Emitter, descriptor: &str) {
    let pointer = abi::symbol_scratch_reg(emitter);
    let length = abi::secondary_scratch_reg(emitter);
    abi::emit_load_from_address(emitter, pointer, descriptor, CALLABLE_DESC_NAME_OFFSET);
    abi::emit_load_from_address(emitter, length, descriptor, CALLABLE_DESC_NAME_LENGTH_OFFSET);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_reg_move(emitter, "x1", pointer);
            abi::emit_reg_move(emitter, "x2", length);
        }
        Arch::X86_64 => {
            abi::emit_reg_move(emitter, "rdi", pointer);
            abi::emit_reg_move(emitter, "rsi", length);
        }
    }
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
}


/// Appends a static diagnostic fragment and optionally finishes the pending warning.
fn emit_static_fragment(emitter: &mut Emitter, fragment: &(String, usize), complete: bool) {
    let (pointer, length) = match emitter.target.arch {
        Arch::AArch64 => ("x1", "x2"),
        Arch::X86_64 => ("rdi", "rsi"),
    };
    abi::emit_symbol_address(emitter, pointer, &fragment.0);
    abi::emit_load_int_immediate(emitter, length, fragment.1 as i64);
    abi::emit_call_label(
        emitter,
        if complete { "__rt_diag_warning" } else { "__rt_diag_warning_fragment" },
    );
}
