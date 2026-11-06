//! Purpose:
//! Materializes typed PHP string-write offsets and shared diagnostic calls.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_calls::lower()`.
//!
//! Key details:
//! - Numeric spelling uses the bounded shared PHP scanner on every supported target.
//! - Operand values are borrowed; the EIR caller roots them across warning callbacks.

use crate::codegen::{abi, context::FunctionContext, platform::Arch, Result};
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;
use super::{expect_operand, store_if_result};

/// Emits a complete diagnostic without turning runtime warnings into user-generated errors.
pub(super) fn lower_diagnostic(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ctx.load_value_to_result(expect_operand(inst, 0)?)?;
    emit_current_diagnostic(ctx, true);
    Ok(())
}

/// Converts one offset to an integer and throws PHP's selected TypeError for invalid keys.
pub(super) fn lower_offset(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let key = expect_operand(inst, 0)?;
    let ty = ctx.value_php_type(key)?.codegen_repr();
    match ty {
        PhpType::Str => lower_string_offset(ctx, key)?,
        PhpType::Int => { ctx.load_value_to_result(key)?; }
        PhpType::Float => {
            ctx.load_value_to_result(key)?;
            super::builtins::strings::emit_explicit_float_result_to_int(ctx);
            emit_cast_diagnostic(ctx);
        }
        PhpType::Bool => {
            ctx.load_value_to_result(key)?;
            emit_cast_diagnostic(ctx);
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            emit_cast_diagnostic(ctx);
        }
        PhpType::Mixed | PhpType::Union(_) => lower_boxed_offset(ctx, key)?,
        PhpType::Object(name) => emit_invalid_offset(ctx, &name),
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => emit_invalid_offset(ctx, "array"),
        _ => emit_invalid_offset(ctx, "resource"),
    }
    store_if_result(ctx, inst)
}

/// Applies the same dispatch to a dynamically typed key without silently coercing arrays.
fn lower_boxed_offset(ctx: &mut FunctionContext<'_>, key: ValueId) -> Result<()> {
    // -- read the boxed key's runtime tag and payload --
    ctx.load_value_to_result(key)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let done = ctx.next_label("string_write_offset_done");
    let labels: Vec<_> = (0..9).map(|tag| ctx.next_label(&format!("string_write_offset_tag_{tag}"))).collect();
    // -- dispatch each supported tag to its PHP offset conversion --
    for (tag, label) in labels.iter().enumerate() {
        ins(ctx, &format!("cmp x0, #{tag}"), &format!("cmp rax, {tag}"));
        ins(ctx, &format!("b.eq {label}"), &format!("je {label}"));
    }
    emit_invalid_offset(ctx, "resource");
    // -- normalize scalar payloads or construct the selected TypeError --
    for (tag, label) in labels.iter().enumerate() {
        ctx.emitter.label(label);
        match tag {
            0 => abi::emit_reg_move(ctx.emitter, abi::int_result_reg(ctx.emitter),
                crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target)),
            1 => { lower_string_offset(ctx, key)?; }
            2 => {
                ins(ctx, "fmov d0, x1", "movq xmm0, rdi");
                super::builtins::strings::emit_explicit_float_result_to_int(ctx);
                emit_cast_diagnostic(ctx);
            }
            3 | 8 => {
                if tag == 8 { abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0); }
                else { abi::emit_reg_move(ctx.emitter, abi::int_result_reg(ctx.emitter),
                    crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target)); }
                emit_cast_diagnostic(ctx);
            }
            4 | 5 => emit_invalid_offset(ctx, "array"),
            6 => {
                let result = abi::int_result_reg(ctx.emitter);
                abi::emit_reg_move(ctx.emitter, result,
                    crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target));
                super::runtime_class_messages::emit_runtime_class_name_to_string_result(ctx, result, "object");
                concat_fragment(ctx, "Cannot access offset of type ", true);
                concat_fragment(ctx, " on string", false);
                abi::emit_call_label(ctx.emitter, "__rt_str_persist");
                super::exceptions::emit_type_error_from_string_result(ctx);
            }
            _ => emit_invalid_offset(ctx, "resource"),
        }
        abi::emit_jump(ctx.emitter, &done);
    }
    ctx.emitter.label(&done);
    Ok(())
}

/// Rejects noninteger numeric spellings while diagnosing a legal leading-integer suffix once.
fn lower_string_offset(ctx: &mut FunctionContext<'_>, key: ValueId) -> Result<()> {
    // -- materialize string bytes for the bounded numeric scanner --
    ctx.load_value_to_result(key)?;
    if ctx.value_php_type(key)?.codegen_repr() != PhpType::Str {
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        ins(ctx, "mov x0, x1", "mov rax, rdi");
    }
    let (ptr, len) = abi::string_result_regs(ctx.emitter);
    abi::emit_reg_move(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), ptr);
    abi::emit_reg_move(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), len);
    abi::emit_call_label(ctx.emitter, "__rt_str_numeric_value");
    let invalid = ctx.next_label("string_write_offset_invalid");
    let valid = ctx.next_label("string_write_offset_valid");
    let ready = ctx.next_label("string_write_offset_ready");
    // -- reject noninteger numeric spellings before using the offset --
    ins(ctx, "cmp x0, #0", "cmp rax, 0");
    ins(ctx, &format!("b.ne {invalid}"), &format!("jne {invalid}"));
    ins(ctx, "cmp x2, #2", "cmp rdx, 2");
    ins(ctx, &format!("b.ne {valid}"), &format!("jne {valid}"));
    ctx.emitter.label(&invalid);
    emit_invalid_offset(ctx, "string");
    ctx.emitter.label(&valid);
    // -- retain the converted offset while diagnosing a trailing suffix --
    abi::emit_push_reg(ctx.emitter, crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target));
    ins(ctx, "cmp x2, #0", "cmp rdx, 0");
    ins(ctx, &format!("b.eq {ready}"), &format!("je {ready}"));
    static_diagnostic(ctx, "Warning: Illegal string offset \"", false);
    ctx.load_value_to_result(key)?;
    if ctx.value_php_type(key)?.codegen_repr() != PhpType::Str {
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        ins(ctx, "mov x0, x1", "mov rax, rdi");
    }
    emit_current_diagnostic(ctx, false);
    static_diagnostic(ctx, "\"\n", true);
    // -- restore the offset after the warning handler returns --
    ctx.emitter.label(&ready);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    Ok(())
}

/// Preserves the normalized offset while dispatching PHP's scalar cast warning.
fn emit_cast_diagnostic(ctx: &mut FunctionContext<'_>) {
    // -- preserve the offset around PHP's scalar-cast warning --
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    static_diagnostic(ctx, "Warning: String offset cast occurred\n", true);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
}

/// Raises an ordinary TypeError through the existing native exception machinery.
fn emit_invalid_offset(ctx: &mut FunctionContext<'_>, name: &str) {
    super::exceptions::emit_type_error(ctx, &format!("Cannot access offset of type {name} on string"));
}

/// Borrows a static diagnostic fragment in the native string-result register pair.
fn static_diagnostic(ctx: &mut FunctionContext<'_>, text: &str, complete: bool) {
    let (label, len) = ctx.data.add_string(text.as_bytes());
    let (ptr, length) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr, &label);
    abi::emit_load_int_immediate(ctx.emitter, length, len as i64);
    emit_current_diagnostic(ctx, complete);
}

/// Maps the native string result to the shared warning producer's ABI.
fn emit_current_diagnostic(ctx: &mut FunctionContext<'_>, complete: bool) {
    // -- pass the diagnostic pointer and byte length through the target ABI --
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the borrowed diagnostic pointer
        ctx.emitter.instruction("mov rsi, rdx");                                // pass its explicit byte length
    }
    abi::emit_call_label(ctx.emitter, if complete { "__rt_diag_warning" } else { "__rt_diag_warning_fragment" });
}

/// Adds static context around a runtime object class name before constructing its TypeError.
fn concat_fragment(ctx: &mut FunctionContext<'_>, text: &str, prefix: bool) {
    // -- materialize static context around the runtime class name --
    let (label, len) = ctx.data.add_string(text.as_bytes());
    let (ptr, length) = abi::string_result_regs(ctx.emitter);
    let (right, right_len) = if ctx.emitter.target.arch == Arch::AArch64 { ("x3", "x4") } else { ("rdi", "rsi") };
    if prefix {
        abi::emit_reg_move(ctx.emitter, right, ptr);
        abi::emit_reg_move(ctx.emitter, right_len, length);
        abi::emit_symbol_address(ctx.emitter, ptr, &label);
        abi::emit_load_int_immediate(ctx.emitter, length, len as i64);
    } else {
        abi::emit_symbol_address(ctx.emitter, right, &label);
        abi::emit_load_int_immediate(ctx.emitter, right_len, len as i64);
    }
    abi::emit_call_label(ctx.emitter, "__rt_concat");
}

/// Emits equivalent offset conversion instructions for each native architecture.
fn ins(ctx: &mut FunctionContext<'_>, arm: &str, x86: &str) {
    ctx.emitter.instruction(if ctx.emitter.target.arch == Arch::AArch64 { arm } else { x86 }); // preserve offset semantics across the supported target ABIs
}
