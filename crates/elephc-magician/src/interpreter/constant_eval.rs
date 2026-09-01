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
    if name.trim_start_matches('\\').eq_ignore_ascii_case("E_STRICT")
        && crate::eval_php_profile::eval_php_version_id() >= 80400
    {
        values.deprecated(
            "\nDeprecated: Constant E_STRICT is deprecated since 8.4, the error level was removed",
        )?;
    }
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
        "ICONV_MIME_DECODE_STRICT" => {
            Some(EvalPredefinedConstant::Int(EVAL_ICONV_MIME_DECODE_STRICT))
        }
        "ICONV_MIME_DECODE_CONTINUE_ON_ERROR" => Some(EvalPredefinedConstant::Int(
            EVAL_ICONV_MIME_DECODE_CONTINUE_ON_ERROR,
        )),
        // The runtime iconv provider is fixed by the platform: Apple ships GNU libiconv,
        // and elephc's Linux support targets glibc.
        "ICONV_IMPL" => Some(EvalPredefinedConstant::String(
            elephc_iconv::implementation_name(cfg!(target_os = "macos")),
        )),
        "ICONV_VERSION" => Some(EvalPredefinedConstant::String(elephc_iconv::ICONV_VERSION)),
        "PHP_URL_SCHEME" => Some(EvalPredefinedConstant::Int(EVAL_PHP_URL_SCHEME)),
        "PHP_URL_HOST" => Some(EvalPredefinedConstant::Int(EVAL_PHP_URL_HOST)),
        "PHP_URL_PORT" => Some(EvalPredefinedConstant::Int(EVAL_PHP_URL_PORT)),
        "PHP_URL_USER" => Some(EvalPredefinedConstant::Int(EVAL_PHP_URL_USER)),
        "PHP_URL_PASS" => Some(EvalPredefinedConstant::Int(EVAL_PHP_URL_PASS)),
        "PHP_URL_PATH" => Some(EvalPredefinedConstant::Int(EVAL_PHP_URL_PATH)),
        "PHP_URL_QUERY" => Some(EvalPredefinedConstant::Int(EVAL_PHP_URL_QUERY)),
        "PHP_URL_FRAGMENT" => Some(EvalPredefinedConstant::Int(EVAL_PHP_URL_FRAGMENT)),
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
        "OPENSSL_RAW_DATA" => Some(EvalPredefinedConstant::Int(EVAL_OPENSSL_RAW_DATA)),
        "OPENSSL_ZERO_PADDING" => Some(EvalPredefinedConstant::Int(EVAL_OPENSSL_ZERO_PADDING)),
        "OPENSSL_DONT_ZERO_PAD_KEY" => {
            Some(EvalPredefinedConstant::Int(EVAL_OPENSSL_DONT_ZERO_PAD_KEY))
        }
        "ARRAY_FILTER_USE_VALUE" => Some(EvalPredefinedConstant::Int(EVAL_ARRAY_FILTER_USE_VALUE)),
        "ARRAY_FILTER_USE_BOTH" => Some(EvalPredefinedConstant::Int(EVAL_ARRAY_FILTER_USE_BOTH)),
        "ARRAY_FILTER_USE_KEY" => Some(EvalPredefinedConstant::Int(EVAL_ARRAY_FILTER_USE_KEY)),
        "STR_PAD_LEFT" => Some(EvalPredefinedConstant::Int(EVAL_STR_PAD_LEFT)),
        "STR_PAD_RIGHT" => Some(EvalPredefinedConstant::Int(EVAL_STR_PAD_RIGHT)),
        "STR_PAD_BOTH" => Some(EvalPredefinedConstant::Int(EVAL_STR_PAD_BOTH)),
        "COUNT_NORMAL" => Some(EvalPredefinedConstant::Int(EVAL_COUNT_NORMAL)),
        "COUNT_RECURSIVE" => Some(EvalPredefinedConstant::Int(EVAL_COUNT_RECURSIVE)),
        "PHP_ROUND_HALF_UP" => Some(EvalPredefinedConstant::Int(EVAL_PHP_ROUND_HALF_UP)),
        "PHP_ROUND_HALF_DOWN" => Some(EvalPredefinedConstant::Int(EVAL_PHP_ROUND_HALF_DOWN)),
        "PHP_ROUND_HALF_EVEN" => Some(EvalPredefinedConstant::Int(EVAL_PHP_ROUND_HALF_EVEN)),
        "PHP_ROUND_HALF_ODD" => Some(EvalPredefinedConstant::Int(EVAL_PHP_ROUND_HALF_ODD)),
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
        "E_ERROR" => Some(EvalPredefinedConstant::Int(1)),
        "E_WARNING" => Some(EvalPredefinedConstant::Int(2)),
        "E_PARSE" => Some(EvalPredefinedConstant::Int(4)),
        "E_NOTICE" => Some(EvalPredefinedConstant::Int(8)),
        "E_CORE_ERROR" => Some(EvalPredefinedConstant::Int(16)),
        "E_CORE_WARNING" => Some(EvalPredefinedConstant::Int(32)),
        "E_COMPILE_ERROR" => Some(EvalPredefinedConstant::Int(64)),
        "E_COMPILE_WARNING" => Some(EvalPredefinedConstant::Int(128)),
        "E_USER_ERROR" => Some(EvalPredefinedConstant::Int(256)),
        "E_USER_WARNING" => Some(EvalPredefinedConstant::Int(512)),
        "E_USER_NOTICE" => Some(EvalPredefinedConstant::Int(1024)),
        "E_STRICT" => Some(EvalPredefinedConstant::Int(2048)),
        "E_RECOVERABLE_ERROR" => Some(EvalPredefinedConstant::Int(4096)),
        "E_DEPRECATED" => Some(EvalPredefinedConstant::Int(8192)),
        "E_USER_DEPRECATED" => Some(EvalPredefinedConstant::Int(16384)),
        "E_ALL" => Some(EvalPredefinedConstant::Int(
            if crate::eval_php_profile::eval_php_version_id() >= 80400 {
                30719
            } else {
                32767
            },
        )),
        "LC_CTYPE" => Some(EvalPredefinedConstant::Int(libc::LC_CTYPE as i64)),
        "LC_NUMERIC" => Some(EvalPredefinedConstant::Int(libc::LC_NUMERIC as i64)),
        "LC_TIME" => Some(EvalPredefinedConstant::Int(libc::LC_TIME as i64)),
        "LC_COLLATE" => Some(EvalPredefinedConstant::Int(libc::LC_COLLATE as i64)),
        "LC_MONETARY" => Some(EvalPredefinedConstant::Int(libc::LC_MONETARY as i64)),
        "LC_ALL" => Some(EvalPredefinedConstant::Int(libc::LC_ALL as i64)),
        "LC_MESSAGES" => Some(EvalPredefinedConstant::Int(libc::LC_MESSAGES as i64)),
        "INF" => Some(EvalPredefinedConstant::Float(f64::INFINITY)),
        "NAN" => Some(EvalPredefinedConstant::Float(f64::NAN)),
        "PHP_INT_MAX" => Some(EvalPredefinedConstant::Int(i64::MAX)),
        "PHP_EOL" => Some(EvalPredefinedConstant::String("\n")),
        "PHP_OS" => Some(EvalPredefinedConstant::String(eval_php_os_name())),
        // The PHP version surface. The compiler bakes these per compilation from
        // `--php-version` / `--web` (`codegen_support::prescan::collect_constants`) and forwards
        // the profile to this interpreter through `__elephc_eval_set_php_version_id`, so the
        // three profile-dependent entries answer whatever the binary was compiled for.
        "PHP_VERSION" => Some(EvalPredefinedConstant::String(
            crate::eval_php_profile::eval_php_version_string(),
        )),
        "PHP_VERSION_ID" => Some(EvalPredefinedConstant::Int(i64::from(
            crate::eval_php_profile::eval_php_version_id(),
        ))),
        "PHP_MAJOR_VERSION" => Some(EvalPredefinedConstant::Int(EVAL_PHP_MAJOR_VERSION)),
        "PHP_MINOR_VERSION" => Some(EvalPredefinedConstant::Int(
            crate::eval_php_profile::eval_php_minor_version(),
        )),
        "PHP_RELEASE_VERSION" => Some(EvalPredefinedConstant::Int(
            crate::eval_php_profile::eval_php_release_version(),
        )),
        "PHP_EXTRA_VERSION" => Some(EvalPredefinedConstant::String(
            crate::eval_php_profile::eval_php_extra_version(),
        )),
        "PHP_SAPI" => Some(EvalPredefinedConstant::String(EVAL_PHP_SAPI)),
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
        EvalMagicConst::Function => values.string(
            context
                .current_magic_function()
                .or_else(|| context.current_function())
                .unwrap_or(""),
        ),
        EvalMagicConst::Method => values.string(
            context
                .current_magic_method()
                .or_else(|| context.current_function())
                .unwrap_or(""),
        ),
        EvalMagicConst::Class => values.string(context.current_magic_class().unwrap_or("")),
        EvalMagicConst::Namespace => values.string(""),
        EvalMagicConst::Trait => values.string(context.current_magic_trait().unwrap_or("")),
    }
}
