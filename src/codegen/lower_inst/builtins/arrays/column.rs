//! Purpose:
//! Lowers PHP `array_column()` builtin calls for the EIR backend.
//! Selects a concrete-row fast path or the general runtime walker.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays::lower_array_column()`.
//!
//! Key details:
//! - The typed fast paths (`__rt_array_column{,_str,_ref,_mixed}`) apply only to an indexed
//!   array of concrete associative rows, a string column key, and no index key.
//! - Every other shape goes through `__rt_array_column_any` (or its boxed entry), which
//!   takes a caller-owned key block of `(tag, lo, hi)` triples for both keys plus flags.
//! - A non-null index key builds a hash and boxes it as a PHP array; otherwise the result
//!   is an indexed array whose metadata is normalized after extraction so empty results
//!   still carry the correct element type.
//! - Small runtime return codes are converted into PHP's TypeErrors at the call site.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::context::FunctionContext;
use crate::codegen::lower_inst::runtime_class_messages;
use crate::codegen::{CodegenIrError, Result};
use crate::codegen_support::runtime::ARRAY_COLUMN_FLAG_HASH_RESULT;
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::super::{expect_operand, store_if_result};

/// Byte size of the key block handed to the general runtime walker.
const KEY_BLOCK_BYTES: usize = 64;
/// Key-block offset of the column key triple.
const COLUMN_TRIPLE: usize = 0;
/// Key-block offset of the static flags word.
const FLAGS_WORD: usize = 24;
/// Key-block offset of the index key triple.
const INDEX_TRIPLE: usize = 32;
/// Boxed Mixed runtime tag of PHP `null`.
const NULL_TAG: i64 = 8;

/// Lowers `array_column()` by dispatching to the fast path or the general walker.
pub(super) fn lower_array_column(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count_between(inst, "array_column", 2, 3)?;
    let array = expect_operand(inst, 0)?;
    let key = expect_operand(inst, 1)?;
    let index = inst.operands.get(2).copied();
    let index_present = match index {
        Some(index) => !is_static_null(&ctx.value_php_type(index)?),
        None => false,
    };
    let source_ty = ctx.value_php_type(array)?;
    let key_ty = ctx.value_php_type(key)?.codegen_repr();
    if !index_present && key_ty == PhpType::Str {
        if let Some(value_ty) = concrete_row_value_type(&source_ty) {
            return lower_array_column_fast(ctx, inst, array, key, value_ty);
        }
    }
    lower_array_column_general(ctx, inst, array, key, index.filter(|_| index_present))
}

/// Returns true when a key operand is statically PHP `null`.
fn is_static_null(ty: &PhpType) -> bool {
    matches!(ty, PhpType::Void | PhpType::Never)
}

/// Returns the row value type of an indexed array of concrete associative rows.
fn concrete_row_value_type(ty: &PhpType) -> Option<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(inner) => match inner.codegen_repr() {
            PhpType::AssocArray { value, .. } => Some(value.codegen_repr()),
            _ => None,
        },
        _ => None,
    }
}

/// Lowers the typed fast path: concrete associative rows and a string column key.
fn lower_array_column_fast(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    key: ValueId,
    value_ty: PhpType,
) -> Result<()> {
    let result_elem_ty = array_column_result_element_type(inst, &value_ty)?;
    // Numeric-string column keys ("5") address integer row keys, exactly like `$row["5"]`;
    // the row helpers forward the normalized pair straight to `__rt_hash_get`.
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_string_value_to_regs(key, "x1", "x2")?;
            abi::emit_call_label(ctx.emitter, "__rt_hash_normalize_key");
            ctx.load_value_to_reg(array, "x0")?;
        }
        Arch::X86_64 => {
            ctx.load_string_value_to_regs(key, "rax", "rdx")?;
            abi::emit_call_label(ctx.emitter, "__rt_hash_normalize_key");
            ctx.emitter.instruction("mov rsi, rax");                            // normalized key low word into the second argument
            ctx.load_value_to_reg(array, "rdi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, array_column_runtime_helper(&value_ty));
    super::normalize_indexed_array_result(ctx, "array_column", &value_ty, &result_elem_ty)?;
    super::box_array_result_for_mixed_builtin(ctx, inst, &result_elem_ty);
    store_if_result(ctx, inst)
}

/// Lowers every other shape through the general runtime walker.
fn lower_array_column_general(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    key: ValueId,
    index: Option<ValueId>,
) -> Result<()> {
    let source_ty = ctx.value_php_type(array)?.codegen_repr();
    let boxed = match &source_ty {
        PhpType::Mixed | PhpType::Union(_) => true,
        PhpType::Array(_) | PhpType::AssocArray { .. } => false,
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_column for PHP type {:?}",
                other
            )))
        }
    };
    let hash_result = index.is_some();
    let packed_elem_ty = if hash_result {
        None
    } else {
        Some(array_column_result_element_type(inst, &PhpType::Mixed)?)
    };
    let column_may_fail = may_hold_container(&ctx.value_php_type(key)?);
    let index_may_fail = match index {
        Some(index) => may_hold_container(&ctx.value_php_type(index)?),
        None => false,
    };

    abi::emit_reserve_temporary_stack(ctx.emitter, KEY_BLOCK_BYTES);
    store_key_triple(ctx, Some(key), COLUMN_TRIPLE)?;
    store_key_triple(ctx, index, INDEX_TRIPLE)?;
    let flags = if hash_result { ARRAY_COLUMN_FLAG_HASH_RESULT } else { 0 };
    let scratch = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, scratch, flags);
    abi::emit_store_to_sp(ctx.emitter, scratch, FLAGS_WORD);
    let (arg0, arg1) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x0", "x1"),
        Arch::X86_64 => ("rdi", "rsi"),
    };
    ctx.load_value_to_reg(array, arg0)?;
    abi::emit_temporary_stack_address(ctx.emitter, arg1, 0);
    let helper = if boxed { "__rt_array_column_boxed" } else { "__rt_array_column_any" };
    abi::emit_call_label(ctx.emitter, helper);
    abi::emit_release_temporary_stack(ctx.emitter, KEY_BLOCK_BYTES);

    let column_message = "array_column(): Argument #2 ($column_key) must be of type string|int|null, ";
    let index_message = "array_column(): Argument #3 ($index_key) must be of type string|int|null, ";
    let mut errors: Vec<(i64, ColumnError)> = Vec::new();
    if boxed {
        errors.push((0, ColumnError::Fixed("array_column(): Argument #1 ($array) must be of type array".into())));
    }
    if hash_result {
        errors.push((1, ColumnError::Fixed("Cannot access offset of type array on array".into())));
        errors.push((2, ColumnError::ClassName("Cannot access offset of type ", " on array")));
    }
    if column_may_fail {
        errors.push((3, ColumnError::Fixed(format!("{column_message}array given"))));
        errors.push((5, ColumnError::ClassName(column_message, " given")));
    }
    if index_may_fail {
        errors.push((4, ColumnError::Fixed(format!("{index_message}array given"))));
        errors.push((6, ColumnError::ClassName(index_message, " given")));
    }
    emit_error_dispatch(ctx, &errors);

    match packed_elem_ty {
        Some(result_elem_ty) => {
            super::normalize_indexed_array_result(ctx, "array_column", &PhpType::Mixed, &result_elem_ty)?;
            super::box_array_result_for_mixed_builtin(ctx, inst, &result_elem_ty);
        }
        None => super::box_hash_result_for_mixed_builtin(ctx, inst, &PhpType::Mixed),
    }
    store_if_result(ctx, inst)
}

/// Returns true when a key operand's static type can hold an array or object at runtime.
fn may_hold_container(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
}

/// The TypeError raised for one walker error code.
enum ColumnError {
    /// A message whose wording is fixed.
    Fixed(String),
    /// A message around the offending object's class name, which the walker returns in the
    /// string-result position (AArch64 x1/x2, x86_64 rsi/rdx).
    ClassName(&'static str, &'static str),
}

/// Converts the walker's small error codes into PHP TypeErrors; real results fall through.
fn emit_error_dispatch(ctx: &mut FunctionContext<'_>, errors: &[(i64, ColumnError)]) {
    if errors.is_empty() {
        return;
    }
    let ok = ctx.next_label("array_column_ok");
    let labels: Vec<String> = errors.iter().map(|_| ctx.next_label("array_column_error")).collect();
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #7");                              // heap results are never below the error-code range
            ctx.emitter.instruction(&format!("b.hs {ok}"));                     // a real array result skips the error dispatch
            for ((code, _), label) in errors.iter().zip(&labels) {
                ctx.emitter.instruction(&format!("cmp x0, #{code}"));           // does the walker report this error?
                ctx.emitter.instruction(&format!("b.eq {label}"));              // raise the matching TypeError
            }
            ctx.emitter.instruction(&format!("b {ok}"));                        // unreachable codes keep the returned value
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 7");                              // heap results are never below the error-code range
            ctx.emitter.instruction(&format!("jae {ok}"));                      // a real array result skips the error dispatch
            for ((code, _), label) in errors.iter().zip(&labels) {
                ctx.emitter.instruction(&format!("cmp rax, {code}"));           // does the walker report this error?
                ctx.emitter.instruction(&format!("je {label}"));                // raise the matching TypeError
            }
            ctx.emitter.instruction(&format!("jmp {ok}"));                      // unreachable codes keep the returned value
        }
    }
    for ((_, error), label) in errors.iter().zip(&labels) {
        ctx.emitter.label(label);
        match error {
            ColumnError::Fixed(message) => crate::codegen::lower_inst::exceptions::emit_type_error(ctx, message),
            ColumnError::ClassName(prefix, suffix) => {
                if ctx.emitter.target.arch == Arch::X86_64 {
                    ctx.emitter.instruction("mov rax, rsi");                    // class-name pointer into the string-result register
                }
                runtime_class_messages::emit_concat_static_prefix(ctx, prefix);
                runtime_class_messages::emit_concat_static_suffix(ctx, suffix);
                abi::emit_call_label(ctx.emitter, "__rt_str_persist");
                crate::codegen::lower_inst::exceptions::emit_type_error_from_string_result(ctx);
            }
        }
    }
    ctx.emitter.label(&ok);
}

/// Writes one key operand into the key block as a boxed-Mixed-style `(tag, lo, hi)` triple.
///
/// An absent operand (or a static `null`) is tag 8. Strings stay borrowed: the caller keeps
/// them alive across the runtime call. Boxed keys are unboxed so the runtime sees the payload.
fn store_key_triple(ctx: &mut FunctionContext<'_>, key: Option<ValueId>, offset: usize) -> Result<()> {
    let arm = ctx.emitter.target.arch == Arch::AArch64;
    let (tag, lo, hi) = if arm { ("x0", "x1", "x2") } else { ("rax", "rdi", "rdx") };
    let ty = match key {
        Some(key) => ctx.value_php_type(key)?,
        None => PhpType::Void,
    };
    match (key, ty.codegen_repr()) {
        (None, _) | (_, PhpType::Void) | (_, PhpType::Never) => {
            abi::emit_load_int_immediate(ctx.emitter, tag, NULL_TAG);
            abi::emit_load_int_immediate(ctx.emitter, lo, 0);
            abi::emit_load_int_immediate(ctx.emitter, hi, 0);
        }
        (Some(key), PhpType::Str) => {
            ctx.load_string_value_to_regs(key, lo, hi)?;
            abi::emit_load_int_immediate(ctx.emitter, tag, 1);
        }
        (Some(key), PhpType::Int) | (Some(key), PhpType::Bool) => {
            ctx.load_value_to_reg(key, lo)?;
            let runtime_tag = if ty.codegen_repr() == PhpType::Bool { 3 } else { 0 };
            abi::emit_load_int_immediate(ctx.emitter, tag, runtime_tag);
            abi::emit_load_int_immediate(ctx.emitter, hi, 0);
        }
        (Some(key), PhpType::Float) => {
            let float = abi::float_result_reg(ctx.emitter);
            ctx.load_value_to_reg(key, float)?;
            abi::emit_reg_move(ctx.emitter, lo, float);
            abi::emit_load_int_immediate(ctx.emitter, tag, 2);
            abi::emit_load_int_immediate(ctx.emitter, hi, 0);
        }
        (Some(key), PhpType::TaggedScalar) => {
            ctx.load_value_to_result(key)?;
            if arm {
                ctx.emitter.instruction("mov x9, x1");                          // preserve the inline runtime tag
                ctx.emitter.instruction("mov x1, x0");                          // the scalar payload becomes the low word
                ctx.emitter.instruction("mov x0, x9");                          // the inline tag becomes the triple tag
                ctx.emitter.instruction("mov x2, #0");                          // tagged scalars carry no high word
            } else {
                ctx.emitter.instruction("mov rdi, rax");                        // the scalar payload becomes the low word
                ctx.emitter.instruction("mov rax, rdx");                        // the inline tag becomes the triple tag
                ctx.emitter.instruction("xor edx, edx");                        // tagged scalars carry no high word
            }
        }
        (Some(key), PhpType::Mixed) | (Some(key), PhpType::Union(_)) => {
            ctx.load_value_to_reg(key, abi::int_result_reg(ctx.emitter))?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        }
        (Some(_), other) => {
            return Err(CodegenIrError::unsupported(format!(
                "array_column key PHP type {:?}",
                other
            )))
        }
    }
    abi::emit_store_to_sp(ctx.emitter, tag, offset);
    abi::emit_store_to_sp(ctx.emitter, lo, offset + 8);
    abi::emit_store_to_sp(ctx.emitter, hi, offset + 16);
    Ok(())
}

/// Returns the element type required by the lowered EIR result slot.
fn array_column_result_element_type(inst: &Instruction, value_ty: &PhpType) -> Result<PhpType> {
    match inst.result_php_type.codegen_repr() {
        PhpType::Array(elem) => {
            let result_elem_ty = elem.codegen_repr();
            if &result_elem_ty == value_ty || result_elem_ty == PhpType::Mixed {
                Ok(result_elem_ty)
            } else {
                Err(CodegenIrError::unsupported(format!(
                    "array_column result element PHP type {:?} for source value PHP type {:?}",
                    result_elem_ty,
                    value_ty
                )))
            }
        }
        PhpType::Mixed | PhpType::Union(_) => Ok(value_ty.clone()),
        other => Err(CodegenIrError::unsupported(format!(
            "array_column result PHP type {:?}",
            other
        ))),
    }
}

/// Returns the runtime helper that matches the extracted row value representation.
fn array_column_runtime_helper(value_ty: &PhpType) -> &'static str {
    if value_ty == &PhpType::Str {
        "__rt_array_column_str"
    } else if matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        "__rt_array_column_mixed"
    } else if value_ty.is_refcounted() {
        "__rt_array_column_ref"
    } else {
        "__rt_array_column"
    }
}
