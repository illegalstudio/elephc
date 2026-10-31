//! Purpose:
//! Emits PHP's by-reference temporary warning for descriptor invocations.
//!
//! Called from:
//! - `crate::codegen::runtime_callable_invoker` while marshalling indexed arguments.
//!
//! Key details:
//! - `call_user_func*` always passes by value. A callable with a by-reference parameter
//!   therefore warns once per parameter that receives a temporary.
//! - The warning decision is runtime state because one cached invoker serves both
//!   `call_user_func*` and ordinary dynamic calls, which may pass a real variable reference.
//! - The callable name comes from the descriptor; the parameter name comes from the signature.

use super::*;
use crate::codegen_support::callable_descriptor::{
    CALLABLE_DESC_NAME_LENGTH_OFFSET, CALLABLE_DESC_NAME_OFFSET,
};

/// Warns when an indexed `call_user_func*` argument is a temporary for a by-reference parameter.
pub(super) fn warn_indexed_ref_temporary(
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
    sig: &FunctionSig,
    index: usize,
    elem_ty: &PhpType,
    array_reg: &str,
    elem_size: usize,
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

    // -- a typed element is never a reference cell; a Mixed element may hold one --
    if elem_ty.codegen_repr() != PhpType::Mixed {
        abi::emit_jump(emitter, &warn_label);
    } else {
        let cell = abi::int_result_reg(emitter);
        let tag = abi::secondary_scratch_reg(emitter);
        abi::emit_load_from_address(emitter, cell, array_reg, 24 + index * elem_size);
        abi::emit_load_from_address(emitter, tag, cell, 0);
        super::emit_branch_if_invoker_ref_cell_tag(tag, &skip_label, emitter);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("cmp {tag}, #11"));                // a managed PHP reference supplies writable storage
                emitter.instruction(&format!("b.eq {skip_label}"));             // keep the existing reference cell
                emitter.instruction(&format!("cmp {tag}, #7"));                 // only a boxed value can hide a persistent wrapper
                emitter.instruction(&format!("b.ne {warn_label}"));             // ordinary values need the PHP warning
                abi::emit_load_from_address(emitter, tag, cell, 16);
                emitter.instruction(&format!("cmp {tag}, #1"));                 // flag one identifies a persistent reference
                emitter.instruction(&format!("b.eq {skip_label}"));             // preserve its writable target
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("cmp {tag}, 11"));                 // a managed PHP reference supplies writable storage
                emitter.instruction(&format!("je {skip_label}"));               // keep the existing reference cell
                emitter.instruction(&format!("cmp {tag}, 7"));                  // only a boxed value can hide a persistent wrapper
                emitter.instruction(&format!("jne {warn_label}"));              // ordinary values need the PHP warning
                abi::emit_load_from_address(emitter, tag, cell, 16);
                emitter.instruction(&format!("cmp {tag}, 1"));                  // flag one identifies a persistent reference
                emitter.instruction(&format!("je {skip_label}"));               // preserve its writable target
            }
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
    abi::emit_load_symbol_to_reg(emitter, scratch, "_concat_off", 0);
    abi::emit_store_to_sp(emitter, scratch, 0);

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
    let location_prefix = data.add_string(b" in ");
    emit_static_fragment(emitter, &location_prefix, false);
    emit_diagnostic_file_fragment(emitter);
    let line_prefix = data.add_string(b" on line ");
    emit_static_fragment(emitter, &line_prefix, false);
    abi::emit_load_symbol_to_reg(
        emitter,
        abi::int_result_reg(emitter),
        "_php_diagnostic_line",
        0,
    );
    abi::emit_call_label(emitter, "__rt_itoa");
    if emitter.target.arch == Arch::X86_64 {
        abi::emit_reg_move(emitter, "rdi", "rax");
        abi::emit_reg_move(emitter, "rsi", "rdx");
    }
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    let newline = data.add_string(b"\n");
    emit_static_fragment(emitter, &newline, true);

    abi::emit_load_temporary_stack_slot(emitter, scratch, 0);
    abi::emit_store_reg_to_symbol(emitter, scratch, "_concat_off", 0);
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

/// Emits the current diagnostic call site's file name as a fragment.
fn emit_diagnostic_file_fragment(emitter: &mut Emitter) {
    let pointer = abi::symbol_scratch_reg(emitter);
    let length = abi::secondary_scratch_reg(emitter);
    abi::emit_load_symbol_to_reg(emitter, pointer, "_php_diagnostic_file", 0);
    abi::emit_load_symbol_to_reg(emitter, length, "_php_diagnostic_file_len", 0);
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
