//! Purpose:
//! Shares checker-inferred inline callback parameter types with EIR closure lowering.
//!
//! Called from:
//! - `crate::ir_lower::program::lower()` installs the map for one program lowering.
//! - `crate::ir_lower::expr::closures::lower_closure_with_context()` reads it by span.
//!
//! Key details:
//! - The hints are contextual parameter types the checker already proved for a closure literal
//!   handed to an array callback (element/accumulator types).
//! - The map is scoped to one lowering call; a nested lowering restores the outer map on drop.
//! - A missing hint keeps the existing `Mixed` fallback, so ordinary closures are unaffected.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::span::Span;
use crate::types::PhpType;

thread_local! {
    static CLOSURE_PARAM_HINTS: RefCell<Option<HashMap<Span, Vec<PhpType>>>> =
        RefCell::new(None);
}

/// Installs `hints` for the duration of the returned scope.
pub(crate) fn install(hints: &HashMap<Span, Vec<PhpType>>) -> ClosureHintScope {
    let previous = CLOSURE_PARAM_HINTS.with(|slot| slot.replace(Some(hints.clone())));
    ClosureHintScope { previous }
}

/// Returns the contextual parameter types recorded for a closure expression.
pub(super) fn param_hints(span: Span) -> Option<Vec<PhpType>> {
    CLOSURE_PARAM_HINTS.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|hints| hints.get(&span).cloned())
    })
}

/// Restores the previous hint map when the enclosing lowering finishes.
pub(crate) struct ClosureHintScope {
    previous: Option<HashMap<Span, Vec<PhpType>>>,
}

impl Drop for ClosureHintScope {
    /// Restores the hint map that was active before this lowering scope.
    fn drop(&mut self) {
        let previous = self.previous.take();
        CLOSURE_PARAM_HINTS.with(|slot| {
            slot.replace(previous);
        });
    }
}
