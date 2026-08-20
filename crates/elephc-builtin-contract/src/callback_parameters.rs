//! Purpose:
//! Declares which fixed parameters of shared builtin contracts receive PHP callables.
//!
//! Called from:
//! - `BuiltinContract::callback_parameter_names()` and declaration reachability scanning.
//!
//! Key details:
//! - Callback roles are independent of PHP parameter spelling and broad `mixed` storage types.
//! - Every entry uses a stable builtin ID and PHP-visible named-argument key.

use crate::BuiltinId;

/// One shared builtin's callback-bearing fixed parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CallbackParameters {
    /// Stable identity of the builtin contract.
    id: BuiltinId,
    /// Canonical builtin name used to keep this table reviewable and sorted.
    builtin: &'static str,
    /// PHP-visible parameter names that can contain callable descriptors.
    names: &'static [&'static str],
}

const CALLBACK: &[&str] = &["callback"];
const HANDLER: &[&str] = &["handler"];

const CALLBACK_PARAMETERS: &[CallbackParameters] = &[
    callback_parameters("array_all", CALLBACK),
    callback_parameters("array_any", CALLBACK),
    callback_parameters("array_filter", CALLBACK),
    callback_parameters("array_find", CALLBACK),
    callback_parameters("array_map", CALLBACK),
    callback_parameters("array_reduce", CALLBACK),
    callback_parameters("array_udiff", CALLBACK),
    callback_parameters("array_uintersect", CALLBACK),
    callback_parameters("array_walk", CALLBACK),
    callback_parameters("array_walk_recursive", CALLBACK),
    callback_parameters("call_user_func", CALLBACK),
    callback_parameters("call_user_func_array", CALLBACK),
    callback_parameters("iterator_apply", CALLBACK),
    callback_parameters("ob_start", CALLBACK),
    callback_parameters("pcntl_signal", HANDLER),
    callback_parameters("preg_replace_callback", CALLBACK),
    callback_parameters("spl_autoload_register", CALLBACK),
    callback_parameters("spl_autoload_unregister", CALLBACK),
    callback_parameters("uasort", CALLBACK),
    callback_parameters("uksort", CALLBACK),
    callback_parameters("usort", CALLBACK),
];

/// Builds one const callback-parameter record from a canonical builtin name.
const fn callback_parameters(
    builtin: &'static str,
    names: &'static [&'static str],
) -> CallbackParameters {
    CallbackParameters {
        id: BuiltinId::from_canonical_name(builtin),
        builtin,
        names,
    }
}

/// Returns the fixed parameter names whose values may be invoked as callbacks.
pub(crate) fn names(id: BuiltinId) -> &'static [&'static str] {
    CALLBACK_PARAMETERS
        .iter()
        .find(|entry| entry.id == id)
        .map_or(&[], |entry| entry.names)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies callback metadata is sorted, unique, and independent of parameter spelling.
    #[test]
    fn callback_parameter_contracts_are_stable_and_structural() {
        let mut previous_name = None;
        let mut ids = std::collections::HashSet::new();
        for entry in CALLBACK_PARAMETERS {
            if let Some(previous) = previous_name {
                assert!(previous < entry.builtin);
            }
            assert!(ids.insert(entry.id));
            assert!(!entry.names.is_empty());
            previous_name = Some(entry.builtin);
        }
        assert_eq!(
            names(BuiltinId::from_canonical_name("array_map")),
            &["callback"]
        );
        assert_eq!(
            names(BuiltinId::from_canonical_name("pcntl_signal")),
            &["handler"]
        );
        assert!(names(BuiltinId::from_canonical_name("strlen")).is_empty());
        let differently_named = callback_parameters("future_builtin", &["handler"]);
        assert_eq!(differently_named.names, &["handler"]);
    }
}
