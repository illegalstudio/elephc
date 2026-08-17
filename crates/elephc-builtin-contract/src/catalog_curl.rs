//! Purpose:
//! Canonical contracts for the PHP-visible `ext/curl` surface (`curl_*`), compiled
//! only when the `curl` Cargo feature is enabled.
//!
//! Called from:
//! - `crate::registry` when assembling the complete shared contract catalog.
//!
//! Key details:
//! - Every entry is `BuiltinKind::PreludeProvided`: the compiler implements these
//!   names as elephc-PHP wrappers injected by `elephc::curl_prelude`, which call the
//!   internal `__elephc_curl_*` registry builtins in `catalog_data`.
//! - THIS FILE IS FEATURE-GATED, and the gate is load-bearing rather than cosmetic.
//!   `crate::registry::contracts()` is the input to both backends' coverage audits,
//!   and Magician's audit requires exactly one eval binding per eval-supported
//!   contract. Magician's `ext/curl` eval homes are themselves behind that crate's
//!   own `curl` feature (see `elephc_magician::interpreter::builtins::curl`'s module
//!   doc: unconditional `elephc_curl_*` references would force every `eval()`-using
//!   program to link the pinned native libcurl). Publishing these contracts
//!   unconditionally would therefore make the default curl-free
//!   `libelephc_magician.a` fail its own coverage assertion at registry init.
//!   `elephc-magician`'s `curl` feature turns this one on, so the two move together.
//! - Because the generated-docs exporter (`tools/gen_builtins.rs`) links Magician
//!   WITHOUT its `curl` feature, these entries stay out of the generated PHP builtin
//!   catalog, exactly as they did before the shared-contract migration. The curl
//!   surface is documented by hand in `docs/php/curl.md`.

use crate::{Area, BuiltinContract, BuiltinId, BuiltinKind, DefaultSpec, ParamSpec, TypeSpec};

/// Builds one by-value parameter, optionally with a PHP default.
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
}

/// Builds one by-reference parameter, optionally with a PHP default.
macro_rules! by_ref_param {
    ($name:literal, $ty:ident) => {
        ParamSpec {
            name: $name,
            ty: TypeSpec::$ty,
            default: None,
            by_ref: true,
        }
    };
    ($name:literal, $ty:ident = $default:expr) => {
        ParamSpec {
            name: $name,
            ty: TypeSpec::$ty,
            default: Some($default),
            by_ref: true,
        }
    };
}

/// Builds one prelude-provided `ext/curl` contract.
macro_rules! curl_surface {
    ($name:literal, [$($param:expr),* $(,)?], $returns:ident, $summary:literal $(,)?) => {
        BuiltinContract {
            id: BuiltinId::from_canonical_name($name),
            name: $name,
            area: Area::Curl,
            kind: BuiltinKind::PreludeProvided,
            params: &[$($param),*],
            variadic: None,
            min_args: None,
            max_args: None,
            arity_error: None,
            returns: TypeSpec::$returns,
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

pub(crate) static CURL_CONTRACTS: &[BuiltinContract] = &[
    curl_surface!(
        "curl_close",
        [param!("handle", Mixed)],
        Void,
        "Closes a cURL session."
    ),
    curl_surface!(
        "curl_copy_handle",
        [param!("handle", Mixed)],
        Mixed,
        "Copies a cURL handle along with all of its preferences."
    ),
    curl_surface!(
        "curl_errno",
        [param!("handle", Mixed)],
        Int,
        "Returns the last error number for a cURL session."
    ),
    curl_surface!(
        "curl_error",
        [param!("handle", Mixed)],
        Str,
        "Returns a string describing the last cURL error."
    ),
    curl_surface!(
        "curl_escape",
        [param!("handle", Mixed), param!("string", Str)],
        Str,
        "URL-encodes a string with the given cURL handle."
    ),
    curl_surface!(
        "curl_exec",
        [param!("handle", Mixed)],
        Mixed,
        "Performs a cURL session."
    ),
    curl_surface!(
        "curl_getinfo",
        [
            param!("handle", Mixed),
            param!("option", Int = DefaultSpec::Null),
        ],
        Mixed,
        "Gets information about the last transfer."
    ),
    curl_surface!(
        "curl_init",
        [param!("url", Str = DefaultSpec::Null)],
        Mixed,
        "Initializes a cURL session."
    ),
    curl_surface!(
        "curl_multi_add_handle",
        [param!("multi_handle", Mixed), param!("handle", Mixed)],
        Int,
        "Adds a normal cURL handle to a cURL multi handle."
    ),
    curl_surface!(
        "curl_multi_close",
        [param!("multi_handle", Mixed)],
        Void,
        "Closes a set of cURL handles."
    ),
    curl_surface!(
        "curl_multi_errno",
        [param!("multi_handle", Mixed)],
        Int,
        "Returns the last multi curl error number."
    ),
    curl_surface!(
        "curl_multi_exec",
        [
            param!("multi_handle", Mixed),
            by_ref_param!("still_running", Int),
        ],
        Int,
        "Runs the sub-connections of the current cURL handle."
    ),
    curl_surface!(
        "curl_multi_get_handles",
        [param!("multi_handle", Mixed)],
        Mixed,
        "Returns the cURL handles currently attached to a cURL multi handle."
    ),
    curl_surface!(
        "curl_multi_getcontent",
        [param!("handle", Mixed)],
        Str,
        "Returns the content of a cURL handle if CURLOPT_RETURNTRANSFER is set."
    ),
    curl_surface!(
        "curl_multi_info_read",
        [
            param!("multi_handle", Mixed),
            by_ref_param!("queued_messages", Int = DefaultSpec::Null),
        ],
        Mixed,
        "Gets information about the current transfers."
    ),
    curl_surface!("curl_multi_init", [], Mixed, "Returns a new cURL multi handle."),
    curl_surface!(
        "curl_multi_remove_handle",
        [param!("multi_handle", Mixed), param!("handle", Mixed)],
        Int,
        "Removes a multi handle from a set of cURL handles."
    ),
    curl_surface!(
        "curl_multi_select",
        [
            param!("multi_handle", Mixed),
            param!("timeout", Float = DefaultSpec::Float(1.0)),
        ],
        Int,
        "Waits until there is activity on any cURL multi connection."
    ),
    curl_surface!(
        "curl_multi_setopt",
        [
            param!("multi_handle", Mixed),
            param!("option", Int),
            param!("value", Mixed),
        ],
        Bool,
        "Sets an option on a cURL multi handle."
    ),
    curl_surface!(
        "curl_multi_strerror",
        [param!("error_code", Int)],
        Str,
        "Returns string describing error code."
    ),
    curl_surface!(
        "curl_pause",
        [param!("handle", Mixed), param!("flags", Int)],
        Int,
        "Pauses and unpauses a connection."
    ),
    curl_surface!(
        "curl_reset",
        [param!("handle", Mixed)],
        Void,
        "Resets all options of a libcurl session handle."
    ),
    curl_surface!(
        "curl_setopt",
        [
            param!("handle", Mixed),
            param!("option", Int),
            param!("value", Mixed),
        ],
        Bool,
        "Sets an option for a cURL transfer."
    ),
    curl_surface!(
        "curl_setopt_array",
        [param!("handle", Mixed), param!("options", Mixed)],
        Bool,
        "Sets multiple options for a cURL transfer."
    ),
    curl_surface!(
        "curl_share_close",
        [param!("share_handle", Mixed)],
        Void,
        "Closes a cURL share handle."
    ),
    curl_surface!(
        "curl_share_errno",
        [param!("share_handle", Mixed)],
        Int,
        "Returns the last share curl error number."
    ),
    curl_surface!("curl_share_init", [], Mixed, "Initializes a cURL share handle."),
    curl_surface!(
        "curl_share_init_persistent",
        [param!("share_options", Mixed)],
        Mixed,
        "Initializes a persistent cURL share handle."
    ),
    curl_surface!(
        "curl_share_setopt",
        [
            param!("share_handle", Mixed),
            param!("option", Int),
            param!("value", Mixed),
        ],
        Bool,
        "Sets an option for a cURL share handle."
    ),
    curl_surface!(
        "curl_share_strerror",
        [param!("error_code", Int)],
        Str,
        "Returns string describing the given error code."
    ),
    curl_surface!(
        "curl_strerror",
        [param!("error_code", Int)],
        Str,
        "Returns string describing the given error code."
    ),
    curl_surface!(
        "curl_unescape",
        [param!("handle", Mixed), param!("string", Str)],
        Str,
        "Decodes the given URL-encoded string."
    ),
    curl_surface!(
        "curl_upkeep",
        [param!("handle", Mixed)],
        Bool,
        "Performs any connection upkeep checks."
    ),
    curl_surface!("curl_version", [], Mixed, "Gets cURL version information."),
];
