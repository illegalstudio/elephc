//! Purpose:
//! Evaluates EvalIR constants, dynamic constant fetches, predefined constants, and magic constants.
//!
//! Called from:
//! - `crate::interpreter::eval_expr()` for constant and magic-constant expression nodes.
//!
//! Key details:
//! - Dynamic constants prefer eval context declarations before predefined fallback constants.
//! - Magic file and directory values come from the current eval call-site context.

use super::*;

/// Converts one EvalIR constant into a runtime-cell handle.
pub(super) fn eval_const(
    value: &EvalConst,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match value {
        EvalConst::Null => values.null(),
        EvalConst::Bool(value) => values.bool_value(*value),
        EvalConst::Int(value) => values.int(*value),
        EvalConst::Float(value) => values.float(*value),
        EvalConst::String(value) => values.string(value),
    }
}

/// Loads a retained value for one eval-defined dynamic constant.
pub(super) fn eval_const_fetch(
    name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(value) = eval_predefined_constant(name, values)? {
        return Ok(value);
    }
    let Some(value) = context.constant(name) else {
        return Err(EvalStatus::RuntimeFatal);
    };
    values.retain(value)
}

/// Fetches a namespaced constant and falls back to the global constant namespace.
pub(super) fn eval_namespaced_const_fetch(
    name: &str,
    fallback_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(value) = eval_predefined_constant(name, values)? {
        return Ok(value);
    }
    if let Some(value) = context.constant(name) {
        return values.retain(value);
    }
    eval_const_fetch(fallback_name, context, values)
}

/// Materializes one eval-visible predefined constant into a runtime cell.
fn eval_predefined_constant(
    name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(value) = eval_predefined_constant_value(name) else {
        return Ok(None);
    };
    match value {
        EvalPredefinedConstant::Int(value) => values.int(value).map(Some),
        EvalPredefinedConstant::Float(value) => values.float(value).map(Some),
        EvalPredefinedConstant::String(value) => values.string(value).map(Some),
    }
}

/// Returns eval-visible predefined constants that do not live in dynamic context.
pub(in crate::interpreter) fn eval_predefined_constant_value(
    name: &str,
) -> Option<EvalPredefinedConstant> {
    match name.trim_start_matches('\\') {
        "PATHINFO_DIRNAME" => Some(EvalPredefinedConstant::Int(EVAL_PATHINFO_DIRNAME)),
        "PATHINFO_BASENAME" => Some(EvalPredefinedConstant::Int(EVAL_PATHINFO_BASENAME)),
        "PATHINFO_EXTENSION" => Some(EvalPredefinedConstant::Int(EVAL_PATHINFO_EXTENSION)),
        "PATHINFO_FILENAME" => Some(EvalPredefinedConstant::Int(EVAL_PATHINFO_FILENAME)),
        "PATHINFO_ALL" => Some(EvalPredefinedConstant::Int(EVAL_PATHINFO_ALL)),
        "FNM_NOESCAPE" => Some(EvalPredefinedConstant::Int(EVAL_FNM_NOESCAPE)),
        "FNM_PATHNAME" => Some(EvalPredefinedConstant::Int(EVAL_FNM_PATHNAME)),
        "FNM_PERIOD" => Some(EvalPredefinedConstant::Int(EVAL_FNM_PERIOD)),
        "FNM_CASEFOLD" => Some(EvalPredefinedConstant::Int(EVAL_FNM_CASEFOLD)),
        "LOCK_SH" => Some(EvalPredefinedConstant::Int(EVAL_LOCK_SH)),
        "LOCK_EX" => Some(EvalPredefinedConstant::Int(EVAL_LOCK_EX)),
        "LOCK_UN" => Some(EvalPredefinedConstant::Int(EVAL_LOCK_UN)),
        "LOCK_NB" => Some(EvalPredefinedConstant::Int(EVAL_LOCK_NB)),
        "ARRAY_FILTER_USE_VALUE" => Some(EvalPredefinedConstant::Int(EVAL_ARRAY_FILTER_USE_VALUE)),
        "ARRAY_FILTER_USE_BOTH" => Some(EvalPredefinedConstant::Int(EVAL_ARRAY_FILTER_USE_BOTH)),
        "ARRAY_FILTER_USE_KEY" => Some(EvalPredefinedConstant::Int(EVAL_ARRAY_FILTER_USE_KEY)),
        "COUNT_NORMAL" => Some(EvalPredefinedConstant::Int(EVAL_COUNT_NORMAL)),
        "COUNT_RECURSIVE" => Some(EvalPredefinedConstant::Int(EVAL_COUNT_RECURSIVE)),
        "PREG_SPLIT_NO_EMPTY" => Some(EvalPredefinedConstant::Int(EVAL_PREG_SPLIT_NO_EMPTY)),
        "PREG_SPLIT_DELIM_CAPTURE" => {
            Some(EvalPredefinedConstant::Int(EVAL_PREG_SPLIT_DELIM_CAPTURE))
        }
        "PREG_SPLIT_OFFSET_CAPTURE" => {
            Some(EvalPredefinedConstant::Int(EVAL_PREG_SPLIT_OFFSET_CAPTURE))
        }
        "PREG_PATTERN_ORDER" => Some(EvalPredefinedConstant::Int(EVAL_PREG_PATTERN_ORDER)),
        "PREG_SET_ORDER" => Some(EvalPredefinedConstant::Int(EVAL_PREG_SET_ORDER)),
        "PREG_OFFSET_CAPTURE" => Some(EvalPredefinedConstant::Int(EVAL_PREG_OFFSET_CAPTURE)),
        "PREG_UNMATCHED_AS_NULL" => Some(EvalPredefinedConstant::Int(EVAL_PREG_UNMATCHED_AS_NULL)),
        "JSON_ERROR_NONE" => Some(EvalPredefinedConstant::Int(EVAL_JSON_ERROR_NONE)),
        "JSON_ERROR_DEPTH" => Some(EvalPredefinedConstant::Int(EVAL_JSON_ERROR_DEPTH)),
        "JSON_ERROR_STATE_MISMATCH" => {
            Some(EvalPredefinedConstant::Int(EVAL_JSON_ERROR_STATE_MISMATCH))
        }
        "JSON_ERROR_CTRL_CHAR" => Some(EvalPredefinedConstant::Int(EVAL_JSON_ERROR_CTRL_CHAR)),
        "JSON_ERROR_SYNTAX" => Some(EvalPredefinedConstant::Int(EVAL_JSON_ERROR_SYNTAX)),
        "JSON_ERROR_UTF8" => Some(EvalPredefinedConstant::Int(EVAL_JSON_ERROR_UTF8)),
        "JSON_ERROR_RECURSION" => Some(EvalPredefinedConstant::Int(EVAL_JSON_ERROR_RECURSION)),
        "JSON_ERROR_INF_OR_NAN" => Some(EvalPredefinedConstant::Int(EVAL_JSON_ERROR_INF_OR_NAN)),
        "JSON_ERROR_UNSUPPORTED_TYPE" => Some(EvalPredefinedConstant::Int(
            EVAL_JSON_ERROR_UNSUPPORTED_TYPE,
        )),
        "JSON_ERROR_INVALID_PROPERTY_NAME" => Some(EvalPredefinedConstant::Int(
            EVAL_JSON_ERROR_INVALID_PROPERTY_NAME,
        )),
        "JSON_ERROR_UTF16" => Some(EvalPredefinedConstant::Int(EVAL_JSON_ERROR_UTF16)),
        "JSON_HEX_TAG" => Some(EvalPredefinedConstant::Int(EVAL_JSON_HEX_TAG)),
        "JSON_HEX_AMP" => Some(EvalPredefinedConstant::Int(EVAL_JSON_HEX_AMP)),
        "JSON_HEX_APOS" => Some(EvalPredefinedConstant::Int(EVAL_JSON_HEX_APOS)),
        "JSON_HEX_QUOT" => Some(EvalPredefinedConstant::Int(EVAL_JSON_HEX_QUOT)),
        "JSON_BIGINT_AS_STRING" => Some(EvalPredefinedConstant::Int(EVAL_JSON_BIGINT_AS_STRING)),
        "JSON_FORCE_OBJECT" => Some(EvalPredefinedConstant::Int(EVAL_JSON_FORCE_OBJECT)),
        "JSON_NUMERIC_CHECK" => Some(EvalPredefinedConstant::Int(EVAL_JSON_NUMERIC_CHECK)),
        "JSON_UNESCAPED_SLASHES" => Some(EvalPredefinedConstant::Int(EVAL_JSON_UNESCAPED_SLASHES)),
        "JSON_UNESCAPED_UNICODE" => Some(EvalPredefinedConstant::Int(EVAL_JSON_UNESCAPED_UNICODE)),
        "JSON_PARTIAL_OUTPUT_ON_ERROR" => Some(EvalPredefinedConstant::Int(
            EVAL_JSON_PARTIAL_OUTPUT_ON_ERROR,
        )),
        "JSON_PRETTY_PRINT" => Some(EvalPredefinedConstant::Int(EVAL_JSON_PRETTY_PRINT)),
        "JSON_PRESERVE_ZERO_FRACTION" => Some(EvalPredefinedConstant::Int(
            EVAL_JSON_PRESERVE_ZERO_FRACTION,
        )),
        "JSON_INVALID_UTF8_IGNORE" => {
            Some(EvalPredefinedConstant::Int(EVAL_JSON_INVALID_UTF8_IGNORE))
        }
        "JSON_INVALID_UTF8_SUBSTITUTE" => Some(EvalPredefinedConstant::Int(
            EVAL_JSON_INVALID_UTF8_SUBSTITUTE,
        )),
        "JSON_THROW_ON_ERROR" => Some(EvalPredefinedConstant::Int(EVAL_JSON_THROW_ON_ERROR)),
        "INF" => Some(EvalPredefinedConstant::Float(f64::INFINITY)),
        "NAN" => Some(EvalPredefinedConstant::Float(f64::NAN)),
        "PHP_INT_MAX" => Some(EvalPredefinedConstant::Int(i64::MAX)),
        "PHP_EOL" => Some(EvalPredefinedConstant::String("\n")),
        "PHP_OS" => Some(EvalPredefinedConstant::String(eval_php_os_name())),
        "DIRECTORY_SEPARATOR" => Some(EvalPredefinedConstant::String("/")),
        _ => None,
    }
}

/// Returns the PHP OS constant for the host platform running the eval bridge.
fn eval_php_os_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "Darwin"
    } else {
        "Linux"
    }
}

/// Resolves one eval magic constant against fragment and dynamic-call metadata.
pub(super) fn eval_magic_const(
    magic: &EvalMagicConst,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match magic {
        EvalMagicConst::File => values.string(&context.eval_file_magic()),
        EvalMagicConst::Dir => values.string(context.call_dir()),
        EvalMagicConst::Line(line) => values.int(*line),
        EvalMagicConst::Function => values.string(context.current_function().unwrap_or("")),
        EvalMagicConst::Method => values.string(context.current_function().unwrap_or("")),
        EvalMagicConst::Class | EvalMagicConst::Namespace | EvalMagicConst::Trait => {
            values.string("")
        }
    }
}
