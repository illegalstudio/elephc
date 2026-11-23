//! Purpose:
//! Canonical contracts for PHP's `ext/filter` procedural function surface.
//!
//! Called from:
//! - `crate::registry` when assembling the complete shared contract catalog.
//!
//! Key details:
//! - Every entry is `BuiltinKind::PreludeProvided`: the injected filter prelude declares
//!   the PHP-visible wrappers over the `elephc_filter` bridge.
//! - Parameter types spell PHP 8.5's `filter.stub.php`; the prelude widens several of them
//!   to `mixed` so its explicit weak-coercion adapter can match internal-filter diagnostics.
//! - The constants and the two `Filter\*` exceptions live in the shared constant and class
//!   catalogs; only functions are listed here.

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

/// Builds one filter-family contract.
macro_rules! filter_contract {
    (
        $name:literal,
        [$($param:expr),* $(,)?], $returns:expr, $summary:literal
        $(,)?
    ) => {
        BuiltinContract {
            id: BuiltinId::from_canonical_name($name),
            name: $name,
            area: Area::Filter,
            module: PhpModule::Filter,
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

/// `array|int $options` on `filter_var()`, `filter_input()` and `filter_var_array()`.
const OPTIONS: TypeSpec = TypeSpec::Union(&[TypeSpec::Array, TypeSpec::Int]);

/// The `ext/filter` function contracts, in PHP stub order.
pub(crate) static CONTRACTS: &[BuiltinContract] = &[
    filter_contract!(
        "filter_has_var",
        [
            param!("input_type", TypeSpec::Int),
            param!("var_name", TypeSpec::Str),
        ],
        TypeSpec::Bool,
        "Checks whether a request input of the given type contains the named variable."
    ),
    filter_contract!(
        "filter_id",
        [param!("name", TypeSpec::Str)],
        TypeSpec::Union(&[TypeSpec::Int, TypeSpec::False]),
        "Returns the filter id for the given filter name, or false when the name is unknown."
    ),
    filter_contract!(
        "filter_input",
        [
            param!("type", TypeSpec::Int),
            param!("var_name", TypeSpec::Str),
            param!("filter", TypeSpec::Int, DefaultSpec::Constant("FILTER_DEFAULT")),
            param!("options", OPTIONS, DefaultSpec::Int(0)),
        ],
        TypeSpec::Mixed,
        "Returns the filtered request input for the named variable, or null when it is absent."
    ),
    filter_contract!(
        "filter_input_array",
        [
            param!("type", TypeSpec::Int),
            param!(
                "options",
                OPTIONS,
                DefaultSpec::Constant("FILTER_DEFAULT")
            ),
            param!("add_empty", TypeSpec::Bool, DefaultSpec::Bool(true)),
        ],
        TypeSpec::Union(&[TypeSpec::Array, TypeSpec::False, TypeSpec::Null]),
        "Returns filtered request inputs, keyed by the variable names in the options definition."
    ),
    filter_contract!(
        "filter_list",
        [],
        TypeSpec::Array,
        "Returns the list of all supported filter names."
    ),
    filter_contract!(
        "filter_var",
        [
            param!("value", TypeSpec::Mixed),
            param!("filter", TypeSpec::Int, DefaultSpec::Constant("FILTER_DEFAULT")),
            param!("options", OPTIONS, DefaultSpec::Int(0)),
        ],
        TypeSpec::Mixed,
        "Filters a variable with the given filter and returns the filtered value."
    ),
    filter_contract!(
        "filter_var_array",
        [
            param!("array", TypeSpec::Array),
            param!("options", OPTIONS, DefaultSpec::Constant("FILTER_DEFAULT")),
            param!("add_empty", TypeSpec::Bool, DefaultSpec::Bool(true)),
        ],
        TypeSpec::Union(&[TypeSpec::Array, TypeSpec::False, TypeSpec::Null]),
        "Filters several values from one array using a definition of filters and flags."
    ),
];

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Structural guards for the filter function contracts.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Catches an accidental name/arity drift from PHP 8.5's filter stub.

    use super::*;

    /// The procedural surface is exactly the seven functions PHP 8.5 exposes.
    #[test]
    fn declares_the_full_php_85_surface() {
        assert_eq!(CONTRACTS.len(), 7);
        let names: Vec<&str> = CONTRACTS.iter().map(|contract| contract.name).collect();
        for required in [
            "filter_has_var",
            "filter_id",
            "filter_input",
            "filter_input_array",
            "filter_list",
            "filter_var",
            "filter_var_array",
        ] {
            assert!(names.contains(&required), "missing {required}");
        }
    }

    /// Every contract belongs to `ext/filter` and is prelude-provided.
    #[test]
    fn every_contract_is_a_filter_prelude_surface() {
        for contract in CONTRACTS {
            assert_eq!(contract.area, Area::Filter);
            assert_eq!(contract.module, PhpModule::Filter);
            assert_eq!(contract.kind, BuiltinKind::PreludeProvided);
            assert!(!contract.extension && !contract.internal);
        }
    }
}
