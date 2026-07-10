//! Purpose:
//! Defines type-system metadata for compiler-supported PHP Fiber behavior.
//! Tracks Fiber class relationships and result typing used by builtin type injection and checks.
//!
//! Called from:
//! - `crate::types::checker::builtin_types`
//! - `crate::types::checker`
//!
//! Key details:
//! - Fiber typing is modeled at compile time; runtime scheduling behavior remains outside this module.

use crate::errors::CompileError;
use crate::span::Span;

use super::FunctionSig;

/// Maximum number of start arguments a Fiber callback can accept.
pub(crate) const FIBER_START_ARG_LIMIT: usize = 7;

/// Returns the number of visible parameters for a Fiber callback signature.
/// For non-variadic functions this equals `param_count`; for variadic functions
/// it is `param_count + 1` to account for the variadic slot.
pub(crate) fn visible_param_count(param_count: usize, variadic: bool) -> usize {
    param_count + usize::from(variadic)
}

/// Validates that a `FunctionSig` is a valid Fiber callback signature.
/// Returns an error if the callback has more than `FIBER_START_ARG_LIMIT` fixed
/// start parameters, or has any fixed by-reference start parameters. Variadic
/// callbacks are allowed because wrappers build the variadic tail from the
/// supplied boxed start arguments.
pub(crate) fn validate_callback_signature(
    sig: &FunctionSig,
    visible_param_count: usize,
    span: Span,
) -> Result<(), CompileError> {
    let fixed_param_count = if sig.variadic.is_some() {
        visible_param_count.saturating_sub(1)
    } else {
        visible_param_count
    };

    if fixed_param_count > FIBER_START_ARG_LIMIT {
        return Err(CompileError::new(
            span,
            &format!(
                "Fiber callbacks support at most {} start arguments, got {}",
                FIBER_START_ARG_LIMIT, fixed_param_count
            ),
        ));
    }

    if sig
        .ref_params
        .iter()
        .take(fixed_param_count)
        .any(|by_ref| *by_ref)
    {
        return Err(CompileError::new(
            span,
            "Fiber callbacks cannot receive start arguments by reference",
        ));
    }

    Ok(())
}
