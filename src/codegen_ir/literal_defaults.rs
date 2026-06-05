//! Purpose:
//! Converts supported literal property defaults into backend-native values.
//! Keeps EIR object and static-property initialization on the same subset.
//!
//! Called from:
//! - `crate::codegen_ir::block_emit` static-property initialization.
//! - `crate::codegen_ir::lower_inst::objects` object allocation.
//!
//! Key details:
//! - This is intentionally narrower than full PHP expression lowering: only
//!   scalar, string, null, and indexed-array literals with scalar/string/null
//!   elements land here.

use crate::codegen::platform::Arch;
use crate::codegen::{
    abi, emit_box_current_value_as_mixed, emit_release_pushed_refcounted_temp_after_array_push,
};
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

use super::context::FunctionContext;
use super::{CodegenIrError, Result};

/// Literal default value that the EIR backend can write directly.
pub(crate) enum LiteralDefaultValue {
    Int(i64),
    Bool(bool),
    Float(f64),
    Str(String),
    Array {
        elem_type: PhpType,
        elements: Vec<LiteralArrayElement>,
    },
}

/// Literal indexed-array element that can be materialized without evaluating code.
pub(crate) enum LiteralArrayElement {
    Int(i64),
    Bool(bool),
    Float(f64),
    Str(String),
    Null,
}

/// Converts a supported default expression into a direct storage value.
pub(crate) fn literal_default_value(
    context: &str,
    php_type: &PhpType,
    expr: &ExprKind,
    op_name: &str,
) -> Result<LiteralDefaultValue> {
    match (php_type, expr) {
        (PhpType::Int, ExprKind::IntLiteral(value)) => Ok(LiteralDefaultValue::Int(*value)),
        (PhpType::Int, ExprKind::Negate(inner)) => match &inner.kind {
            ExprKind::IntLiteral(value) => value
                .checked_neg()
                .map(LiteralDefaultValue::Int)
                .ok_or_else(|| unsupported_literal_default(context, php_type, op_name)),
            _ => Err(unsupported_literal_default(context, php_type, op_name)),
        },
        (PhpType::Bool, ExprKind::BoolLiteral(value)) => Ok(LiteralDefaultValue::Bool(*value)),
        (PhpType::Float, ExprKind::FloatLiteral(value)) => Ok(LiteralDefaultValue::Float(*value)),
        (PhpType::Float, ExprKind::IntLiteral(value)) => Ok(LiteralDefaultValue::Float(*value as f64)),
        (PhpType::Float, ExprKind::Negate(inner)) => match &inner.kind {
            ExprKind::FloatLiteral(value) => Ok(LiteralDefaultValue::Float(-value)),
            ExprKind::IntLiteral(value) => Ok(LiteralDefaultValue::Float(-(*value as f64))),
            _ => Err(unsupported_literal_default(context, php_type, op_name)),
        },
        (PhpType::Str, ExprKind::StringLiteral(value)) => Ok(LiteralDefaultValue::Str(value.clone())),
        (PhpType::Array(elem_type), ExprKind::ArrayLiteral(items)) => {
            let elem_type = elem_type.codegen_repr();
            let elements = items
                .iter()
                .map(|item| literal_array_element(context, &elem_type, &item.kind, op_name))
                .collect::<Result<Vec<_>>>()?;
            Ok(LiteralDefaultValue::Array {
                elem_type,
                elements,
            })
        }
        _ => Err(unsupported_literal_default(context, php_type, op_name)),
    }
}

/// Emits an indexed-array literal default into the canonical result register.
pub(super) fn emit_array_literal_default_to_result(
    ctx: &mut FunctionContext<'_>,
    elem_type: &PhpType,
    elements: &[LiteralArrayElement],
) -> Result<()> {
    emit_array_literal_allocation(ctx, elem_type, elements.len())?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    for element in elements {
        let value_type = emit_array_element_value(ctx, element);
        append_array_literal_element(ctx, elem_type, &value_type)?;
    }
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    Ok(())
}

/// Converts one supported literal expression into an indexed-array element payload.
fn literal_array_element(
    context: &str,
    elem_type: &PhpType,
    expr: &ExprKind,
    op_name: &str,
) -> Result<LiteralArrayElement> {
    match elem_type.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable => match expr {
            ExprKind::IntLiteral(value) => Ok(LiteralArrayElement::Int(*value)),
            ExprKind::BoolLiteral(value) => Ok(LiteralArrayElement::Bool(*value)),
            ExprKind::FloatLiteral(value) => Ok(LiteralArrayElement::Float(*value)),
            ExprKind::StringLiteral(value) => Ok(LiteralArrayElement::Str(value.clone())),
            ExprKind::Null => Ok(LiteralArrayElement::Null),
            ExprKind::Negate(inner) => match &inner.kind {
                ExprKind::IntLiteral(value) => value
                    .checked_neg()
                    .map(LiteralArrayElement::Int)
                    .ok_or_else(|| unsupported_literal_default(context, elem_type, op_name)),
                ExprKind::FloatLiteral(value) => Ok(LiteralArrayElement::Float(-value)),
                _ => Err(unsupported_literal_default(context, elem_type, op_name)),
            },
            _ => Err(unsupported_literal_default(context, elem_type, op_name)),
        },
        PhpType::Int => match expr {
            ExprKind::IntLiteral(value) => Ok(LiteralArrayElement::Int(*value)),
            ExprKind::Negate(inner) => match &inner.kind {
                ExprKind::IntLiteral(value) => value
                    .checked_neg()
                    .map(LiteralArrayElement::Int)
                    .ok_or_else(|| unsupported_literal_default(context, elem_type, op_name)),
                _ => Err(unsupported_literal_default(context, elem_type, op_name)),
            },
            _ => Err(unsupported_literal_default(context, elem_type, op_name)),
        },
        PhpType::Bool => match expr {
            ExprKind::BoolLiteral(value) => Ok(LiteralArrayElement::Bool(*value)),
            _ => Err(unsupported_literal_default(context, elem_type, op_name)),
        },
        PhpType::Float => match expr {
            ExprKind::FloatLiteral(value) => Ok(LiteralArrayElement::Float(*value)),
            ExprKind::IntLiteral(value) => Ok(LiteralArrayElement::Float(*value as f64)),
            ExprKind::Negate(inner) => match &inner.kind {
                ExprKind::FloatLiteral(value) => Ok(LiteralArrayElement::Float(-value)),
                ExprKind::IntLiteral(value) => Ok(LiteralArrayElement::Float(-(*value as f64))),
                _ => Err(unsupported_literal_default(context, elem_type, op_name)),
            },
            _ => Err(unsupported_literal_default(context, elem_type, op_name)),
        },
        PhpType::Str => match expr {
            ExprKind::StringLiteral(value) => Ok(LiteralArrayElement::Str(value.clone())),
            _ => Err(unsupported_literal_default(context, elem_type, op_name)),
        },
        _ => Err(unsupported_literal_default(context, elem_type, op_name)),
    }
}

/// Allocates an indexed array sized for a literal property/static-property default.
fn emit_array_literal_allocation(
    ctx: &mut FunctionContext<'_>,
    elem_type: &PhpType,
    element_count: usize,
) -> Result<()> {
    let capacity = element_count.max(4);
    let elem_size = array_element_size(elem_type)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x1", elem_size);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", elem_size);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    Ok(())
}

/// Emits one literal array element into the canonical result register(s).
fn emit_array_element_value(
    ctx: &mut FunctionContext<'_>,
    element: &LiteralArrayElement,
) -> PhpType {
    match element {
        LiteralArrayElement::Int(value) => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), *value);
            PhpType::Int
        }
        LiteralArrayElement::Bool(value) => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), i64::from(*value));
            PhpType::Bool
        }
        LiteralArrayElement::Float(value) => {
            let label = ctx.data.add_float(*value);
            abi::emit_load_symbol_to_reg(ctx.emitter, abi::float_result_reg(ctx.emitter), &label, 0);
            PhpType::Float
        }
        LiteralArrayElement::Str(value) => {
            let (label, len) = ctx.data.add_string(value.as_bytes());
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
            abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
            PhpType::Str
        }
        LiteralArrayElement::Null => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
            PhpType::Void
        }
    }
}

/// Appends the current literal element value to the array pointer saved on the stack.
fn append_array_literal_element(
    ctx: &mut FunctionContext<'_>,
    elem_type: &PhpType,
    value_type: &PhpType,
) -> Result<()> {
    match elem_type.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable => {
            emit_box_current_value_as_mixed(ctx.emitter, &value_type.codegen_repr());
            append_refcounted_array_literal_element(ctx, &PhpType::Mixed);
        }
        PhpType::Int | PhpType::Bool => append_scalar_array_literal_element(ctx),
        PhpType::Float => append_float_array_literal_element(ctx),
        PhpType::Str => append_string_array_literal_element(ctx),
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array default element PHP type {:?}",
                other
            )))
        }
    }
    Ok(())
}

/// Appends the current integer-like result to the array pointer saved on the stack.
fn append_scalar_array_literal_element(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // pass the scalar literal payload to the indexed-array append helper
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
            abi::emit_push_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // pass the scalar literal payload to the indexed-array append helper
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
            abi::emit_push_reg(ctx.emitter, "rax");
        }
    }
}

/// Appends the current floating-point result to the array pointer saved on the stack.
fn append_float_array_literal_element(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fmov x1, d0");                             // pass the float literal bits to the indexed-array append helper
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
            abi::emit_push_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("movq rsi, xmm0");                          // pass the float literal bits to the indexed-array append helper
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
            abi::emit_push_reg(ctx.emitter, "rax");
        }
    }
}

/// Appends the current string result to the array pointer saved on the stack.
fn append_string_array_literal_element(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
            abi::emit_push_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // pass the string literal pointer to the indexed-array append helper
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
            abi::emit_push_reg(ctx.emitter, "rax");
        }
    }
}

/// Appends the current refcounted result and releases the temporary owner after insertion.
fn append_refcounted_array_literal_element(ctx: &mut FunctionContext<'_>, value_type: &PhpType) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x9");
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("mov x1, x0");                              // pass the boxed literal payload to the refcounted append helper
            ctx.emitter.instruction("mov x0, x9");                              // pass the saved indexed-array pointer to the refcounted append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
            emit_release_pushed_refcounted_temp_after_array_push(ctx.emitter, value_type);
            abi::emit_push_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "r11");
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rsi, rax");                            // pass the boxed literal payload to the refcounted append helper
            ctx.emitter.instruction("mov rdi, r11");                            // pass the saved indexed-array pointer to the refcounted append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
            emit_release_pushed_refcounted_temp_after_array_push(ctx.emitter, value_type);
            abi::emit_push_reg(ctx.emitter, "rax");
        }
    }
}

/// Returns the runtime slot width for an indexed-array default.
fn array_element_size(elem_type: &PhpType) -> Result<i64> {
    match elem_type.codegen_repr() {
        PhpType::Str | PhpType::Never => Ok(16),
        PhpType::Int
        | PhpType::Bool
        | PhpType::Float
        | PhpType::Mixed
        | PhpType::Union(_)
        | PhpType::Iterable
        | PhpType::Void => Ok(8),
        other => Err(CodegenIrError::unsupported(format!(
            "array default element PHP type {:?}",
            other
        ))),
    }
}

/// Builds the unsupported-feature error for default forms outside this slice.
fn unsupported_literal_default(
    context: &str,
    php_type: &PhpType,
    op_name: &str,
) -> CodegenIrError {
    CodegenIrError::unsupported(format!(
        "{} for default value of {} with PHP type {:?}",
        op_name, context, php_type
    ))
}
