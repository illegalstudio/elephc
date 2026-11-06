//! Purpose:
//! Lowers JSON state, encode, decode, and validation builtins for the EIR backend.
//! Bridges already-evaluated EIR operands to the shared JSON runtime helpers.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::lower_language_construct_call()`.
//!
//! Key details:
//! - JSON error state is runtime-global and must be reset after PHP arguments
//!   have already been evaluated by preceding EIR instructions.

use crate::codegen::{abi, emit_box_current_value_as_mixed};
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::super::{load_value_to_first_int_arg, predicates};
use super::{expect_operand, store_if_result};

/// Tracks how `json_decode()` should derive decoded object shape.
#[derive(Clone, Copy, PartialEq, Eq)]
enum AssocArg {
    Explicit,
    FromFlags,
}

/// Lowers `json_decode(json, associative?, depth?, flags?)` through the shared JSON decoder runtime.
pub(crate) fn lower_json_decode(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "json_decode", 1, 4)?;
    let json = expect_operand(inst, 0)?;
    let assoc_arg = json_decode_assoc_arg(ctx, inst)?;

    reset_json_decode_state(ctx);
    lower_json_decode_flags(ctx, inst, assoc_arg)?;
    lower_json_decode_depth(ctx, inst)?;
    if matches!(assoc_arg, AssocArg::Explicit) {
        lower_json_decode_assoc(ctx, inst)?;
    }
    let source_temp = load_json_source_string(ctx, json, "json_decode source")?;
    store_json_error_source_ptr(ctx);
    abi::emit_call_label(ctx.emitter, "__rt_json_decode_mixed");
    clear_json_diagnostic_source_refs(ctx);
    release_json_source_temp(ctx, source_temp);
    box_null_on_json_decode_failure(ctx);
    store_if_result(ctx, inst)
}

/// Lowers `json_encode(value, flags?, depth?)` through the shared JSON encoder runtime.
pub(crate) fn lower_json_encode(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "json_encode", 1, 3)?;
    let value = expect_operand(inst, 0)?;
    let value_ty = ctx.value_php_type(value)?.codegen_repr();

    reset_json_encode_state(ctx);
    lower_json_encode_depth(ctx, inst)?;
    lower_json_encode_flags(ctx, inst)?;
    load_json_encode_value(ctx, value, &value_ty)?;
    emit_json_encode_loaded_value(ctx, &value_ty);
    clear_json_diagnostic_source_refs(ctx);
    box_json_encode_result(ctx);
    store_if_result(ctx, inst)
}

/// Publishes the state expected by the default two-argument `json_decode`.
fn prepare_json_decode_count_state(ctx: &mut FunctionContext<'_>) {
    reset_json_decode_state(ctx);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 512);
    abi::emit_store_reg_to_symbol(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        "_json_depth_limit",
        0,
    );
    abi::emit_store_zero_to_symbol(ctx.emitter, "_json_active_flags", 0);
    abi::emit_store_zero_to_symbol(ctx.emitter, "_json_decode_assoc", 0);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
    abi::emit_store_reg_to_symbol(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        "_json_decode_assoc",
        0,
    );
}

/// Lowers the count-only `count(json_decode($s, true))` fast path selected by the sink.
///
/// The runtime helper validates the whole document and returns the top-level count, or a
/// status code for a scalar root / invalid JSON / unsupported key shape. Scalar and invalid
/// roots raise PHP's exact `count()` TypeError without building a Mixed graph; the rare
/// unsupported object-key case falls back to the full decoder and then counts its root.
pub(crate) fn lower_json_decode_count(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "json_decode_count", 2, 2)?;
    let source = expect_operand(inst, 0)?;

    prepare_json_decode_count_state(ctx);
    let source_temp = load_json_source_string(ctx, source, "json_decode_count source")?;
    abi::emit_call_label(ctx.emitter, "__rt_json_decode_count");

    let fallback = ctx.next_label("json_decode_count_fallback");
    let fast = ctx.next_label("json_decode_count_fast");
    let done = ctx.next_label("json_decode_count_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // is this a real top-level count?
            ctx.emitter.instruction(&format!("b.ge {fast}"));                   // counts are non-negative
            ctx.emitter.instruction("cmp x0, #-8");                             // is the exact decoder required?
            ctx.emitter.instruction(&format!("b.eq {fallback}"));               // fall back for numeric roots and escaped keys
            release_json_source_temp(ctx, source_temp);
            let null = ctx.next_label("json_decode_count_null");
            let int = ctx.next_label("json_decode_count_int");
            let float = ctx.next_label("json_decode_count_float");
            let string = ctx.next_label("json_decode_count_string");
            let false_ = ctx.next_label("json_decode_count_false");
            let true_ = ctx.next_label("json_decode_count_true");
            ctx.emitter.instruction("cmp x0, #-2");                             // int root?
            ctx.emitter.instruction(&format!("b.eq {int}"));                    // int root
            ctx.emitter.instruction("cmp x0, #-3");                             // float root?
            ctx.emitter.instruction(&format!("b.eq {float}"));                  // float root
            ctx.emitter.instruction("cmp x0, #-4");                             // string root?
            ctx.emitter.instruction(&format!("b.eq {string}"));                 // string root
            ctx.emitter.instruction("cmp x0, #-5");                             // false root?
            ctx.emitter.instruction(&format!("b.eq {false_}"));                 // false root
            ctx.emitter.instruction("cmp x0, #-6");                             // true root?
            ctx.emitter.instruction(&format!("b.eq {true_}"));                  // true root
            ctx.emitter.instruction(&format!("b {null}"));                      // invalid JSON and null roots
            ctx.emitter.label(&int);
            super::count_empty::emit_count_type_error(ctx, "int");
            ctx.emitter.label(&float);
            super::count_empty::emit_count_type_error(ctx, "float");
            ctx.emitter.label(&string);
            super::count_empty::emit_count_type_error(ctx, "string");
            ctx.emitter.label(&false_);
            super::count_empty::emit_count_type_error(ctx, "false");
            ctx.emitter.label(&true_);
            super::count_empty::emit_count_type_error(ctx, "true");
            ctx.emitter.label(&null);
            super::count_empty::emit_count_type_error(ctx, "null");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // is this a real top-level count?
            ctx.emitter.instruction(&format!("jns {fast}"));                    // counts are non-negative
            ctx.emitter.instruction("cmp rax, -8");                             // is the exact decoder required?
            ctx.emitter.instruction(&format!("je {fallback}"));                 // fall back for numeric roots and escaped keys
            release_json_source_temp(ctx, source_temp);
            let null = ctx.next_label("json_decode_count_null_x");
            let int = ctx.next_label("json_decode_count_int_x");
            let float = ctx.next_label("json_decode_count_float_x");
            let string = ctx.next_label("json_decode_count_string_x");
            let false_ = ctx.next_label("json_decode_count_false_x");
            let true_ = ctx.next_label("json_decode_count_true_x");
            ctx.emitter.instruction("cmp rax, -2");                             // int root?
            ctx.emitter.instruction(&format!("je {int}"));                      // int root
            ctx.emitter.instruction("cmp rax, -3");                             // float root?
            ctx.emitter.instruction(&format!("je {float}"));                    // float root
            ctx.emitter.instruction("cmp rax, -4");                             // string root?
            ctx.emitter.instruction(&format!("je {string}"));                   // string root
            ctx.emitter.instruction("cmp rax, -5");                             // false root?
            ctx.emitter.instruction(&format!("je {false_}"));                   // false root
            ctx.emitter.instruction("cmp rax, -6");                             // true root?
            ctx.emitter.instruction(&format!("je {true_}"));                    // true root
            ctx.emitter.instruction(&format!("jmp {null}"));                    // invalid JSON and null roots
            ctx.emitter.label(&int);
            super::count_empty::emit_count_type_error(ctx, "int");
            ctx.emitter.label(&float);
            super::count_empty::emit_count_type_error(ctx, "float");
            ctx.emitter.label(&string);
            super::count_empty::emit_count_type_error(ctx, "string");
            ctx.emitter.label(&false_);
            super::count_empty::emit_count_type_error(ctx, "false");
            ctx.emitter.label(&true_);
            super::count_empty::emit_count_type_error(ctx, "true");
            ctx.emitter.label(&null);
            super::count_empty::emit_count_type_error(ctx, "null");
        }
    }

    ctx.emitter.label(&fallback);
    release_json_source_temp(ctx, source_temp);
    prepare_json_decode_count_state(ctx);
    let fallback_temp = load_json_source_string(ctx, source, "json_decode_count source")?;
    store_json_error_source_ptr(ctx);
    abi::emit_call_label(ctx.emitter, "__rt_json_decode_mixed");
    clear_json_diagnostic_source_refs(ctx);
    release_json_source_temp(ctx, fallback_temp);
    emit_decoded_count_result(ctx);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&fast);
    release_json_source_temp(ctx, source_temp);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Counts a decoded root Mixed cell, releasing it and raising PHP's exact TypeError for scalars.
fn emit_decoded_count_result(ctx: &mut FunctionContext<'_>) {
    let countable = ctx.next_label("json_decode_count_countable");
    let int = ctx.next_label("json_decode_count_result_int");
    let string = ctx.next_label("json_decode_count_result_string");
    let float = ctx.next_label("json_decode_count_result_float");
    let bool_ = ctx.next_label("json_decode_count_result_bool");
    let true_ = ctx.next_label("json_decode_count_result_true");
    let result_done = ctx.next_label("json_decode_count_result_done");
    let null = ctx.next_label("json_decode_count_result_null");
    let resource = ctx.next_label("json_decode_count_result_resource");
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_reserve_temporary_stack(ctx.emitter, 16);
    abi::emit_store_to_sp(ctx.emitter, result, 0);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {null}"));                // a failed decode behaves like null
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // a failed decode behaves like null
            ctx.emitter.instruction(&format!("jz {null}"));                     // a failed decode behaves like null
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            for tag in [4u8, 5, 6] {
                ctx.emitter.instruction(&format!("cmp x0, #{tag}"));            // countable container tags
                ctx.emitter.instruction(&format!("b.eq {countable}"));          // countable container tag
            }
            ctx.emitter.instruction("cmp x0, #0");                              // int payload?
            ctx.emitter.instruction(&format!("b.eq {int}"));                    // int payload tag
            ctx.emitter.instruction("cmp x0, #1");                              // string payload?
            ctx.emitter.instruction(&format!("b.eq {string}"));                 // string payload tag
            ctx.emitter.instruction("cmp x0, #2");                              // float payload?
            ctx.emitter.instruction(&format!("b.eq {float}"));                  // float payload tag
            ctx.emitter.instruction("cmp x0, #3");                              // bool payload?
            ctx.emitter.instruction(&format!("b.eq {bool_}"));                  // bool payload tag
            ctx.emitter.instruction("cmp x0, #9");                              // resource payload?
            ctx.emitter.instruction(&format!("b.eq {resource}"));               // resource payload tag
            ctx.emitter.instruction(&format!("b {null}"));                      // every other tag is null
        }
        Arch::X86_64 => {
            for tag in [4u8, 5, 6] {
                ctx.emitter.instruction(&format!("cmp rax, {tag}"));            // countable container tags
                ctx.emitter.instruction(&format!("je {countable}"));            // countable container tag
            }
            ctx.emitter.instruction("cmp rax, 0");                              // int payload?
            ctx.emitter.instruction(&format!("je {int}"));                      // int payload tag
            ctx.emitter.instruction("cmp rax, 1");                              // string payload?
            ctx.emitter.instruction(&format!("je {string}"));                   // string payload tag
            ctx.emitter.instruction("cmp rax, 2");                              // float payload?
            ctx.emitter.instruction(&format!("je {float}"));                    // float payload tag
            ctx.emitter.instruction("cmp rax, 3");                              // bool payload?
            ctx.emitter.instruction(&format!("je {bool_}"));                    // bool payload tag
            ctx.emitter.instruction("cmp rax, 9");                              // resource payload?
            ctx.emitter.instruction(&format!("je {resource}"));                 // resource payload tag
            ctx.emitter.instruction(&format!("jmp {null}"));                    // every other tag is null
        }
    }
    ctx.emitter.label(&countable);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 0);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_count");
    abi::emit_push_reg(ctx.emitter, result);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 16);
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    abi::emit_pop_reg(ctx.emitter, result);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_jump(ctx.emitter, &result_done);
    ctx.emitter.label(&int);
    release_decoded_count_cell(ctx);
    super::count_empty::emit_count_type_error(ctx, "int");
    ctx.emitter.label(&string);
    release_decoded_count_cell(ctx);
    super::count_empty::emit_count_type_error(ctx, "string");
    ctx.emitter.label(&float);
    release_decoded_count_cell(ctx);
    super::count_empty::emit_count_type_error(ctx, "float");
    ctx.emitter.label(&bool_);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x1, {true_}"));              // true payload
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rdi, rdi");                           // true payload
            ctx.emitter.instruction(&format!("jnz {true_}"));                   // true payload tag
        }
    }
    release_decoded_count_cell(ctx);
    super::count_empty::emit_count_type_error(ctx, "false");
    ctx.emitter.label(&true_);
    release_decoded_count_cell(ctx);
    super::count_empty::emit_count_type_error(ctx, "true");
    ctx.emitter.label(&resource);
    release_decoded_count_cell(ctx);
    super::count_empty::emit_count_type_error(ctx, "resource");
    ctx.emitter.label(&null);
    release_decoded_count_cell(ctx);
    super::count_empty::emit_count_type_error(ctx, "null");
    ctx.emitter.label(&result_done);
}

/// Releases the decoded Mixed cell saved in the count-result temp slot.
fn release_decoded_count_cell(ctx: &mut FunctionContext<'_>) {
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 0);
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
}

/// Lowers the raw string form of `json_encode()` selected by the length sink.
///
/// The encoder runs exactly as the boxed builtin does, including flags, depth, error state and
/// throws, but the result is left as the encoder's raw string instead of being persisted and
/// boxed. The sink only selects this target when one `strlen()` consumes the string immediately,
/// so a scratch-backed result stays valid and a grown heap block is released once by that
/// string's ordinary release.
pub(crate) fn lower_json_encode_str(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "json_encode", 1, 3)?;
    let value = expect_operand(inst, 0)?;
    let value_ty = ctx.value_php_type(value)?.codegen_repr();

    reset_json_encode_state(ctx);
    lower_json_encode_depth(ctx, inst)?;
    lower_json_encode_flags(ctx, inst)?;
    load_json_encode_value(ctx, value, &value_ty)?;
    emit_json_encode_loaded_value(ctx, &value_ty);
    clear_json_diagnostic_source_refs(ctx);
    finish_json_encode_str(ctx);
    store_if_result(ctx, inst)
}

/// Keeps the encoder's raw string, or returns the empty string for a reported JSON error.
///
/// `json_encode()` answers `false` unless `JSON_PARTIAL_OUTPUT_ON_ERROR` is active; casting that
/// false to a string yields `""`, whose length is zero. The successful path leaves the encoder's
/// pointer/length untouched.
fn finish_json_encode_str(ctx: &mut FunctionContext<'_>) {
    let string_label = ctx.next_label("json_encode_str_result");
    let done_label = ctx.next_label("json_encode_str_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            abi::emit_load_symbol_to_reg(ctx.emitter, "x9", "_json_last_error", 0);
            ctx.emitter.instruction(&format!("cbz x9, {}", string_label));      // no JSON error means the encoder string is valid
            abi::emit_load_symbol_to_reg(ctx.emitter, "x9", "_json_active_flags", 0);
            ctx.emitter.instruction("tst x9, #512");                            // JSON_PARTIAL_OUTPUT_ON_ERROR keeps the partial string
            ctx.emitter.instruction(&format!("b.ne {}", string_label));         // partial output still exposes its encoded length
            abi::emit_pop_reg_pair(ctx.emitter, "x10", "x11");
            ctx.emitter.instruction("mov x1, xzr");                             // (string)false is the empty string
            ctx.emitter.instruction("mov x2, xzr");                             // the empty string has zero length
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the valid encoder string
            ctx.emitter.label(&string_label);
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", "_json_last_error", 0);
            ctx.emitter.instruction("test r10, r10");                           // check whether the encoder reported an error
            ctx.emitter.instruction(&format!("jz {}", string_label));           // no JSON error means the encoder string is valid
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", "_json_active_flags", 0);
            ctx.emitter.instruction("test r10, 512");                           // JSON_PARTIAL_OUTPUT_ON_ERROR keeps the partial string
            ctx.emitter.instruction(&format!("jnz {}", string_label));          // partial output still exposes its encoded length
            abi::emit_pop_reg_pair(ctx.emitter, "r10", "r11");
            ctx.emitter.instruction("xor eax, eax");                            // (string)false is the empty string
            ctx.emitter.instruction("xor edx, edx");                            // the empty string has zero length
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the valid encoder string
            ctx.emitter.label(&string_label);
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Lowers `json_last_error()` by reading the shared runtime error-code symbol.
pub(crate) fn lower_json_last_error(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "json_last_error", 0)?;
    abi::emit_load_symbol_to_reg(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        "_json_last_error",
        0,
    );
    store_if_result(ctx, inst)
}

/// Lowers `json_last_error_msg()` through the runtime message lookup table.
pub(crate) fn lower_json_last_error_msg(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "json_last_error_msg", 0)?;
    abi::emit_call_label(ctx.emitter, "__rt_json_last_error_msg");
    store_if_result(ctx, inst)
}

/// Lowers `json_validate(json, depth?, flags?)` into the shared validator runtime.
pub(crate) fn lower_json_validate(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "json_validate", 1, 3)?;
    let json = expect_operand(inst, 0)?;

    reset_json_validation_state(ctx);
    lower_json_validate_flags(ctx, inst)?;
    lower_json_validate_depth(ctx, inst)?;
    let source_temp = load_json_source_string(ctx, json, "json_validate source")?;
    store_json_error_source_ptr(ctx);
    abi::emit_call_label(ctx.emitter, "__rt_json_validate");
    clear_json_diagnostic_source_refs(ctx);
    release_json_source_temp(ctx, source_temp);
    store_if_result(ctx, inst)
}

/// Clears JSON decoder state after all EIR operands have already evaluated.
fn reset_json_decode_state(ctx: &mut FunctionContext<'_>) {
    abi::emit_store_zero_to_symbol(ctx.emitter, "_json_last_error", 0);
    abi::emit_store_zero_to_symbol(ctx.emitter, "_json_active_depth", 0);
    abi::emit_store_zero_to_symbol(ctx.emitter, "_json_error_location_active", 0);
    clear_json_diagnostic_source_refs(ctx);
}

/// Clears observable JSON error and parser state after all EIR operands have evaluated.
fn reset_json_validation_state(ctx: &mut FunctionContext<'_>) {
    abi::emit_store_zero_to_symbol(ctx.emitter, "_json_last_error", 0);
    abi::emit_store_zero_to_symbol(ctx.emitter, "_json_active_depth", 0);
    abi::emit_store_zero_to_symbol(ctx.emitter, "_json_error_location_active", 0);
    clear_json_diagnostic_source_refs(ctx);
}

/// Clears JSON encoder state after all EIR operands have already evaluated.
fn reset_json_encode_state(ctx: &mut FunctionContext<'_>) {
    abi::emit_store_zero_to_symbol(ctx.emitter, "_json_last_error", 0);
    abi::emit_store_zero_to_symbol(ctx.emitter, "_json_active_depth", 0);
    abi::emit_store_zero_to_symbol(ctx.emitter, "_json_indent_depth", 0);
    abi::emit_store_zero_to_symbol(ctx.emitter, "_json_error_location_active", 0);
    clear_json_diagnostic_source_refs(ctx);
}

/// Classifies the optional `json_decode()` associative argument.
fn json_decode_assoc_arg(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<AssocArg> {
    if inst.operands.len() < 2 {
        return Ok(AssocArg::FromFlags);
    }
    let assoc = expect_operand(inst, 1)?;
    match ctx.value_php_type(assoc)? {
        PhpType::Void | PhpType::Never => Ok(AssocArg::FromFlags),
        _ => Ok(AssocArg::Explicit),
    }
}

/// Stores `json_decode()` flags and derives assoc mode from flags when required.
fn lower_json_decode_flags(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    assoc_arg: AssocArg,
) -> Result<()> {
    if inst.operands.len() >= 4 {
        let flags = expect_operand(inst, 3)?;
        require_integer_like(ctx.load_value_to_result(flags)?, "json_decode flags")?;
        let reg = abi::int_result_reg(ctx.emitter);
        abi::emit_store_reg_to_symbol(ctx.emitter, reg, "_json_active_flags", 0);
        if matches!(assoc_arg, AssocArg::FromFlags) {
            write_json_decode_assoc_from_flags(ctx);
        }
    } else {
        abi::emit_store_zero_to_symbol(ctx.emitter, "_json_active_flags", 0);
        if matches!(assoc_arg, AssocArg::FromFlags) {
            abi::emit_store_zero_to_symbol(ctx.emitter, "_json_decode_assoc", 0);
        }
    }
    Ok(())
}

/// Stores the strict-depth limit used by `json_decode()`.
fn lower_json_decode_depth(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() >= 3 {
        let depth = expect_operand(inst, 2)?;
        require_integer_like(ctx.load_value_to_result(depth)?, "json_decode depth")?;
        subtract_one_from_int_result(ctx);
        abi::emit_store_reg_to_symbol(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            "_json_depth_limit",
            0,
        );
    } else {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 511);
        abi::emit_store_reg_to_symbol(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            "_json_depth_limit",
            0,
        );
    }
    Ok(())
}

/// Stores an explicit `json_decode()` associative argument after PHP truthiness coercion.
fn lower_json_decode_assoc(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let assoc = expect_operand(inst, 1)?;
    emit_json_decode_assoc_truthiness(ctx, assoc)?;
    abi::emit_store_reg_to_symbol(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        "_json_decode_assoc",
        0,
    );
    Ok(())
}

/// Emits PHP truthiness for the explicit `json_decode()` associative operand.
fn emit_json_decode_assoc_truthiness(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    match ctx.value_php_type(value)? {
        PhpType::Bool | PhpType::Int => {
            ctx.load_value_to_result(value)?;
            predicates::emit_int_result_nonzero_bool(ctx);
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            predicates::emit_float_result_nonzero_bool(ctx);
        }
        PhpType::Str => {
            predicates::emit_string_truthiness(ctx, value)?;
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "json_decode associative for PHP type {:?}",
                other
            )))
        }
    }
    Ok(())
}

/// Stores the active `json_encode()` depth limit.
fn lower_json_encode_depth(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() >= 3 {
        let depth = expect_operand(inst, 2)?;
        require_integer_like(ctx.load_value_to_result(depth)?, "json_encode depth")?;
        abi::emit_store_reg_to_symbol(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            "_json_depth_limit",
            0,
        );
    } else {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 512);
        abi::emit_store_reg_to_symbol(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            "_json_depth_limit",
            0,
        );
    }
    Ok(())
}

/// Stores the active `json_encode()` flag bitmask.
fn lower_json_encode_flags(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() >= 2 {
        let flags = expect_operand(inst, 1)?;
        require_integer_like(ctx.load_value_to_result(flags)?, "json_encode flags")?;
        abi::emit_store_reg_to_symbol(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            "_json_active_flags",
            0,
        );
    } else {
        abi::emit_store_zero_to_symbol(ctx.emitter, "_json_active_flags", 0);
    }
    Ok(())
}

/// Loads the value being JSON-encoded into the canonical ABI result registers.
fn load_json_encode_value(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    if matches!(value_ty, PhpType::Void | PhpType::Never) {
        return Ok(());
    }
    ctx.load_value_to_result(value)?;
    Ok(())
}

/// Dispatches a loaded PHP value to the appropriate JSON runtime encoder.
fn emit_json_encode_loaded_value(ctx: &mut FunctionContext<'_>, value_ty: &PhpType) {
    match value_ty {
        PhpType::Int => {
            abi::emit_call_label(ctx.emitter, "__rt_itoa");
        }
        PhpType::Float => {
            abi::emit_call_label(ctx.emitter, "__rt_json_encode_float");
        }
        PhpType::Bool => {
            abi::emit_call_label(ctx.emitter, "__rt_json_encode_bool");
        }
        PhpType::Str => {
            abi::emit_call_label(ctx.emitter, "__rt_json_encode_str");
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_call_label(ctx.emitter, "__rt_json_encode_null");
        }
        PhpType::Array(elem_ty) => match elem_ty.as_ref().codegen_repr() {
            PhpType::Int => abi::emit_call_label(ctx.emitter, "__rt_json_encode_array_int"),
            PhpType::Str => abi::emit_call_label(ctx.emitter, "__rt_json_encode_array_str"),
            _ => abi::emit_call_label(ctx.emitter, "__rt_json_encode_array_dynamic"),
        },
        PhpType::AssocArray { .. } => {
            abi::emit_call_label(ctx.emitter, "__rt_json_encode_assoc");
        }
        PhpType::Iterable => {
            emit_json_encode_iterable(ctx);
        }
        PhpType::Object(class_name) => {
            emit_json_encode_object(ctx, class_name);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            abi::emit_call_label(ctx.emitter, "__rt_json_encode_mixed");
        }
        PhpType::TaggedScalar => emit_json_encode_tagged_scalar(ctx),
        _ => {
            abi::emit_call_label(ctx.emitter, "__rt_json_encode_null");
        }
    }
}

/// Emits the tag test a tagged nullable int needs, choosing between the null and int encoders.
///
/// `int|null` is the one union `codegen_repr()` does not map to `Mixed`: it stays an unboxed
/// two-word `{payload, tag}` pair. It therefore reached the `_` arm above and encoded as `null`
/// for every value, silently (#1121).
///
/// The tag decides, and nothing is boxed. Boxing would allocate on every call, and it changes
/// what the runtime hands back — the failure #1121 records for `abs`, where the result type comes
/// from a hook that reads the argument. Here the two encoders already exist and the value is an
/// int or a null with no third case.
fn emit_json_encode_tagged_scalar(ctx: &mut FunctionContext<'_>) {
    let null_case = ctx.next_label("json_encode_tagged_null");
    let done = ctx.next_label("json_encode_tagged_done");

    crate::codegen::sentinels::emit_branch_if_tagged_scalar_null(ctx.emitter, &null_case);
    crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(ctx.emitter);
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&null_case);
    abi::emit_call_label(ctx.emitter, "__rt_json_encode_null");
    ctx.emitter.label(&done);
}

/// Emits heap-kind dispatch for iterable JSON values.
fn emit_json_encode_iterable(ctx: &mut FunctionContext<'_>) {
    let indexed_case = ctx.next_label("json_encode_iter_indexed");
    let assoc_case = ctx.next_label("json_encode_iter_assoc");
    let object_case = ctx.next_label("json_encode_iter_object");
    let null_case = ctx.next_label("json_encode_iter_null");
    let done = ctx.next_label("json_encode_iter_done");

    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #2");                              // check whether the iterable is backed by an indexed array
            ctx.emitter.instruction(&format!("b.eq {}", indexed_case));         // encode indexed-array iterables with the array encoder
            ctx.emitter.instruction("cmp x0, #3");                              // check whether the iterable is backed by a hash table
            ctx.emitter.instruction(&format!("b.eq {}", assoc_case));           // encode hash-backed iterables with the associative encoder
            ctx.emitter.instruction("cmp x0, #4");                              // check whether the iterable is backed by an object
            ctx.emitter.instruction(&format!("b.eq {}", object_case));          // encode object-backed iterables with the object encoder
            ctx.emitter.instruction(&format!("b {}", null_case));               // unknown iterable heap kinds encode as JSON null
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 2");                              // check whether the iterable is backed by an indexed array
            ctx.emitter.instruction(&format!("je {}", indexed_case));           // encode indexed-array iterables with the array encoder
            ctx.emitter.instruction("cmp rax, 3");                              // check whether the iterable is backed by a hash table
            ctx.emitter.instruction(&format!("je {}", assoc_case));             // encode hash-backed iterables with the associative encoder
            ctx.emitter.instruction("cmp rax, 4");                              // check whether the iterable is backed by an object
            ctx.emitter.instruction(&format!("je {}", object_case));            // encode object-backed iterables with the object encoder
            ctx.emitter.instruction(&format!("jmp {}", null_case));             // unknown iterable heap kinds encode as JSON null
        }
    }

    ctx.emitter.label(&indexed_case);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_call_label(ctx.emitter, "__rt_json_encode_array_dynamic");
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&assoc_case);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_call_label(ctx.emitter, "__rt_json_encode_assoc");
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&object_case);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_call_label(ctx.emitter, "__rt_json_encode_object");
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&null_case);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_call_label(ctx.emitter, "__rt_json_encode_null");

    ctx.emitter.label(&done);
}

/// Emits object JSON encoding, including stdClass dynamic-property hashes.
fn emit_json_encode_object(ctx: &mut FunctionContext<'_>, class_name: &str) {
    if crate::types::checker::builtin_stdclass::is_stdclass(class_name) {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("ldr x0, [x0, #8]");                    // load the stdClass dynamic-property hash pointer
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rax, QWORD PTR [rax + 8]");        // load the stdClass dynamic-property hash pointer
            }
        }
        abi::emit_call_label(ctx.emitter, "__rt_json_encode_stdclass");
    } else {
        abi::emit_call_label(ctx.emitter, "__rt_json_encode_object");
    }
}

/// Boxes the JSON encoder string-or-false result into the Mixed-compatible result slot.
fn box_json_encode_result(ctx: &mut FunctionContext<'_>) {
    let string_label = ctx.next_label("json_encode_string_result");
    let done_label = ctx.next_label("json_encode_boxed_result");

    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            abi::emit_load_symbol_to_reg(ctx.emitter, "x9", "_json_last_error", 0);
            ctx.emitter.instruction(&format!("cbz x9, {}", string_label));      // no JSON error means the string result is valid
            abi::emit_load_symbol_to_reg(ctx.emitter, "x9", "_json_active_flags", 0);
            ctx.emitter.instruction("tst x9, #512");                            // JSON_PARTIAL_OUTPUT_ON_ERROR keeps the partial string result
            ctx.emitter.instruction(&format!("b.ne {}", string_label));         // partial-output flag means return the encoded string
            abi::emit_pop_reg_pair(ctx.emitter, "x10", "x11");
            ctx.emitter.instruction("mov x0, #0");                              // false payload for json_encode failure
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the string boxing path after returning false
            ctx.emitter.label(&string_label);
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            ctx.emitter.instruction(
                "mov r10, QWORD PTR [rip + _json_last_error]"
            );                                                                  // load the current JSON error code
            ctx.emitter.instruction("test r10, r10");                           // check whether the encoder reported an error
            ctx.emitter.instruction(&format!("jz {}", string_label));           // no JSON error means the string result is valid
            ctx.emitter.instruction(
                "mov r10, QWORD PTR [rip + _json_active_flags]"
            );                                                                  // load the active JSON flag bitmask
            ctx.emitter.instruction("test r10, 512");                           // JSON_PARTIAL_OUTPUT_ON_ERROR keeps the partial string result
            ctx.emitter.instruction(&format!("jnz {}", string_label));          // partial-output flag means return the encoded string
            abi::emit_pop_reg_pair(ctx.emitter, "r10", "r11");
            ctx.emitter.instruction("xor eax, eax");                            // false payload for json_encode failure
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the string boxing path after returning false
            ctx.emitter.label(&string_label);
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
            ctx.emitter.label(&done_label);
        }
    }
}

/// Stores the active `json_validate()` flags, keeping only PHP's accepted bit.
fn lower_json_validate_flags(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() < 3 {
        abi::emit_store_zero_to_symbol(ctx.emitter, "_json_active_flags", 0);
        return Ok(());
    }
    let flags = expect_operand(inst, 2)?;
    require_integer_like(ctx.load_value_to_result(flags)?, "json_validate flags")?;
    mask_json_validate_flags(ctx);
    abi::emit_store_reg_to_symbol(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        "_json_active_flags",
        0,
    );
    Ok(())
}

/// Stores the strict depth limit used by the shared JSON validator.
fn lower_json_validate_depth(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() >= 2 {
        let depth = expect_operand(inst, 1)?;
        require_integer_like(ctx.load_value_to_result(depth)?, "json_validate depth")?;
        subtract_one_from_int_result(ctx);
        abi::emit_store_reg_to_symbol(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            "_json_depth_limit",
            0,
        );
    } else {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 511);
        abi::emit_store_reg_to_symbol(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            "_json_depth_limit",
            0,
        );
    }
    Ok(())
}

/// Loads a JSON source string into the runtime helper's expected result registers.
///
/// Returns true when a temporary stack slot was reserved for a boxed source. A concrete string
/// payload is borrowed from the live Mixed cell, avoiding the persist copy that previously
/// leaked once per `json_decode()` call. Other boxed tags still materialize a heap string,
/// whose pointer is saved so the caller can release it after the JSON helper returns.
fn load_json_source_string(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    context: &str,
) -> Result<bool> {
    match ctx.value_php_type(value)? {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            ctx.load_string_value_to_regs(value, ptr_reg, len_reg)?;
            Ok(false)
        }
        PhpType::Int => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_itoa");
            Ok(false)
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_ftoa");
            Ok(false)
        }
        PhpType::Bool => {
            lower_bool_json_source(ctx, value)?;
            Ok(false)
        }
        PhpType::Void | PhpType::Never => {
            emit_static_string_result(ctx, b"");
            Ok(false)
        }
        PhpType::Mixed | PhpType::Union(_) => {
            load_boxed_json_source_string(ctx, value)?;
            Ok(true)
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            context,
            other
        ))),
    }
}

/// Loads a boxed JSON source, borrowing string payloads and saving any materialized copy.
fn load_boxed_json_source_string(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    abi::emit_reserve_temporary_stack(ctx.emitter, 16);
    // The low slot holds a heap pointer only when the boxed source had to be stringified.
    abi::emit_load_int_immediate(ctx.emitter, abi::symbol_scratch_reg(ctx.emitter), 0);
    abi::emit_store_to_sp(ctx.emitter, abi::symbol_scratch_reg(ctx.emitter), 0);
    load_value_to_first_int_arg(ctx, value)?;
    abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), 8);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let borrowed = ctx.next_label("json_source_string_borrowed");
    let done = ctx.next_label("json_source_string_ready");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #1");                              // is the boxed payload already a string?
            ctx.emitter.instruction(&format!("b.eq {borrowed}"));               // borrow its live heap bytes without copying
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 8);
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
            ctx.emitter.instruction("str x1, [sp]");                            // save the persisted copy for the caller to release
            ctx.emitter.instruction(&format!("b {done}"));                      // join the borrowed and persisted source paths
            ctx.emitter.label(&borrowed);
            // x1/x2 already hold the borrowed pointer and length.
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 1");                              // is the boxed payload already a string?
            ctx.emitter.instruction(&format!("je {borrowed}"));                 // borrow its live heap bytes without copying
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", 8);
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // save the persisted copy for the caller to release
            ctx.emitter.instruction(&format!("jmp {done}"));                    // join the borrowed and persisted source paths
            ctx.emitter.label(&borrowed);
            ctx.emitter.instruction("mov rax, rdi");                            // move the borrowed pointer into the string result register
            // rdx already holds the borrowed length.
            ctx.emitter.label(&done);
        }
    }
    Ok(())
}

/// Releases the stringified source copy saved by `load_json_source_string`, preserving the result.
fn release_json_source_temp(ctx: &mut FunctionContext<'_>, reserved: bool) {
    if !reserved {
        return;
    }
    let result = abi::int_result_reg(ctx.emitter);
    let skip = ctx.next_label("json_source_no_temp");
    abi::emit_push_reg(ctx.emitter, result);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 16);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz {}, {}", result, skip));      // a borrowed string has no persisted copy
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {}, {}", result, result));   // a borrowed string has no persisted copy
            ctx.emitter.instruction(&format!("jz {}", skip));                   // skip the free when the source was borrowed
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
    ctx.emitter.label(&skip);
    abi::emit_pop_reg(ctx.emitter, result);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
}

/// Coerces a dynamic boolean JSON source to PHP's string form.
fn lower_bool_json_source(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let true_label = ctx.next_label("json_source_bool_true");
    let done_label = ctx.next_label("json_source_bool_done");
    ctx.load_value_to_result(value)?;
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &true_label);
    emit_static_string_result(ctx, b"");
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&true_label);
    emit_static_string_result(ctx, b"1");
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Materializes a static string result pair for scalar JSON source coercions.
fn emit_static_string_result(ctx: &mut FunctionContext<'_>, bytes: &[u8]) {
    let (label, len) = ctx.data.add_string(bytes);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
}

/// Publishes the original decode or validate source and clears its coordinates.
///
/// The length bounds the deferred line/column scan. The candidate slot starts
/// empty so a later container or syntax error can publish an absolute position
/// without inheriting a previous call's pointer.
fn store_json_error_source_ptr(ctx: &mut FunctionContext<'_>) {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_store_reg_to_symbol(ctx.emitter, ptr_reg, "_json_error_source_ptr", 0);
    abi::emit_store_reg_to_symbol(ctx.emitter, len_reg, "_json_error_source_len", 0);
    abi::emit_store_zero_to_symbol(ctx.emitter, "_json_error_candidate_ptr", 0);
    abi::emit_store_zero_to_symbol(ctx.emitter, "_json_error_line", 0);
    abi::emit_store_zero_to_symbol(ctx.emitter, "_json_error_column", 0);
}

/// Drops diagnostic-source pointers after a public JSON operation returns.
///
/// Numeric line/column state is left intact for `json_last_error_msg()`.
/// Recursive `__rt_json_decode_mixed` returns do not call this helper.
fn clear_json_diagnostic_source_refs(ctx: &mut FunctionContext<'_>) {
    abi::emit_store_zero_to_symbol(ctx.emitter, "_json_error_source_ptr", 0);
    abi::emit_store_zero_to_symbol(ctx.emitter, "_json_error_source_len", 0);
    abi::emit_store_zero_to_symbol(ctx.emitter, "_json_error_candidate_ptr", 0);
}

/// Extracts `JSON_OBJECT_AS_ARRAY` from the loaded flags and stores decode assoc mode.
fn write_json_decode_assoc_from_flags(ctx: &mut FunctionContext<'_>) {
    let reg = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("and {reg}, {reg}, #1"));          // keep JSON_OBJECT_AS_ARRAY when associative is null or missing
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("and {reg}, 1"));                  // keep JSON_OBJECT_AS_ARRAY when associative is null or missing
        }
    }
    abi::emit_store_reg_to_symbol(ctx.emitter, reg, "_json_decode_assoc", 0);
}

/// Boxes `Mixed(null)` when `__rt_json_decode_mixed` reports a decode failure.
fn box_null_on_json_decode_failure(ctx: &mut FunctionContext<'_>) {
    let done_label = ctx.next_label("json_decode_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x0, {}", done_label));       // valid JSON already returned a boxed Mixed value
            ctx.emitter.instruction("mov x0, #8");                              // tag = Mixed null
            ctx.emitter.instruction("mov x1, #0");                              // value_lo = 0 for null
            ctx.emitter.instruction("mov x2, #0");                              // value_hi = 0 for null
            ctx.emitter.instruction("bl __rt_mixed_from_value");                // box Mixed(null) for decode failures
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // valid JSON returns a non-null Mixed pointer
            ctx.emitter.instruction(&format!("jne {}", done_label));            // skip null boxing when decode succeeded
            ctx.emitter.instruction("mov rax, 8");                              // tag = Mixed null
            ctx.emitter.instruction("mov rdi, 0");                              // value_lo = 0 for null
            ctx.emitter.instruction("mov rsi, 0");                              // value_hi = 0 for null
            ctx.emitter.instruction("call __rt_mixed_from_value");              // box Mixed(null) for decode failures
            ctx.emitter.label(&done_label);
        }
    }
}

/// Masks unsupported validate flags, preserving only `JSON_INVALID_UTF8_IGNORE`.
fn mask_json_validate_flags(ctx: &mut FunctionContext<'_>) {
    let reg = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x9, #1048576");                        // mask = JSON_INVALID_UTF8_IGNORE, the only json_validate flag PHP allows
            ctx.emitter.instruction(&format!("and {reg}, {reg}, x9"));          // ignore dynamically supplied unsupported validate flags
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("and {reg}, 1048576"));            // keep only JSON_INVALID_UTF8_IGNORE for dynamic validate flags
        }
    }
}

/// Applies the strict-depth `depth - 1` runtime convention in the integer result register.
fn subtract_one_from_int_result(ctx: &mut FunctionContext<'_>) {
    let reg = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("sub {reg}, {reg}, #1"));          // convert PHP JSON depth to the runtime strict-depth limit
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("sub {reg}, 1"));                  // convert PHP JSON depth to the runtime strict-depth limit
        }
    }
}

/// Verifies a value can be passed as a JSON integer option.
fn require_integer_like(ty: PhpType, context: &str) -> Result<()> {
    if matches!(ty, PhpType::Int | PhpType::Bool) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for PHP type {:?}",
        context,
        ty
    )))
}

/// Verifies that the builtin call has between the expected lowered operand counts.
fn ensure_arg_count_between(
    inst: &Instruction,
    name: &str,
    min: usize,
    max: usize,
) -> Result<()> {
    if (min..=max).contains(&inst.operands.len()) {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "{} expected {} to {} args, got {}",
        name,
        min,
        max,
        inst.operands.len()
    )))
}
