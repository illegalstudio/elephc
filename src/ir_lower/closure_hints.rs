//! Purpose:
//! Shares checker-proven parameter and scalar result contracts with EIR closure lowering.
//!
//! Called from:
//! - `crate::ir_lower::program::lower()` installs the map for one program lowering.
//! - `crate::ir_lower::expr::closures::lower_closure_with_context()` reads it by span.
//!
//! Key details:
//! - Parameter hints retain callback element/accumulator types; result contracts retain kinds.
//! - Results are keyed by enclosing body and closure span, with conflicts stored as Mixed.
//! - The map is scoped to one lowering call; a nested lowering restores the outer map on drop.
//! - A missing hint keeps the existing `Mixed` fallback, so ordinary closures are unaffected.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::span::Span;
use crate::types::PhpType;

thread_local! {
    static CLOSURE_HINTS: RefCell<Option<ClosureHints>> =
        RefCell::new(None);
}

/// Immutable checker maps scoped to one lowering activation.
struct ClosureHints {
    parameters: HashMap<Span, Vec<PhpType>>,
    results: HashMap<(String, Span), PhpType>,
}

/// Installs `hints` for the duration of the returned scope.
pub(crate) fn install(hints: &HashMap<Span, Vec<PhpType>>,
    results: &HashMap<(String, Span), PhpType>) -> ClosureHintScope {
    let hints = ClosureHints { parameters: hints.clone(), results: results.clone() };
    let previous = CLOSURE_HINTS.with(|slot| slot.replace(Some(hints)));
    ClosureHintScope { previous }
}

/// Returns the contextual parameter types recorded for a closure expression.
pub(super) fn param_hints(span: Span) -> Option<Vec<PhpType>> {
    CLOSURE_HINTS.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|hints| hints.parameters.get(&span).cloned())
    })
}

/// Returns the checked scalar result independently of syntactic fallback inference.
pub(super) fn result_type(profile: elephc_semantics::SemanticProfile, source: crate::source::SourceMode,
    scope: &str, span: Span) -> Option<PhpType> {
    if source == crate::source::SourceMode::Internal
        || !profile.contains(elephc_semantics::StrictFeature::Coercions) { return None; }
    CLOSURE_HINTS.with(|slot| slot.borrow().as_ref()
        .and_then(|hints| hints.results.get(&(scope.to_string(), span)).cloned()))
}

/// Restores the previous hint map when the enclosing lowering finishes.
pub(crate) struct ClosureHintScope {
    previous: Option<ClosureHints>,
}

impl Drop for ClosureHintScope {
    /// Restores the hint map that was active before this lowering scope.
    fn drop(&mut self) {
        let previous = self.previous.take();
        CLOSURE_HINTS.with(|slot| {
            slot.replace(previous);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nested lowerings restore both maps and keep identical spans in different bodies separate.
    #[test]
    fn strict_closure_contract_scopes_restore_parameters_and_results() {
        let span = Span::dummy();
        let parameters = HashMap::from([(span, vec![PhpType::Int])]);
        let results = HashMap::from([(("outer".to_string(), span), PhpType::Float)]);
        let profile = elephc_semantics::SemanticProfile::PERMISSIVE.with(elephc_semantics::StrictFeature::Coercions, true);
        let result = |name| result_type(profile, crate::source::SourceMode::Php, name, span);
        let prior = result("outer");
        {
            let _outer = install(&parameters, &results);
            assert_eq!(param_hints(span), Some(vec![PhpType::Int]));
            assert_eq!(result("outer"), Some(PhpType::Float));
            assert_eq!(result("other"), None);
            assert_eq!(result_type(profile, crate::source::SourceMode::Internal, "outer", span), None);
            assert_eq!(result_type(elephc_semantics::SemanticProfile::PERMISSIVE,
                crate::source::SourceMode::Php, "outer", span), None);
            {
                let parameters = HashMap::from([(span, vec![PhpType::Str])]);
                let results = HashMap::from([(("outer".to_string(), span), PhpType::Str)]);
                let _nested = install(&parameters, &results);
                assert_eq!(param_hints(span), Some(vec![PhpType::Str]));
                assert_eq!(result("outer"), Some(PhpType::Str));
            }
            assert_eq!(param_hints(span), Some(vec![PhpType::Int]));
            assert_eq!(result("outer"), Some(PhpType::Float));
        }
        assert_eq!(result("outer"), prior);
    }
}
