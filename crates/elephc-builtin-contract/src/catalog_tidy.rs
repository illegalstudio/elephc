//! Purpose:
//! Canonical contracts for PHP's `ext/tidy` procedural function surface.
//!
//! Called from:
//! - `crate::registry` when assembling the complete shared contract catalog.
//!
//! Key details:
//! - Every entry is `BuiltinKind::PreludeProvided`: the injected tidy prelude
//!   declares the PHP-visible wrappers over the `elephc_tidy` bridge extern block.
//! - Parameter types spell PHP 8.5's `tidy.stub.php`; the `tidy` / `tidyNode`
//!   class types are `Mixed` because the catalog has no class-type spelling.
//! - The four `tidy_get_*` node accessors and all config/status accessors are in
//!   this list; `tidy` and `tidyNode` methods live on the class contracts.

use crate::{
    Area, BuiltinContract, BuiltinId, BuiltinKind, DefaultSpec, ParamSpec, PhpModule, TypeSpec,
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

/// Builds one tidy-family contract.
macro_rules! tidy_contract {
    (
        $name:literal,
        [$($param:expr),* $(,)?], $returns:expr, $summary:literal
        $(,)?
    ) => {
        BuiltinContract {
            id: BuiltinId::from_canonical_name($name),
            name: $name,
            area: Area::Tidy,
            module: PhpModule::Tidy,
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
            deprecation: None,
            extension: false,
            internal: false,
            requirements: &[],
        }
    };
}

/// The `array|string|null $config` parameter shared by every parse/repair entry.
const CONFIG: TypeSpec = TypeSpec::Union(&[TypeSpec::Array, TypeSpec::Str, TypeSpec::Null]);

/// The `ext/tidy` function contracts, in PHP stub order.
pub(crate) static CONTRACTS: &[BuiltinContract] = &[
    tidy_contract!(
        "tidy_parse_string",
        [
            param!("string", TypeSpec::Str),
            param!("config", CONFIG, DefaultSpec::Null),
            param!("encoding", TypeSpec::Nullable(&TypeSpec::Str), DefaultSpec::Null),
        ],
        TypeSpec::Mixed,
        "Parses a string of HTML, applies the optional configuration and returns a tidy document object."
    ),
    tidy_contract!(
        "tidy_get_error_buffer",
        [param!("tidy", TypeSpec::Mixed)],
        TypeSpec::Union(&[TypeSpec::Str, TypeSpec::False]),
        "Returns the warnings and errors produced while parsing the document, or false when there are none."
    ),
    tidy_contract!(
        "tidy_get_output",
        [param!("tidy", TypeSpec::Mixed)],
        TypeSpec::Str,
        "Returns the repaired markup of the document."
    ),
    tidy_contract!(
        "tidy_parse_file",
        [
            param!("filename", TypeSpec::Str),
            param!("config", CONFIG, DefaultSpec::Null),
            param!("encoding", TypeSpec::Nullable(&TypeSpec::Str), DefaultSpec::Null),
            param!("useIncludePath", TypeSpec::Bool, DefaultSpec::Bool(false)),
        ],
        TypeSpec::Mixed,
        "Parses an HTML file, optionally searching the include path, and returns a tidy document object."
    ),
    tidy_contract!(
        "tidy_clean_repair",
        [param!("tidy", TypeSpec::Mixed)],
        TypeSpec::Bool,
        "Executes the configured cleanup and repair operations on the document."
    ),
    tidy_contract!(
        "tidy_repair_string",
        [
            param!("string", TypeSpec::Str),
            param!("config", CONFIG, DefaultSpec::Null),
            param!("encoding", TypeSpec::Nullable(&TypeSpec::Str), DefaultSpec::Null),
        ],
        TypeSpec::Union(&[TypeSpec::Str, TypeSpec::False]),
        "Repairs a string of HTML and returns the repaired markup."
    ),
    tidy_contract!(
        "tidy_repair_file",
        [
            param!("filename", TypeSpec::Str),
            param!("config", CONFIG, DefaultSpec::Null),
            param!("encoding", TypeSpec::Nullable(&TypeSpec::Str), DefaultSpec::Null),
            param!("useIncludePath", TypeSpec::Bool, DefaultSpec::Bool(false)),
        ],
        TypeSpec::Union(&[TypeSpec::Str, TypeSpec::False]),
        "Repairs an HTML file and returns the repaired markup."
    ),
    tidy_contract!(
        "tidy_diagnose",
        [param!("tidy", TypeSpec::Mixed)],
        TypeSpec::Bool,
        "Runs the configured accessibility and compatibility diagnostics on a parsed document."
    ),
    tidy_contract!(
        "tidy_get_release",
        [],
        TypeSpec::Str,
        "Returns the release date of the linked libtidy."
    ),
    tidy_contract!(
        "tidy_get_opt_doc",
        [param!("tidy", TypeSpec::Mixed), param!("option", TypeSpec::Str)],
        TypeSpec::Union(&[TypeSpec::Str, TypeSpec::False]),
        "Returns libtidy's documentation for a configuration option, or false when it has none."
    ),
    tidy_contract!(
        "tidy_get_config",
        [param!("tidy", TypeSpec::Mixed)],
        TypeSpec::Array,
        "Returns the document's complete configuration as an associative array."
    ),
    tidy_contract!(
        "tidy_get_status",
        [param!("tidy", TypeSpec::Mixed)],
        TypeSpec::Int,
        "Returns the document status: 0 for clean, 1 for warnings, 2 for errors."
    ),
    tidy_contract!(
        "tidy_get_html_ver",
        [param!("tidy", TypeSpec::Mixed)],
        TypeSpec::Int,
        "Returns the detected HTML version of the document."
    ),
    tidy_contract!(
        "tidy_is_xhtml",
        [param!("tidy", TypeSpec::Mixed)],
        TypeSpec::Bool,
        "Returns whether the document was detected as XHTML."
    ),
    tidy_contract!(
        "tidy_is_xml",
        [param!("tidy", TypeSpec::Mixed)],
        TypeSpec::Bool,
        "Returns whether the document was detected as generic XML."
    ),
    tidy_contract!(
        "tidy_error_count",
        [param!("tidy", TypeSpec::Mixed)],
        TypeSpec::Int,
        "Returns the number of errors encountered for the document."
    ),
    tidy_contract!(
        "tidy_warning_count",
        [param!("tidy", TypeSpec::Mixed)],
        TypeSpec::Int,
        "Returns the number of warnings encountered for the document."
    ),
    tidy_contract!(
        "tidy_access_count",
        [param!("tidy", TypeSpec::Mixed)],
        TypeSpec::Int,
        "Returns the number of accessibility warnings encountered for the document."
    ),
    tidy_contract!(
        "tidy_config_count",
        [param!("tidy", TypeSpec::Mixed)],
        TypeSpec::Int,
        "Returns the number of configuration errors encountered for the document."
    ),
    tidy_contract!(
        "tidy_getopt",
        [param!("tidy", TypeSpec::Mixed), param!("option", TypeSpec::Str)],
        TypeSpec::Union(&[TypeSpec::Str, TypeSpec::Int, TypeSpec::Bool]),
        "Returns the current value of one configuration option."
    ),
    tidy_contract!(
        "tidy_get_root",
        [param!("tidy", TypeSpec::Mixed)],
        TypeSpec::Mixed,
        "Returns the root node of the document tree, or null when there is none."
    ),
    tidy_contract!(
        "tidy_get_html",
        [param!("tidy", TypeSpec::Mixed)],
        TypeSpec::Mixed,
        "Returns the document's html element node, or null when there is none."
    ),
    tidy_contract!(
        "tidy_get_head",
        [param!("tidy", TypeSpec::Mixed)],
        TypeSpec::Mixed,
        "Returns the document's head element node, or null when there is none."
    ),
    tidy_contract!(
        "tidy_get_body",
        [param!("tidy", TypeSpec::Mixed)],
        TypeSpec::Mixed,
        "Returns the document's body element node, or null when there is none."
    ),
];

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Structural guards for the tidy function contracts.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Catches an accidental name/arity drift from PHP 8.5's tidy stub.

    use super::*;

    /// The procedural surface is exactly the 24 functions PHP 8.5 exposes.
    #[test]
    fn declares_the_full_php_85_surface() {
        assert_eq!(CONTRACTS.len(), 24);
        let names: Vec<&str> = CONTRACTS.iter().map(|contract| contract.name).collect();
        for required in [
            "tidy_parse_string",
            "tidy_get_error_buffer",
            "tidy_get_output",
            "tidy_parse_file",
            "tidy_clean_repair",
            "tidy_repair_string",
            "tidy_repair_file",
            "tidy_diagnose",
            "tidy_get_release",
            "tidy_get_opt_doc",
            "tidy_get_config",
            "tidy_get_status",
            "tidy_get_html_ver",
            "tidy_is_xhtml",
            "tidy_is_xml",
            "tidy_error_count",
            "tidy_warning_count",
            "tidy_access_count",
            "tidy_config_count",
            "tidy_getopt",
            "tidy_get_root",
            "tidy_get_html",
            "tidy_get_head",
            "tidy_get_body",
        ] {
            assert!(names.contains(&required), "missing {required}");
        }
    }

    /// Every contract belongs to `ext/tidy` and is prelude-provided.
    #[test]
    fn every_contract_is_prelude_provided() {
        for contract in CONTRACTS {
            assert_eq!(contract.area, Area::Tidy);
            assert_eq!(contract.module, PhpModule::Tidy);
            assert_eq!(contract.kind, BuiltinKind::PreludeProvided);
        }
    }
}
