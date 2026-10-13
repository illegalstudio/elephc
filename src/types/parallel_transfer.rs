//! Purpose:
//! Classifies PHP types that may cross an isolated Parallel runtime-context boundary.
//!
//! Called from:
//! - Parallel task capture, argument, and result validation in the type checker.
//!
//! Key details:
//! - Scalars and recursively transferable arrays copy by value.
//! - Objects, resources, pointers, buffers, callables, iterables, and unresolved `mixed`
//!   are rejected; a direct `Elephc\Async\Cancellation` task input is the sole capability exception.

use std::fmt;

use super::PhpType;

/// First non-transferable type component found in a Parallel boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ParallelTransferRejection {
    Mixed,
    Iterable,
    Callable,
    Object(String),
    CancellationNotDirect,
    Packed(String),
    Pointer,
    Resource,
    Buffer,
}

impl fmt::Display for ParallelTransferRejection {
    /// Formats the rejected runtime shape and the direct-only Cancellation capability rule.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mixed => formatter.write_str("mixed value whose runtime shape is not proven"),
            Self::Iterable => formatter.write_str("iterable with an unresolved runtime shape"),
            Self::Callable => formatter.write_str("callable value"),
            Self::Object(class) => write!(formatter, "object of class {class}"),
            Self::CancellationNotDirect => formatter.write_str(
                "Elephc\\Async\\Cancellation may only be passed as a direct task argument or captured directly; it cannot be nested or returned",
            ),
            Self::Packed(class) => write!(formatter, "packed object of class {class}"),
            Self::Pointer => formatter.write_str("native pointer"),
            Self::Resource => formatter.write_str("resource"),
            Self::Buffer => formatter.write_str("Buffer handle"),
        }
    }
}

/// Returns the first reason `ty` cannot cross an isolated runtime-context boundary.
pub(crate) fn parallel_transfer_rejection(
    ty: &PhpType,
) -> Option<ParallelTransferRejection> {
    parallel_transfer_rejection_at(ty, true)
}

/// Returns the first reason `ty` cannot cross from a worker back to its parent.
/// Cancellation capabilities are job-local inputs and cannot be task results.
pub(crate) fn parallel_transfer_return_rejection(
    ty: &PhpType,
) -> Option<ParallelTransferRejection> {
    parallel_transfer_rejection_at(ty, false)
}

/// Recursively rejects runtime-backed identities, allowing Cancellation only at the input root.
fn parallel_transfer_rejection_at(
    ty: &PhpType,
    allow_direct_cancellation: bool,
) -> Option<ParallelTransferRejection> {
    match ty {
        PhpType::Int
        | PhpType::Float
        | PhpType::Str
        | PhpType::Bool
        | PhpType::False
        | PhpType::Void
        | PhpType::Never
        | PhpType::TaggedScalar => None,
        PhpType::Array(element) => parallel_transfer_rejection_at(element, false),
        PhpType::AssocArray { key, value } => {
            parallel_transfer_rejection_at(key, false)
                .or_else(|| parallel_transfer_rejection_at(value, false))
        }
        PhpType::Union(members) => members
            .iter()
            .find_map(|member| parallel_transfer_rejection_at(member, false)),
        PhpType::Object(class)
            if is_parallel_cancellation(class) && allow_direct_cancellation =>
        {
            None
        }
        PhpType::Object(class) if is_parallel_cancellation(class) => {
            Some(ParallelTransferRejection::CancellationNotDirect)
        }
        PhpType::Mixed => Some(ParallelTransferRejection::Mixed),
        PhpType::Iterable => Some(ParallelTransferRejection::Iterable),
        PhpType::Callable => Some(ParallelTransferRejection::Callable),
        PhpType::Object(class) => Some(ParallelTransferRejection::Object(class.clone())),
        PhpType::Packed(class) => Some(ParallelTransferRejection::Packed(class.clone())),
        PhpType::Pointer(_) => Some(ParallelTransferRejection::Pointer),
        PhpType::Resource(_) => Some(ParallelTransferRejection::Resource),
        PhpType::Buffer(_) => Some(ParallelTransferRejection::Buffer),
    }
}

/// Recognizes the canonical Cancellation class case-insensitively with an optional leading slash.
pub(crate) fn is_parallel_cancellation(class: &str) -> bool {
    class
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("Elephc\\Async\\Cancellation")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Accepts copied scalars, recursively safe arrays and the direct Cancellation input capability.
    fn scalars_nested_arrays_and_cancellation_are_transferable() {
        for ty in [
            PhpType::Int,
            PhpType::Float,
            PhpType::Str,
            PhpType::Bool,
            PhpType::False,
            PhpType::Void,
            PhpType::Never,
            PhpType::TaggedScalar,
            PhpType::Array(Box::new(PhpType::Array(Box::new(PhpType::Str)))),
            PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Union(vec![PhpType::Int, PhpType::False])),
            },
            PhpType::Object("\\Elephc\\Async\\Cancellation".to_string()),
        ] {
            assert_eq!(parallel_transfer_rejection(&ty), None, "{ty:?}");
        }
    }

    #[test]
    /// Rejects Cancellation when nested in arrays/unions or returned across the worker boundary.
    fn cancellation_is_only_transferable_as_a_direct_task_input() {
        let cancellation = PhpType::Object("\\Elephc\\Async\\Cancellation".to_string());
        assert_eq!(
            parallel_transfer_rejection(&PhpType::Array(Box::new(cancellation.clone()))),
            Some(ParallelTransferRejection::CancellationNotDirect),
        );
        assert_eq!(
            parallel_transfer_rejection(&PhpType::Union(vec![PhpType::Int, cancellation.clone()])),
            Some(ParallelTransferRejection::CancellationNotDirect),
        );
        assert_eq!(
            parallel_transfer_return_rejection(&cancellation),
            Some(ParallelTransferRejection::CancellationNotDirect),
        );
    }

    #[test]
    /// Rejects unresolved and identity-bearing types, including a buffer nested inside an array.
    fn identity_and_runtime_backed_types_are_rejected_recursively() {
        let cases = [
            (PhpType::Mixed, ParallelTransferRejection::Mixed),
            (PhpType::Iterable, ParallelTransferRejection::Iterable),
            (PhpType::Callable, ParallelTransferRejection::Callable),
            (
                PhpType::Object("DateTime".to_string()),
                ParallelTransferRejection::Object("DateTime".to_string()),
            ),
            (
                PhpType::Packed("Point".to_string()),
                ParallelTransferRejection::Packed("Point".to_string()),
            ),
            (PhpType::Pointer(None), ParallelTransferRejection::Pointer),
            (PhpType::Resource(None), ParallelTransferRejection::Resource),
            (
                PhpType::Array(Box::new(PhpType::Buffer(Box::new(PhpType::Int)))),
                ParallelTransferRejection::Buffer,
            ),
        ];
        for (ty, expected) in cases {
            assert_eq!(parallel_transfer_rejection(&ty), Some(expected), "{ty:?}");
        }
    }
}
