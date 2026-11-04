//! Purpose:
//! Declares the PHP-visible `ext/gmp` procedural surface as shared builtin contracts.
//!
//! Called from:
//! - `crate::registry` while assembling the authoritative builtin catalog.
//! - AOT and Magician home-file joins keyed by the same stable `BuiltinId`s.
//! - The GMP prelude detector, which uses [`FUNCTIONS`] as its exact name set.
//!
//! Key details:
//! - Signatures are php-src 8.5's `GMP|string|int` surface. The checker defers
//!   value/coercion diagnostics to the runtime helper, so these types are for
//!   arity, named arguments, docs, reflection metadata and strict-PHP visibility.
//! - `GMP` has no dedicated `TypeSpec`; class parameters use `Object`, while
//!   the AOT home files resolve their exact result type to `PhpType::Object("GMP")`.
//! - Entries stay in canonical lowercase name order, as the registry requires.

use crate::{
    Area, BuiltinContract, BuiltinId, BuiltinKind, DefaultSpec, ParamSpec, PhpModule, TypeSpec,
};

/// The libgmp version pinned by the managed native package and reported by `GMP_VERSION`.
pub const GMP_VERSION: &str = "6.3.0";

/// Canonical PHP names owned by the GMP extension, in stable order.
pub const FUNCTIONS: &[&str] = &[
    "gmp_abs",
    "gmp_add",
    "gmp_and",
    "gmp_binomial",
    "gmp_clrbit",
    "gmp_cmp",
    "gmp_com",
    "gmp_div",
    "gmp_div_q",
    "gmp_div_qr",
    "gmp_div_r",
    "gmp_divexact",
    "gmp_export",
    "gmp_fact",
    "gmp_gcd",
    "gmp_gcdext",
    "gmp_hamdist",
    "gmp_import",
    "gmp_init",
    "gmp_intval",
    "gmp_invert",
    "gmp_jacobi",
    "gmp_kronecker",
    "gmp_lcm",
    "gmp_legendre",
    "gmp_mod",
    "gmp_mul",
    "gmp_neg",
    "gmp_nextprime",
    "gmp_or",
    "gmp_perfect_power",
    "gmp_perfect_square",
    "gmp_popcount",
    "gmp_pow",
    "gmp_powm",
    "gmp_prob_prime",
    "gmp_random_bits",
    "gmp_random_range",
    "gmp_random_seed",
    "gmp_root",
    "gmp_rootrem",
    "gmp_scan0",
    "gmp_scan1",
    "gmp_setbit",
    "gmp_sign",
    "gmp_sqrt",
    "gmp_sqrtrem",
    "gmp_strval",
    "gmp_sub",
    "gmp_testbit",
    "gmp_xor",
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

/// The PHP `GMP|string|int` operand union.
const OPERAND: TypeSpec = TypeSpec::Union(&[TypeSpec::Object, TypeSpec::Str, TypeSpec::Int]);
/// `gmp_init()`'s narrower `string|int` operand.
const INIT_OPERAND: TypeSpec = TypeSpec::Union(&[TypeSpec::Str, TypeSpec::Int]);
/// A fresh `GMP` object.
const GMP_RESULT: TypeSpec = TypeSpec::Object;
/// `GMP_MSW_FIRST | GMP_NATIVE_ENDIAN`, whose stable values are 1 and 16.
const DEFAULT_FLAGS: DefaultSpec = DefaultSpec::Int(17);

/// One procedural GMP contract.
macro_rules! gmp_contract {
    ($name:literal, [$($param:expr),*], $returns:expr, $summary:literal) => {
        BuiltinContract {
            id: BuiltinId::from_canonical_name($name),
            name: $name,
            area: Area::Math,
            module: PhpModule::Gmp,
            since: None,
            kind: BuiltinKind::Function,
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

pub(crate) static CONTRACTS: &[BuiltinContract] = &[
    gmp_contract!("gmp_abs", [param!("num", OPERAND)], GMP_RESULT, "Absolute value of a GMP number."),
    gmp_contract!("gmp_add", [param!("num1", OPERAND), param!("num2", OPERAND)], GMP_RESULT, "Adds two GMP numbers."),
    gmp_contract!("gmp_and", [param!("num1", OPERAND), param!("num2", OPERAND)], GMP_RESULT, "Bitwise AND of two GMP numbers."),
    gmp_contract!("gmp_binomial", [param!("num", OPERAND), param!("k", TypeSpec::Int)], GMP_RESULT, "Computes a binomial coefficient."),
    gmp_contract!("gmp_clrbit", [param!("num", TypeSpec::Object), param!("index", TypeSpec::Int)], TypeSpec::Void, "Clears a bit in a GMP number."),
    gmp_contract!("gmp_cmp", [param!("num1", OPERAND), param!("num2", OPERAND)], TypeSpec::Int, "Compares two GMP numbers."),
    gmp_contract!("gmp_com", [param!("num", OPERAND)], GMP_RESULT, "One's complement of a GMP number."),
    gmp_contract!("gmp_div", [param!("num1", OPERAND), param!("num2", OPERAND), param!("rounding_mode", TypeSpec::Int, DefaultSpec::Int(0))], GMP_RESULT, "Alias of gmp_div_q()."),
    gmp_contract!("gmp_div_q", [param!("num1", OPERAND), param!("num2", OPERAND), param!("rounding_mode", TypeSpec::Int, DefaultSpec::Int(0))], GMP_RESULT, "Divides two GMP numbers."),
    gmp_contract!("gmp_div_qr", [param!("num1", OPERAND), param!("num2", OPERAND), param!("rounding_mode", TypeSpec::Int, DefaultSpec::Int(0))], TypeSpec::Array, "Divides two GMP numbers and returns quotient and remainder."),
    gmp_contract!("gmp_div_r", [param!("num1", OPERAND), param!("num2", OPERAND), param!("rounding_mode", TypeSpec::Int, DefaultSpec::Int(0))], GMP_RESULT, "Returns the remainder of a GMP division."),
    gmp_contract!("gmp_divexact", [param!("num1", OPERAND), param!("num2", OPERAND)], GMP_RESULT, "Performs an exact GMP division."),
    gmp_contract!("gmp_export", [param!("num", OPERAND), param!("word_size", TypeSpec::Int, DefaultSpec::Int(1)), param!("flags", TypeSpec::Int, DEFAULT_FLAGS)], TypeSpec::Str, "Exports a GMP number as a binary string."),
    gmp_contract!("gmp_fact", [param!("num", OPERAND)], GMP_RESULT, "Computes a factorial."),
    gmp_contract!("gmp_gcd", [param!("num1", OPERAND), param!("num2", OPERAND)], GMP_RESULT, "Greatest common divisor."),
    gmp_contract!("gmp_gcdext", [param!("num1", OPERAND), param!("num2", OPERAND)], TypeSpec::Array, "Extended greatest common divisor."),
    gmp_contract!("gmp_hamdist", [param!("num1", OPERAND), param!("num2", OPERAND)], TypeSpec::Int, "Hamming distance between two GMP numbers."),
    gmp_contract!("gmp_import", [param!("data", TypeSpec::Str), param!("word_size", TypeSpec::Int, DefaultSpec::Int(1)), param!("flags", TypeSpec::Int, DEFAULT_FLAGS)], GMP_RESULT, "Imports a binary string as a GMP number."),
    gmp_contract!("gmp_init", [param!("num", INIT_OPERAND), param!("base", TypeSpec::Int, DefaultSpec::Int(0))], GMP_RESULT, "Creates a GMP number from an integer or string."),
    gmp_contract!("gmp_intval", [param!("num", OPERAND)], TypeSpec::Int, "Converts a GMP number to an integer."),
    gmp_contract!("gmp_invert", [param!("num1", OPERAND), param!("num2", OPERAND)], TypeSpec::Union(&[TypeSpec::Object, TypeSpec::False]), "Modular inverse."),
    gmp_contract!("gmp_jacobi", [param!("num1", OPERAND), param!("num2", OPERAND)], TypeSpec::Int, "Jacobi symbol."),
    gmp_contract!("gmp_kronecker", [param!("num1", OPERAND), param!("num2", OPERAND)], TypeSpec::Int, "Kronecker symbol."),
    gmp_contract!("gmp_lcm", [param!("num1", OPERAND), param!("num2", OPERAND)], GMP_RESULT, "Least common multiple."),
    gmp_contract!("gmp_legendre", [param!("num1", OPERAND), param!("num2", OPERAND)], TypeSpec::Int, "Legendre symbol."),
    gmp_contract!("gmp_mod", [param!("num1", OPERAND), param!("num2", OPERAND)], GMP_RESULT, "Modulo of two GMP numbers."),
    gmp_contract!("gmp_mul", [param!("num1", OPERAND), param!("num2", OPERAND)], GMP_RESULT, "Multiplies two GMP numbers."),
    gmp_contract!("gmp_neg", [param!("num", OPERAND)], GMP_RESULT, "Negates a GMP number."),
    gmp_contract!("gmp_nextprime", [param!("num", OPERAND)], GMP_RESULT, "Returns the next prime greater than a GMP number."),
    gmp_contract!("gmp_or", [param!("num1", OPERAND), param!("num2", OPERAND)], GMP_RESULT, "Bitwise OR of two GMP numbers."),
    gmp_contract!("gmp_perfect_power", [param!("num", OPERAND)], TypeSpec::Bool, "Checks whether a GMP number is a perfect power."),
    gmp_contract!("gmp_perfect_square", [param!("num", OPERAND)], TypeSpec::Bool, "Checks whether a GMP number is a perfect square."),
    gmp_contract!("gmp_popcount", [param!("num", OPERAND)], TypeSpec::Int, "Population count of a GMP number."),
    gmp_contract!("gmp_pow", [param!("num", OPERAND), param!("exponent", TypeSpec::Int)], GMP_RESULT, "Raises a GMP number to an integer power."),
    gmp_contract!("gmp_powm", [param!("num", OPERAND), param!("exponent", OPERAND), param!("modulus", OPERAND)], GMP_RESULT, "Modular exponentiation."),
    gmp_contract!("gmp_prob_prime", [param!("num", OPERAND), param!("repetitions", TypeSpec::Int, DefaultSpec::Int(10))], TypeSpec::Int, "Probable-prime test."),
    gmp_contract!("gmp_random_bits", [param!("bits", TypeSpec::Int)], GMP_RESULT, "Generates a random GMP number with the given bit count."),
    gmp_contract!("gmp_random_range", [param!("min", OPERAND), param!("max", OPERAND)], GMP_RESULT, "Generates a random GMP number in a range."),
    gmp_contract!("gmp_random_seed", [param!("seed", OPERAND)], TypeSpec::Void, "Seeds PHP's GMP random generator."),
    gmp_contract!("gmp_root", [param!("num", OPERAND), param!("nth", TypeSpec::Int)], GMP_RESULT, "Integer nth root."),
    gmp_contract!("gmp_rootrem", [param!("num", OPERAND), param!("nth", TypeSpec::Int)], TypeSpec::Array, "Integer nth root and remainder."),
    gmp_contract!("gmp_scan0", [param!("num1", OPERAND), param!("start", TypeSpec::Int)], TypeSpec::Int, "Finds the first zero bit."),
    gmp_contract!("gmp_scan1", [param!("num1", OPERAND), param!("start", TypeSpec::Int)], TypeSpec::Int, "Finds the first set bit."),
    gmp_contract!("gmp_setbit", [param!("num", TypeSpec::Object), param!("index", TypeSpec::Int), param!("value", TypeSpec::Bool, DefaultSpec::Bool(true))], TypeSpec::Void, "Sets or clears a bit in a GMP number."),
    gmp_contract!("gmp_sign", [param!("num", OPERAND)], TypeSpec::Int, "Sign of a GMP number."),
    gmp_contract!("gmp_sqrt", [param!("num", OPERAND)], GMP_RESULT, "Integer square root."),
    gmp_contract!("gmp_sqrtrem", [param!("num", OPERAND)], TypeSpec::Array, "Integer square root and remainder."),
    gmp_contract!("gmp_strval", [param!("num", OPERAND), param!("base", TypeSpec::Int, DefaultSpec::Int(10))], TypeSpec::Str, "Formats a GMP number in a base."),
    gmp_contract!("gmp_sub", [param!("num1", OPERAND), param!("num2", OPERAND)], GMP_RESULT, "Subtracts two GMP numbers."),
    gmp_contract!("gmp_testbit", [param!("num", OPERAND), param!("index", TypeSpec::Int)], TypeSpec::Bool, "Tests a bit in a GMP number."),
    gmp_contract!("gmp_xor", [param!("num1", OPERAND), param!("num2", OPERAND)], GMP_RESULT, "Bitwise XOR of two GMP numbers."),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the catalog stays canonical, sorted, and complete.
    #[test]
    fn gmp_contracts_are_sorted_and_cover_every_function() {
        assert_eq!(CONTRACTS.len(), FUNCTIONS.len());
        assert_eq!(CONTRACTS.len(), 51);
        let mut previous = "";
        for (contract, expected) in CONTRACTS.iter().zip(FUNCTIONS.iter()) {
            assert_eq!(contract.name, *expected);
            assert!(previous < contract.name, "{} must follow {previous}", contract.name);
            assert_eq!(contract.module, PhpModule::Gmp);
            assert_eq!(contract.kind, BuiltinKind::Function);
            assert!(!contract.extension && !contract.internal);
            previous = contract.name;
        }
    }

    /// Verifies every GMP result that is a GMP object spells the shared operand domain
    /// without inventing a class-specific `TypeSpec` the contract crate cannot represent.
    #[test]
    fn gmp_signatures_use_the_shared_operand_union() {
        let add = CONTRACTS.iter().find(|contract| contract.name == "gmp_add").unwrap();
        assert_eq!(add.params[0].ty, OPERAND);
        assert_eq!(add.returns, GMP_RESULT);
        let invert = CONTRACTS.iter().find(|contract| contract.name == "gmp_invert").unwrap();
        assert_eq!(invert.returns, TypeSpec::Union(&[TypeSpec::Object, TypeSpec::False]));
        let setbit = CONTRACTS.iter().find(|contract| contract.name == "gmp_setbit").unwrap();
        assert_eq!(setbit.params[0].ty, TypeSpec::Object);
        assert_eq!(setbit.params[2].default, Some(DefaultSpec::Bool(true)));
    }
}
