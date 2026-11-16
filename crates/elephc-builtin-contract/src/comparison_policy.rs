//! Purpose:
//! Declares builtin comparison modes independently of compiler and eval representations.
//!
//! Called from:
//! - `BuiltinContract::comparison_policy()` and both backends' evaluated argument boundaries.
//!
//! Key details:
//! - Stable builtin identities own the policy; parameter roles use PHP argument names.
//! - Existing defaults remain unchanged. Checked calls must explicitly select identity mode.
//! - Callback alternatives let source code choose its comparison without hidden PHP coercion.

use crate::BuiltinId;

/// A builtin's automatic comparison contract under strict comparisons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComparisonPolicy {
    /// A fixed boolean mode parameter must select PHP identity comparison.
    Identity { parameter: &'static str },
    /// This API has no general identity mode; use an explicit source comparator instead.
    ComparatorRequired { alternative: &'static str },
    /// A caller-selected comparator must produce an original integer result when invoked.
    Comparator { parameter: &'static str },
}

impl ComparisonPolicy {
    /// Gives compiler and runtime consumers the same actionable diagnostic.
    pub fn message(self, name: &str) -> String {
        match self {
            Self::Identity { parameter } => format!(
                "--strict-comparisons: {name}() requires explicit ${parameter} = true"),
            Self::ComparatorRequired { alternative } => format!(
                "--strict-comparisons: {name}() uses automatic PHP comparisons; use {alternative}"),
            Self::Comparator { parameter } => format!(
                "--strict-comparisons: {name}() requires an integer result from ${parameter}"),
        }
    }

    /// Distinguishes eager argument-mode checks from deferred comparator-result checks.
    pub const fn requires_argument_check(self) -> bool {
        !self.checks_callback_result()
    }

    /// Identifies callback consumer failures independently of the callback body's own effects.
    pub const fn checks_callback_result(self) -> bool {
        matches!(self, Self::Comparator { .. })
    }
}

/// One canonical builtin's comparison policy, sorted by PHP name for review.
struct Entry { id: BuiltinId, policy: ComparisonPolicy }

/// Constructs metadata using the same stable identity as the owning builtin contract.
const fn entry(name: &'static str, policy: ComparisonPolicy) -> Entry {
    Entry { id: BuiltinId::from_canonical_name(name), policy }
}

use ComparisonPolicy::{Comparator, ComparatorRequired, Identity};

const POLICIES: &[Entry] = &[
    entry("array_diff", ComparatorRequired { alternative: "array_udiff() with an explicit comparator" }),
    entry("array_diff_assoc", ComparatorRequired { alternative: "an explicit loop with a source-selected comparator" }),
    entry("array_intersect", ComparatorRequired { alternative: "array_uintersect() with an explicit comparator" }),
    entry("array_intersect_assoc", ComparatorRequired { alternative: "an explicit loop with a source-selected comparator" }),
    entry("array_multisort", ComparatorRequired { alternative: "usort() with an explicit comparator" }),
    entry("array_search", Identity { parameter: "strict" }),
    entry("array_udiff", Comparator { parameter: "callback" }),
    entry("array_uintersect", Comparator { parameter: "callback" }),
    entry("array_unique", ComparatorRequired { alternative: "in_array() with strict: true while collecting unique values" }),
    entry("arsort", ComparatorRequired { alternative: "uasort() with an explicit comparator" }),
    entry("asort", ComparatorRequired { alternative: "uasort() with an explicit comparator" }),
    entry("in_array", Identity { parameter: "strict" }),
    entry("krsort", ComparatorRequired { alternative: "uksort() with an explicit comparator" }),
    entry("ksort", ComparatorRequired { alternative: "uksort() with an explicit comparator" }),
    entry("max", ComparatorRequired { alternative: "array_reduce() with an explicit comparator" }),
    entry("min", ComparatorRequired { alternative: "array_reduce() with an explicit comparator" }),
    entry("natcasesort", ComparatorRequired { alternative: "uasort() with an explicit comparator" }),
    entry("natsort", ComparatorRequired { alternative: "uasort() with an explicit comparator" }),
    entry("rsort", ComparatorRequired { alternative: "usort() with an explicit comparator" }),
    entry("sort", ComparatorRequired { alternative: "usort() with an explicit comparator" }),
    entry("uasort", Comparator { parameter: "callback" }),
    entry("uksort", Comparator { parameter: "callback" }),
    entry("usort", Comparator { parameter: "callback" }),
];

/// Looks up comparison metadata without another backend-specific PHP name table.
pub(crate) fn policy(id: BuiltinId) -> Option<ComparisonPolicy> {
    POLICIES.iter().find(|entry| entry.id == id).map(|entry| entry.policy)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every policy belongs to one registered contract and every mode role exists in that contract.
    #[test]
    fn strict_comparison_builtin_contract_roles_are_canonical() {
        let mut previous = None;
        for entry in POLICIES {
            let contract = crate::lookup_id(entry.id).expect("registered comparison contract");
            assert!(previous.is_none_or(|name| name < contract.name));
            previous = Some(contract.name);
            if let Identity { parameter } = entry.policy {
                let parameter = contract.params.iter().find(|slot| slot.name == parameter).unwrap();
                assert_eq!(parameter.ty, crate::TypeSpec::Bool);
                assert_eq!(parameter.default, Some(crate::DefaultSpec::Bool(false)));
            }
            if let Comparator { parameter } = entry.policy {
                let parameter = contract.params.iter().find(|slot| slot.name == parameter).unwrap();
                assert!(!parameter.by_ref);
                assert!(parameter.default.is_none());
                assert!(!entry.policy.requires_argument_check());
            }
            assert_eq!(contract.comparison_policy(), Some(entry.policy));
        }
        for name in ["strcmp", "array_keys"] {
            assert_eq!(crate::lookup(name).unwrap().comparison_policy(), None, "{name}");
        }
    }
}
