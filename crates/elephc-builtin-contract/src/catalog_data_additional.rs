//! Purpose:
//! Canonical shared contracts for registry-backed builtins added after the initial catalog import.
//!
//! Called from:
//! - `crate::registry::contracts()` when assembling the complete builtin catalog.
//!
//! Key details:
//! - Entries remain backend-neutral; checker, lowering, and interpreter hooks join by `BuiltinId`.

use crate::{
    PhpModule,
    Area, BuiltinContract, BuiltinId, BuiltinKind, DefaultSpec, ParamSpec, TypeSpec,
    VariadicSpec,
};

macro_rules! param {
    ($name:literal, $ty:ident) => {
        ParamSpec {
            name: $name,
            ty: TypeSpec::$ty,
            default: None,
            by_ref: false,
        }
    };
    ($name:literal, $ty:ident = $default:expr) => {
        ParamSpec {
            name: $name,
            ty: TypeSpec::$ty,
            default: Some($default),
            by_ref: false,
        }
    };
    (ref $name:literal, $ty:ident = $default:expr) => {
        ParamSpec {
            name: $name,
            ty: TypeSpec::$ty,
            default: Some($default),
            by_ref: true,
        }
    };
    (ref $name:literal, $ty:ident) => {
        ParamSpec {
            name: $name,
            ty: TypeSpec::$ty,
            default: None,
            by_ref: true,
        }
    };
}

macro_rules! contract {
    (
        $name:literal, $area:ident, $module:ident, [$($param:expr),* $(,)?],
        $variadic:expr, $returns:ident, $summary:literal, $manual:literal
    ) => {
        BuiltinContract {
            id: BuiltinId::from_canonical_name($name),
            name: $name,
            area: Area::$area,
            module: PhpModule::$module,
            since: None,
            kind: BuiltinKind::Function,
            params: &[$($param),*],
            variadic: $variadic,
            min_args: None,
            max_args: None,
            arity_error: None,
            returns: TypeSpec::$returns,
            by_ref_return: false,
            summary: $summary,
            examples: &[],
            php_manual: Some($manual),
            deprecation: None,
            extension: false,
            internal: false,
            requirements: &[],
        }
    };
}

pub(crate) static CONTRACTS: &[BuiltinContract] = &[
    contract!(
        "error_log",
        System,
        Standard,
        [
            param!("message", Str),
            param!("message_type", Int = DefaultSpec::Int(0)),
            param!("destination", Str = DefaultSpec::Str("")),
            param!("additional_headers", Str = DefaultSpec::Str("")),
        ],
        None,
        Bool,
        "Writes a message to the configured error log destination.",
        "function.error-log"
    ),
    contract!(
        "extract",
        Array,
        Standard,
        [
            param!("array", Mixed),
            param!("flags", Int = DefaultSpec::Int(0)),
            param!("prefix", Str = DefaultSpec::Str("")),
        ],
        None,
        Int,
        "Imports array entries as variables in the current scope.",
        "function.extract"
    ),
    contract!(
        "filter_var",
        System,
        Filter,
        [
            param!("value", Mixed),
            param!("filter", Int = DefaultSpec::Int(516)),
            param!("options", Mixed = DefaultSpec::Int(0)),
        ],
        None,
        Mixed,
        "Filters a variable with a specified filter.",
        "function.filter-var"
    ),
    contract!(
        "get_debug_type",
        Types,
        Standard,
        [param!("value", Mixed)],
        None,
        Str,
        "Returns a debug-oriented PHP type or class name.",
        "function.get-debug-type"
    ),
    contract!(
        "header_remove",
        System,
        Standard,
        [param!("name", Str = DefaultSpec::Null)],
        None,
        Void,
        "Removes one or all pending HTTP response headers.",
        "function.header-remove"
    ),
    contract!(
        "headers_sent",
        System,
        Standard,
        [
            param!(ref "filename", Mixed = DefaultSpec::Null),
            param!(ref "line", Mixed = DefaultSpec::Null),
        ],
        None,
        Bool,
        "Reports whether output has already committed response headers.",
        "function.headers-sent"
    ),
    contract!(
        "headers_list",
        System,
        Standard,
        [],
        None,
        Array,
        "Returns the pending response headers as `Name: value` lines.",
        "function.headers-list"
    ),
    contract!(
        "preg_grep",
        System,
        Pcre,
        [
            param!("pattern", Str),
            param!("array", Mixed),
            param!("flags", Int = DefaultSpec::Int(0)),
        ],
        None,
        Mixed,
        "Returns entries whose values match a regular expression while preserving keys.",
        "function.preg-grep"
    ),
    contract!(
        "preg_last_error",
        System,
        Pcre,
        [],
        None,
        Int,
        "Returns the error code of the last PCRE regex execution.",
        "function.preg-last-error"
    ),
    contract!(
        "preg_last_error_msg",
        System,
        Pcre,
        [],
        None,
        Str,
        "Returns the error message of the last PCRE regex execution.",
        "function.preg-last-error-msg"
    ),
    contract!(
        "setlocale",
        System,
        Standard,
        [param!("category", Int), param!("locales", Mixed)],
        Some(VariadicSpec::value("rest")),
        Mixed,
        "Sets locale information for the process.",
        "function.setlocale"
    ),
    contract!(
        "unpack",
        String,
        Standard,
        [
            param!("format", Str),
            param!("string", Str),
            param!("offset", Int = DefaultSpec::Int(0)),
        ],
        None,
        Mixed,
        "Unpacks binary data according to a format string.",
        "function.unpack"
    ),
];
