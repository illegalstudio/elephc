//! Purpose:
//! Converts representable PHP default expressions into eval metadata.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - Constant evaluation stays bounded and PHP-equivalent for supported values.
//! - The folding itself lives in `crate::codegen::const_default_values`, shared with the
//!   descriptor callable invoker, so eval registration and runtime materialization can never
//!   disagree about which defaults are representable. This module only adapts the eval
//!   `EvalNativeDefaultContext` to that resolver.

use super::*;

use crate::codegen::const_default_values::{
    const_default_bool as shared_bool_default, const_default_float as shared_float_default,
    const_default_int_value as shared_int_default, resolve_array_const_default,
    resolve_const_default_at, resolve_literal_const_default,
};

/// Adapts one eval default context to the shared constant resolver scope.
fn shared_default_context<'a>(
    default_context: &EvalNativeDefaultContext<'a>,
) -> crate::codegen::const_default_values::ConstDefaultContext<'a> {
    crate::codegen::const_default_values::ConstDefaultContext {
        module: default_context.module,
        current_class: default_context.current_class,
    }
}

/// Converts a PHP signature default into the compact eval bridge default ABI.
pub(super) fn eval_native_callable_default(
    expr: &Expr,
    default_context: &EvalNativeDefaultContext<'_>,
) -> Option<EvalNativeCallableDefault> {
    eval_native_callable_default_at(expr, default_context, 0)
}

/// Converts a PHP default expression while preserving a recursion limit for constants.
pub(super) fn eval_native_callable_default_at(
    expr: &Expr,
    default_context: &EvalNativeDefaultContext<'_>,
    depth: usize,
) -> Option<EvalNativeCallableDefault> {
    resolve_const_default_at(expr, &shared_default_context(default_context), depth)
}

/// Builds one bool default metadata value.
pub(super) fn eval_native_bool_default(value: bool) -> EvalNativeCallableDefault {
    shared_bool_default(value)
}

/// Builds one int default metadata value.
pub(super) fn eval_native_int_default(value: i64) -> EvalNativeCallableDefault {
    shared_int_default(value)
}

/// Builds one float default metadata value.
pub(super) fn eval_native_float_default(value: f64) -> EvalNativeCallableDefault {
    shared_float_default(value)
}

/// Converts scalar/string/empty-array defaults into the compact eval bridge default ABI.
pub(super) fn eval_native_literal_default(expr: &Expr) -> Option<EvalNativeCallableDefault> {
    resolve_literal_const_default(expr)
}

/// Converts supported array-valued defaults into compact eval bridge metadata.
pub(super) fn eval_native_array_default(
    expr: &Expr,
    default_context: &EvalNativeDefaultContext<'_>,
    depth: usize,
) -> Option<EvalNativeCallableDefault> {
    resolve_array_const_default(expr, &shared_default_context(default_context), depth)
}
