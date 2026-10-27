//! Purpose:
//! Validates selected enum method argument values before storage coercion or arity errors.
//!
//! Called from:
//! - Enum interface call rejection and runtime unpack materialization.
//!
//! Key details:
//! - Callers stage every source expression before checking parameter-order constraints.
//! - Weak scalar acceptance is distinct from casts that silently convert invalid values to zero.
//! - Diagnostics name the selected implementation, parameter and runtime PHP type.

use super::*;

/// Checks one supplied parameter while leaving its rooted value and ownership unchanged.
pub(super) fn guard_parameter(
    ctx: &mut LoweringContext<'_, '_>, signature: &FunctionSig, index: usize,
    variable: &Expr, class: &str, method: &str, span: Span,
) {
    guard_parameter_at(ctx, signature, index, variable, None, class, method, span);
}

/// Checks one variadic value using its unpacked PHP position rather than the collector slot.
pub(super) fn guard_variadic_element(
    ctx: &mut LoweringContext<'_, '_>, signature: &FunctionSig, index: usize,
    variable: &Expr, position: &Expr, class: &str, method: &str, span: Span,
) {
    guard_parameter_at(ctx, signature, index, variable, Some(position), class, method, span);
}

/// Builds a selected-implementation TypeError without consuming the caller's rooted value.
fn guard_parameter_at(
    ctx: &mut LoweringContext<'_, '_>, signature: &FunctionSig, index: usize,
    variable: &Expr, position: Option<&Expr>, class: &str, method: &str, span: Span,
) {
    let Some((name, ty)) = signature.params.get(index) else { return; };
    if !signature.declared_params.get(index).copied().unwrap_or(false) { return; }
    let accepted = if crate::source::current_strict_types() {
        crate::types::param_binding::runtime_strict_param_acceptance(ty, variable)
    } else {
        accepts_type(ty, variable, span)
    };
    let Some(accepted) = accepted else { return; };
    let accepted = lower_expr(ctx, &accepted);
    let ok = ctx.builder.create_named_block("enum.argument.type.ok", Vec::new());
    let bad = ctx.builder.create_named_block("enum.argument.type.bad", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: accepted.value, then_target: ok, then_args: Vec::new(),
        else_target: bad, else_args: Vec::new(),
    });
    ctx.builder.position_at_end(bad);
    let argument = signature.params.iter().take(index).filter(|(name, _)|
        name != crate::func_args::HIDDEN_ARGC_PARAM && name != crate::func_args::HIDDEN_ARGS_PARAM
    ).count() + 1;
    let expected = diagnostic_type(ty);
    let literal = |text| Expr::new(ExprKind::StringLiteral(text), span);
    let prefix = if let Some(position) = position {
        concat(literal(format!("{class}::{method}(): Argument #")),
            concat(position.clone(), literal(format!(" must be of type {expected}, ")), span), span)
    } else {
        literal(format!("{class}::{method}(): Argument #{argument} (${name}) must be of type {expected}, "))
    };
    let message = concat(prefix, concat(runtime_type(variable, ty, span),
        Expr::new(ExprKind::StringLiteral(" given".to_string()), span), span), span);
    throw_message(ctx, "TypeError", message, span);
    ctx.builder.position_at_end(ok);
    bind_implicit_integer(ctx, ty, variable, span);
}

/// Diagnoses and snapshots a selected integer conversion before a union erases its source tag.
fn bind_implicit_integer(
    ctx: &mut LoweringContext<'_, '_>, ty: &PhpType, variable: &Expr, span: Span,
) {
    if crate::source::current_strict_types() { return; }
    let Some(condition) = crate::types::param_binding::runtime_int_binding_needs_diagnostics(ty, variable)
        else { return; };
    let ExprKind::Variable(name) = &variable.kind else {
        unreachable!("enum argument guards only bind staged argument slots");
    };
    let condition = lower_expr(ctx, &condition);
    let convert = ctx.builder.create_named_block("enum.argument.int.convert", Vec::new());
    let done = ctx.builder.create_named_block("enum.argument.int.done", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: condition.value, then_target: convert, then_args: Vec::new(),
        else_target: done, else_args: Vec::new(),
    });
    ctx.builder.position_at_end(convert);
    let source = lower_expr(ctx, variable);
    let converted = ctx.emit_value(Op::Cast, vec![source.value], Some(Immediate::ImplicitIntCast),
        PhpType::Int, Op::Cast.default_effects(), Some(span));
    store_value_into_temp(ctx, name, PhpType::Mixed, converted, span);
    ctx.builder.terminate(Terminator::Br { target: done, args: Vec::new() });
    ctx.builder.position_at_end(done);
}

/// Builds the runtime predicate for PHP weak scalar and class parameter acceptance.
fn accepts_type(ty: &PhpType, value: &Expr, span: Span) -> Option<Expr> {
    let predicate = |name| predicate(name, value, span);
    let either = |a, b| Expr::new(ExprKind::BinaryOp {
        left: Box::new(a), op: BinOp::Or, right: Box::new(b),
    }, span);
    Some(match ty {
        PhpType::Int => crate::types::param_binding::runtime_weak_int_param_acceptance(value),
        PhpType::Float => either(predicate("is_numeric"), predicate("is_bool")),
        PhpType::Str => either(predicate("is_scalar"), Expr::new(ExprKind::InstanceOf {
            value: Box::new(value.clone()),
            target: InstanceOfTarget::Name(Name::unqualified("Stringable")),
        }, span)),
        PhpType::Bool => predicate("is_scalar"),
        PhpType::False => Expr::new(ExprKind::BinaryOp {
            left: Box::new(value.clone()), op: BinOp::StrictEq,
            right: Box::new(Expr::new(ExprKind::BoolLiteral(false), span)),
        }, span),
        PhpType::Iterable => either(predicate("is_array"), Expr::new(ExprKind::InstanceOf {
            value: Box::new(value.clone()),
            target: InstanceOfTarget::Name(Name::unqualified("Traversable")),
        }, span)),
        PhpType::Callable => predicate("is_callable"),
        PhpType::Array(_) | PhpType::AssocArray { .. } => predicate("is_array"),
        PhpType::Object(class) if class.is_empty() || class.eq_ignore_ascii_case("object") =>
            predicate("is_object"),
        PhpType::Object(class) => Expr::new(ExprKind::InstanceOf {
            value: Box::new(value.clone()), target: InstanceOfTarget::Name(Name::from(class.as_str())),
        }, span),
        PhpType::Union(types) => {
            let mut predicates = types.iter().filter_map(|ty| accepts_type(ty, value, span));
            let first = predicates.next()?;
            predicates.fold(first, either)
        }
        PhpType::Void => predicate("is_null"),
        _ => return None,
    })
}

/// Calls a shared builtin without rebuilding its registry metadata.
fn predicate(name: &str, value: &Expr, span: Span) -> Expr {
    Expr::new(ExprKind::FunctionCall {
        name: Name::unqualified(name), args: vec![value.clone()],
    }, span)
}

/// Produces PHP's diagnostic type names instead of gettype's historical aliases.
fn runtime_type(value: &Expr, expected: &PhpType, span: Span) -> Expr {
    runtime_type_with_bool_literals(value, contains_false(expected), span)
}

/// Produces PHP's `get_debug_type()` spelling for a runtime diagnostic.
pub(super) fn runtime_debug_type(value: &Expr, span: Span) -> Expr {
    runtime_type_with_bool_literals(value, true, span)
}

/// Builds a normalized runtime type expression with optional bool-value spelling.
fn runtime_type_with_bool_literals(value: &Expr, bool_literals: bool, span: Span) -> Expr {
    let mut result = predicate("gettype", value, span);
    for (test, text) in [
        ("is_null", "null"), ("is_bool", "bool"), ("is_float", "float"), ("is_int", "int"),
    ] {
        let text = if test == "is_bool" && bool_literals {
            Expr::new(ExprKind::Ternary {
                condition: Box::new(value.clone()),
                then_expr: Box::new(Expr::new(ExprKind::StringLiteral("true".into()), span)),
                else_expr: Box::new(Expr::new(ExprKind::StringLiteral("false".into()), span)),
            }, span)
        } else { Expr::new(ExprKind::StringLiteral(text.to_string()), span) };
        result = Expr::new(ExprKind::Ternary {
            condition: Box::new(predicate(test, value, span)),
            then_expr: Box::new(text),
            else_expr: Box::new(result),
        }, span);
    }
    Expr::new(ExprKind::Ternary {
        condition: Box::new(predicate("is_object", value, span)),
        then_expr: Box::new(predicate("get_class", value, span)),
        else_expr: Box::new(result),
    }, span)
}

/// Shares canonical PHP union and nullable spelling without changing declaration metadata.
fn diagnostic_type(ty: &PhpType) -> String {
    crate::types::diagnostic_names::php_type_name(ty)
}

/// Literal-bool constraints use PHP's true/false diagnostic names rather than generic bool.
fn contains_false(ty: &PhpType) -> bool {
    match ty {
        PhpType::False => true,
        PhpType::Union(members) => members.iter().any(contains_false),
        _ => false,
    }
}

/// Joins owned diagnostic fragments through ordinary EIR string lowering.
pub(super) fn concat(left: Expr, right: Expr, span: Span) -> Expr {
    Expr::new(ExprKind::BinaryOp {
        left: Box::new(left), op: BinOp::Concat, right: Box::new(right),
    }, span)
}

/// Raises a catchable exception while the caller's argument owners remain published.
pub(super) fn throw_message(
    ctx: &mut LoweringContext<'_, '_>, class: &str, message: Expr, span: Span,
) {
    let exception = lower_expr(ctx, &Expr::new(ExprKind::NewObject {
        class_name: Name::unqualified(class), args: vec![message],
    }, span));
    ctx.builder.terminate(Terminator::Throw { value: exception.value });
}
