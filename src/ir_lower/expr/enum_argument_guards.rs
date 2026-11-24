//! Purpose:
//! Validates selected enum method argument values before storage coercion or arity errors.
//!
//! Called from:
//! - Enum interface call rejection and runtime unpack materialization.
//! - Function, method and closure entry, for by-reference parameters of a union of `null`,
//!   arrays and objects ([`guard_reference_parameter_entry`]).
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
pub(super) fn accepts_type(ty: &PhpType, value: &Expr, span: Span) -> Option<Expr> {
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
        PhpType::Callable | PhpType::DeclaredCallable => predicate("is_callable"),
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
pub(super) fn runtime_type(value: &Expr, expected: &PhpType, span: Span) -> Expr {
    runtime_type_with_bool_literals(value, contains_false(expected), span)
}

/// Produces PHP's `get_debug_type()` spelling for a runtime diagnostic.
pub(crate) fn runtime_debug_type(value: &Expr, span: Span) -> Expr {
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
    let object_type = Expr::new(ExprKind::Ternary {
        condition: Box::new(predicate("is_object", value, span)),
        then_expr: Box::new(predicate("get_class", value, span)),
        else_expr: Box::new(result),
    }, span);
    // A native object hint can still carry a missing-selection null word.
    // Test that representation before static object predicates select get_class.
    Expr::new(ExprKind::Ternary {
        condition: Box::new(predicate("is_null", value, span)),
        then_expr: Box::new(Expr::string_lit("null")),
        else_expr: Box::new(object_type),
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

/// Re-checks every by-reference property write performed by the call that just returned.
///
/// `mark` is the guard-list length captured before the call's arguments were lowered, so only
/// guards produced by this call are drained. The property is untouched while the callee runs
/// (it receives a boxed temporary); a refused value therefore needs no restoration, and the
/// `TypeError` is thrown before the write-back.
pub(super) fn drain_reference_property_guards(
    ctx: &mut LoweringContext<'_, '_>,
    mark: usize,
    span: Span,
) {
    if ctx.reference_property_guards.len() <= mark {
        return;
    }
    let guards: Vec<_> = ctx.reference_property_guards.drain(mark..).collect();
    for guard in guards {
        let temp_expr = Expr::new(ExprKind::Variable(guard.temp.clone()), span);
        let Some(declared) = guard.declared.clone() else {
            super::assignments::lower_non_local_assignment_write(
                ctx, &guard.place, &temp_expr, guard.place.span,
            );
            continue;
        };
        let accepted = if crate::source::current_strict_types() {
            crate::types::param_binding::runtime_strict_param_acceptance(&declared, &temp_expr)
        } else {
            accepts_type(&declared, &temp_expr, span)
        };
        let Some(predicate) = accepted else {
            super::assignments::lower_non_local_assignment_write(
                ctx, &guard.place, &temp_expr, guard.place.span,
            );
            continue;
        };
        let condition = lower_expr(ctx, &predicate);
        let ok = ctx.builder.create_named_block("ref_property.write.ok", Vec::new());
        let bad = ctx.builder.create_named_block("ref_property.write.bad", Vec::new());
        ctx.builder.terminate(Terminator::CondBr {
            cond: condition.value,
            then_target: ok,
            then_args: Vec::new(),
            else_target: bad,
            else_args: Vec::new(),
        });
        ctx.builder.position_at_end(bad);
        let type_name = diagnostic_type(&declared);
        let value_type = runtime_type(&temp_expr, &declared, span);
        let suffix = format!(
            " to reference held by property {}::${} of type {}",
            guard.class_name, guard.property, type_name
        );
        let message = concat(
            Expr::new(ExprKind::StringLiteral("Cannot assign ".to_string()), span),
            concat(
                value_type,
                Expr::new(ExprKind::StringLiteral(suffix), span),
                span,
            ),
            span,
        );
        throw_message(ctx, "TypeError", message, span);
        ctx.builder.position_at_end(ok);
        super::assignments::lower_non_local_assignment_write(
            ctx, &guard.place, &temp_expr, guard.place.span,
        );
    }
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

/// Whether a by-reference parameter of type `ty` is checked at callee entry: a union whose
/// members are all `null`, arrays or objects (`?array`, `?A`, `array|A`, `A|B|null`). Such a
/// parameter is a boxed cell in the callee, which a caller's `mixed` local may fill with any
/// value; these members never coerce, so the check is exact.
pub(crate) fn reference_parameter_needs_entry_guard(ty: &PhpType) -> bool {
    matches!(ty, PhpType::Union(members) if members.iter().all(|member| {
        member.is_php_array()
            || matches!(
                member,
                PhpType::Void | PhpType::Object(_) | PhpType::Array(_) | PhpType::AssocArray { .. }
            )
    }))
}

/// Checks every element a `callable ...$name` variadic (or `?callable ...$name`, `callable|false
/// ...$name`) collected, on entry to the callee, raising php's `f(): Argument #N must be of type
/// T, U given` `TypeError` for the first element the declared element type does not accept.
/// `first_position` is the 1-based argument number of the first element.
pub(crate) fn guard_declared_callable_variadic_entry(
    ctx: &mut LoweringContext<'_, '_>,
    function: &str,
    first_position: usize,
    name: &str,
    element_type: &PhpType,
) {
    let span = Span::synthetic();
    let key = ctx.declare_hidden_temp(PhpType::Mixed);
    let value = ctx.declare_hidden_temp(PhpType::Mixed);
    let number = ctx.declare_hidden_temp(PhpType::Int);
    let variable = |local: &str| Expr::new(ExprKind::Variable(local.to_string()), span);
    let literal = |text: String| Expr::new(ExprKind::StringLiteral(text), span);
    let element = variable(&value);
    // Positional entries are numbered from the variadic's position; a named entry (string key)
    // reports the next positional number and does not consume one, as PHP does.
    crate::ir_lower::stmt::lower_stmt(ctx, &Stmt::assign(
        &number,
        Expr::new(ExprKind::IntLiteral(first_position as i64), span),
    ));
    let argument = variable(&number);
    let message = concat(
        literal(format!("{function}(): Argument #")),
        concat(argument, concat(
            literal(format!(" must be of type {}, ", diagnostic_type(element_type))),
            concat(runtime_debug_type(&element, span), literal(" given".to_string()), span),
            span,
        ), span),
        span,
    );
    let exception = Expr::new(ExprKind::NewObject {
        class_name: Name::unqualified("TypeError"),
        args: vec![message],
    }, span);
    let accepted = accepts_type(element_type, &element, span)
        .unwrap_or_else(|| predicate("is_callable", &element, span));
    let check = Stmt::new(StmtKind::If {
        condition: Expr::new(ExprKind::Not(Box::new(accepted)), span),
        then_body: vec![Stmt::new(StmtKind::Throw(exception), span)],
        elseif_clauses: Vec::new(),
        else_body: None,
    }, span);
    let advance = Stmt::new(StmtKind::If {
        condition: predicate("is_int", &variable(&key), span),
        then_body: vec![Stmt::assign(&number, Expr::new(ExprKind::BinaryOp {
            left: Box::new(variable(&number)),
            op: BinOp::Add,
            right: Box::new(Expr::new(ExprKind::IntLiteral(1), span)),
        }, span))],
        elseif_clauses: Vec::new(),
        else_body: None,
    }, span);
    let guard = Stmt::new(StmtKind::Foreach {
        array: variable(name),
        key_var: Some(key),
        value_var: value,
        value_by_ref: false,
        body: vec![check, advance],
    }, span);
    crate::ir_lower::stmt::lower_stmt(ctx, &guard);
}

/// Checks a value returned through a declared `callable` (or a union with `callable`) before
/// it leaves the function, raising php's `f(): Return value must be of type T, U returned`
/// `TypeError`.
pub(crate) fn guard_declared_callable_return(
    ctx: &mut LoweringContext<'_, '_>,
    function: &str,
    value: LoweredValue,
    return_type: &PhpType,
    span: Span,
) {
    let prefix = format!(
        "{function}(): Return value must be of type {}, ",
        diagnostic_type(return_type)
    );
    guard_declared_callable_value(ctx, value, return_type, prefix, " returned", span);
}

/// Checks `value` against a declared `callable` (or a union with `callable`), raising a
/// `TypeError` whose message is `prefix`, the value's `get_debug_type()` spelling, then `suffix`.
///
/// The value keeps its owner; the check reads a retained copy in a hidden temp and releases
/// it on both edges, so the check never outlives the value's last visible owner. An owning
/// temporary (a property read, a call result) is pinned across the check, so the throw
/// releases it; the ok path retires the pin without a release and the caller keeps it.
pub(crate) fn guard_declared_callable_value(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    declared: &PhpType,
    prefix: String,
    suffix: &str,
    span: Span,
) {
    let value_type = ctx.builder.value_php_type(value.value);
    let temp = ctx.declare_hidden_temp(value_type.clone());
    let copy = crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span));
    store_value_into_temp(ctx, &temp, value_type, copy, span);
    let checked = Expr::new(ExprKind::Variable(temp.clone()), span);
    let Some(accepted) = accepts_type(declared, &checked, span) else {
        release_checked_copy(ctx, &temp, span);
        return;
    };
    let pins = crate::ir_lower::expr::pin_in_flight_owners(ctx, &[value.value], span);
    let accepted = lower_expr(ctx, &accepted);
    let ok = ctx.builder.create_named_block("callable.value.ok", Vec::new());
    let bad = ctx.builder.create_named_block("callable.value.bad", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: accepted.value, then_target: ok, then_args: Vec::new(),
        else_target: bad, else_args: Vec::new(),
    });
    ctx.builder.position_at_end(bad);
    let literal = |text: String| Expr::new(ExprKind::StringLiteral(text), span);
    let message = concat(
        literal(prefix),
        concat(runtime_debug_type(&checked, span), literal(suffix.to_string()), span),
        span,
    );
    let exception = lower_expr(ctx, &Expr::new(ExprKind::NewObject {
        class_name: Name::unqualified("TypeError"), args: vec![message],
    }, span));
    release_checked_copy(ctx, &temp, span);
    ctx.builder.terminate(Terminator::Throw { value: exception.value });
    ctx.builder.position_at_end(ok);
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, pins, span);
    release_checked_copy(ctx, &temp, span);
}

/// Releases the copy a check read and clears its slot, so the checked value dies when its
/// owner lets go of it rather than when the frame returns.
fn release_checked_copy(ctx: &mut LoweringContext<'_, '_>, temp: &str, span: Span) {
    let slot = ctx.local_slots[temp];
    ctx.emit_void(Op::ReleaseLocalSlot, Vec::new(), Some(Immediate::LocalSlot(slot)),
        Op::ReleaseLocalSlot.default_effects(), Some(span));
    ctx.emit_void(Op::UnsetLocal, Vec::new(), Some(Immediate::LocalSlot(slot)),
        Op::UnsetLocal.default_effects(), Some(span));
}

/// Checks a by-reference parameter's value against its declared union on entry to the callee,
/// after every argument was evaluated (php's `RECV` order), raising php's
/// `f(): Argument #N ($name) must be of type T, U given` `TypeError` for anything else.
pub(crate) fn guard_reference_parameter_entry(
    ctx: &mut LoweringContext<'_, '_>,
    function: &str,
    position: usize,
    name: &str,
    ty: &PhpType,
) {
    guard_reference_argument_value(ctx, function, position, name, name, ty, "");
}

/// Checks the value `local` holds against by-reference parameter `name` of type `ty`, throwing
/// `<function>(): Argument #<position> ($name) must be of type T, U given<suffix>`.
///
/// The callee entry checks its own parameter; a caller that runs the check in parameter order
/// with its other deferred checks passes its local and the `, called in` suffix.
pub(crate) fn guard_reference_argument_value(
    ctx: &mut LoweringContext<'_, '_>,
    function: &str,
    position: usize,
    name: &str,
    local: &str,
    ty: &PhpType,
    suffix: &str,
) {
    let span = Span::synthetic();
    let variable = Expr::new(ExprKind::Variable(local.to_string()), span);
    let Some(accepted) = accepts_type(ty, &variable, span) else { return; };
    let accepted = lower_expr(ctx, &accepted);
    let ok = ctx.builder.create_named_block("ref.param.type.ok", Vec::new());
    let bad = ctx.builder.create_named_block("ref.param.type.bad", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: accepted.value, then_target: ok, then_args: Vec::new(),
        else_target: bad, else_args: Vec::new(),
    });
    ctx.builder.position_at_end(bad);
    let literal = |text| Expr::new(ExprKind::StringLiteral(text), span);
    let prefix = literal(format!(
        "{function}(): Argument #{position} (${name}) must be of type {}, ",
        diagnostic_type(ty)
    ));
    let message = concat(prefix, concat(runtime_debug_type(&variable, span),
        literal(format!(" given{suffix}")), span), span);
    throw_message(ctx, "TypeError", message, span);
    ctx.builder.position_at_end(ok);
}
