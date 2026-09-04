//! Purpose:
//! The single answer to what a builtin leaves in a by-REFERENCE argument whose array the callee
//! FILLS IN PLACE, rather than replacing the caller's slot.
//!
//! Called from:
//! - `crate::types::checker::inference::expr::effects` to bind the caller's variable.
//! - `crate::ir_lower::expr::array_builtin_args` to create that variable and type it.
//!
//! Key details:
//! - `preg_match($pattern, $subject, $matches)` hands the callee an ARRAY and the callee writes
//!   into it. That is why an undeclared `$matches` cannot be created as the boxed null every
//!   other out-parameter starts as — MEASURED, it read back as `bool(true)`, because the runtime
//!   wrote through the Mixed cell as though it were an array.
//! - The element type cannot come from the contract: `TypeSpec` has no array form, so the
//!   catalog records only `DefaultSpec::EmptyArray` for the parameter. This module is where the
//!   shape it cannot express lives, and its test asserts every entry still matches the
//!   registry's own `by_ref` + `EmptyArray` facts, so the two cannot drift apart.
//! - The checker and the lowering keep SEPARATE type maps for the caller's variable. Both read
//!   this one answer: without the lowering's half, `count($matches)` said 3 while every
//!   `$matches[$i]` read `NULL`, because the lowering still believed `array<never>`.

use crate::types::PhpType;

/// Returns the array type a builtin leaves in the by-reference argument at `index`.
///
/// `None` for every other parameter of every other builtin, including the ordinary out-parameters
/// (`stream_socket_client`'s `$errno`, `preg_replace`'s `$count`) whose callee REPLACES the
/// caller's slot with a scalar. Those keep the boxed-null creation and their `writes` type.
pub fn filled_array_arg_type(canonical: &str, index: usize) -> Option<PhpType> {
    match (canonical, index) {
        // php fills `$matches` with the whole match then one string per capture group, and an
        // unmatched optional group is the EMPTY STRING rather than a gap: MEASURED,
        // `preg_match("/(a)?(b)/", "b", $m)` answers `["b", "", "b"]`.
        ("preg_match", 2) => Some(PhpType::Array(Box::new(PhpType::Str))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies every entry names a parameter the registry itself marks as a filled array.
    ///
    /// The element type is this module's to state; that the parameter is by-reference and starts
    /// as an empty array is the CONTRACT's, and this is what keeps the two from disagreeing.
    #[test]
    fn every_entry_matches_the_registrys_own_facts() {
        for (name, index) in [("preg_match", 2usize)] {
            let def = crate::builtins::registry::lookup(name).expect("builtin must be registered");
            let param = def.spec.params.get(index).expect("parameter must exist");
            assert!(param.by_ref, "{name} parameter #{index} must be by-reference");
            assert_eq!(
                param.default,
                Some(crate::builtins::spec::DefaultSpec::EmptyArray),
                "{name} parameter #{index} must default to an empty array"
            );
            assert!(
                matches!(filled_array_arg_type(name, index), Some(PhpType::Array(_))),
                "{name} parameter #{index} must answer an array type"
            );
        }
    }

    /// Verifies an ordinary out-parameter is not mistaken for one of these.
    #[test]
    fn an_ordinary_out_parameter_answers_nothing() {
        assert!(filled_array_arg_type("preg_match", 1).is_none());
        assert!(filled_array_arg_type("stream_socket_client", 1).is_none());
        assert!(filled_array_arg_type("sort", 0).is_none());
    }
}
