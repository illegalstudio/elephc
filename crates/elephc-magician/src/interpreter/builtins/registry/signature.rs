//! Purpose:
//! Signature-shape metadata derived from PHP-visible eval builtin declarations.
//!
//! Called from:
//! - `crate::interpreter::builtin_metadata`
//! - builtin registry tests and argument binding audits.
//!
//! Key details:
//! - Declarative specs are the only signature source after builtin migration.
//! - Default values let named calls skip optional parameters without changing
//!   positional semantics.

use super::{eval_declared_builtin_default_value, eval_declared_builtin_spec};
use elephc_builtin_contract::DefaultSpec;

/// Comparison-friendly shape for one eval builtin signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::interpreter) struct EvalBuiltinSignatureShape {
    /// Number of leading parameters that must be supplied.
    pub(in crate::interpreter) required_param_count: usize,
    /// Number of parameters that have defaults.
    pub(in crate::interpreter) default_param_count: usize,
    /// Variadic parameter name when this builtin accepts extra arguments.
    pub(in crate::interpreter) variadic: Option<&'static str>,
    /// Parameter names that are passed by reference.
    pub(in crate::interpreter) by_ref_params: &'static [&'static str],
}

/// Runtime-materializable default value for one eval builtin parameter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::interpreter) enum EvalBuiltinDefaultValue {
    /// PHP null default.
    Null,
    /// PHP boolean default.
    Bool(bool),
    /// PHP integer default.
    Int(i64),
    /// PHP float default.
    Float(f64),
    /// PHP string default represented as UTF-8 text.
    String(&'static str),
    /// PHP empty indexed array default.
    EmptyArray,
    /// PHP class constant or enum case default.
    ClassConstant {
        /// Fully qualified PHP class or enum name without a leading slash.
        class: &'static str,
        /// PHP-visible constant or case name.
        name: &'static str,
    },
}

/// Returns signature-shape metadata for one PHP-visible eval builtin.
pub(in crate::interpreter) fn eval_builtin_signature_shape(
    name: &str,
) -> Option<EvalBuiltinSignatureShape> {
    if let Some(spec) = eval_declared_builtin_spec(name) {
        return Some(
        EvalBuiltinSignatureShape {
            required_param_count: spec.required_param_count(),
            default_param_count: spec.default_param_count(),
            variadic: spec.variadic,
            by_ref_params: spec.by_ref_param_names(),
        });
    }
    elephc_builtin_contract::lookup(name).map(|contract| EvalBuiltinSignatureShape {
        required_param_count: contract
            .params
            .iter()
            .take_while(|param| param.default.is_none())
            .count(),
        default_param_count: contract
            .params
            .iter()
            .filter(|param| param.default.is_some())
            .count(),
        variadic: contract.variadic,
        by_ref_params: &[],
    })
}

/// Returns the concrete default value for one optional builtin parameter.
pub(in crate::interpreter) fn eval_builtin_default_value(
    name: &str,
    param_index: usize,
) -> Option<EvalBuiltinDefaultValue> {
    if let Some(value) = eval_declared_builtin_default_value(name, param_index) {
        return Some(value);
    }
    let default = elephc_builtin_contract::lookup(name)?
        .params
        .get(param_index)?
        .default?;
    Some(match default {
        DefaultSpec::Null => EvalBuiltinDefaultValue::Null,
        DefaultSpec::Bool(value) => EvalBuiltinDefaultValue::Bool(value),
        DefaultSpec::Int(value) => EvalBuiltinDefaultValue::Int(value),
        DefaultSpec::Float(value) => EvalBuiltinDefaultValue::Float(value),
        DefaultSpec::Str(value) => EvalBuiltinDefaultValue::String(value),
        DefaultSpec::IntMax => EvalBuiltinDefaultValue::Int(i64::MAX),
        DefaultSpec::EmptyArray => EvalBuiltinDefaultValue::EmptyArray,
        DefaultSpec::ClassConstant { class, name } => EvalBuiltinDefaultValue::ClassConstant { class, name },
        DefaultSpec::Constant(name) => {
            if let Some((class, constant)) = name.split_once("::") {
                return Some(EvalBuiltinDefaultValue::ClassConstant { class, name: constant });
            }
            use crate::interpreter::{constant_eval::eval_predefined_constant_value, EvalPredefinedConstant};
            match eval_predefined_constant_value(name)? {
                EvalPredefinedConstant::Int(value) => EvalBuiltinDefaultValue::Int(value),
                EvalPredefinedConstant::Float(value) => EvalBuiltinDefaultValue::Float(value),
                EvalPredefinedConstant::String(value) => EvalBuiltinDefaultValue::String(value),
            }
        }
        // Textual prelude defaults are not reparsed by registry argument binding.
        DefaultSpec::Expr(_) => return None,
    })
}
