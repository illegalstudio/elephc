//! Purpose:
//! Provides boolean, symbol, argument-validation, and literal helpers shared by builtin lowering groups.
//!
//! Called from:
//! - Sibling modules under `crate::codegen::lower_inst::builtins`.
//!
//! Key details:
//! - Centralizes representation-neutral metadata checks without owning individual builtin semantics.

use super::*;

/// Emits a boolean immediate into the integer result register.
pub(in crate::codegen::lower_inst) fn emit_static_bool(ctx: &mut FunctionContext<'_>, value: bool) {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        i64::from(value),
    );
}

/// Returns true when a static callable name resolves to any known callable function.
pub(in crate::codegen::lower_inst) fn callable_name_exists(
    ctx: &FunctionContext<'_>,
    name: &str,
    strict_php: bool,
) -> bool {
    ctx.function_variant_group_name(name).is_some()
        || ctx.function_by_name(name).is_some()
        || ctx.has_extern_function(name)
        || is_php_visible_builtin_function_for_profile(
            name.trim_start_matches('\\'),
            strict_php,
        )
}

/// Checks whether a PHP symbol is present in an iterator of known names.
pub(in crate::codegen::lower_inst) fn contains_folded<'a>(
    mut names: impl Iterator<Item = &'a String>,
    needle: &str,
) -> bool {
    let needle_key = php_symbol_key(needle.trim_start_matches('\\'));
    names.any(|name| php_symbol_key(name.trim_start_matches('\\')) == needle_key)
}

/// Returns true for internal helper classes that should not be visible to PHP class_exists().
pub(in crate::codegen::lower_inst) fn is_internal_synthetic_class_name(name: &str) -> bool {
    php_symbol_key(name).starts_with("__elephc")
}

/// Returns a string literal value defined by a `ConstStr` instruction.
///
/// The remaining callers (`is_callable()` static-string folding, `method_exists()` /
/// `property_exists()`) still require a compile-time name; `function_exists()` no longer does and
/// uses [`maybe_const_string_operand`] plus [`lower_dynamic_function_exists`] instead.
pub(in crate::codegen::lower_inst) fn const_string_operand(ctx: &FunctionContext<'_>, value: ValueId) -> Result<String> {
    maybe_const_string_operand(ctx, value)?.ok_or_else(|| {
        CodegenIrError::unsupported("builtin requires a literal string name")
    })
}

/// Returns a string literal operand when a value is produced by `ConstStr`.
pub(in crate::codegen::lower_inst) fn maybe_const_string_operand(ctx: &FunctionContext<'_>, value: ValueId) -> Result<Option<String>> {
    let value_ref = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if !matches!(inst_ref.op, Op::ConstStr | Op::ConstClassName) {
        return Ok(None);
    }
    let Some(Immediate::Data(data)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(
            "ConstStr operand has no data id",
        ));
    };
    let values = match inst_ref.op {
        Op::ConstStr => &ctx.module.data.strings,
        Op::ConstClassName => &ctx.module.data.class_names,
        _ => unreachable!("constant-string opcode was checked above"),
    };
    values
        .get(data.as_raw() as usize)
        .cloned()
        .map(Some)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))
}

/// Verifies that the builtin call has the expected number of lowered operands.
pub(in crate::codegen::lower_inst) fn ensure_arg_count(inst: &Instruction, name: &str, expected: usize) -> Result<()> {
    if inst.operands.len() == expected {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "{} expected {} args, got {}",
        name,
        expected,
        inst.operands.len()
    )))
}

/// Verifies that the builtin call has at least the expected number of lowered operands.
pub(in crate::codegen::lower_inst) fn ensure_min_arg_count(inst: &Instruction, name: &str, expected: usize) -> Result<()> {
    if inst.operands.len() >= expected {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "{} expected at least {} args, got {}",
        name,
        expected,
        inst.operands.len()
    )))
}

/// Verifies that the builtin call has between the expected lowered operand counts.
pub(in crate::codegen::lower_inst) fn ensure_arg_count_between(
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
