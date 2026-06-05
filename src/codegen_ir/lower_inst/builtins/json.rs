//! Purpose:
//! Lowers JSON state and validation builtins for the EIR backend.
//! Bridges already-evaluated EIR operands to the shared JSON runtime helpers.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - JSON error state is runtime-global and must be reset after PHP arguments
//!   have already been evaluated by preceding EIR instructions.

use crate::codegen::{abi, emit_box_current_value_as_mixed};
use crate::codegen::platform::Arch;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::super::load_value_to_first_int_arg;
use super::{expect_operand, store_if_result};

/// Lowers `json_encode(value, flags?, depth?)` through the shared JSON encoder runtime.
pub(super) fn lower_json_encode(
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
    box_json_encode_result(ctx);
    store_if_result(ctx, inst)
}

/// Lowers `json_last_error()` by reading the shared runtime error-code symbol.
pub(super) fn lower_json_last_error(
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
pub(super) fn lower_json_last_error_msg(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "json_last_error_msg", 0)?;
    abi::emit_call_label(ctx.emitter, "__rt_json_last_error_msg");
    store_if_result(ctx, inst)
}

/// Lowers `json_validate(json, depth?, flags?)` into the shared validator runtime.
pub(super) fn lower_json_validate(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "json_validate", 1, 3)?;
    let json = expect_operand(inst, 0)?;

    reset_json_validation_state(ctx);
    lower_json_validate_flags(ctx, inst)?;
    lower_json_validate_depth(ctx, inst)?;
    load_json_source_for_validate(ctx, json)?;
    abi::emit_call_label(ctx.emitter, "__rt_json_validate");
    store_if_result(ctx, inst)
}

/// Clears observable JSON error and parser state after all EIR operands have evaluated.
fn reset_json_validation_state(ctx: &mut FunctionContext<'_>) {
    abi::emit_store_zero_to_symbol(ctx.emitter, "_json_last_error", 0);
    abi::emit_store_zero_to_symbol(ctx.emitter, "_json_active_depth", 0);
    abi::emit_store_zero_to_symbol(ctx.emitter, "_json_error_location_active", 0);
    abi::emit_store_zero_to_symbol(ctx.emitter, "_json_error_source_ptr", 0);
}

/// Clears JSON encoder state after all EIR operands have already evaluated.
fn reset_json_encode_state(ctx: &mut FunctionContext<'_>) {
    abi::emit_store_zero_to_symbol(ctx.emitter, "_json_last_error", 0);
    abi::emit_store_zero_to_symbol(ctx.emitter, "_json_active_depth", 0);
    abi::emit_store_zero_to_symbol(ctx.emitter, "_json_indent_depth", 0);
    abi::emit_store_zero_to_symbol(ctx.emitter, "_json_error_location_active", 0);
    abi::emit_store_zero_to_symbol(ctx.emitter, "_json_error_source_ptr", 0);
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
        _ => {
            abi::emit_call_label(ctx.emitter, "__rt_json_encode_null");
        }
    }
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
            ctx.emitter.instruction("mov r10, QWORD PTR [rip + _json_last_error]"); // load the current JSON error code
            ctx.emitter.instruction("test r10, r10");                           // check whether the encoder reported an error
            ctx.emitter.instruction(&format!("jz {}", string_label));           // no JSON error means the string result is valid
            ctx.emitter.instruction("mov r10, QWORD PTR [rip + _json_active_flags]"); // load the active JSON flag bitmask
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

/// Loads the JSON source string into the runtime helper's expected result registers.
fn load_json_source_for_validate(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    match ctx.value_php_type(value)? {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            ctx.load_string_value_to_regs(value, ptr_reg, len_reg)
        }
        PhpType::Int => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_itoa");
            Ok(())
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_ftoa");
            Ok(())
        }
        PhpType::Bool => lower_bool_json_source(ctx, value),
        PhpType::Void | PhpType::Never => {
            emit_static_string_result(ctx, b"");
            Ok(())
        }
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "json_validate source for PHP type {:?}",
            other
        ))),
    }
}

/// Coerces a dynamic boolean JSON source to PHP's string form.
fn lower_bool_json_source(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let true_label = ctx.next_label("json_validate_bool_true");
    let done_label = ctx.next_label("json_validate_bool_done");
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
            ctx.emitter.instruction(&format!("sub {reg}, {reg}, #1"));          // convert PHP json_validate depth to the runtime strict-depth limit
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("sub {reg}, 1"));                  // convert PHP json_validate depth to the runtime strict-depth limit
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
