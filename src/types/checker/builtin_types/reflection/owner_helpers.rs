//! Purpose:
//! Builds Reflection owner constructors, attribute accessors, and variadic defaults.
//!
//! Called from:
//! - The Reflection checker metadata facade and sibling builders.
//!
//! Key details:
//! - Synthetic callable metadata stays aligned with direct special lowering.

use super::*;

/// Builds a public `__construct` method for a reflection owner class using the
/// provided parameter list: each tuple is (name, type_expr, default, by_ref).
pub(super) fn builtin_reflection_owner_constructor_method(
    params: Vec<(&str, Option<TypeExpr>, Option<Expr>, bool)>,
) -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        name: "__construct".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: params
            .into_iter()
            .map(|(name, ty, default, by_ref)| (name.to_string(), ty, default, by_ref))
            .collect(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: None,
        by_ref_return: false,
        body: Vec::new(),
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns a public `getAttributes()` method over the private `__attrs` property.
///
/// PHP's signature is `getAttributes(?string $name = null, int $flags = 0)`, and the `$name`
/// filter is not decoration: Symfony's container asks for one attribute at a time and treats
/// the answer as already filtered — `RegisterAutoconfigureAttributesPass` throws as soon as it
/// sees more than it expected. This method took NO parameters at all, so the AOT path refused
/// `getAttributes(A::class)` outright ("expects 0 arguments, got 1") and the eval bridge answered
/// with every attribute on the target (issue #983).
///
/// The body filters on the attribute's class name, case-insensitively, which is what PHP does
/// with `$flags = 0`: measured against 8.5.10, `getAttributes("markerone")` finds `#[MarkerOne]`
/// while `getAttributes("\\MarkerOne")` finds nothing, so the comparison folds ASCII case but
/// does not resolve a leading separator.
/// `$flags` is declared so the PHP signature matches, but its only documented value,
/// `ReflectionAttribute::IS_INSTANCEOF`, would need a subclass test against a class name that is
/// only known at runtime. `Checker::reject_unsupported_reflection_attribute_filter_flags` refuses
/// that call at compile time rather than answering with a silent subset, and the body throws for
/// the spellings the checker cannot see through — a first-class callable
/// (`$r->getAttributes(...)`) and `call_user_func_array([$r, 'getAttributes'], $args)` both reach
/// the method without a visible argument list, and both answered with the subset before this
/// throw existed. A null `$name` filters nothing, so PHP ignores `$flags` there. Both paths
/// collect into a fresh typed array, giving returned elements independent ownership.
pub(super) fn builtin_reflection_owner_get_attributes_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let name = variable_expr("name", dummy_span);
    let attribute = variable_expr("attribute", dummy_span);
    let source = reflection_this_property("__attrs", dummy_span);
    let attribute_name = method_call_expr(attribute.clone(), "getName", Vec::new(), dummy_span);

    let name_is_null = binary_expr(
        name.clone(),
        BinOp::StrictEq,
        Expr::new(ExprKind::Null, dummy_span),
        dummy_span,
    );
    let flags_requested = binary_expr(
        variable_expr("flags", dummy_span),
        BinOp::StrictNotEq,
        Expr::new(ExprKind::IntLiteral(0), dummy_span),
        dummy_span,
    );
    // `strcasecmp($attribute->getName(), $name) === 0`. PHP compares the two class names the way
    // it compares every class name — folding ASCII case — so `===` on the two strings would miss
    // `getAttributes("markerone")` for `#[MarkerOne]`.
    let matches_name = binary_expr(
        Expr::new(
            ExprKind::FunctionCall {
                name: Name::unqualified("strcasecmp".to_string()),
                args: vec![attribute_name, name.clone()],
            },
            dummy_span,
        ),
        BinOp::StrictEq,
        Expr::new(ExprKind::IntLiteral(0), dummy_span),
        dummy_span,
    );
    let matches_filter = binary_expr(name_is_null, BinOp::Or, matches_name, dummy_span);
    let unsupported_flags = binary_expr(
        binary_expr(name.clone(), BinOp::StrictNotEq, Expr::new(ExprKind::Null, dummy_span), dummy_span),
        BinOp::And, flags_requested, dummy_span,
    );

    ClassMethod {
        name: "getAttributes".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            (
                "name".to_string(),
                Some(TypeExpr::Nullable(Box::new(TypeExpr::Str))),
                null_expr(),
                false,
            ),
            (
                "flags".to_string(),
                Some(TypeExpr::Int),
                Some(Expr::new(ExprKind::IntLiteral(0), dummy_span)),
                false,
            ),
        ],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(array_type()),
        by_ref_return: false,
        body: vec![
            Stmt::new(
                StmtKind::If {
                    condition: unsupported_flags,
                    then_body: vec![throw_new_reflection_exception(
                        string_lit(
                            "ReflectionAttribute::IS_INSTANCEOF is not supported yet: it needs a \
                             subclass test on a class name known only at runtime, and AOT mode \
                             has no name-keyed class hierarchy query",
                            dummy_span,
                        ),
                        dummy_span,
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                dummy_span,
            ),
            Stmt::new(
                StmtKind::TypedAssign {
                    type_expr: object_array_type("ReflectionAttribute"),
                    name: "matched".to_string(),
                    value: Expr::new(ExprKind::ArrayLiteral(Vec::new()), dummy_span),
                },
                dummy_span,
            ),
            Stmt::new(
                StmtKind::Foreach {
                    array: source,
                    key_var: None,
                    value_var: "attribute".to_string(),
                    value_by_ref: false,
                    body: vec![Stmt::new(
                        StmtKind::If {
                            condition: matches_filter,
                            then_body: vec![
                                // The loop variable is whatever `__attrs` holds, and that differs
                                // by who populated the slot: the AOT emitter stamps the array
                                // `Object(ReflectionAttribute)` and stores bare pointers, while the
                                // eval bridge builds a plain Mixed array. Landing the element in a
                                // typed local first normalizes both to an object before it reaches
                                // an `array<ReflectionAttribute>`, whose elements the CALLER reads
                                // as bare pointers because of the `getAttributes` signature patch.
                                // Pushing the loop variable straight through instead stores a Mixed
                                // box that the caller then misreads as an object.
                                Stmt::new(
                                    StmtKind::TypedAssign {
                                        type_expr: TypeExpr::Named(Name::unqualified(
                                            "ReflectionAttribute",
                                        )),
                                        name: "selected".to_string(),
                                        value: attribute,
                                    },
                                    dummy_span,
                                ),
                                Stmt::new(
                                    StmtKind::ArrayPush {
                                        array: "matched".to_string(),
                                        value: variable_expr("selected", dummy_span),
                                    },
                                    dummy_span,
                                ),
                            ],
                            elseif_clauses: Vec::new(),
                            else_body: None,
                        },
                        dummy_span,
                    )],
                },
                dummy_span,
            ),
            Stmt::new(
                StmtKind::Return(Some(variable_expr("matched", dummy_span))),
                dummy_span,
            ),
        ],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Marks a synthesized variadic method signature as callable with no variadic arguments.
pub(super) fn make_reflection_variadic_optional(sig: &mut crate::types::FunctionSig) {
    if sig.variadic.is_some() {
        if let Some(default) = sig.defaults.last_mut() {
            *default = empty_array();
        }
    }
}
