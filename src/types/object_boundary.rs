//! Purpose:
//! Decides when a declared class-like boundary (return, parameter or typed property) accepts a
//! `mixed` or wider-object value at compile time and leaves PHP's instance check to run time.
//!
//! Called from:
//! - The checker's return, parameter-binding and property-assignment compatibility checks.
//! - EIR return and argument coercion, which emit the matching runtime check.
//! - The optimizer's return-boundary throw facts.
//!
//! Key details:
//! - The checker and EIR lowering MUST agree: every boundary the checker admits through
//!   `runtime_object_boundary` is guarded by lowering, so a non-object never reaches storage
//!   whose ABI expects an object pointer.
//! - Targets are classes, interfaces, `object`, and unions of those with `null`. A union with a
//!   scalar member, and `callable`, keep their existing rules.
//! - Sources are boxed: `mixed`, or a union that names the bare `object` type (`object|null`).
//!   A union of named classes keeps the static rules, which already prove it or refuse it.

use super::PhpType;

/// The members a declared class-like boundary accepts at run time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObjectBoundary {
    /// Accepted class or interface names; the empty name stands for PHP `object`.
    pub(crate) classes: Vec<String>,
    /// Whether the declaration admits `null`.
    pub(crate) nullable: bool,
}

/// Returns the runtime accept set of a class-like declaration, or `None` for any other type.
pub(crate) fn declared_object_boundary(expected: &PhpType) -> Option<ObjectBoundary> {
    match expected {
        PhpType::Object(class) => Some(ObjectBoundary { classes: vec![class.clone()], nullable: false }),
        PhpType::Union(members) => {
            let mut classes = Vec::new();
            let mut nullable = false;
            for member in members {
                match member {
                    PhpType::Object(class) => classes.push(class.clone()),
                    PhpType::Void => nullable = true,
                    _ => return None,
                }
            }
            (!classes.is_empty()).then_some(ObjectBoundary { classes, nullable })
        }
        _ => None,
    }
}

/// Returns whether a source type is one whose class only a runtime check can establish.
pub(crate) fn runtime_checked_object_source(actual: &PhpType) -> bool {
    match actual {
        PhpType::Mixed => true,
        PhpType::Union(members) => {
            members.iter().any(|member| matches!(member, PhpType::Object(class) if class.is_empty()))
                && members.iter().all(|member| matches!(member, PhpType::Object(_) | PhpType::Void))
        }
        _ => false,
    }
}

/// Returns the runtime check a `mixed` or wider-object `actual` needs at an `expected` boundary.
pub(crate) fn runtime_object_boundary(expected: &PhpType, actual: &PhpType) -> Option<ObjectBoundary> {
    if !runtime_checked_object_source(actual) {
        return None;
    }
    declared_object_boundary(expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies classes, `object` and nullable class unions are boundaries; scalars are not.
    #[test]
    fn declared_object_boundary_accepts_class_like_declarations_only() {
        let class = PhpType::Object("Timer".into());
        assert_eq!(
            declared_object_boundary(&PhpType::Union(vec![class.clone(), PhpType::Void])),
            Some(ObjectBoundary { classes: vec!["Timer".into()], nullable: true })
        );
        assert_eq!(
            declared_object_boundary(&PhpType::Object(String::new())),
            Some(ObjectBoundary { classes: vec![String::new()], nullable: false })
        );
        for rejected in [
            PhpType::Union(vec![class.clone(), PhpType::Int]),
            PhpType::Union(vec![PhpType::Void]),
            PhpType::Callable,
            PhpType::Mixed,
        ] {
            assert_eq!(declared_object_boundary(&rejected), None, "{rejected:?}");
        }
    }

    /// Verifies only `mixed` and unions naming the bare `object` type earn the runtime check.
    #[test]
    fn runtime_checked_sources_are_mixed_or_wider_objects() {
        let target = PhpType::Object("Fiber".into());
        let object_or_null = PhpType::Union(vec![PhpType::Object(String::new()), PhpType::Void]);
        assert!(runtime_object_boundary(&target, &PhpType::Mixed).is_some());
        assert!(runtime_object_boundary(&target, &object_or_null).is_some());
        for static_source in [
            PhpType::Union(vec![PhpType::Object("Fiber".into()), PhpType::Void]),
            PhpType::Union(vec![PhpType::Object(String::new()), PhpType::Int]),
            PhpType::Object(String::new()),
            PhpType::Int,
        ] {
            assert!(runtime_object_boundary(&target, &static_source).is_none(), "{static_source:?}");
        }
    }
}
