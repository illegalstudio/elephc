//! Purpose:
//! Tracks named-parameter matching against visible function signatures.
//! Detects duplicate assignments and distinguishes regular parameters, variadics, and unknown names.
//!
//! Called from:
//! - `crate::types::call_args::planner`
//!
//! Key details:
//! - Matching uses caller-visible regular parameters so hidden implementation parameters do not leak into PHP semantics.

use crate::types::FunctionSig;

/// Discriminates a named-parameter match into regular positional, variadic, or unknown categories.
pub(super) enum NamedParamMatch {
    Regular(usize),
    Variadic,
    Unknown,
}

/// Error raised when a named argument is assigned to a parameter that already received a value.
pub(super) struct DuplicateNamedParam {
    pub(super) param_idx: usize,
}

/// Tracks which regular (non-variadic) parameters have received a named argument.
///
/// Used to detect duplicate named assignments while resolving PHP call arguments.
pub(super) struct NamedParamTracker {
    assigned: Vec<bool>,
}

impl NamedParamTracker {
    /// Creates a tracker for `regular_param_count` visible parameters.
    pub(super) fn new(regular_param_count: usize) -> Self {
        Self {
            assigned: vec![false; regular_param_count],
        }
    }

    /// Looks up `name` in the signature, returns the match kind, and records a duplicate error
    /// if the parameter was already assigned through this tracker.
    pub(super) fn assign(
        &mut self,
        sig: &FunctionSig,
        regular_param_count: usize,
        name: &str,
        allow_unknown_named_variadic: bool,
    ) -> Result<NamedParamMatch, DuplicateNamedParam> {
        match match_named_param(sig, regular_param_count, name, allow_unknown_named_variadic) {
            NamedParamMatch::Regular(param_idx) => {
                if self.assigned.get(param_idx).copied().unwrap_or(false) {
                    Err(DuplicateNamedParam { param_idx })
                } else {
                    self.assigned[param_idx] = true;
                    Ok(NamedParamMatch::Regular(param_idx))
                }
            }
            other => Ok(other),
        }
    }
}

/// Returns the number of visible regular parameters for named-argument matching.
///
/// If the signature is variadic, excludes the variadic slot from the count so that
/// named arguments address only the caller-visible parameters.
pub(crate) fn regular_param_count(sig: &FunctionSig) -> usize {
    if sig.variadic.is_some() {
        sig.params.len().saturating_sub(1)
    } else {
        sig.params.len()
    }
}

/// Searches for a parameter named `name` among the first `regular_param_count` parameters.
///
/// Returns its index within those parameters, or `None` if no visible parameter matches.
pub(crate) fn named_param_index(
    sig: &FunctionSig,
    regular_param_count: usize,
    name: &str,
) -> Option<usize> {
    sig.params
        .iter()
        .take(regular_param_count)
        .position(|(param_name, _)| param_name == name)
}

/// Determines the match kind for a named argument `name` against signature `sig`.
///
/// - Returns `NamedParamMatch::Regular(idx)` if `name` matches a visible parameter at index `idx`.
/// - Returns `NamedParamMatch::Variadic` if `name` is not a visible param but the signature is variadic
///   and `allow_unknown_named_variadic` is `true`.
/// - Returns `NamedParamMatch::Unknown` if `name` does not match any parameter and cannot be absorbed
///   by a variadic parameter.
pub(super) fn match_named_param(
    sig: &FunctionSig,
    regular_param_count: usize,
    name: &str,
    allow_unknown_named_variadic: bool,
) -> NamedParamMatch {
    if let Some(param_idx) = named_param_index(sig, regular_param_count, name) {
        NamedParamMatch::Regular(param_idx)
    } else if allow_unknown_named_variadic && sig.variadic.is_some() {
        NamedParamMatch::Variadic
    } else {
        NamedParamMatch::Unknown
    }
}
