//! Purpose:
//! Converts shared constant-expression folds into ReflectionClass constant metadata.
//!
//! Called from:
//! - `super::names_constants::reflection_constant_value()` for expressions it does not fold.
//!
//! Key details:
//! - Projections, branch selectors, coalescing and logical operators reuse
//!   `crate::codegen::const_default_values`, so Reflection agrees with property defaults.
//! - Enum singletons read by a discarded operand stay ordered reads beside the result.
//! - Runtime-only values, such as object constructors and `PHP_BINARY`, still decline.

use super::*;
use crate::codegen::const_default_values::{
    self, ConstDefaultArrayKey, ConstDefaultContext, ConstDefaultValue,
};

/// Folds one constant expression through the shared resolver in its declaration scope.
pub(super) fn reflection_resolved_constant_value(
    ctx: &FunctionContext<'_>,
    current_class: &str,
    expr: &Expr,
) -> Option<ReflectionConstantValue> {
    let context = ConstDefaultContext {
        module: ctx.module,
        current_class: (!current_class.is_empty()).then_some(current_class),
    };
    reflection_constant_from_resolved(const_default_values::resolve_const_default(expr, &context)?)
}

/// Converts one resolved constant tree, normalizing array keys as PHP stores them.
fn reflection_constant_from_resolved(value: ConstDefaultValue) -> Option<ReflectionConstantValue> {
    Some(match value {
        ConstDefaultValue::Scalar { kind, payload } => match kind {
            const_default_values::CONST_DEFAULT_NULL => ReflectionConstantValue::Null,
            const_default_values::CONST_DEFAULT_BOOL => ReflectionConstantValue::Bool(payload != 0),
            const_default_values::CONST_DEFAULT_INT => ReflectionConstantValue::Int(payload),
            const_default_values::CONST_DEFAULT_FLOAT =>
                ReflectionConstantValue::Float(f64::from_bits(payload as u64)),
            const_default_values::CONST_DEFAULT_EMPTY_ARRAY => ReflectionConstantValue::Array(Vec::new()),
            _ => return None,
        },
        ConstDefaultValue::String(value) => ReflectionConstantValue::Str(value),
        ConstDefaultValue::EnumCase { enum_name, case_name } =>
            ReflectionConstantValue::EnumCase { enum_name, case_name },
        ConstDefaultValue::Evaluated { value, enum_cases } => ReflectionConstantValue::Evaluated {
            value: Box::new(reflection_constant_from_resolved(*value)?), enum_cases,
        },
        ConstDefaultValue::Array(elements) => reflection_constant_array(elements)?,
        ConstDefaultValue::Object { .. } => return None,
    })
}

/// Builds a list or keyed array with PHP's append cursor and last-write duplicate rule.
fn reflection_constant_array(
    elements: Vec<const_default_values::ConstDefaultArrayElement>,
) -> Option<ReflectionConstantValue> {
    let mut entries: Vec<ReflectionConstantAssocEntry> = Vec::with_capacity(elements.len());
    let mut next_int = 0i64;
    for element in elements {
        let key = match element.key {
            Some(ConstDefaultArrayKey::Int(key)) => ReflectionDefaultArrayKey::Int(key),
            Some(ConstDefaultArrayKey::String(key)) => ReflectionDefaultArrayKey::Str(key),
            None => ReflectionDefaultArrayKey::Int(next_int),
        };
        if let ReflectionDefaultArrayKey::Int(key) = key {
            next_int = next_int.max(key.checked_add(1)?);
        }
        let value = reflection_constant_from_resolved(element.default)?;
        if let Some(existing) = entries.iter_mut().find(|entry| same_key(&entry.key, &key)) {
            existing.value = value;
        } else {
            entries.push(ReflectionConstantAssocEntry { key, value });
        }
    }
    let is_list = entries.iter().enumerate().all(|(index, entry)|
        matches!(entry.key, ReflectionDefaultArrayKey::Int(key) if key == index as i64));
    Some(if is_list {
        ReflectionConstantValue::Array(entries.into_iter().map(|entry| entry.value).collect())
    } else {
        ReflectionConstantValue::AssocArray(entries)
    })
}

/// Compares two normalized keys, decoding string literals to their PHP bytes.
fn same_key(left: &ReflectionDefaultArrayKey, right: &ReflectionDefaultArrayKey) -> bool {
    match (left, right) {
        (ReflectionDefaultArrayKey::Int(left), ReflectionDefaultArrayKey::Int(right)) => left == right,
        (ReflectionDefaultArrayKey::Str(left), ReflectionDefaultArrayKey::Str(right)) =>
            elephc_builtin_contract::string_literal::literal_bytes(left)
                == elephc_builtin_contract::string_literal::literal_bytes(right),
        _ => false,
    }
}
