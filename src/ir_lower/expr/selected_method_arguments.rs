//! Purpose:
//! Shares runtime-selected method argument validation between enum and source-method dispatch.
//!
//! Called from:
//! - EIR class selection before lowering arguments against a concrete method signature.
//!
//! Key details:
//! - The shared CallArgPlan remains authoritative for names, spreads, defaults and arity.
//! - A rejected candidate evaluates supplied arguments once before its catchable error.

use super::*;

/// Retains binding-failure provenance so named holes fail before supplied-argument guards.
struct MethodArgumentError {
    exception_class: &'static str,
    message: String,
    guard_supplied_arguments: bool,
}

/// Rejects an invalid selected binding without letting raw arguments reach its physical ABI.
pub(super) fn reject_invalid(
    ctx: &mut LoweringContext<'_, '_>, class: &str, method: &str,
    signature: &FunctionSig, args: &[Expr], span: Span,
) -> bool {
    // PHP treats defaults before a later required parameter as required,
    // including named omissions. Keep declaration metadata unchanged.
    let regular = crate::types::call_args::regular_param_count(signature);
    let binding_signature = crate::types::call_args::required_argument_binding_signature(signature);
    let plan = crate::types::call_args::plan_call_args_with_regular_param_count_and_assoc_spreads(
        &binding_signature, args, span, regular,
        false, signature.variadic.is_some()
            && !crate::func_args::sig_collects_surplus_args(signature),
        &assoc_spread_sources(ctx, args),
    );
    let error = match plan {
        Ok(plan) => plan.statically_missing_positionals(signature).map(|(provided, required)| {
            let bound = if required < crate::types::call_args::regular_param_count(signature) {
                "at least"
            } else { "exactly" };
            MethodArgumentError {
                exception_class: "ArgumentCountError",
                message: format!(
                    "Too few arguments to function {class}::{method}(), {provided} passed and {bound} {required} expected"
                ),
                guard_supplied_arguments: true,
            }
        }),
        Err(error) => Some(method_argument_error(
            class, method, signature, args, error,
        )),
    };
    if let Some(error) = error {
        terminate_method_argument_error(ctx, args, Some((class, method, signature)),
            error, span);
        return true;
    }
    false
}

/// Evaluates rejected branch arguments once and throws through ordinary operand-owner cleanup.
fn terminate_method_argument_error(
    ctx: &mut LoweringContext<'_, '_>, args: &[Expr],
    selected: Option<(&str, &str, &FunctionSig)>,
    error: MethodArgumentError, span: Span,
) {
    let mut staged = Vec::new();
    for arg in args {
        let value = lower_call_source_arg(ctx, arg);
        let value = LoweredValue { value, ir_type: ctx.builder.value_type(value) };
        let ty = ctx.builder.value_php_type(value.value);
        let slot = ctx.declare_synthetic_php_local(ty.clone());
        store_value_into_temp(ctx, &slot, ty, value, arg.span);
        register_owned_call_operand(ctx, ctx.local_slots[&slot], arg.span);
        staged.push(slot);
    }
    if error.guard_supplied_arguments {
        if let Some((class, method, signature)) = selected {
            let bindings = staged_argument_bindings(ctx, args, &staged, signature, span);
            for (index, value) in bindings {
                // Shared predicates and implicit casts require rooted Mixed storage even for
                // directly supplied typed values, not only tagged reads projected from spreads.
                let argument = lower_expr(ctx, &value);
                // Identity boxing would consume the owner of an existing Mixed projection.
                let argument = if matches!(argument.ir_type,
                    IrType::Heap(IrHeapKind::Mixed | IrHeapKind::Union))
                {
                    argument
                } else { ctx.box_value_as_mixed(argument, PhpType::Mixed, Some(span)) };
                let slot = ctx.declare_synthetic_php_local(PhpType::Mixed);
                store_value_into_temp(ctx, &slot, PhpType::Mixed, argument, span);
                register_owned_call_operand(ctx, ctx.local_slots[&slot], span);
                let value = Expr::new(ExprKind::Variable(slot), span);
                enum_argument_guards::guard_parameter(ctx, signature, index,
                    &value, class, method, span);
            }
        }
    }
    let exception = Expr::new(ExprKind::NewObject {
        class_name: Name::unqualified(error.exception_class),
        args: vec![Expr::new(ExprKind::StringLiteral(error.message), span)],
    }, span);
    let exception = lower_expr(ctx, &exception);
    ctx.builder.terminate(Terminator::Throw { value: exception.value });
}

/// Plans supplied parameter guards from staged values without reevaluating unpack expressions.
fn staged_argument_bindings(
    ctx: &mut LoweringContext<'_, '_>, args: &[Expr], staged: &[String],
    signature: &FunctionSig, span: Span,
) -> Vec<(usize, Expr)> {
    let mut projected = Vec::new();
    for (arg, slot) in args.iter().zip(staged) {
        let source = Expr::new(ExprKind::Variable(slot.clone()), span);
        let element = |index: Expr| Expr::new(ExprKind::ArrayAccess {
            array: Box::new(source.clone()), index: Box::new(index),
        }, span);
        match &arg.kind {
            ExprKind::Spread(inner) => match &inner.kind {
                ExprKind::ArrayLiteral(items)
                    if !items.iter().any(|item| matches!(item.kind, ExprKind::Spread(_))) =>
                {
                    projected.extend(items.iter().enumerate().map(|(index, _)|
                        element(Expr::new(ExprKind::IntLiteral(index as i64), span))));
                }
                ExprKind::ArrayLiteralAssoc(pairs) => {
                    let pairs = pairs.iter().map(|(key, _)| {
                        // Key conversion already ran while staging the source array. Reuse
                        // the diagnosed-key coercion rather than running PHP diagnostics again.
                        let lookup = if matches!(key.kind, ExprKind::FloatLiteral(_)) {
                            let value = lower_expr(ctx, key);
                            let value = coerce_array_key_to_int_at_span(ctx, value, Some(key.span), true);
                            let slot = ctx.declare_synthetic_php_local(PhpType::Int);
                            store_value_into_temp(ctx, &slot, PhpType::Int, value, key.span);
                            Expr::new(ExprKind::Variable(slot), span)
                        } else { key.clone() };
                        (key.clone(), element(lookup))
                    }).collect();
                    projected.push(Expr::new(ExprKind::Spread(Box::new(Expr::new(
                        ExprKind::ArrayLiteralAssoc(pairs), arg.span,
                    ))), arg.span));
                }
                _ => return Vec::new(),
            },
            ExprKind::NamedArg { name, .. } => projected.push(Expr::new(ExprKind::NamedArg {
                name: name.clone(), value: Box::new(source),
            }, arg.span)),
            _ => projected.push(source),
        }
    }
    let projected = crate::types::call_args::expand_static_assoc_spread_args(&projected);
    if projected.iter().any(is_spread_arg) { return Vec::new(); }
    // Defaults are placeholders for the shared planner's omitted-slot provenance only.
    // They are never evaluated, and guard iteration stops at the first required omission.
    let regular = crate::types::call_args::regular_param_count(signature);
    let required = crate::types::call_args::required_param_count(signature);
    let mut partial = signature.clone();
    for default in partial.defaults.iter_mut().take(regular) {
        if default.is_none() { *default = Some(Expr::new(ExprKind::Null, span)); }
    }
    crate::types::call_args::plan_call_args(&partial, &projected, span, false, false)
        .map(|plan| {
            let defaults = plan.default_argument_mask();
            plan.normalized_args().into_iter().take(regular).enumerate()
                .take_while(|(parameter, _)|
                    !defaults.get(*parameter).copied().unwrap_or(false) || *parameter >= required)
                .filter(|(parameter, _)| !defaults.get(*parameter).copied().unwrap_or(false))
                .collect()
        })
        .unwrap_or_default()
}

/// Counts statically projectable values while retaining named-argument entries.
fn statically_supplied_argument_count(args: &[Expr]) -> Option<usize> {
    args.iter().try_fold(0usize, |count, arg| match &arg.kind {
        ExprKind::Spread(inner) => match &inner.kind {
            ExprKind::ArrayLiteral(items)
                if !items.iter().any(|item| matches!(item.kind, ExprKind::Spread(_))) =>
            {
                Some(count + items.len())
            }
            _ => None,
        },
        _ => Some(count + 1),
    })
}

/// Maps shared binding failures to PHP's selected implementation exception and diagnostic.
fn method_argument_error(
    class: &str, method: &str, signature: &FunctionSig, args: &[Expr],
    error: crate::types::call_args::CallArgPlanError,
) -> MethodArgumentError {
    use crate::types::call_args::CallArgPlanError;
    let (exception_class, message) = match error {
        CallArgPlanError::MissingRequired { param_idx, .. } => {
            let expanded = crate::types::call_args::expand_static_assoc_spread_args(args);
            let later_named_parameter = expanded.iter().any(|arg| {
                let ExprKind::NamedArg { name, .. } = &arg.kind else { return false };
                crate::types::call_args::named_param_index(
                    signature,
                    crate::types::call_args::regular_param_count(signature),
                    name,
                ).is_some_and(|index| index > param_idx)
            });
            if !later_named_parameter {
                if let Some(provided) = statically_supplied_argument_count(&expanded) {
                    let required = crate::types::call_args::required_param_count(signature);
                    let bound = if required < crate::types::call_args::regular_param_count(signature) {
                        "at least"
                    } else {
                        "exactly"
                    };
                    return MethodArgumentError {
                        exception_class: "ArgumentCountError",
                        message: format!(
                            "Too few arguments to function {class}::{method}(), {} passed and {bound} {required} expected",
                            provided,
                        ),
                        guard_supplied_arguments: true,
                    };
                }
            }
            (
                "ArgumentCountError",
                format!(
                    "{class}::{method}(): Argument #{} (${}) not passed",
                    param_idx + 1,
                    signature.params[param_idx].0,
                ),
            )
        }
        CallArgPlanError::UnknownNamed { name, .. } =>
            ("Error", format!("Unknown named parameter ${name}")),
        CallArgPlanError::Duplicate { name, .. } =>
            ("Error", format!("Named parameter ${name} overwrites previous argument")),
        _ => ("Error", format!("Invalid arguments to method {method}()")),
    };
    MethodArgumentError { exception_class, message, guard_supplied_arguments: false }
}
