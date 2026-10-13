//! Purpose:
//! Reads entry-time named variadic arguments for materialized and printed AOT backtraces.
//!
//! Called from:
//! - The parent backtrace frame reader after its positional argument snapshot is consumed.
//!
//! Key details:
//! - Named values come from a generated full variadic snapshot, never the mutated user local.
//! - Numeric keys stay on the positional path; string keys preserve their order and spelling.
//! - Materialized values are independent Mixed cells; dense trace arrays promote only for names.
//! - Iterator cursor -2 still carries the final entry; only -1 means there is no value to read.

use super::{abi, Arch, FunctionContext, PhpType, Result};

const STATE_BYTES: usize = 96;
const DEST: usize = 0;
const SOURCE: usize = 8;
const CURSOR: usize = 16;
const KEY_LO: usize = 24;
const KEY_HI: usize = 32;
const VALUE_LO: usize = 40;
const VALUE_HI: usize = 48;
const VALUE_TAG: usize = 56;
const PROMOTED: usize = 64;
const TEMP_BOX: usize = 72;
const VALUE_BOX: usize = 80;
const NEW_DEST: usize = 88;

/// Selects whether the same ordered named-entry reader builds an array or writes a trace line.
#[derive(Clone, Copy)]
enum Mode { Append, Print }

/// Adds named snapshot entries to the freshly materialized positional trace array.
pub(super) fn append_named_arguments(ctx: &mut FunctionContext<'_>) -> Result<()> {
    emit_named_entries(ctx, Mode::Append)
}

/// Prints named arguments after positional/default-count processing without truncating names.
pub(super) fn print_named_arguments(ctx: &mut FunctionContext<'_>) -> Result<()> {
    emit_named_entries(ctx, Mode::Print)
}

/// Boxes a fresh raw Array/Hash trace argument result without assuming its physical shape.
pub(super) fn box_argument_array(ctx: &mut FunctionContext<'_>) {
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result);
    abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
    let hash = ctx.next_label("backtrace_args_box_hash");
    let done = ctx.next_label("backtrace_args_box_done");
    branch_if_result_equals(ctx, 3, &hash);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    super::emit_box_current_owned_value_as_mixed(ctx.emitter, &PhpType::Array(Box::new(PhpType::Mixed)));
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&hash);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    super::emit_box_current_owned_value_as_mixed(ctx.emitter, &PhpType::AssocArray {
        key: Box::new(PhpType::Mixed), value: Box::new(PhpType::Mixed),
    });
    ctx.emitter.label(&done);
}

/// Walks the original variadic Hash and snapshots only its named values into the selected sink.
fn emit_named_entries(ctx: &mut FunctionContext<'_>, mode: Mode) -> Result<()> {
    let Some(slot) = ctx.local_slot_by_name(crate::func_args::TRACE_VARIADIC_SNAPSHOT) else {
        return Ok(());
    };
    abi::emit_reserve_temporary_stack(ctx.emitter, STATE_BYTES);
    abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), DEST);
    for offset in [CURSOR, PROMOTED] {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), offset);
    }
    let ty = ctx.load_local_to_result(slot)?;
    if matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        let payload = crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
        abi::emit_reg_move(ctx.emitter, abi::int_result_reg(ctx.emitter), payload);
    }
    abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), SOURCE);
    abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
    let walk = ctx.next_label("backtrace_named_walk");
    let next = ctx.next_label("backtrace_named_next");
    let done = ctx.next_label("backtrace_named_done");
    branch_if_result_equals(ctx, 3, &walk);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&walk);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), SOURCE);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), CURSOR);
    abi::emit_call_label(ctx.emitter, "__rt_hash_iter_next_value");
    branch_if_result_equals(ctx, -1, &done);
    let registers = match ctx.emitter.target.arch {
        Arch::AArch64 => ["x0", "x1", "x2", "x3", "x4", "x5"],
        Arch::X86_64 => ["rax", "rdi", "rdx", "rcx", "r8", "r9"],
    };
    for (register, offset) in registers.into_iter().zip([CURSOR, KEY_LO, KEY_HI, VALUE_LO, VALUE_HI, VALUE_TAG]) {
        abi::emit_store_to_sp(ctx.emitter, register, offset);
    }
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), KEY_HI);
    super::emit_branch_if_negative(ctx, &next);
    snapshot_value(ctx);
    match mode {
        Mode::Append => append_entry(ctx),
        Mode::Print => print_entry(ctx),
    }
    ctx.emitter.label(&next);
    abi::emit_jump(ctx.emitter, &walk);
    ctx.emitter.label(&done);
    if matches!(mode, Mode::Append) {
        abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), DEST);
    }
    abi::emit_release_temporary_stack(ctx.emitter, STATE_BYTES);
    Ok(())
}

/// Copies the iterator value into an independent zval, dereferencing any reference wrappers.
fn snapshot_value(ctx: &mut FunctionContext<'_>) {
    let (tag, low, high) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x5", "x3", "x4"),
        Arch::X86_64 => ("r9", "rcx", "r8"),
    };
    for (register, offset) in [(tag, VALUE_TAG), (low, VALUE_LO), (high, VALUE_HI)] {
        abi::emit_load_temporary_stack_slot(ctx.emitter, register, offset);
    }
    crate::codegen::emit_box_runtime_payload_as_mixed(ctx.emitter, tag, low, high);
    abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), TEMP_BOX);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let reference = ctx.next_label("backtrace_named_reference");
    let copied = ctx.next_label("backtrace_named_copied");
    branch_if_result_equals(ctx,
        crate::codegen_support::callable_invoker_args::INVOKER_ARG_REF_CELL_TAG, &reference);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), TEMP_BOX);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_clone");
    abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), VALUE_BOX);
    abi::emit_jump(ctx.emitter, &copied);
    ctx.emitter.label(&reference);
    let storage = abi::symbol_scratch_reg(ctx.emitter);
    let source_tag = abi::secondary_scratch_reg(ctx.emitter);
    let (pointer, representation) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x1", "x2"),
        Arch::X86_64 => ("rdi", "rdx"),
    };
    abi::emit_reg_move(ctx.emitter, storage, pointer);
    abi::emit_reg_move(ctx.emitter, source_tag, representation);
    let prefix = ctx.next_label("backtrace_named_source");
    crate::codegen_support::callable_invoker_args::emit_materialize_ref_cell_value_as_mixed(
        storage, source_tag, ctx.emitter, &mut |suffix| format!("{prefix}_{suffix}"),
    );
    abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), NEW_DEST);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_clone");
    abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), VALUE_BOX);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), NEW_DEST);
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    ctx.emitter.label(&copied);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), TEMP_BOX);
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
}

/// Promotes the trace destination only on its first named key, then transfers the owned value.
fn append_entry(ctx: &mut FunctionContext<'_>) {
    let ready = ctx.next_label("backtrace_named_hash_ready");
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), PROMOTED);
    let result = abi::int_result_reg(ctx.emitter).to_string();
    let promote = ctx.next_label("backtrace_named_promote");
    super::emit_branch_if_zero(ctx, &result, &promote);
    abi::emit_jump(ctx.emitter, &ready);
    ctx.emitter.label(&promote);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), DEST);
    abi::emit_call_label(ctx.emitter, "__rt_array_to_hash");
    abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), NEW_DEST);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), DEST);
    abi::emit_call_label(ctx.emitter, "__rt_decref_array");
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), NEW_DEST);
    abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), DEST);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
    abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), PROMOTED);
    ctx.emitter.label(&ready);
    for (index, offset) in [(0, DEST), (1, KEY_LO), (2, KEY_HI), (3, VALUE_BOX)] {
        abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, index), offset);
    }
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 4), 0);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 5), 7);
    abi::emit_call_label(ctx.emitter, "__rt_hash_set");
    abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), DEST);
}

/// Renders one named value with its source spelling and releases the temporary snapshot cell.
fn print_entry(ctx: &mut FunctionContext<'_>) {
    super::emit_printed_argument_separator(ctx, STATE_BYTES);
    let (pointer, length) = abi::string_result_regs(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, pointer, KEY_LO);
    abi::emit_load_temporary_stack_slot(ctx.emitter, length, KEY_HI);
    abi::emit_write_stdout(ctx.emitter, &PhpType::Str);
    super::write_static_trace_bytes(ctx, ": ");
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), VALUE_BOX);
    abi::emit_call_label(ctx.emitter, "__rt_backtrace_print_arg");
    super::increment_printed_argument_count(ctx, STATE_BYTES);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), VALUE_BOX);
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
}

/// Branches when the canonical integer result equals a small runtime discriminator.
fn branch_if_result_equals(ctx: &mut FunctionContext<'_>, expected: i64, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp x0, #{expected}"));           // compare the runtime discriminator with the requested kind
            ctx.emitter.instruction(&format!("b.eq {label}"));                  // select the matching runtime representation
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp rax, {expected}"));           // compare the runtime discriminator with the requested kind
            ctx.emitter.instruction(&format!("je {label}"));                    // select the matching runtime representation
        }
    }
}
