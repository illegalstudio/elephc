//! Purpose:
//! Builds ReflectionMethod and ReflectionProperty predicate methods.
//!
//! Called from:
//! - The Reflection checker metadata facade and sibling builders.
//!
//! Key details:
//! - Modifier masks, prototypes, hooks, and constant fallbacks retain their metadata contracts.

use super::*;

/// Returns `ReflectionMethod::getPrototype()` backed by a retained prototype reflector.
pub(super) fn builtin_reflection_method_get_prototype_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        type_params: Vec::new(),
        name: "getPrototype".to_string(),
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
        return_type: Some(TypeExpr::Named(Name::unqualified("ReflectionMethod"))),
        by_ref_return: false,
        body: vec![
            Stmt::new(
                StmtKind::If {
                    condition: Expr::new(
                        ExprKind::Not(Box::new(reflection_this_property(
                            "__has_prototype",
                            dummy_span,
                        ))),
                        dummy_span,
                    ),
                    then_body: vec![throw_new_reflection_exception(
                        string_lit("Method does not have a prototype", dummy_span),
                        dummy_span,
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                dummy_span,
            ),
            Stmt::new(
                StmtKind::Return(Some(reflection_this_property(
                    "__prototype",
                    dummy_span,
                ))),
                dummy_span,
            ),
        ],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns `ReflectionProperty::isDefault()` backed by the dynamic-property slot.
pub(super) fn builtin_reflection_property_is_default_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        type_params: Vec::new(),
        name: "isDefault".to_string(),
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
        return_type: Some(bool_type()),
        by_ref_return: false,
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(
                ExprKind::Not(Box::new(reflection_this_property(
                    "__is_dynamic",
                    dummy_span,
                ))),
                dummy_span,
            ))),
            dummy_span,
        )],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns a public Reflection method that always reports PHP `false` as `string|false`.
pub(super) fn builtin_reflection_constant_false_union_method(method_name: &str) -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        type_params: Vec::new(),
        name: method_name.to_string(),
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
        return_type: Some(string_or_bool_type()),
        by_ref_return: false,
        body: vec![Stmt::new(StmtKind::Return(false_bool()), dummy_span)],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns a public Reflection predicate that always reports PHP `false`.
pub(super) fn builtin_reflection_constant_false_bool_method(method_name: &str) -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        type_params: Vec::new(),
        name: method_name.to_string(),
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
        return_type: Some(bool_type()),
        by_ref_return: false,
        body: vec![Stmt::new(StmtKind::Return(false_bool()), dummy_span)],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns a public Reflection method that always reports an empty array.
pub(super) fn builtin_reflection_constant_empty_array_method(method_name: &str) -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        type_params: Vec::new(),
        name: method_name.to_string(),
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
        return_type: Some(array_type()),
        by_ref_return: false,
        body: vec![Stmt::new(StmtKind::Return(empty_array()), dummy_span)],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns a public Reflection method that always reports PHP `null` as mixed.
pub(super) fn builtin_reflection_constant_null_mixed_method(method_name: &str) -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        type_params: Vec::new(),
        name: method_name.to_string(),
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
        return_type: Some(mixed_type()),
        by_ref_return: false,
        body: vec![Stmt::new(StmtKind::Return(null_expr()), dummy_span)],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns a `ReflectionMethod` predicate derived from its case-insensitive method name.
pub(super) fn builtin_reflection_method_name_predicate_method(
    method_name: &str,
    expected_name: &str,
) -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let lower_name = strtolower_call(reflection_this_property("__name", dummy_span), dummy_span);
    let comparison = binary_expr(
        lower_name,
        BinOp::StrictEq,
        string_lit(expected_name, dummy_span),
        dummy_span,
    );
    ClassMethod {
        type_params: Vec::new(),
        name: method_name.to_string(),
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
        return_type: Some(bool_type()),
        by_ref_return: false,
        body: vec![Stmt::new(StmtKind::Return(Some(comparison)), dummy_span)],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns `ReflectionProperty::hasType()` backed by a nullable private `__type` slot.
pub(super) fn builtin_reflection_property_has_type_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        type_params: Vec::new(),
        name: "hasType".to_string(),
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
        return_type: Some(bool_type()),
        by_ref_return: false,
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(
                ExprKind::BinaryOp {
                    left: Box::new(Expr::new(
                        ExprKind::PropertyAccess {
                            object: Box::new(Expr::new(ExprKind::This, dummy_span)),
                            property: "__type".to_string(),
                        },
                        dummy_span,
                    )),
                    op: BinOp::StrictNotEq,
                    right: Box::new(Expr::new(ExprKind::Null, dummy_span)),
                },
                dummy_span,
            ))),
            dummy_span,
        )],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns a public ReflectionProperty predicate over one `__modifiers` bit.
pub(super) fn builtin_reflection_property_modifier_mask_method(method_name: &str, mask: i64) -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let masked_modifiers = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(reflection_this_property("__modifiers", dummy_span)),
            op: BinOp::BitAnd,
            right: Box::new(Expr::new(ExprKind::IntLiteral(mask), dummy_span)),
        },
        dummy_span,
    );
    let comparison = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(masked_modifiers),
            op: BinOp::StrictNotEq,
            right: Box::new(Expr::new(ExprKind::IntLiteral(0), dummy_span)),
        },
        dummy_span,
    );
    ClassMethod {
        type_params: Vec::new(),
        name: method_name.to_string(),
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
        return_type: Some(bool_type()),
        by_ref_return: false,
        body: vec![Stmt::new(StmtKind::Return(Some(comparison)), dummy_span)],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns `ReflectionProperty::hasHook()` backed by the retained hook method map.
pub(super) fn builtin_reflection_property_has_hook_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let hook_kind = reflection_property_hook_type_value(dummy_span);
    let has_hook = function_call(
        "array_key_exists",
        vec![
            hook_kind,
            reflection_this_property("__hooks", dummy_span),
        ],
        dummy_span,
    );
    ClassMethod {
        type_params: Vec::new(),
        name: "hasHook".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("type".to_string(), Some(mixed_type()), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(bool_type()),
        by_ref_return: false,
        body: vec![Stmt::new(StmtKind::Return(Some(has_hook)), dummy_span)],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns `ReflectionProperty::getHook()` backed by the retained hook method map.
pub(super) fn builtin_reflection_property_get_hook_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let hook_kind = reflection_property_hook_type_value(dummy_span);
    let hooks = reflection_this_property("__hooks", dummy_span);
    let hook_method = Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(hooks.clone()),
            index: Box::new(hook_kind.clone()),
        },
        dummy_span,
    );
    let has_hook = function_call("array_key_exists", vec![hook_kind, hooks], dummy_span);
    ClassMethod {
        type_params: Vec::new(),
        name: "getHook".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("type".to_string(), Some(mixed_type()), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(nullable_object_type("ReflectionMethod")),
        by_ref_return: false,
        body: vec![
            Stmt::new(
                StmtKind::If {
                    condition: has_hook,
                    then_body: vec![Stmt::new(
                        StmtKind::Return(Some(hook_method)),
                        dummy_span,
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                dummy_span,
            ),
            Stmt::new(
                StmtKind::Return(Some(null_value(dummy_span))),
                dummy_span,
            ),
        ],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Builds `$type->value` for `PropertyHookType` arguments accepted by hook APIs.
pub(super) fn reflection_property_hook_type_value(span: crate::span::Span) -> Expr {
    Expr::new(
        ExprKind::PropertyAccess {
            object: Box::new(variable_expr("type", span)),
            property: "value".to_string(),
        },
        span,
    )
}
