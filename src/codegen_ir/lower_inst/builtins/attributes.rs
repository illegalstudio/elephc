//! Purpose:
//! Lowers class-level PHP attribute metadata builtins for the EIR backend.
//! Materializes attribute name and literal argument arrays from EIR class metadata.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - Class and attribute lookup follows PHP's case-insensitive symbol rules.
//! - Captured literal attribute arguments are boxed as owned Mixed cells.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::{Immediate, Instruction, Op, ValueDef, ValueId};
use crate::names::php_symbol_key;
use crate::types::{AttrArgValue, ClassInfo, PhpType};

use super::super::super::context::FunctionContext;

/// Lowers `class_attribute_names(class)` into an indexed string array.
pub(super) fn lower_class_attribute_names(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "class_attribute_names", 1)?;
    let class = super::expect_operand(inst, 0)?;
    let class_name = const_string_operand(ctx, class, "class_attribute_names")?;
    let names = class_info(ctx, &class_name)
        .map(|info| info.attribute_names.clone())
        .unwrap_or_default();

    emit_string_array(ctx, &names)?;
    super::store_if_result(ctx, inst)
}

/// Lowers `class_attribute_args(class, attr)` into an indexed Mixed array.
pub(super) fn lower_class_attribute_args(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "class_attribute_args", 2)?;
    let class = super::expect_operand(inst, 0)?;
    let attr = super::expect_operand(inst, 1)?;
    let class_name = const_string_operand(ctx, class, "class_attribute_args")?;
    let attr_name = const_string_operand(ctx, attr, "class_attribute_args")?;
    let attr_args = attribute_args(ctx, &class_name, &attr_name);

    emit_mixed_array(ctx, &attr_args)?;
    super::store_if_result(ctx, inst)
}

/// Returns captured literal args for the first matching class attribute.
fn attribute_args(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    attr_name: &str,
) -> Vec<AttrArgValue> {
    let attr_key = php_symbol_key(attr_name.trim_start_matches('\\'));
    class_info(ctx, class_name)
        .and_then(|info| {
            info.attribute_names.iter().enumerate().find_map(|(idx, name)| {
                let candidate = php_symbol_key(name.trim_start_matches('\\'));
                (candidate == attr_key).then(|| {
                    info.attribute_args
                        .get(idx)
                        .and_then(Clone::clone)
                        .unwrap_or_default()
                })
            })
        })
        .unwrap_or_default()
}

/// Looks up class metadata by PHP-style case-insensitive name.
fn class_info<'a>(ctx: &'a FunctionContext<'_>, class_name: &str) -> Option<&'a ClassInfo> {
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    ctx.module
        .class_infos
        .iter()
        .find(|(candidate, _)| php_symbol_key(candidate.trim_start_matches('\\')) == class_key)
        .map(|(_, info)| info)
}

/// Allocates and fills an indexed array of attribute-name strings.
fn emit_string_array(ctx: &mut FunctionContext<'_>, names: &[String]) -> Result<()> {
    allocate_indexed_array(ctx, names.len().max(1), 16);
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_string_array_fill_aarch64(ctx, names),
        Arch::X86_64 => emit_string_array_fill_x86_64(ctx, names),
    }
    Ok(())
}

/// Appends attribute-name strings to the current result array on AArch64.
fn emit_string_array_fill_aarch64(ctx: &mut FunctionContext<'_>, names: &[String]) {
    ctx.emitter.instruction("str x0, [sp, #-16]!");                             // park the attribute-name array while appending names
    for name in names {
        let (label, len) = ctx.data.add_string(name.as_bytes());
        ctx.emitter.instruction("ldr x0, [sp]");                                // reload the attribute-name array for this append
        abi::emit_symbol_address(ctx.emitter, "x1", &label);
        abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        ctx.emitter.instruction("str x0, [sp]");                                // preserve the possibly-grown attribute-name array
    }
    ctx.emitter.instruction("ldr x0, [sp], #16");                               // restore the final attribute-name array as the result
}

/// Appends attribute-name strings to the current result array on x86_64.
fn emit_string_array_fill_x86_64(ctx: &mut FunctionContext<'_>, names: &[String]) {
    ctx.emitter.instruction("push rax");                                        // park the attribute-name array while appending names
    ctx.emitter.instruction("sub rsp, 8");                                      // keep stack alignment stable across append helper calls
    for name in names {
        let (label, len) = ctx.data.add_string(name.as_bytes());
        ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");                // reload the attribute-name array for this append
        abi::emit_symbol_address(ctx.emitter, "rsi", &label);
        abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
        abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");                // preserve the possibly-grown attribute-name array
    }
    ctx.emitter.instruction("add rsp, 8");                                      // drop the temporary alignment slot
    ctx.emitter.instruction("pop rax");                                         // restore the final attribute-name array as the result
}

/// Allocates and fills an indexed array of boxed Mixed attribute arguments.
fn emit_mixed_array(ctx: &mut FunctionContext<'_>, attr_args: &[AttrArgValue]) -> Result<()> {
    allocate_indexed_array(ctx, attr_args.len().max(1), 8);
    crate::codegen::emit_array_value_type_stamp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &PhpType::Mixed,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_mixed_array_fill_aarch64(ctx, attr_args),
        Arch::X86_64 => emit_mixed_array_fill_x86_64(ctx, attr_args),
    }
    Ok(())
}

/// Appends boxed Mixed attribute arguments to the current result array on AArch64.
fn emit_mixed_array_fill_aarch64(
    ctx: &mut FunctionContext<'_>,
    attr_args: &[AttrArgValue],
) {
    ctx.emitter.instruction("str x0, [sp, #-16]!");                             // park the attribute-arg array while boxing values
    for arg in attr_args {
        emit_box_arg_aarch64(ctx, arg);
        ctx.emitter.instruction("mov x1, x0");                                  // pass the boxed attribute argument as the append value
        ctx.emitter.instruction("ldr x0, [sp]");                                // reload the attribute-arg array for this append
        abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        ctx.emitter.instruction("str x0, [sp]");                                // preserve the possibly-grown attribute-arg array
    }
    ctx.emitter.instruction("ldr x0, [sp], #16");                               // restore the final attribute-arg array as the result
}

/// Appends boxed Mixed attribute arguments to the current result array on x86_64.
fn emit_mixed_array_fill_x86_64(
    ctx: &mut FunctionContext<'_>,
    attr_args: &[AttrArgValue],
) {
    ctx.emitter.instruction("push rax");                                        // park the attribute-arg array while boxing values
    ctx.emitter.instruction("sub rsp, 8");                                      // keep stack alignment stable across helper calls
    for arg in attr_args {
        emit_box_arg_x86_64(ctx, arg);
        ctx.emitter.instruction("mov rsi, rax");                                // pass the boxed attribute argument as the append value
        ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");                // reload the attribute-arg array for this append
        abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");                // preserve the possibly-grown attribute-arg array
    }
    ctx.emitter.instruction("add rsp, 8");                                      // drop the temporary alignment slot
    ctx.emitter.instruction("pop rax");                                         // restore the final attribute-arg array as the result
}

/// Allocates an indexed array with the requested capacity and element stride.
fn allocate_indexed_array(ctx: &mut FunctionContext<'_>, capacity: usize, stride: i64) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x1", stride);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", stride);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
}

/// Boxes one captured attribute argument into the AArch64 Mixed-cell ABI.
fn emit_box_arg_aarch64(ctx: &mut FunctionContext<'_>, arg: &AttrArgValue) {
    match arg {
        AttrArgValue::Null => {
            ctx.emitter.instruction("mov x0, #8");                              // runtime tag 8 = null payload
            ctx.emitter.instruction("mov x1, xzr");                             // null mixed payloads carry no low word
            ctx.emitter.instruction("mov x2, xzr");                             // null mixed payloads carry no high word
        }
        AttrArgValue::Int(value) => {
            ctx.emitter.instruction("mov x0, #0");                              // runtime tag 0 = integer payload
            ctx.emitter.instruction(&format!("mov x1, #{}", value));            // pass the captured integer as the mixed low word
            ctx.emitter.instruction("mov x2, xzr");                             // integer mixed payloads do not use the high word
        }
        AttrArgValue::Bool(value) => {
            ctx.emitter.instruction("mov x0, #3");                              // runtime tag 3 = boolean payload
            ctx.emitter.instruction(&format!("mov x1, #{}", *value as u64));    // pass the captured boolean as the mixed low word
            ctx.emitter.instruction("mov x2, xzr");                             // boolean mixed payloads do not use the high word
        }
        AttrArgValue::Str(value) => {
            let bytes = crate::string_bytes::literal_bytes(value);
            let (label, len) = ctx.data.add_string(&bytes);
            ctx.emitter.instruction("mov x0, #1");                              // runtime tag 1 = string payload
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            ctx.emitter.instruction(&format!("mov x2, #{}", len));              // pass the captured string length as the mixed high word
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
}

/// Boxes one captured attribute argument into the x86_64 Mixed-cell ABI.
fn emit_box_arg_x86_64(ctx: &mut FunctionContext<'_>, arg: &AttrArgValue) {
    match arg {
        AttrArgValue::Null => {
            ctx.emitter.instruction("mov rax, 8");                              // runtime tag 8 = null payload
            ctx.emitter.instruction("xor rdi, rdi");                            // null mixed payloads carry no low word
            ctx.emitter.instruction("xor rsi, rsi");                            // null mixed payloads carry no high word
        }
        AttrArgValue::Int(value) => {
            ctx.emitter.instruction("mov rax, 0");                              // runtime tag 0 = integer payload
            ctx.emitter.instruction(&format!("mov rdi, {}", value));            // pass the captured integer as the mixed low word
            ctx.emitter.instruction("xor rsi, rsi");                            // integer mixed payloads do not use the high word
        }
        AttrArgValue::Bool(value) => {
            ctx.emitter.instruction("mov rax, 3");                              // runtime tag 3 = boolean payload
            ctx.emitter.instruction(&format!("mov rdi, {}", *value as u64));    // pass the captured boolean as the mixed low word
            ctx.emitter.instruction("xor rsi, rsi");                            // boolean mixed payloads do not use the high word
        }
        AttrArgValue::Str(value) => {
            let bytes = crate::string_bytes::literal_bytes(value);
            let (label, len) = ctx.data.add_string(&bytes);
            ctx.emitter.instruction("mov rax, 1");                              // runtime tag 1 = string payload
            abi::emit_symbol_address(ctx.emitter, "rdi", &label);
            ctx.emitter.instruction(&format!("mov rsi, {}", len));              // pass the captured string length as the mixed high word
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
}

/// Returns a string literal value defined by a `ConstStr` instruction operand.
fn const_string_operand(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    builtin: &str,
) -> Result<String> {
    let value_ref = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Err(CodegenIrError::unsupported(format!(
            "{} with non-literal string argument",
            builtin
        )));
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if inst_ref.op != Op::ConstStr {
        return Err(CodegenIrError::unsupported(format!(
            "{} with non-literal string argument",
            builtin
        )));
    }
    let Some(Immediate::Data(data)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(format!(
            "{} string literal has no data id",
            builtin
        )));
    };
    ctx.module
        .data
        .strings
        .get(data.as_raw() as usize)
        .cloned()
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))
}
