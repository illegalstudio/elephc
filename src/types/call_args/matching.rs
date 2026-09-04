//! Purpose:
//! Tracks named-parameter matching against visible function signatures.
//! Detects duplicate assignments and distinguishes regular parameters, variadics, and unknown names.
//!
//! Called from:
//! - `crate::types::call_args::planner`
//!
//! Key details:
//! - Matching uses caller-visible regular parameters so hidden implementation parameters do not leak into PHP semantics.

use crate::types::{FunctionSig, PhpType};

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
/// The type ONE positional argument occupies at `index` in a call to `sig`.
///
/// NOT `sig.params[index].1`. Past the regular parameters a variadic signature keeps collecting,
/// and the collector's own entry describes the COLLECTION: `int ...$r` is stored as one parameter
/// of type `array<int>`, and the lowered callee receives exactly that — one array — while the
/// CALLER materializes each argument separately and pushes it in. A caller that reads
/// `params[regular]` therefore builds an argument typed `array<int>` where one `int` belongs.
///
/// Two callers need this and MUST agree, or the thunk's declared parameters and the call codegen
/// emits describe different frames: `ir_lower::lower_dynamic_constructor_thunk` declares the
/// padding thunk's parameters, and `codegen::…::dynamic_new_candidate` materializes the arguments
/// passed to it. They share this function rather than each deriving the rule.
///
/// Returns `None` when `index` is past the regular parameters of a signature that cannot collect,
/// which is a call the caller has to refuse rather than materialize.
pub(crate) fn positional_param_type(sig: &FunctionSig, index: usize) -> Option<PhpType> {
    let regular = regular_param_count(sig);
    if index < regular {
        return sig.params.get(index).map(|(_, ty)| ty.clone());
    }
    sig.variadic.as_ref()?;
    sig.params.get(regular)?;
    // MIXED, NOT THE ELEMENT TYPE. A caller materializing an overflow argument has to BOX it —
    // that is the one slot codegen boxes rather than reinterpreting — because materializing it AS
    // the declared element type performs no conversion: `new $c("x")` on `int ...$r` came back
    // holding the string's ADDRESS read as an integer. What the collector actually declares is a
    // separate question, answered by `variadic_element_type`, and it is the CALLEE's business:
    // the padding thunk casts each boxed argument down to it, in PHP, where php's own coercion
    // rules can be spelled out.
    Some(PhpType::Mixed)
}

/// What ONE element of `sig`'s variadic collector is declared to be, or `None` when it collects
/// anything.
///
/// The collector's own signature entry describes the COLLECTION — `int ...$r` is stored as one
/// parameter of type `array<int>` — so the element is that array's inner type. Both spellings are
/// tolerated on purpose: a path that stores the element type directly still answers correctly, so
/// this cannot become the silent half of a disagreement.
pub(crate) fn variadic_element_type(sig: &FunctionSig) -> Option<PhpType> {
    let variadic = sig.variadic.as_ref()?;
    let (_, collector) = sig.params.iter().find(|(name, _)| name == variadic)?;
    Some(match collector {
        PhpType::Array(element) => (**element).clone(),
        other => other.clone(),
    })
}

/// If the signature is variadic, excludes the variadic slot from the count so that
/// named arguments address only the caller-visible parameters.
pub(crate) fn regular_param_count(sig: &FunctionSig) -> usize {
    let physical = if sig.variadic.is_some() {
        sig.params.len().saturating_sub(1)
    } else {
        sig.params.len()
    };
    physical.saturating_sub(usize::from(
        crate::func_args::sig_has_hidden_argc_param(sig),
    ))
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
