//! Purpose:
//! Declares the core `password` extension's Argon2 provider as shared builtin contracts.
//!
//! Called from:
//! - `crate::registry` while assembling the authoritative builtin catalog.
//! - The sodium prelude detector and `elephc_builtin_contract` support tables.
//!
//! Key details:
//! - These functions are PHP-visible under the `standard` module, matching the PHP 8.5
//!   baseline. The AOT route is `PreludeProvided`: the sodium prelude declares their
//!   synthetic bodies, and the function set is injected when a program names them.
//! - bcrypt is advertised only through `password_algos()`/`password_get_info()`; the
//!   Argon2i/Argon2id provider is implemented over `elephc_sodium`.
//! - Entries stay in canonical lowercase name order, as the registry requires.

use crate::{Area, BuiltinContract, BuiltinId, BuiltinKind, DefaultSpec, ParamSpec, PhpModule, TypeSpec};

/// Canonical PHP names owned by the password provider, in stable order.
pub const FUNCTIONS: &[&str] = &[
    "password_algos",
    "password_get_info",
    "password_hash",
    "password_needs_rehash",
    "password_verify",
];

/// Returns the canonical names as an iterator for requirements/parity joins.
pub fn contract_names() -> impl Iterator<Item = &'static str> {
    FUNCTIONS.iter().copied()
}

/// One ordinary by-value parameter.
macro_rules! param {
    ($name:literal, $ty:expr) => {
        ParamSpec { name: $name, ty: $ty, default: None, by_ref: false }
    };
    ($name:literal, $ty:expr, $default:expr) => {
        ParamSpec { name: $name, ty: $ty, default: Some($default), by_ref: false }
    };
}

/// One password-provider contract.
macro_rules! password_contract {
    ($name:literal, [$($param:expr),*], $returns:expr, $summary:literal) => {
        BuiltinContract {
            id: BuiltinId::from_canonical_name($name),
            name: $name,
            area: Area::Password,
            module: PhpModule::Standard,
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

/// The `password` provider's PHP-visible contracts.
pub(crate) static CONTRACTS: &[BuiltinContract] = &[
    password_contract!("password_algos", [], TypeSpec::Array, "Returns the supported password hashing algorithms."),
    password_contract!("password_get_info", [param!("hash", TypeSpec::Str)], TypeSpec::Array, "Returns information about a password hash."),
    password_contract!("password_hash", [param!("password", TypeSpec::Str), param!("algo", TypeSpec::Union(&[TypeSpec::Str, TypeSpec::Int, TypeSpec::Null])), param!("options", TypeSpec::Array, DefaultSpec::EmptyArray)], TypeSpec::Str, "Hashes a password."),
    password_contract!("password_needs_rehash", [param!("hash", TypeSpec::Str), param!("algo", TypeSpec::Union(&[TypeSpec::Str, TypeSpec::Int, TypeSpec::Null])), param!("options", TypeSpec::Array, DefaultSpec::EmptyArray)], TypeSpec::Bool, "Reports whether a password hash needs rehashing."),
    password_contract!("password_verify", [param!("password", TypeSpec::Str), param!("hash", TypeSpec::Str)], TypeSpec::Bool, "Verifies a password against a hash."),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the list and contract array agree and stay alphabetical.
    #[test]
    fn contracts_match_the_function_list() {
        assert_eq!(CONTRACTS.len(), FUNCTIONS.len());
        for (contract, name) in CONTRACTS.iter().zip(FUNCTIONS) {
            assert_eq!(contract.name, *name);
            assert_eq!(contract.module, PhpModule::Standard);
        }
        let mut sorted = FUNCTIONS.to_vec();
        sorted.sort_unstable();
        assert_eq!(sorted, FUNCTIONS);
    }
}
