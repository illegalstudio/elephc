//! Purpose:
//! Converts ordinary PHP closures in runtime fragments into interpreter functions.
//!
//! Called from:
//! - The Magician AST-to-EvalIR expression lowering.
//!
//! Key details:
//! - Captures keep the exact written order and reference mode.

use elephc_chopper::syntax::parser::ast::{Expr, ExprKind};

use crate::eval_ir::{EvalClosureCapture, EvalExpr, EvalFunction, EvalSourceLocation};

use super::{Scope, attributes::lower_attributes, lower_branch, signatures::lower_signature};

/// Lowers an anonymous function with its lexical captures and reflection range.
pub(super) fn lower_closure(scope: &Scope<'_>, expr: Expr) -> Option<EvalExpr> {
    let ExprKind::Closure {
        params,
        param_attributes,
        variadic,
        variadic_by_ref,
        variadic_type,
        return_type,
        body,
        is_arrow: false,
        is_static,
        by_ref_return: false,
        captures,
        capture_refs,
    } = expr.kind else {
        return None;
    };

    let signature = lower_signature(scope, params, variadic, variadic_by_ref, variadic_type, return_type, false)?;
    let mut lowered_captures: Vec<EvalClosureCapture> = Vec::with_capacity(captures.len());
    for name in captures {
        if signature.names.contains(&name) || lowered_captures.iter().any(|capture| capture.name() == name) {
            return None;
        }
        let by_ref = capture_refs.contains(&name);
        lowered_captures.push(EvalClosureCapture::new(name, by_ref));
    }
    let body = lower_branch(scope, body)?;
    super::validate_body_jumps(scope, &body)?;
    let span = expr.span;
    let parameter_attributes = param_attributes
        .into_iter()
        .map(|groups| lower_attributes(scope, groups))
        .collect();
    let function = EvalFunction::new(crate::parser::state::next_closure_function_name(), signature.names, body)
        .with_source_location(EvalSourceLocation::new(i64::from(span.line), i64::from(span.end_line)))
        .with_parameter_attributes(parameter_attributes)
        .with_parameter_types(signature.types)
        .with_parameter_defaults(signature.defaults)
        .with_parameter_by_ref_flags(signature.by_ref)
        .with_parameter_variadic_flags(signature.is_variadic)
        .with_return_type(signature.return_type)
        .with_strict_types(scope.strict_types);
    Some(EvalExpr::Closure { function, captures: lowered_captures, is_static })
}
