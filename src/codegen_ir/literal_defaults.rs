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
//!   scalar, string, null, indexed-array literals with scalar/string/null
//!   elements, and associative-array literals (empty, positional, or with
//!   constant integer/string keys and scalar/string/null values) land here.

use crate::codegen::platform::Arch;
use crate::codegen::{
    abi, emit_box_current_value_as_mixed, emit_release_pushed_refcounted_temp_after_array_push,
    runtime_value_tag,
};
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

use super::context::FunctionContext;
use super::{CodegenIrError, Result};

/// Literal default value that the EIR backend can write directly.
#[derive(Clone)]
pub(crate) enum LiteralDefaultValue {
    Int(i64),
    Bool(bool),
    Float(f64),
    Str(String),
    Null,
    NullSentinel,
    TaggedNull,
    BoxedNull,
    BoxedInt(i64),
    BoxedBool(bool),
    BoxedFloat(f64),
    BoxedStr(String),
    Array {
        elem_type: PhpType,
        elements: Vec<LiteralArrayElement>,
    },
    AssocArray {
        value_type: PhpType,
        entries: Vec<LiteralAssocEntry>,
    },
    EmptyAssocArray {
        value_type: PhpType,
    },
}

/// Literal indexed-array element that can be materialized without evaluating code.
#[derive(Clone)]
pub(crate) enum LiteralArrayElement {
    Int(i64),
    Bool(bool),
    Float(f64),
    Str(String),
    Null,
}

/// Literal associative-array key that can be materialized without evaluating code. Positional
/// literals stored into hash slots use integer keys; `ArrayLiteralAssoc` keys may be integer or
/// string. String keys are normalized at emit time so PHP numeric-string keys become integer keys.
#[derive(Clone)]
pub(crate) enum LiteralArrayKey {
    Int(i64),
    Str(String),
}

/// One literal associative-array entry: a constant key paired with a constant value, ready to be
/// inserted into a freshly allocated hash without evaluating any runtime expression.
#[derive(Clone)]
pub(crate) struct LiteralAssocEntry {
    key: LiteralArrayKey,
    value: LiteralArrayElement,
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
        (PhpType::Mixed | PhpType::Union(_), ExprKind::StringLiteral(value)) => {
            Ok(LiteralDefaultValue::BoxedStr(value.clone()))
        }
        // A scalar literal default for a `mixed`/union slot is boxed into a tagged Mixed cell,
        // mirroring the string and null cases above.
        (PhpType::Mixed | PhpType::Union(_), ExprKind::IntLiteral(value)) => {
            Ok(LiteralDefaultValue::BoxedInt(*value))
        }
        (PhpType::Mixed | PhpType::Union(_), ExprKind::BoolLiteral(value)) => {
            Ok(LiteralDefaultValue::BoxedBool(*value))
        }
        (PhpType::Mixed | PhpType::Union(_), ExprKind::FloatLiteral(value)) => {
            Ok(LiteralDefaultValue::BoxedFloat(*value))
        }
        (PhpType::Mixed | PhpType::Union(_), ExprKind::Negate(inner)) => match &inner.kind {
            ExprKind::IntLiteral(value) => value
                .checked_neg()
                .map(LiteralDefaultValue::BoxedInt)
                .ok_or_else(|| unsupported_literal_default(context, php_type, op_name)),
            ExprKind::FloatLiteral(value) => Ok(LiteralDefaultValue::BoxedFloat(-value)),
            _ => Err(unsupported_literal_default(context, php_type, op_name)),
        },
        (php_type, ExprKind::Null) if php_type.codegen_repr() == PhpType::TaggedScalar => {
            Ok(LiteralDefaultValue::TaggedNull)
        }
        (PhpType::Mixed | PhpType::Union(_), ExprKind::Null) => Ok(LiteralDefaultValue::BoxedNull),
        (PhpType::Void | PhpType::Never, ExprKind::Null) => Ok(LiteralDefaultValue::NullSentinel),
        (PhpType::Void | PhpType::Never, _) => Ok(LiteralDefaultValue::NullSentinel),
        (PhpType::Object(_), ExprKind::Null) => Ok(LiteralDefaultValue::Null),
        (php_type, ExprKind::Null) if php_type.codegen_repr().is_refcounted() => {
            Ok(LiteralDefaultValue::Null)
        }
        (PhpType::AssocArray { value, .. }, ExprKind::ArrayLiteral(items)) => {
            let value_type = value.as_ref().codegen_repr();
            if items.is_empty() {
                return Ok(LiteralDefaultValue::EmptyAssocArray { value_type });
            }
            // A positional literal stored into hash storage takes PHP's implicit 0,1,2,... keys.
            let entries = items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    Ok(LiteralAssocEntry {
                        key: LiteralArrayKey::Int(index as i64),
                        value: literal_array_element(context, &value_type, &item.kind, op_name)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(LiteralDefaultValue::AssocArray {
                value_type,
                entries,
            })
        }
        (PhpType::AssocArray { value, .. }, ExprKind::ArrayLiteralAssoc(items)) => {
            let value_type = value.as_ref().codegen_repr();
            let entries = items
                .iter()
                .map(|(key, value_expr)| {
                    Ok(LiteralAssocEntry {
                        key: literal_array_key(context, &key.kind, op_name)?,
                        value: literal_array_element(
                            context,
                            &value_type,
                            &value_expr.kind,
                            op_name,
                        )?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(LiteralDefaultValue::AssocArray {
                value_type,
                entries,
            })
        }
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

/// Emits a string literal default into the canonical string result registers.
pub(crate) fn emit_string_literal_default_to_result(
    ctx: &mut FunctionContext<'_>,
    value: &str,
) {
    let (label, len) = ctx.data.add_string(value.as_bytes());
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
}

/// Emits a string literal default boxed as a Mixed value.
pub(crate) fn emit_boxed_string_literal_default_to_result(
    ctx: &mut FunctionContext<'_>,
    value: &str,
) {
    emit_string_literal_default_to_result(ctx, value);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
}

/// Emits a boxed integer literal default into the canonical result register: loads the immediate
/// into the integer result register, then boxes it as a `Mixed` cell (runtime tag 0).
pub(crate) fn emit_boxed_int_literal_to_result(ctx: &mut FunctionContext<'_>, value: i64) {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), value);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
}

/// Emits a boxed boolean literal default into the canonical result register: materializes `0`/`1`
/// in the integer result register, then boxes it as a `Mixed` cell (runtime tag 3).
pub(crate) fn emit_boxed_bool_literal_to_result(ctx: &mut FunctionContext<'_>, value: bool) {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        i64::from(value),
    );
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
}

/// Emits a boxed float literal default into the canonical result register: loads the literal from
/// the `.rodata` float pool into the float result register, then boxes it as a `Mixed` cell (tag 2).
pub(crate) fn emit_boxed_float_literal_to_result(ctx: &mut FunctionContext<'_>, value: f64) {
    let label = ctx.data.add_float(value);
    let scratch = abi::symbol_scratch_reg(ctx.emitter);
    let float_reg = abi::float_result_reg(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, scratch, &label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("ldr {}, [{}]", float_reg, scratch)); // load the boxed float literal default through the symbol scratch register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("movsd {}, QWORD PTR [{}]", float_reg, scratch)); // load the boxed float literal default through the symbol scratch register
        }
    }
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Float);
}

/// Emits an empty associative-array literal default into the canonical result register.
pub(crate) fn emit_empty_assoc_array_literal_to_result(
    ctx: &mut FunctionContext<'_>,
    value_type: &PhpType,
) {
    let capacity_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let value_tag_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_int_immediate(ctx.emitter, capacity_reg, 16);
    abi::emit_load_int_immediate(
        ctx.emitter,
        value_tag_reg,
        crate::codegen::runtime_value_tag(&value_type.codegen_repr()) as i64,
    );
    abi::emit_call_label(ctx.emitter, "__rt_hash_new");
}

/// Emits a boxed PHP null default into the canonical result register.
pub(crate) fn emit_boxed_null_literal_to_result(ctx: &mut FunctionContext<'_>) {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        0x7fff_ffff_ffff_fffe,
    );
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
}

/// Emits PHP null as an inline tagged scalar literal default.
pub(crate) fn emit_tagged_null_literal_to_result(ctx: &mut FunctionContext<'_>) {
    crate::codegen::sentinels::emit_tagged_scalar_null(ctx.emitter);
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

/// Emits an associative-array literal default into the canonical result register. Allocates an
/// empty hash, then inserts each constant key/value entry through `__rt_hash_set`. The hash pointer
/// flows through the result register across insertions because the runtime may reallocate it on
/// growth. For each entry the normalized key is staged on the temporary stack while the value is
/// materialized, so value-staging calls (string persistence, key normalization) cannot clobber it.
pub(crate) fn emit_assoc_array_literal_default_to_result(
    ctx: &mut FunctionContext<'_>,
    value_type: &PhpType,
    entries: &[LiteralAssocEntry],
) -> Result<()> {
    emit_empty_assoc_array_literal_to_result(ctx, value_type);
    for entry in entries {
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                materialize_assoc_literal_key_aarch64(ctx, &entry.key);
                abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
                let actual_value_type = emit_array_element_value(ctx, &entry.value);
                materialize_assoc_literal_value(ctx, value_type, &actual_value_type)?;
                abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
                abi::emit_pop_reg(ctx.emitter, "x0");
                abi::emit_load_int_immediate(
                    ctx.emitter,
                    "x5",
                    assoc_literal_value_tag(value_type, &actual_value_type),
                );
            }
            Arch::X86_64 => {
                materialize_assoc_literal_key_x86_64(ctx, &entry.key);
                abi::emit_push_reg_pair(ctx.emitter, "rsi", "rdx");
                let actual_value_type = emit_array_element_value(ctx, &entry.value);
                materialize_assoc_literal_value(ctx, value_type, &actual_value_type)?;
                abi::emit_pop_reg_pair(ctx.emitter, "rsi", "rdx");
                abi::emit_pop_reg(ctx.emitter, "rdi");
                abi::emit_load_int_immediate(
                    ctx.emitter,
                    "r9",
                    assoc_literal_value_tag(value_type, &actual_value_type),
                );
            }
        }
        abi::emit_call_label(ctx.emitter, "__rt_hash_set");
    }
    Ok(())
}

/// Materializes a constant associative-array key into the AArch64 hash key registers `x1`/`x2`.
/// Integer keys load directly with the integer-key sentinel in `x2`; string keys load the literal
/// pointer/length and run `__rt_hash_normalize_key`, so PHP numeric-string keys collapse to integer
/// keys while genuine string keys keep `x1`/`x2` as pointer/length.
fn materialize_assoc_literal_key_aarch64(ctx: &mut FunctionContext<'_>, key: &LiteralArrayKey) {
    match key {
        LiteralArrayKey::Int(value) => {
            abi::emit_load_int_immediate(ctx.emitter, "x1", *value);
            abi::emit_load_int_immediate(ctx.emitter, "x2", -1);
        }
        LiteralArrayKey::Str(value) => {
            let (label, len) = ctx.data.add_string(value.as_bytes());
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_normalize_key");
        }
    }
}

/// Materializes a constant associative-array key into the x86_64 hash key registers `rsi`/`rdx`.
/// Integer keys load directly with the integer-key sentinel in `rdx`; string keys load the literal
/// pointer/length into `rax`/`rdx` (the `__rt_hash_normalize_key` input registers), normalize
/// (result in `rax`/`rdx`), and move the normalized low word into `rsi` for the `__rt_hash_set` ABI.
fn materialize_assoc_literal_key_x86_64(ctx: &mut FunctionContext<'_>, key: &LiteralArrayKey) {
    match key {
        LiteralArrayKey::Int(value) => {
            abi::emit_load_int_immediate(ctx.emitter, "rsi", *value);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", -1);
        }
        LiteralArrayKey::Str(value) => {
            let (label, len) = ctx.data.add_string(value.as_bytes());
            abi::emit_symbol_address(ctx.emitter, "rax", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_normalize_key");
            ctx.emitter.instruction("mov rsi, rax");                            // move the normalized key low word into the hash ABI key register
        }
    }
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

/// Converts a constant associative-array key expression into a materializable hash key, applying
/// PHP's literal key coercions: booleans and floats become integer keys, `null` becomes the empty
/// string key, and numeric strings are normalized to integer keys later at emit time. Unsupported
/// key forms fall through to the shared unsupported-default error rather than miscompiling.
fn literal_array_key(context: &str, expr: &ExprKind, op_name: &str) -> Result<LiteralArrayKey> {
    match expr {
        ExprKind::StringLiteral(value) => Ok(LiteralArrayKey::Str(value.clone())),
        ExprKind::IntLiteral(value) => Ok(LiteralArrayKey::Int(*value)),
        ExprKind::BoolLiteral(value) => Ok(LiteralArrayKey::Int(i64::from(*value))),
        ExprKind::Null => Ok(LiteralArrayKey::Str(String::new())),
        ExprKind::FloatLiteral(value) => Ok(LiteralArrayKey::Int(*value as i64)),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => value
                .checked_neg()
                .map(LiteralArrayKey::Int)
                .ok_or_else(|| unsupported_literal_default(context, &PhpType::Int, op_name)),
            ExprKind::FloatLiteral(value) => Ok(LiteralArrayKey::Int((-value) as i64)),
            _ => Err(unsupported_literal_default(context, &PhpType::Int, op_name)),
        },
        _ => Err(unsupported_literal_default(context, &PhpType::Int, op_name)),
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

/// Materializes the current literal value as the payload consumed by `__rt_hash_set`.
fn materialize_assoc_literal_value(
    ctx: &mut FunctionContext<'_>,
    storage_value_type: &PhpType,
    actual_value_type: &PhpType,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => materialize_assoc_literal_value_aarch64(
            ctx,
            storage_value_type,
            actual_value_type,
        ),
        Arch::X86_64 => materialize_assoc_literal_value_x86_64(
            ctx,
            storage_value_type,
            actual_value_type,
        ),
    }
}

/// Materializes the current literal value for AArch64 hash insertion.
fn materialize_assoc_literal_value_aarch64(
    ctx: &mut FunctionContext<'_>,
    storage_value_type: &PhpType,
    actual_value_type: &PhpType,
) -> Result<()> {
    if matches!(storage_value_type.codegen_repr(), PhpType::Mixed | PhpType::Iterable) {
        return materialize_assoc_literal_concrete_value_aarch64(ctx, actual_value_type);
    }
    match actual_value_type.codegen_repr() {
        PhpType::Int | PhpType::Bool | PhpType::Float => {
            if matches!(actual_value_type.codegen_repr(), PhpType::Float) {
                ctx.emitter.instruction("fmov x3, d0");                         // pass the literal float bits as the hash value low word
            } else {
                ctx.emitter.instruction("mov x3, x0");                          // pass the literal scalar payload as the hash value low word
            }
            ctx.emitter.instruction("mov x4, xzr");                             // scalar hash values do not use the high payload word
            Ok(())
        }
        PhpType::Str => {
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            ctx.emitter.instruction("mov x3, x1");                              // pass the persistent literal string pointer as the hash value low word
            ctx.emitter.instruction("mov x4, x2");                              // pass the literal string length as the hash value high word
            Ok(())
        }
        PhpType::Void | PhpType::Never => {
            ctx.emitter.instruction("mov x3, xzr");                             // null hash values use a zero low payload word
            ctx.emitter.instruction("mov x4, xzr");                             // null hash values use a zero high payload word
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "assoc array default element PHP type {:?}",
            other
        ))),
    }
}

/// Materializes the current literal value for x86_64 hash insertion.
fn materialize_assoc_literal_value_x86_64(
    ctx: &mut FunctionContext<'_>,
    storage_value_type: &PhpType,
    actual_value_type: &PhpType,
) -> Result<()> {
    if matches!(storage_value_type.codegen_repr(), PhpType::Mixed | PhpType::Iterable) {
        return materialize_assoc_literal_concrete_value_x86_64(ctx, actual_value_type);
    }
    match actual_value_type.codegen_repr() {
        PhpType::Int | PhpType::Bool => {
            ctx.emitter.instruction("mov rcx, rax");                            // pass the literal scalar payload as the hash value low word
            ctx.emitter.instruction("xor r8, r8");                              // scalar hash values do not use the high payload word
            Ok(())
        }
        PhpType::Float => {
            ctx.emitter.instruction("movq rcx, xmm0");                          // pass the literal float bits as the hash value low word
            ctx.emitter.instruction("xor r8, r8");                              // scalar hash values do not use the high payload word
            Ok(())
        }
        PhpType::Str => {
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            ctx.emitter.instruction("mov rcx, rax");                            // pass the persistent literal string pointer as the hash value low word
            ctx.emitter.instruction("mov r8, rdx");                             // pass the literal string length as the hash value high word
            Ok(())
        }
        PhpType::Void | PhpType::Never => {
            ctx.emitter.instruction("xor rcx, rcx");                            // null hash values use a zero low payload word
            ctx.emitter.instruction("xor r8, r8");                              // null hash values use a zero high payload word
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "assoc array default element PHP type {:?}",
            other
        ))),
    }
}

/// Materializes the current concrete literal value for Mixed-capable AArch64 hash storage.
fn materialize_assoc_literal_concrete_value_aarch64(
    ctx: &mut FunctionContext<'_>,
    actual_value_type: &PhpType,
) -> Result<()> {
    match actual_value_type.codegen_repr() {
        PhpType::Void | PhpType::Never => {
            ctx.emitter.instruction("mov x3, xzr");                             // null Mixed hash values use a zero low payload word
            ctx.emitter.instruction("mov x4, xzr");                             // null Mixed hash values use a zero high payload word
            Ok(())
        }
        PhpType::Int | PhpType::Bool => {
            ctx.emitter.instruction("mov x3, x0");                              // pass the concrete scalar payload as the Mixed hash value low word
            ctx.emitter.instruction("mov x4, xzr");                             // concrete scalar Mixed values do not use the high payload word
            Ok(())
        }
        PhpType::Float => {
            ctx.emitter.instruction("fmov x3, d0");                             // pass the concrete float bits as the Mixed hash value low word
            ctx.emitter.instruction("mov x4, xzr");                             // concrete float Mixed values do not use the high payload word
            Ok(())
        }
        PhpType::Str => {
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            ctx.emitter.instruction("mov x3, x1");                              // pass the persistent string pointer as the Mixed hash value low word
            ctx.emitter.instruction("mov x4, x2");                              // pass the string length as the Mixed hash value high word
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "assoc array default Mixed element PHP type {:?}",
            other
        ))),
    }
}

/// Materializes the current concrete literal value for Mixed-capable x86_64 hash storage.
fn materialize_assoc_literal_concrete_value_x86_64(
    ctx: &mut FunctionContext<'_>,
    actual_value_type: &PhpType,
) -> Result<()> {
    match actual_value_type.codegen_repr() {
        PhpType::Void | PhpType::Never => {
            ctx.emitter.instruction("xor rcx, rcx");                            // null Mixed hash values use a zero low payload word
            ctx.emitter.instruction("xor r8, r8");                              // null Mixed hash values use a zero high payload word
            Ok(())
        }
        PhpType::Int | PhpType::Bool => {
            ctx.emitter.instruction("mov rcx, rax");                            // pass the concrete scalar payload as the Mixed hash value low word
            ctx.emitter.instruction("xor r8, r8");                              // concrete scalar Mixed values do not use the high payload word
            Ok(())
        }
        PhpType::Float => {
            ctx.emitter.instruction("movq rcx, xmm0");                          // pass the concrete float bits as the Mixed hash value low word
            ctx.emitter.instruction("xor r8, r8");                              // concrete float Mixed values do not use the high payload word
            Ok(())
        }
        PhpType::Str => {
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            ctx.emitter.instruction("mov rcx, rax");                            // pass the persistent string pointer as the Mixed hash value low word
            ctx.emitter.instruction("mov r8, rdx");                             // pass the string length as the Mixed hash value high word
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "assoc array default Mixed element PHP type {:?}",
            other
        ))),
    }
}

/// Returns the runtime value tag to pass for one literal hash insertion.
fn assoc_literal_value_tag(storage_value_type: &PhpType, actual_value_type: &PhpType) -> i64 {
    if matches!(storage_value_type.codegen_repr(), PhpType::Mixed | PhpType::Iterable) {
        runtime_value_tag(&actual_value_type.codegen_repr()) as i64
    } else {
        runtime_value_tag(&storage_value_type.codegen_repr()) as i64
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
