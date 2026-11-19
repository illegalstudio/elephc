//! Purpose:
//! Canonical contracts for PHP's deprecated `ext/zip` procedural API declared by the
//! injected zip prelude over `ZipArchive`.
//!
//! Called from:
//! - `crate::registry` when assembling the complete shared contract catalog.
//!
//! Key details:
//! - Every entry is `BuiltinKind::PreludeProvided`: the compiler implements these names as
//!   elephc-PHP declarations injected by `elephc::zip_prelude`, which call the
//!   `elephc_zip` bridge through the OO API.
//! - php-src's stubs use `resource` parameters; the neutral catalog spells them `mixed`
//!   because it has no resource type, and the parity gate accepts that widening.
//! - All ten functions are deprecated since PHP 8.0 in favour of `ZipArchive`.

use crate::{
    Area, BuiltinContract, BuiltinDeprecation, BuiltinId, BuiltinKind, DefaultSpec, ParamSpec,
    PhpModule, PhpVersion, TypeSpec,
};

/// Builds one by-value parameter, optionally with a PHP default.
macro_rules! param {
    ($name:literal, $ty:expr) => {
        ParamSpec {
            name: $name,
            ty: $ty,
            default: None,
            by_ref: false,
        }
    };
    ($name:literal, $ty:expr, $default:expr) => {
        ParamSpec {
            name: $name,
            ty: $ty,
            default: Some($default),
            by_ref: false,
        }
    };
}

/// Builds one deprecated procedural zip contract.
macro_rules! zip_contract {
    ($name:literal, [$($param:expr),* $(,)?], $returns:expr, $summary:literal) => {
        BuiltinContract {
            id: BuiltinId::from_canonical_name($name),
            name: $name,
            area: Area::Io,
            module: PhpModule::Zip,
            since: None,
            kind: BuiltinKind::PreludeProvided,
            params: &[$($param),*],
            variadic: None,
            variadic_by_ref: false,
            min_args: None,
            max_args: None,
            arity_error: None,
            returns: $returns,
            by_ref_return: false,
            summary: $summary,
            examples: &[],
            php_manual: None,
            deprecation: Some(BuiltinDeprecation::since(
                PhpVersion::Php80,
                "use the ZipArchive API instead",
            )),
            extension: false,
            internal: false,
            requirements: &[],
        }
    };
}

/// The deprecated procedural zip contracts, in PHP stub order.
pub(crate) static CONTRACTS: &[BuiltinContract] = &[
    zip_contract!(
        "zip_open",
        [param!("filename", TypeSpec::Str)],
        TypeSpec::Mixed,
        "Opens a zip archive and returns a directory resource, or a libzip error code."
    ),
    zip_contract!(
        "zip_close",
        [param!("zip", TypeSpec::Mixed)],
        TypeSpec::Void,
        "Closes a zip directory resource opened by zip_open()."
    ),
    zip_contract!(
        "zip_read",
        [param!("zip", TypeSpec::Mixed)],
        TypeSpec::Mixed,
        "Returns the next entry resource in an open zip directory, or false at the end."
    ),
    zip_contract!(
        "zip_entry_open",
        [
            param!("zip_dp", TypeSpec::Mixed),
            param!("zip_entry", TypeSpec::Mixed),
            param!("mode", TypeSpec::Str, DefaultSpec::Str("rb")),
        ],
        TypeSpec::Bool,
        "Opens a zip entry for reading."
    ),
    zip_contract!(
        "zip_entry_close",
        [param!("zip_entry", TypeSpec::Mixed)],
        TypeSpec::Bool,
        "Closes an open zip entry resource."
    ),
    zip_contract!(
        "zip_entry_read",
        [
            param!("zip_entry", TypeSpec::Mixed),
            param!("len", TypeSpec::Int, DefaultSpec::Int(1024)),
        ],
        TypeSpec::Union(&[TypeSpec::Str, TypeSpec::False]),
        "Reads up to len bytes from an open zip entry."
    ),
    zip_contract!(
        "zip_entry_name",
        [param!("zip_entry", TypeSpec::Mixed)],
        TypeSpec::Union(&[TypeSpec::Str, TypeSpec::False]),
        "Returns the name of a zip entry."
    ),
    zip_contract!(
        "zip_entry_compressedsize",
        [param!("zip_entry", TypeSpec::Mixed)],
        TypeSpec::Union(&[TypeSpec::Int, TypeSpec::False]),
        "Returns the compressed size of a zip entry."
    ),
    zip_contract!(
        "zip_entry_filesize",
        [param!("zip_entry", TypeSpec::Mixed)],
        TypeSpec::Union(&[TypeSpec::Int, TypeSpec::False]),
        "Returns the uncompressed size of a zip entry."
    ),
    zip_contract!(
        "zip_entry_compressionmethod",
        [param!("zip_entry", TypeSpec::Mixed)],
        TypeSpec::Union(&[TypeSpec::Str, TypeSpec::False]),
        "Returns the compression method of a zip entry."
    ),
];
