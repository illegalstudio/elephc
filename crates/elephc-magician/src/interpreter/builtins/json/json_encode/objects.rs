//! Purpose:
//! Classifies eval objects before the iterative JSON encoder traverses them.
//!
//! Called from:
//! - `super::traversal::EvalJsonTraversal::encode_object()`.
//!
//! Key details:
//! - Enum `jsonSerialize()` takes precedence over backing scalars and unit enum errors.
//! - Native enum metadata uses property initialization when implicit interfaces are absent.

use super::*;

/// Selects the PHP JSON representation for one object value.
pub(super) enum EvalJsonObjectEncoding {
    Properties,
    SerializableEnum,
    BackedEnum(RuntimeCellHandle),
    UnitEnum,
}

/// Classifies an object and acquires any owned backing scalar needed by traversal.
pub(super) fn eval_json_object_encoding(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalJsonObjectEncoding, EvalStatus> {
    let is_enum = match dynamic_object_is_a(value, "UnitEnum", false, context, values)? {
        Some(is_enum) => is_enum,
        None => {
            // Native enum metadata does not expose the implicit UnitEnum interface.
            let class_name = runtime_object_class_name(value, values)?;
            values.enum_exists(&class_name)?
        }
    };
    if !is_enum {
        return Ok(EvalJsonObjectEncoding::Properties);
    }

    let is_serializable = dynamic_object_is_a(value, "JsonSerializable", false, context, values)?
        .map_or_else(|| values.object_is_a(value, "JsonSerializable", false), Ok)?;
    if is_serializable {
        return Ok(EvalJsonObjectEncoding::SerializableEnum);
    }

    let is_backed = match dynamic_object_is_a(value, "BackedEnum", false, context, values)? {
        Some(is_backed) => is_backed,
        // Native enum value slots use declared property initialization metadata.
        None => values.property_is_initialized(value, "value")?,
    };
    if is_backed {
        return eval_property_get_result(value, "value", context, values)
            .map(EvalJsonObjectEncoding::BackedEnum);
    }
    Ok(EvalJsonObjectEncoding::UnitEnum)
}
