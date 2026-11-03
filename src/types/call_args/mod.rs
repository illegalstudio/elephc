//! Purpose:
//! Exposes the shared semantic planner for function-like call arguments.
//! Normalizes positional, named, variadic, and static spread arguments for checker and codegen consumers.
//!
//! Called from:
//! - `crate::types::checker::functions`
//! - `crate::codegen::expr::calls`
//!
//! Key details:
//! - All call surfaces must use this planner so PHP named/spread semantics stay consistent.

mod matching;
mod plan;
mod planner;
mod static_spread;

pub(crate) use matching::{
    by_ref_suffix_start, by_ref_suffix_start_of, named_param_index, spread_binding_window, positional_param_type, regular_param_count,
    required_param_count,
    required_argument_binding_signature, spread_reaches_by_ref_suffix,
    variadic_element_type,
};
pub(crate) use plan::{
    CallArgPlan, CallArgPlanError, PlannedRegularArg, PlannedSourceValue, SpreadBoundsCheck,
};
pub(crate) use planner::{
    plan_call_args, plan_call_args_with_regular_param_count_and_assoc_spreads,
    validate_no_spread_after_named,
};
pub(crate) use static_spread::{
    coalesce_planned_indexed_spreads, coalesce_planned_indexed_spreads_with_prefix,
    expand_planned_positional_spreads, expand_static_assoc_spread_args, has_named_args,
};

/// Rewrites a `Closure::bind()` scope given as an object's class NAME into the object itself.
///
/// php accepts the new scope as an object or as a class name and treats an object as its class,
/// so `Closure::bind($f, $kernel, $kernel::class)` and `Closure::bind($f, $kernel, $kernel)` are
/// the same call. Only the object form was honoured for a class known at run time: the name form
/// left the closure in the global scope, and every private or protected member it touched raised
/// "Cannot access private property" -- Symfony's debug commands bind `$this->buildContainer()`
/// to the kernel exactly this way. `get_class($object)` is the same name.
pub(crate) fn closure_bind_args_with_object_scope(
    args: &[crate::parser::ast::Expr],
) -> std::borrow::Cow<'_, [crate::parser::ast::Expr]> {
    use crate::parser::ast::ExprKind;
    let Some(scope) = args.get(2) else {
        return std::borrow::Cow::Borrowed(args);
    };
    let object = match &scope.kind {
        ExprKind::ObjectClassName { object } => Some(object.as_ref().clone()),
        ExprKind::FunctionCall { name, args: inner }
            if crate::names::php_symbol_key(name.as_str().trim_start_matches('\\'))
                == "get_class"
                && inner.len() == 1
                && !matches!(inner[0].kind, ExprKind::Spread(_) | ExprKind::NamedArg { .. }) =>
        {
            Some(inner[0].clone())
        }
        _ => None,
    };
    match object {
        Some(object) => {
            let mut rewritten = args.to_vec();
            rewritten[2] = object;
            std::borrow::Cow::Owned(rewritten)
        }
        None => std::borrow::Cow::Borrowed(args),
    }
}
