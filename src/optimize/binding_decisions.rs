//! Purpose:
//! Carries checker binding decisions, mixed-storage names, and native buffer read proofs
//! into post-typecheck AST passes without losing their typing or ownership invariants.
//!
//! Called from:
//! - `crate::pipeline`, which installs the sets around post-typecheck optimization.
//! - `crate::optimize::control::binding_decisions`, the walker both CLONING passes ask before
//!   duplicating a node: `crate::optimize::control::dce`'s tail-sinking and the single-case switch
//!   rewrite in `crate::optimize::control::switch`.
//! - `crate::optimize::propagate`, whose constant substitution would otherwise replace a read of
//!   a boxed local with a literal.
//!
//! Key details:
//! - The checker records `local_bind_kill_sites` / `local_retype_sites` / `mixed_storage_store_sites`
//!   BY SPAN, and EIR lowering consults them by span at every `unset` argument and every
//!   assignment. A pass that clones an AST node clones its span, so both copies then answer to the
//!   same decision — and abandoning a binding is not idempotent (it releases the old value and
//!   re-binds the name to a fresh slot), so the second copy is lowered against the FIRST copy's
//!   post-rebind state.
//! - The mixed-storage NAMES are a separate fact with a separate consumer. A marked local is bound
//!   `mixed` for its whole frame and every read of it must observe that type; substituting a
//!   literal at a read hands lowering a CONCRETE type the checker never approved, which is how
//!   `$a = 42/"hello" divergently; $a = 99; echo strlen($a);` reached the checked-builtin fast path
//!   with an `Int` operand and panicked the compiler.
//! - Installed as scoped thread-locals, matching `with_callable_effect_analysis` and
//!   `with_by_ref_signatures`. The SPAN set is installed around the prune, normalize AND dce
//!   phases, because the cloning passes live in all three. All sets are empty by default. The
//!   SPAN set has an explicit `is_empty()` fast path (`has_local_binding_decisions`) that keeps
//!   the scan off the hot path entirely; the NAME set has none — `local_has_mixed_storage` is a
//!   plain hash lookup, which on an empty set is already the cheapest thing it could do.
//! - Buffer read proofs are installed only for propagation. They remove intrinsic warning-handler
//!   effects, never effects from evaluating a receiver or index or from a bounds failure.

use std::cell::RefCell;
use std::collections::HashSet;

use crate::span::Span;

thread_local! {
    /// Spans carrying a checker local-binding decision, for the duration of one optimizer run.
    static ACTIVE_BINDING_DECISION_SPANS: RefCell<HashSet<Span>> = RefCell::new(HashSet::new());
    /// Local names the checker compiled as boxed `mixed` storage, for one optimizer run.
    static ACTIVE_MIXED_STORAGE_LOCALS: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    /// Indexed reads the checker proved cannot dispatch PHP warning handlers.
    static ACTIVE_BUFFER_READ_SITES: RefCell<HashSet<Span>> = RefCell::new(HashSet::new());
}

/// Installs checker-proven buffer reads for one constant-propagation run.
pub(crate) fn with_buffer_read_sites<R>(sites: HashSet<Span>, f: impl FnOnce() -> R) -> R {
    ACTIVE_BUFFER_READ_SITES.with(|slot| {
        let previous = slot.replace(sites);
        let result = f();
        slot.replace(previous);
        result
    })
}

/// Returns whether all checker observations at this read used native buffer storage.
pub(crate) fn is_buffer_read_site(span: Span) -> bool {
    ACTIVE_BUFFER_READ_SITES.with(|slot| slot.borrow().contains(&span))
}

/// Installs `spans` as the active local-binding decision set for the duration of `f`.
pub(crate) fn with_local_binding_decision_spans<R>(spans: HashSet<Span>, f: impl FnOnce() -> R) -> R {
    ACTIVE_BINDING_DECISION_SPANS.with(|slot| {
        let previous = slot.replace(spans);
        let result = f();
        slot.replace(previous);
        result
    })
}

/// Returns whether any local-binding decision is in play at all.
///
/// Almost every program answers `false` here (a kill or retype is rare), which is what keeps the
/// scan below off the hot path entirely.
pub(crate) fn has_local_binding_decisions() -> bool {
    ACTIVE_BINDING_DECISION_SPANS.with(|slot| !slot.borrow().is_empty())
}

/// Returns whether `span` carries a checker local-binding decision.
pub(crate) fn span_carries_local_binding_decision(span: Span) -> bool {
    ACTIVE_BINDING_DECISION_SPANS.with(|slot| slot.borrow().contains(&span))
}

/// Installs `names` as the active mixed-storage local set for the duration of `f`.
///
/// The set is program-wide (the union over every body the checker marked), not per-body: the AST
/// passes have no body identity to key on, and over-approximating only costs the optimization on a
/// name some other function boxed. Marking is rare enough for that to be free in practice.
pub(crate) fn with_mixed_storage_locals<R>(names: HashSet<String>, f: impl FnOnce() -> R) -> R {
    ACTIVE_MIXED_STORAGE_LOCALS.with(|slot| {
        let previous = slot.replace(names);
        let result = f();
        slot.replace(previous);
        result
    })
}

/// Returns whether `name` is a local the checker compiled as boxed `mixed` storage.
///
/// A `true` answer means the local's LOGICAL type is `mixed` at every point of its frame, so no
/// post-typecheck pass may hand lowering a narrower view of it.
pub(crate) fn local_has_mixed_storage(name: &str) -> bool {
    ACTIVE_MIXED_STORAGE_LOCALS.with(|slot| slot.borrow().contains(name))
}
