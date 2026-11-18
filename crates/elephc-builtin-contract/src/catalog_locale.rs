//! Purpose:
//! Canonical contracts for PHP's locale functions declared by the injected locale prelude.
//!
//! Called from:
//! - `crate::registry` when assembling the complete shared contract catalog.
//!
//! Key details:
//! - Both entries are `BuiltinKind::PreludeProvided`: `elephc::locale_prelude` injects the
//!   PHP declarations, and their bodies call the internal libc helpers.
//! - `setlocale` keeps PHP's first `$locales` parameter plus the `...$rest` tail so array
//!   candidates and additional locale arguments are both accepted.
//! - The `LC_*` and `CODESET` constants live in `crate::catalog_constants_locale`.

use crate::{
    Area, BuiltinContract, BuiltinId, BuiltinKind, ParamSpec, PhpModule, TypeSpec,
};

/// Builds one by-value parameter.
macro_rules! param {
    ($name:literal, $ty:expr) => {
        ParamSpec {
            name: $name,
            ty: $ty,
            default: None,
            by_ref: false,
        }
    };
}

/// Builds one locale function contract.
macro_rules! locale_contract {
    (
        $name:literal,
        [$($param:expr),* $(,)?], variadic: $variadic:literal,
        $summary:literal
    ) => {
        BuiltinContract {
            id: BuiltinId::from_canonical_name($name),
            name: $name,
            area: Area::String,
            module: PhpModule::Standard,
            since: None,
            kind: BuiltinKind::PreludeProvided,
            params: &[$($param),*],
            variadic: Some($variadic),
            variadic_by_ref: false,
            min_args: None,
            max_args: None,
            arity_error: None,
            returns: TypeSpec::Union(&[TypeSpec::Str, TypeSpec::False]),
            by_ref_return: false,
            summary: $summary,
            examples: &[],
            php_manual: None,
            deprecation: None,
            extension: false,
            internal: false,
            requirements: &[],
        }
    };
    (
        $name:literal,
        [$($param:expr),* $(,)?],
        $summary:literal
    ) => {
        BuiltinContract {
            id: BuiltinId::from_canonical_name($name),
            name: $name,
            area: Area::String,
            module: PhpModule::Standard,
            since: None,
            kind: BuiltinKind::PreludeProvided,
            params: &[$($param),*],
            variadic: None,
            variadic_by_ref: false,
            min_args: None,
            max_args: None,
            arity_error: None,
            returns: TypeSpec::Union(&[TypeSpec::Str, TypeSpec::False]),
            by_ref_return: false,
            summary: $summary,
            examples: &[],
            php_manual: None,
            deprecation: None,
            extension: false,
            internal: false,
            requirements: &[],
        }
    };
}

/// The locale function contracts, in PHP stub order.
pub(crate) static CONTRACTS: &[BuiltinContract] = &[
    locale_contract!(
        "setlocale",
        [
            param!("category", TypeSpec::Int),
            param!("locales", TypeSpec::Mixed),
        ],
        variadic: "rest",
        "Sets the process locale from one or more candidates and returns the active locale, or false on failure."
    ),
    locale_contract!(
        "nl_langinfo",
        [param!("item", TypeSpec::Int)],
        "Returns locale information for one item; elephc exposes the portable CODESET query."
    ),
];
