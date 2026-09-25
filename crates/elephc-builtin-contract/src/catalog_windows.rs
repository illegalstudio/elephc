//! Purpose:
//! Declares the PHP standard-extension `sapi_windows_*` contracts.
//!
//! Called from:
//! - `crate::registry::contracts()` when assembling the shared builtin catalog.
//!
//! Key details:
//! - These functions are PHP-visible only on Windows; compiler semantics enforce that target
//!   availability while this dependency-neutral catalog keeps their PHP signatures stable.

use crate::{Area, BuiltinContract, BuiltinId, BuiltinKind, DefaultSpec, ParamSpec, PhpModule, TypeSpec};

const fn contract(
    name: &'static str,
    params: &'static [ParamSpec],
    returns: TypeSpec,
    summary: &'static str,
) -> BuiltinContract {
    BuiltinContract {
        id: BuiltinId::from_canonical_name(name),
        name,
        area: Area::System,
        module: PhpModule::Standard,
        since: None,
        kind: BuiltinKind::Function,
        params,
        variadic: None,
        variadic_by_ref: false,
        min_args: None,
        max_args: None,
        arity_error: None,
        returns,
        by_ref_return: false,
        summary,
        examples: &[],
        php_manual: None,
        deprecation: None,
        extension: false,
        internal: false,
        requirements: &[],
    }
}

/// Standard-library Windows SAPI contracts from PHP 8.5.6.
pub(crate) static CONTRACTS: &[BuiltinContract] = &[
    contract(
        "sapi_windows_vt100_support",
        &[
            ParamSpec { name: "stream", ty: TypeSpec::Mixed, default: None, by_ref: false },
            ParamSpec { name: "enable", ty: TypeSpec::Nullable(&TypeSpec::Bool), default: Some(DefaultSpec::Null), by_ref: false },
        ],
        TypeSpec::Bool,
        "Queries or changes VT100 support for a Windows console stream.",
    ),
    contract(
        "sapi_windows_cp_set",
        &[ParamSpec { name: "codepage", ty: TypeSpec::Int, default: None, by_ref: false }],
        TypeSpec::Bool,
        "Sets the active Windows console code page.",
    ),
    contract(
        "sapi_windows_cp_get",
        &[ParamSpec { name: "kind", ty: TypeSpec::Str, default: Some(DefaultSpec::Str("")), by_ref: false }],
        TypeSpec::Int,
        "Returns the active, ANSI, or OEM Windows code page.",
    ),
    contract(
        "sapi_windows_cp_conv",
        &[
            ParamSpec { name: "in_codepage", ty: TypeSpec::Mixed, default: None, by_ref: false },
            ParamSpec { name: "out_codepage", ty: TypeSpec::Mixed, default: None, by_ref: false },
            ParamSpec { name: "subject", ty: TypeSpec::Str, default: None, by_ref: false },
        ],
        TypeSpec::Nullable(&TypeSpec::Str),
        "Converts a string between Windows code pages.",
    ),
    contract(
        "sapi_windows_cp_is_utf8",
        &[],
        TypeSpec::Bool,
        "Reports whether the active Windows code page is UTF-8 compatible.",
    ),
    contract(
        "sapi_windows_set_ctrl_handler",
        &[
            ParamSpec { name: "handler", ty: TypeSpec::Nullable(&TypeSpec::Callable), default: None, by_ref: false },
            ParamSpec { name: "add", ty: TypeSpec::Bool, default: Some(DefaultSpec::Bool(true)), by_ref: false },
        ],
        TypeSpec::Bool,
        "Installs or removes a Windows console control handler.",
    ),
    contract(
        "sapi_windows_generate_ctrl_event",
        &[
            ParamSpec { name: "event", ty: TypeSpec::Int, default: None, by_ref: false },
            ParamSpec { name: "pid", ty: TypeSpec::Int, default: Some(DefaultSpec::Int(0)), by_ref: false },
        ],
        TypeSpec::Bool,
        "Generates a Windows console control event.",
    ),
];
