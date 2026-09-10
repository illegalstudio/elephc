//! Purpose:
//! Implements the default boxed builtin adapter for test and embedding value providers.
//!
//! Called from:
//! - RuntimeValueOps::runtime_builtin_call and providers adding specialized bridges.
//!
//! Key details:
//! - Production Magician overrides this adapter with the generated versioned C ABI.

use super::*;

/// Executes a builtin using primitive value operations, or returns None when unsupported.
pub(crate) fn default_builtin_call(values: &mut (impl RuntimeValueOps + ?Sized), id: RuntimeBuiltinId, args: &[RuntimeCellHandle]) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let result = match (id, args) {
        (RuntimeBuiltinId::Boolval, [value]) => values.cast_bool(*value)?,
        (RuntimeBuiltinId::Floatval, [value]) => values.cast_float(*value)?,
        (RuntimeBuiltinId::Intval, [value]) => values.cast_int(*value)?,
        (RuntimeBuiltinId::IsArray, [value]) => {
            let is_array = matches!(values.type_tag(*value)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC);
            values.bool_value(is_array)?
        }
        (RuntimeBuiltinId::IsNull, [value]) => {
            let is_null = values.is_null(*value)?;
            values.bool_value(is_null)?
        }
        (RuntimeBuiltinId::Abs, [value]) => values.abs(*value)?,
        (RuntimeBuiltinId::Ceil, [value]) => values.ceil(*value)?,
        (RuntimeBuiltinId::Floor, [value]) => values.floor(*value)?,
        (RuntimeBuiltinId::Sqrt, [value]) => values.sqrt(*value)?,
        (RuntimeBuiltinId::Fdiv, [left, right]) => values.fdiv(*left, *right)?,
        (RuntimeBuiltinId::Fmod, [left, right]) => values.fmod(*left, *right)?,
        (RuntimeBuiltinId::Pow, [left, right]) => values.pow(*left, *right)?,
        (RuntimeBuiltinId::Round, [value]) => values.round(*value, None)?,
        (RuntimeBuiltinId::Round, [value, precision]) => {
            values.round(*value, Some(*precision))?
        }
        (RuntimeBuiltinId::Strrev, [value]) => values.strrev(*value)?,
        (RuntimeBuiltinId::ArrayKeyExists, [key, array]) => {
            values.array_key_exists(*key, *array)?
        }
        (RuntimeBuiltinId::ObGetLevel, []) => {
            let level = values.ob_level()?;
            values.int(level)?
        }
        (RuntimeBuiltinId::ObGetLength, []) => match values.ob_length()? {
            Some(length) => values.int(length)?,
            None => values.bool_value(false)?,
        },
        (RuntimeBuiltinId::ObClean, []) => {
            let cleaned = values.ob_clean()?;
            values.bool_value(cleaned)?
        }
        (RuntimeBuiltinId::ObFlush, []) => {
            let flushed = values.ob_flush()?;
            values.bool_value(flushed)?
        }
        (RuntimeBuiltinId::ObEndClean, []) => {
            let ended = values.ob_end(false)?;
            values.bool_value(ended)?
        }
        (RuntimeBuiltinId::ObEndFlush, []) => {
            let ended = values.ob_end(true)?;
            values.bool_value(ended)?
        }
        _ => return Ok(None),
    };
    Ok(Some(result))
}
