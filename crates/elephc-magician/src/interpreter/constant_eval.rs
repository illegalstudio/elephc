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
use elephc_builtin_contract::ConstValue;

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
    let name = name.trim_start_matches('\\');
    if let Some(value) = eval_target_dependent_constant(name) {
        return Some(value);
    }
    let constant = elephc_builtin_contract::lookup_constant(name)?;
    if !matches!(
        elephc_builtin_contract::eval_constant_support(constant),
        elephc_builtin_contract::BackendSupport::Implemented(_)
    ) {
        return None;
    }
    match constant.value {
        ConstValue::Int(value) => Some(EvalPredefinedConstant::Int(value)),
        ConstValue::Float(value) => Some(EvalPredefinedConstant::Float(value)),
        ConstValue::Str(value) => Some(EvalPredefinedConstant::String(value)),
        // Booleans, null, and stream resources have no predefined-constant representation
        // here; the catalog's eval route already reports them unsupported.
        ConstValue::Bool(_) | ConstValue::Null | ConstValue::StreamResource(_) => None,
        // A target-dependent name without an arm above is a catalog/interpreter mismatch,
        // which `every_catalogued_constant_has_its_declared_eval_route` reports.
        ConstValue::TargetDependent(_) => None,
    }
}

/// Values the catalog marks `TargetDependent`: Magician computes them for the target it is
/// linked into and the PHP profile it emulates, under the catalogued name.
fn eval_target_dependent_constant(name: &str) -> Option<EvalPredefinedConstant> {
    let is_macos = cfg!(target_os = "macos");
    Some(match name {
        "ICONV_IMPL" => EvalPredefinedConstant::String(elephc_iconv::implementation_name(is_macos)),
        "ICONV_VERSION" => EvalPredefinedConstant::String(elephc_iconv::ICONV_VERSION),
        "PHP_OS" => EvalPredefinedConstant::String(eval_php_os_name()),
        "PHP_VERSION" => EvalPredefinedConstant::String(
            crate::eval_php_profile::eval_php_version_string(),
        ),
        "PHP_VERSION_ID" => EvalPredefinedConstant::Int(i64::from(
            crate::eval_php_profile::eval_php_version_id(),
        )),
        "PHP_MAJOR_VERSION" => EvalPredefinedConstant::Int(EVAL_PHP_MAJOR_VERSION),
        "PHP_MINOR_VERSION" => EvalPredefinedConstant::Int(
            crate::eval_php_profile::eval_php_minor_version(),
        ),
        "PHP_RELEASE_VERSION" => EvalPredefinedConstant::Int(EVAL_PHP_RELEASE_VERSION),
        "PHP_EXTRA_VERSION" => EvalPredefinedConstant::String(EVAL_PHP_EXTRA_VERSION),
        "PHP_SAPI" => EvalPredefinedConstant::String(EVAL_PHP_SAPI),
        "DIRECTORY_SEPARATOR" => EvalPredefinedConstant::String("/"),
        // Platform `fnmatch(3)` flag values, matching the compiler's per-target table.
        "FNM_NOESCAPE" => EvalPredefinedConstant::Int(if is_macos { 1 } else { 2 }),
        "FNM_PATHNAME" => EvalPredefinedConstant::Int(if is_macos { 2 } else { 1 }),
        _ => return None,
    })
}

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

#[cfg(test)]
mod curl_constant_fallback_tests {
    use super::*;

    /// `CURLOPT_URL`/`CURLOPT_RETURNTRANSFER` resolve through the same predefined-constant
    /// path `JSON_PRETTY_PRINT` etc. use, table-driven through the shared constant catalog
    /// (`elephc_builtin_contract` `catalog_constants_curl`) rather than a 689-arm hand-written
    /// match — this is a pure-data lookup, so it needs no bridge, no feature flag, and no
    /// linked libcurl to verify (the catalog publishes curl constants unconditionally).
    #[test]
    fn curl_constants_resolve_through_the_predefined_constant_fallback() {
        assert!(matches!(
            eval_predefined_constant_value("CURLOPT_URL"),
            Some(EvalPredefinedConstant::Int(10002))
        ));
        assert!(matches!(
            eval_predefined_constant_value("CURLOPT_RETURNTRANSFER"),
            Some(EvalPredefinedConstant::Int(19913))
        ));
        assert!(matches!(
            eval_predefined_constant_value("CURLE_OK"),
            Some(EvalPredefinedConstant::Int(0))
        ));
    }

    /// A name that merely LOOKS curl-ish must not resolve — the fallback is a whole-name
    /// table lookup, not a prefix match, matching every other predefined-constant arm in
    /// this file.
    #[test]
    fn a_curl_shaped_unknown_name_does_not_resolve() {
        assert!(eval_predefined_constant_value("CURLOPT_NOT_A_REAL_OPTION").is_none());
    }

    /// Verifies Magician resolves exactly the constants whose catalog eval route says it
    /// does: every `Implemented` one has a value, every `Unsupported` one has none.
    #[test]
    fn every_catalogued_constant_has_its_declared_eval_route() {
        use elephc_builtin_contract::{constants, eval_constant_support, BackendSupport};
        for constant in constants() {
            let resolved = eval_predefined_constant_value(constant.name).is_some();
            match eval_constant_support(constant) {
                BackendSupport::Implemented(_) => assert!(
                    resolved,
                    "{} is eval-supported per the catalog but does not resolve",
                    constant.name
                ),
                BackendSupport::Unsupported(reason) => assert!(
                    !resolved,
                    "{} resolves in eval but the catalog says {reason:?}",
                    constant.name
                ),
            }
        }
    }

    /// The existing (pre-curl) predefined constants must still resolve unchanged: the new
    /// fallback arm must not have shadowed or reordered anything above it in the match.
    #[test]
    fn pre_existing_predefined_constants_still_resolve() {
        assert!(matches!(
            eval_predefined_constant_value("JSON_PRETTY_PRINT"),
            Some(EvalPredefinedConstant::Int(_))
        ));
        assert!(matches!(
            eval_predefined_constant_value("PHP_INT_MAX"),
            Some(EvalPredefinedConstant::Int(i64::MAX))
        ));
    }
}
