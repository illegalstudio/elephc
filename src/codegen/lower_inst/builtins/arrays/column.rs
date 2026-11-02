//! Purpose:
//! Lowers PHP `array_column()` builtin calls for the EIR backend.
//! Materializes indexed or associative collections of associative rows, or a boxed array, plus a
//! string column key into the existing target-aware runtime extraction helpers.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays::lower_array_column()`.
//!
//! Key details:
//! - Result array metadata is normalized after runtime extraction so empty
//!   results still carry the correct indexed-array value type.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::context::FunctionContext;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::super::{expect_operand, store_if_result};

/// Lowers `array_column()` by dispatching to the helper matching row value ownership.
pub(super) fn lower_array_column(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_column", 2)?;
    let array = expect_operand(inst, 0)?;
    let key = expect_operand(inst, 1)?;
    let source_ty = ctx.value_php_type(array)?.codegen_repr();
    let boxed = source_ty == PhpType::Mixed;
    let value_ty = if boxed {
        PhpType::Mixed
    } else {
        array_column_source_value_type(source_ty.clone())?
    };
    require_array_column_key_type(ctx.value_php_type(key)?)?;
    let result_elem_ty = array_column_result_element_type(inst, &value_ty)?;
    if boxed {
        // A boxed source is handed to the runtime as the Mixed cell it is; the helper walks any
        // array shape the cell holds and reports a non-array through a zero result.
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.load_value_to_reg(array, "x0")?;
                ctx.load_string_value_to_regs(key, "x1", "x2")?;
            }
            Arch::X86_64 => {
                ctx.load_value_to_reg(array, "rdi")?;
                ctx.load_string_value_to_regs(key, "rsi", "rdx")?;
            }
        }
        abi::emit_call_label(ctx.emitter, "__rt_array_column_boxed");
        let valid = ctx.next_label("array_column_boxed_valid");
        abi::emit_branch_if_int_result_nonzero(ctx.emitter, &valid);
        crate::codegen::lower_inst::exceptions::emit_type_error(
            ctx, "array_column(): Argument #1 ($array) must be of type array",
        );
        ctx.emitter.label(&valid);
    } else {
        lower_array_column_call(ctx, array, key, &source_ty, &value_ty)?;
    }
    super::normalize_indexed_array_result(ctx, "array_column", &value_ty, &result_elem_ty)?;
    super::box_array_result_for_mixed_builtin(ctx, inst, &result_elem_ty);
    store_if_result(ctx, inst)
}

/// Returns the row value type extracted from an indexed or associative collection of rows.
fn array_column_source_value_type(ty: PhpType) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(inner) => match inner.codegen_repr() {
            PhpType::AssocArray { value, .. } => Ok(value.codegen_repr()),
            PhpType::Mixed | PhpType::Union(_) => Ok(PhpType::Mixed),
            other => Err(CodegenIrError::unsupported(format!(
                "array_column row PHP type {:?}",
                other
            ))),
        },
        PhpType::Mixed | PhpType::Union(_) => Ok(PhpType::Mixed),
        PhpType::AssocArray { value, .. } => match value.codegen_repr() {
            PhpType::AssocArray { value, .. } => Ok(value.codegen_repr()),
            PhpType::Mixed | PhpType::Union(_) => Ok(PhpType::Mixed),
            other => Err(CodegenIrError::unsupported(format!(
                "array_column row PHP type {:?}",
                other
            ))),
        },
        other => Err(CodegenIrError::unsupported(format!(
            "array_column for PHP type {:?}",
            other
        ))),
    }
}

/// Verifies that the column key can be passed through the runtime string-key ABI.
fn require_array_column_key_type(ty: PhpType) -> Result<()> {
    match ty.codegen_repr() {
        PhpType::Str => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "array_column key PHP type {:?}",
            other
        ))),
    }
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

/// Materializes `array_column()` arguments and calls the selected runtime helper.
fn lower_array_column_call(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    key: ValueId,
    source_ty: &PhpType,
    value_ty: &PhpType,
) -> Result<()> {
    let owned_source = materialize_array_column_source(ctx, array, source_ty)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    if owned_source {
        abi::emit_push_reg(ctx.emitter, result_reg);
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_string_value_to_regs(key, "x1", "x2")?;
        }
        Arch::X86_64 => {
            ctx.load_string_value_to_regs(key, "rsi", "rdx")?;
        }
    }
    abi::emit_call_label(ctx.emitter, array_column_runtime_helper(value_ty));
    if owned_source {
        abi::emit_push_reg(ctx.emitter, result_reg);
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("ldr x0, [sp, #16]");                 // recover the owned indexed values array
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");     // recover the owned indexed values array
            }
        }
        abi::emit_call_label(ctx.emitter, "__rt_decref_array");
        abi::emit_pop_reg(ctx.emitter, result_reg);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
    }
    Ok(())
}

/// Materializes an indexed, associative, or gradual array source in the first runtime argument register.
///
/// Associative outer collections become an owned indexed values array because the column runtime
/// helpers traverse indexed row storage. The caller releases that temporary after extraction.
fn materialize_array_column_source(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    source_ty: &PhpType,
) -> Result<bool> {
    if matches!(source_ty, PhpType::Array(_)) {
        let array_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
        ctx.load_value_to_reg(array, array_reg)?;
        return Ok(false);
    }

    if let PhpType::AssocArray { value, .. } = source_ty {
        ctx.load_value_to_result(array)?;
        super::values::emit_loaded_assoc_array_values(ctx, &value.codegen_repr())?;
        return Ok(true);
    }

    if !matches!(source_ty, PhpType::Mixed | PhpType::Union(_)) {
        return Err(CodegenIrError::unsupported(format!(
            "array_column for PHP type {:?}",
            source_ty
        )));
    }

    ctx.load_value_to_result(array)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let accepted = ctx.next_label("array_column_mixed_array");
    let wrong = ctx.next_label("array_column_mixed_wrong_type");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #4");                              // runtime tag 4 denotes indexed array storage
            ctx.emitter.instruction(&format!("b.eq {}", accepted));             // pass gradual indexed arrays to the ordinary column helper
            ctx.emitter.instruction(&format!("b {}", wrong));                   // every other runtime type violates the array boundary
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 4");                              // runtime tag 4 denotes indexed array storage
            ctx.emitter.instruction(&format!("je {}", accepted));               // pass gradual indexed arrays to the ordinary column helper
            ctx.emitter.instruction(&format!("jmp {}", wrong));                 // every other runtime type violates the array boundary
        }
    }
    super::union_type_guard::emit_mixed_wrong_tag_type_error_dispatch(
        ctx,
        &wrong,
        &|given| {
            format!(
                "array_column(): Argument #1 ($array) must be of type array, {} given",
                given
            )
        },
    );
    ctx.emitter.label(&accepted);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // pass the unboxed indexed-array payload to the runtime helper
        }
        Arch::X86_64 => {}
    }
    Ok(false)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies gradual array sources select the boxed-Mixed row representation.
    #[test]
    fn array_column_source_accepts_gradual_type() {
        let value_ty = array_column_source_value_type(PhpType::Mixed)
            .expect("a gradual source should reach the runtime array guard");
        assert_eq!(value_ty, PhpType::Mixed);
    }

    /// Verifies associative outer collections retain the nested row value representation.
    #[test]
    fn array_column_source_accepts_associative_outer_collection() {
        let value_ty = array_column_source_value_type(PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Str),
            }),
        })
        .expect("an associative collection of associative rows should be supported");
        assert_eq!(value_ty, PhpType::Str);
    }
}
