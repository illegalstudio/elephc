//! Purpose:
//! Adds the Reflection members whose answer comes from a runtime value rather than from
//! compile-time metadata: `ReflectionFunction` over a Closure, the name-only `ReflectionClass`
//! its `getClosureScopeClass()` returns, and PHP's public `name` property.
//!
//! Called from:
//! - `builtin_reflection_function()` and `builtin_reflection_class()`, plus the shared owner
//!   builder for the `name` property.
//!
//! Key details:
//! - A runtime reflector is a partially populated owner: the private flag (`__runtime_callable`
//!   or `__name_only`) makes every method that has no runtime answer throw `Error` instead of
//!   returning the empty compile-time defaults, which would read as a wrong answer.
//! - `name` is a virtual get-hooked property reading the private `__name` slot, so every
//!   construction path that populates `__name` populates `name` too.

use super::*;

/// Guard flag of a `ReflectionFunction` that reflects a Closure value.
const RUNTIME_CALLABLE_FLAG: &str = "__runtime_callable";
/// Guard flag of a `ReflectionClass` that only knows its class name.
const NAME_ONLY_FLAG: &str = "__name_only";

/// Adds `isAnonymous()`, `getClosureScopeClass()`, Closure-aware `isClosure()`, the Closure
/// slots, and the runtime-reflector guard to `ReflectionFunction`.
pub(super) fn add_reflection_function_closure_members(class: &mut FlattenedClass) {
    let span = dummy();
    class.properties.push(builtin_property(
        RUNTIME_CALLABLE_FLAG,
        Visibility::Private,
        Some(bool_type()),
        false_bool(),
    ));
    class.properties.push(builtin_property(
        "__callable",
        Visibility::Private,
        Some(mixed_type()),
        null_expr(),
    ));
    let runtime = || reflection_this_property(RUNTIME_CALLABLE_FLAG, span);
    let callable = || reflection_this_property("__callable", span);
    class.methods.retain(|method| method.name != "isClosure");
    class.methods.push(synthetic_method(
        "isClosure",
        Some(bool_type()),
        vec![return_stmt(runtime())],
    ));
    class.methods.push(synthetic_method(
        "isAnonymous",
        Some(bool_type()),
        vec![
            if_stmt(
                runtime(),
                vec![return_stmt(function_call(
                    "__elephc_callable_is_anonymous",
                    vec![callable()],
                    span,
                ))],
            ),
            return_stmt(Expr::new(ExprKind::BoolLiteral(false), span)),
        ],
    ));
    let scope_name = || function_call("__elephc_callable_scope_class", vec![callable()], span);
    class.methods.push(synthetic_method(
        "getClosureScopeClass",
        Some(nullable_object_type("ReflectionClass")),
        vec![
            if_stmt(
                Expr::new(ExprKind::Not(Box::new(runtime())), span),
                vec![return_stmt(Expr::new(ExprKind::Null, span))],
            ),
            if_stmt(
                binary_expr(scope_name(), BinOp::StrictEq, string_lit("", span), span),
                vec![return_stmt(Expr::new(ExprKind::Null, span))],
            ),
            return_stmt(function_call(
                "__elephc_reflection_class_named",
                vec![scope_name()],
                span,
            )),
        ],
    ));
    guard_partial_reflector_methods(
        class,
        RUNTIME_CALLABLE_FLAG,
        &["__construct", "isClosure", "isAnonymous", "getClosureScopeClass"],
        "for a Closure reflected at run time",
    );
}

/// Adds the name-only flag and guard to `ReflectionClass` (and the owners built from it).
pub(super) fn add_reflection_class_name_only_members(class: &mut FlattenedClass) {
    class.properties.push(builtin_property(
        NAME_ONLY_FLAG,
        Visibility::Private,
        Some(bool_type()),
        false_bool(),
    ));
    guard_partial_reflector_methods(
        class,
        NAME_ONLY_FLAG,
        &[
            "__construct",
            "getName",
            &crate::names::property_hook_get_method("name"),
        ],
        "for a class known only by name at run time (a Closure scope class)",
    );
}

/// Adds PHP's public read-only `name` property, backed by the private `__name` slot.
pub(super) fn add_reflection_name_property(class: &mut FlattenedClass) {
    if class.properties.iter().any(|property| property.name == "name") {
        return;
    }
    let span = dummy();
    let mut property = builtin_property("name", Visibility::Public, Some(TypeExpr::Str), None);
    property.hooks = crate::parser::ast::PropertyHooks {
        get: true,
        ..crate::parser::ast::PropertyHooks::none()
    };
    class.properties.push(property);
    class.methods.push(synthetic_method(
        &crate::names::property_hook_get_method("name"),
        Some(TypeExpr::Str),
        vec![return_stmt(reflection_this_property("__name", span))],
    ));
}

/// Prepends `if ($this-><flag>) throw new Error(...)` to every instance method not in `allowed`.
fn guard_partial_reflector_methods(
    class: &mut FlattenedClass,
    flag: &str,
    allowed: &[&str],
    context: &str,
) {
    let span = dummy();
    let class_name = class.name.clone();
    for method in &mut class.methods {
        if method.is_static
            || !method.has_body
            || allowed
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&method.name))
        {
            continue;
        }
        let shown = method
            .name
            .strip_prefix("__propget_")
            .map(|property| format!("{}::${}", class_name, property))
            .unwrap_or_else(|| format!("{}::{}()", class_name, method.name));
        let message = format!("{} is not supported by elephc {}", shown, context);
        let guard = if_stmt(
            reflection_this_property(flag, span),
            vec![Stmt::new(
                StmtKind::Throw(Expr::new(
                    ExprKind::NewObject {
                        class_name: Name::unqualified("Error"),
                        args: vec![string_lit(&message, span)],
                    },
                    span,
                )),
                span,
            )],
        );
        method.body.insert(0, guard);
    }
}

/// Builds a public, parameterless synthetic instance method.
fn synthetic_method(name: &str, return_type: Option<TypeExpr>, body: Vec<Stmt>) -> ClassMethod {
    ClassMethod {
        type_params: Vec::new(),
        name: name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type,
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds `if (<condition>) { <then_body> }`.
fn if_stmt(condition: Expr, then_body: Vec<Stmt>) -> Stmt {
    Stmt::new(
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        dummy(),
    )
}

/// Builds `return <value>;`.
fn return_stmt(value: Expr) -> Stmt {
    Stmt::new(StmtKind::Return(Some(value)), dummy())
}
