//! Purpose:
//! ReflectionFunction construction and bounded runtime name dispatch for the
//! locked DOM, libxml, and SimpleXML internal-function registry.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection::owner_dispatch`.
//!
//! Key details:
//! - Unknown names must throw catchable `ReflectionException` values with code 0
//!   and their raw PHP input preserved in the message on every supported target.

use super::*;
use super::owner_dispatch::emit_reflection_dispatch_jump;

/// Allocates ReflectionFunction metadata from a known function or a bounded runtime extension name.
pub(super) fn lower_reflection_function_new(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let Some(value) = inst.operands.first().copied() else {
        let metadata = empty_reflection_metadata();
        emit_reflection_owner_object(ctx, "ReflectionFunction", &metadata)?;
        let result = inst.result.ok_or_else(|| {
            CodegenIrError::invalid_module("reflection object_new missing result")
        })?;
        return ctx.store_result_value(result);
    };

    if let Some(name) = const_optional_string_operand(ctx, value, "ReflectionFunction")? {
        let metadata = reflection_function_metadata_for_name(ctx, &name)?;
        if metadata.reflected_name.is_none() {
            super::super::super::exceptions::emit_reflection_exception(
                ctx,
                &format!("Function {}() does not exist", name),
            );
            return Ok(());
        }
        if !emit_shared_reflection_owner_factory(ctx, "ReflectionFunction", &name, false)? {
            emit_reflection_owner_object(ctx, "ReflectionFunction", &metadata)?;
        }
    } else {
        emit_runtime_extension_reflection_function(ctx, value)?;
    }

    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("reflection object_new missing result"))?;
    ctx.store_result_value(result)
}

/// Selects DOM, libxml, and SimpleXML function metadata from a case-insensitive runtime name.
fn emit_runtime_extension_reflection_function(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    let mut functions = crate::internal_extensions::registry()
        .function_names()
        .collect::<Vec<_>>();
    functions.sort_unstable_by_key(|name| php_symbol_key(name));
    let done_label = ctx.next_label("reflection_function_done");
    let case_labels = functions
        .iter()
        .map(|_| ctx.next_label("reflection_function_case"))
        .collect::<Vec<_>>();

    for (function, label) in functions.iter().zip(case_labels.iter()) {
        emit_reflection_function_name_compare(ctx, value, function, label)?;
    }
    emit_runtime_reflection_function_exception(ctx, value)?;

    for (function, label) in functions.iter().zip(case_labels.iter()) {
        ctx.emitter.label(label);
        if !emit_shared_reflection_owner_factory(ctx, "ReflectionFunction", function, false)? {
            let metadata = reflection_function_metadata_for_name(ctx, function)?;
            emit_reflection_owner_object(ctx, "ReflectionFunction", &metadata)?;
        }
        emit_reflection_dispatch_jump(ctx, &done_label);
    }

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Builds PHP's missing-function message from the rejected dynamic ReflectionFunction name.
fn emit_runtime_reflection_function_exception(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    const PREFIX: &str = "Function ";
    const SUFFIX: &str = "() does not exist";

    let (prefix_label, prefix_len) = ctx.data.add_string(PREFIX.as_bytes());
    let (suffix_label, suffix_len) = ctx.data.add_string(SUFFIX.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &prefix_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", prefix_len as i64);
            ctx.load_string_value_to_regs(value, "x3", "x4")?;
            abi::emit_call_label(ctx.emitter, "__rt_concat");
            abi::emit_symbol_address(ctx.emitter, "x3", &suffix_label);
            abi::emit_load_int_immediate(ctx.emitter, "x4", suffix_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_concat");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rax", &prefix_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", prefix_len as i64);
            ctx.load_string_value_to_regs(value, "rdi", "rsi")?;
            abi::emit_call_label(ctx.emitter, "__rt_concat");
            abi::emit_symbol_address(ctx.emitter, "rdi", &suffix_label);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", suffix_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_concat");
        }
    }
    super::super::super::exceptions::emit_reflection_exception_from_string_result(ctx);
    Ok(())
}

/// Branches to one ReflectionFunction metadata case for a case-insensitive runtime name.
fn emit_reflection_function_name_compare(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    function_name: &str,
    matched_label: &str,
) -> Result<()> {
    let (label, len) = ctx.data.add_string(function_name.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_string_value_to_regs(value, "x1", "x2")?;
            emit_reflection_function_name_leading_slash_normalization(ctx, "x1", "x2");
            abi::emit_symbol_address(ctx.emitter, "x3", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x4", len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_strcasecmp");
            ctx.emitter.instruction("cmp x0, #0");                              // compare the runtime function name without PHP case sensitivity
            ctx.emitter.instruction(&format!("b.eq {}", matched_label));        // select the matching ReflectionFunction metadata
        }
        Arch::X86_64 => {
            ctx.load_string_value_to_regs(value, "rdi", "rsi")?;
            emit_reflection_function_name_leading_slash_normalization(ctx, "rdi", "rsi");
            abi::emit_symbol_address(ctx.emitter, "rdx", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rcx", len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_strcasecmp");
            ctx.emitter.instruction("test rax, rax");                           // compare the runtime function name without PHP case sensitivity
            ctx.emitter.instruction(&format!("je {}", matched_label));          // select the matching ReflectionFunction metadata
        }
    }
    Ok(())
}

/// Rebases a borrowed dynamic function-name slice after exactly one leading namespace separator.
///
/// The caller reloads the source value before each bounded candidate comparison, so this changes
/// only call-local pointer/length registers and neither mutates nor takes ownership of the PHP
/// string. A doubled separator restores the original slice so it follows PHP's normal missing-
/// function ReflectionException path. This mirrors the literal ReflectionFunction lookup rule.
fn emit_reflection_function_name_leading_slash_normalization(
    ctx: &mut FunctionContext<'_>,
    pointer_reg: &str,
    length_reg: &str,
) {
    let done_label = ctx.next_label("reflection_function_trimmed_name");
    let single_slash_label = ctx.next_label("reflection_function_single_leading_slash");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz {length_reg}, {done_label}")); // preserve an empty borrowed name without dereferencing it
            ctx.emitter.instruction(&format!("ldrb w9, [{pointer_reg}]"));      // inspect the first byte of the dynamic function name
            ctx.emitter.instruction("cmp w9, #92");                             // is the next byte PHP's namespace separator?
            ctx.emitter.instruction(&format!("b.ne {done_label}"));             // keep the original slice when no separator remains
            ctx.emitter.instruction(&format!("add {pointer_reg}, {pointer_reg}, #1")); // rebase the borrowed pointer beyond one separator
            ctx.emitter.instruction(&format!("sub {length_reg}, {length_reg}, #1"));    // keep the borrowed length aligned with the rebased pointer
            ctx.emitter.instruction(&format!("cbz {length_reg}, {done_label}")); // accept one trailing separator as the empty lookup name
            ctx.emitter.instruction(&format!("ldrb w9, [{pointer_reg}]"));      // inspect whether the original name had a second separator
            ctx.emitter.instruction("cmp w9, #92");                             // reject doubled namespace separators like PHP
            ctx.emitter.instruction(&format!("b.ne {single_slash_label}"));     // retain the one-separator rebased slice
            ctx.emitter.instruction(&format!("sub {pointer_reg}, {pointer_reg}, #1")); // restore the original borrowed pointer for an invalid doubled prefix
            ctx.emitter.instruction(&format!("add {length_reg}, {length_reg}, #1"));    // restore the original borrowed length for the exception message
            ctx.emitter.label(&single_slash_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {length_reg}, {length_reg}")); // preserve an empty borrowed name without dereferencing it
            ctx.emitter.instruction(&format!("je {done_label}"));               // skip the byte load when the dynamic name is empty
            ctx.emitter.instruction(&format!("cmp BYTE PTR [{pointer_reg}], 92"));// is the next byte PHP's namespace separator?
            ctx.emitter.instruction(&format!("jne {done_label}"));              // keep the original slice when no separator remains
            ctx.emitter.instruction(&format!("inc {pointer_reg}"));             // rebase the borrowed pointer beyond one separator
            ctx.emitter.instruction(&format!("dec {length_reg}"));              // keep the borrowed length aligned with the rebased pointer
            ctx.emitter.instruction(&format!("test {length_reg}, {length_reg}")); // accept one trailing separator as the empty lookup name
            ctx.emitter.instruction(&format!("je {done_label}"));               // no second byte means the one separator was valid
            ctx.emitter.instruction(&format!("cmp BYTE PTR [{pointer_reg}], 92"));// inspect whether the original name had a second separator
            ctx.emitter.instruction(&format!("jne {single_slash_label}"));      // retain the one-separator rebased slice
            ctx.emitter.instruction(&format!("dec {pointer_reg}"));             // restore the original borrowed pointer for an invalid doubled prefix
            ctx.emitter.instruction(&format!("inc {length_reg}"));              // restore the original borrowed length for the exception message
            ctx.emitter.label(&single_slash_label);
        }
    }
    ctx.emitter.label(&done_label);
}
