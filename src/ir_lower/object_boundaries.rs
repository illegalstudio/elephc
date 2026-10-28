//! Purpose:
//! Rejects runtime nulls at declared non-null object parameter and return boundaries.
//!
//! Called from:
//! - Function entry lowering and declared return storage coercion.
//!
//! Key details:
//! - Native property reads can return null despite their useful static object class hint.
//! - The existing frontend verifies nominal classes; these guards enforce non-nullness.
//! - All call surfaces share the callee entry, including references and inlined methods.

use crate::ir::{Op, Terminator};
use crate::ir_lower::context::{LoweredValue, LoweringContext};
use crate::span::Span;
use crate::types::PhpType;
use crate::parser::ast::{BinOp, Expr, ExprKind, Stmt, StmtKind};

/// Checks declared object parameters before user statements or constructor promotions run.
pub(super) fn guard_parameters(ctx: &mut LoweringContext<'_, '_>) {
    let Some(signature) = ctx.builder.function().signature.clone() else { return; };
    let callable = callable_label(ctx);
    for (index, (name, ty)) in signature.params.iter().enumerate() {
        if signature.variadic.as_ref() == Some(name) {
            if let Some(hint) = crate::types::signatures::variadic_source_element_type_expr(&signature) {
                let element = resolve_context_type(ctx, ctx.type_expr_to_php_type_for_value(hint));
                if crate::types::builtin_classes::native_object_sink_needs_null_guard(&element, ctx.classes, ctx.interfaces) {
                    guard_variadic(ctx, name, index + 1, &element, &callable);
                }
            }
            continue;
        }
        let declared_type = if *ty == PhpType::Mixed && signature.ref_params.get(index).copied().unwrap_or(false) {
            signature.param_type_exprs.get(index).and_then(Option::as_ref)
                .map(|hint| ctx.type_expr_to_php_type_for_value(hint)).unwrap_or_else(|| ty.clone())
        } else { ty.clone() };
        let declared_type = resolve_context_type(ctx, declared_type);
        if name == "this" || !signature.declared_params.get(index).copied().unwrap_or(false)
            || !crate::types::builtin_classes::native_object_sink_needs_null_guard(&declared_type, ctx.classes, ctx.interfaces)
        { continue; }
        let value = ctx.load_local(name, None);
        let expected = crate::types::diagnostic_names::php_type_name(&declared_type);
        let message = format!(
            "{callable}(): Argument #{} (${name}) must be of type {expected}, null given", index + 1,
        );
        reject_null(ctx, value, &message, Span::dummy(), true);
    }
}

/// Resolves relative class hints before testing native inheritance or spelling PHP errors.
fn resolve_context_type(ctx: &LoweringContext<'_, '_>, ty: PhpType) -> PhpType {
    match ty {
        PhpType::Object(class) if class.eq_ignore_ascii_case("self") || class.eq_ignore_ascii_case("static") =>
            PhpType::Object(ctx.current_class.clone().unwrap_or(class)),
        PhpType::Object(class) if class.eq_ignore_ascii_case("parent") => PhpType::Object(
            ctx.current_class.as_ref().and_then(|name| ctx.classes.get(name))
                .and_then(|info| info.parent.clone()).unwrap_or(class)),
        PhpType::Union(members) => PhpType::Union(members.into_iter().map(|member| resolve_context_type(ctx, member)).collect()),
        other => other,
    }
}

/// Checks collector values through ordinary foreach lowering, including named and reference tails.
fn guard_variadic(ctx: &mut LoweringContext<'_, '_>, name: &str, first: usize, ty: &PhpType, callable: &str) {
    let span = Span::dummy();
    let key = ctx.declare_hidden_temp(PhpType::Mixed);
    let value = ctx.declare_hidden_temp(PhpType::Mixed);
    let number = ctx.declare_hidden_temp(PhpType::Int);
    for local in [&key, &value] {
        crate::ir_lower::stmt::lower_stmt(ctx, &Stmt::assign(local, Expr::new(ExprKind::Null, span)));
    }
    crate::ir_lower::stmt::lower_stmt(ctx, &Stmt::assign(&number, Expr::int_lit(first as i64)));
    let expected = crate::types::diagnostic_names::php_type_name(ty);
    let message = Expr::binop(Expr::binop(Expr::string_lit(format!("{callable}(): Argument #")),
        BinOp::Concat, Expr::var(&number)), BinOp::Concat,
        Expr::string_lit(format!(" must be of type {expected}, null given")));
    let error = Expr::new(ExprKind::NewObject {
        class_name: crate::names::Name::unqualified("TypeError"), args: vec![message],
    }, span);
    let call = |function: &str, variable: &str| Expr::new(ExprKind::FunctionCall {
        name: crate::names::Name::unqualified(function), args: vec![Expr::var(variable)],
    }, span);
    let reject = Stmt::new(StmtKind::If {
        condition: call("is_null", &value), then_body: vec![Stmt::new(StmtKind::Throw(error), span)],
        elseif_clauses: vec![], else_body: None,
    }, span);
    // Named entries all use the next positional argument number, regardless of their order.
    let advance = Stmt::new(StmtKind::If {
        condition: call("is_int", &key),
        then_body: vec![Stmt::assign(&number, Expr::binop(Expr::var(&number), BinOp::Add, Expr::int_lit(1)))],
        elseif_clauses: vec![], else_body: None,
    }, span);
    crate::ir_lower::stmt::lower_stmt(ctx, &Stmt::new(StmtKind::Foreach {
        array: Expr::var(name), key_var: Some(key), value_var: value,
        value_by_ref: false, body: vec![reject, advance],
    }, span));
}

/// Selects generators that need an eager validator before constructing their lazy coroutine.
pub(super) fn needs_parameter_guards(
    signature: &crate::types::FunctionSig,
    classes: &std::collections::HashMap<String, crate::types::ClassInfo>,
    interfaces: &std::collections::HashMap<String, crate::types::InterfaceInfo>,
) -> bool {
    signature.params.iter().enumerate().any(|(index, (name, ty))| {
        if !signature.declared_params.get(index).copied().unwrap_or(false) { return false; }
        let hint = signature.param_type_exprs.get(index).and_then(Option::as_ref);
        let declared = if signature.variadic.as_ref() == Some(name)
            || (*ty == PhpType::Mixed && signature.ref_params.get(index).copied().unwrap_or(false)) {
            hint.map(crate::ir_lower::context::type_expr_to_php_type).unwrap_or_else(|| ty.clone())
        } else { ty.clone() };
        crate::types::builtin_classes::native_object_sink_needs_null_guard(&declared, classes, interfaces)
            || (has_relative_class(&declared)
                && classes.keys().any(|class| crate::types::builtin_classes::has_native_dimensions(class, classes)))
    })
}

/// Checks a declared object return before a nullable box loses its runtime null tag.
pub(super) fn guard_return(
    ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Option<Span>,
) {
    if !ctx.return_type_is_declared
        || !crate::types::builtin_classes::native_object_sink_needs_null_guard(
            &ctx.return_php_type, ctx.classes, ctx.interfaces,
        )
    {
        return;
    }
    let expected = crate::types::diagnostic_names::php_type_name(&ctx.return_php_type);
    let callable = callable_label(ctx);
    let message = format!("{callable}(): Return value must be of type {expected}, null returned");
    reject_null(ctx, value, &message, span.unwrap_or_else(Span::dummy), false);
}

/// Gives property accessors the PHP label rather than their synthetic method symbol.
fn callable_label(ctx: &LoweringContext<'_, '_>) -> String {
    if let Some(label) = &ctx.builder.function().display_name { return label.clone(); }
    if let Some((class, method)) = ctx.owner_name().split_once("::") {
        if let Some(info) = ctx.classes.get(class) {
            for property in info.property_hooks.keys() {
                for (kind, accessor) in [("get", crate::names::property_hook_get_method(property)),
                    ("set", crate::names::property_hook_set_method(property))] {
                    if method.eq_ignore_ascii_case(&accessor) { return format!("{class}::${property}::{kind}"); }
                }
            }
        }
    }
    ctx.owner_name().to_string()
}

/// Branches on the shared null representation and balances a temporary on both outcomes.
fn reject_null(
    ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, message: &str,
    span: Span, consume_read: bool,
) {
    let condition = ctx.emit_value(Op::IsNull, vec![value.value], None, PhpType::Bool,
        Op::IsNull.default_effects(), Some(span));
    let owned = ctx.value_is_owning_temporary(value);
    if consume_read && owned {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
    let initialized = ctx.initialized_slots_snapshot();
    let rejected = ctx.builder.create_named_block("object.boundary.null", vec![]);
    let accepted = ctx.builder.create_named_block("object.boundary.accepted", vec![]);
    ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
        then_target: rejected, then_args: vec![], else_target: accepted, else_args: vec![] });
    ctx.builder.position_at_end(rejected);
    if !consume_read && owned {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
    crate::ir_lower::stmt::lower_throw_access_exception(ctx, "TypeError", message, span);
    ctx.builder.position_at_end(accepted);
    ctx.restore_initialized_slots(initialized);
}

/// Finds relative class hints whose native inheritance depends on the enclosing class scope.
fn has_relative_class(ty: &PhpType) -> bool {
    match ty {
        PhpType::Object(class) => ["self", "static", "parent"].iter().any(|name| class.eq_ignore_ascii_case(name)),
        PhpType::Union(members) => members.iter().any(has_relative_class),
        _ => false,
    }
}
