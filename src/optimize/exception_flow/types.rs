//! Purpose:
//! Represents exact and constrained throwable domains for exception-aware optimization.
//! Implements handler intersection, subtraction, exhaustion, and overlap queries.
//!
//! Called from:
//! - `crate::optimize::exception_flow::ExceptionFlowAnalysis`
//!
//! Key details:
//! - Unknown domains carry an upper bound plus source-order handler exclusions.
//! - PHP's `Throwable` domain is exhausted once both `Exception` and `Error` roots are excluded.

use super::hierarchy::ExceptionHierarchy;
use std::collections::BTreeSet;

/// One non-exact throwable domain constrained to subclasses of `upper` and excluding subtrees.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ThrowDomain {
    pub(super) upper: String,
    pub(super) excluded: BTreeSet<String>,
}

/// The throwable values that may escape one expression, statement, or block.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::optimize) struct ThrownTypes {
    pub(super) exact: BTreeSet<String>,
    pub(super) domains: BTreeSet<ThrowDomain>,
}

impl ThrownTypes {
    /// Creates a summary containing one exact runtime throwable class.
    pub(super) fn exact(class_name: &str) -> Self {
        Self {
            exact: BTreeSet::from([class_name.to_string()]),
            domains: BTreeSet::new(),
        }
    }

    /// Creates the conservative domain containing any implementation of `Throwable`.
    pub(super) fn unknown() -> Self {
        Self::domain("Throwable")
    }

    /// Creates a constrained non-exact domain below one catch/class type.
    fn domain(upper: &str) -> Self {
        Self {
            exact: BTreeSet::new(),
            domains: BTreeSet::from([ThrowDomain {
                upper: upper.to_string(),
                excluded: BTreeSet::new(),
            }]),
        }
    }

    /// Returns whether the summary contains no escaping throwable value.
    pub(in crate::optimize) fn is_empty(&self) -> bool {
        self.exact.is_empty() && self.domains.is_empty()
    }

    /// Unions another summary into this one and returns the combined value.
    pub(super) fn combined(mut self, other: Self) -> Self {
        self.exact.extend(other.exact);
        self.domains.extend(other.domains);
        self
    }
}

/// Reachability and caught-value type for one source-order catch clause.
#[derive(Clone, Debug)]
pub(in crate::optimize) struct CatchReachability {
    pub(in crate::optimize) incoming: ThrownTypes,
}

impl CatchReachability {
    /// Returns whether at least one throwable value can enter this handler.
    pub(in crate::optimize) fn is_reachable(&self) -> bool {
        !self.incoming.is_empty()
    }
}

/// Returns whether a constrained unknown domain can overlap a handler type.
pub(super) fn domain_can_match(
    hierarchy: &ExceptionHierarchy,
    domain: &ThrowDomain,
    handler: &str,
) -> bool {
    if domain_is_empty(hierarchy, domain) {
        return false;
    }
    if domain
        .excluded
        .iter()
        .any(|excluded| hierarchy.is_subtype(handler, excluded))
    {
        return false;
    }
    hierarchy.types_overlap(&domain.upper, handler)
}

/// Returns whether exclusions consume an entire constrained throwable domain.
pub(super) fn domain_is_empty(
    hierarchy: &ExceptionHierarchy,
    domain: &ThrowDomain,
) -> bool {
    if domain
        .excluded
        .iter()
        .any(|excluded| hierarchy.is_subtype(&domain.upper, excluded))
    {
        return true;
    }
    if !hierarchy.is_subtype("Throwable", &domain.upper) {
        return false;
    }
    let covers_exception_root = domain
        .excluded
        .iter()
        .any(|excluded| hierarchy.is_subtype("Exception", excluded));
    let covers_error_root = domain
        .excluded
        .iter()
        .any(|excluded| hierarchy.is_subtype("Error", excluded));
    covers_exception_root && covers_error_root
}

/// Intersects an unknown domain with a handler type for caught-variable rethrow tracking.
pub(super) fn intersect_domain(
    hierarchy: &ExceptionHierarchy,
    domain: &ThrowDomain,
    handler: &str,
) -> ThrowDomain {
    let upper = if hierarchy.is_subtype(handler, &domain.upper) {
        handler.to_string()
    } else {
        domain.upper.clone()
    };
    ThrowDomain {
        upper,
        excluded: domain.excluded.clone(),
    }
}

/// Returns whether one exact runtime class belongs to a constrained domain.
pub(super) fn exact_in_domain(
    hierarchy: &ExceptionHierarchy,
    exact: &str,
    domain: &ThrowDomain,
) -> bool {
    !domain_is_empty(hierarchy, domain)
        && hierarchy.is_subtype(exact, &domain.upper)
        && !domain
            .excluded
            .iter()
            .any(|excluded| hierarchy.is_subtype(exact, excluded))
}

/// Returns whether two constrained domains can contain a shared runtime throwable class.
pub(super) fn domains_overlap(
    hierarchy: &ExceptionHierarchy,
    left: &ThrowDomain,
    right: &ThrowDomain,
) -> bool {
    if domain_is_empty(hierarchy, left) || domain_is_empty(hierarchy, right) {
        return false;
    }
    if !hierarchy.types_overlap(&left.upper, &right.upper) {
        return false;
    }
    let narrower = if hierarchy.is_subtype(&left.upper, &right.upper) {
        &left.upper
    } else if hierarchy.is_subtype(&right.upper, &left.upper) {
        &right.upper
    } else {
        return true;
    };
    !left
        .excluded
        .iter()
        .chain(right.excluded.iter())
        .any(|excluded| hierarchy.is_subtype(narrower, excluded))
}
