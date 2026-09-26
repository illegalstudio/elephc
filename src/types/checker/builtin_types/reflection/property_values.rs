//! Purpose:
//! Builds ReflectionProperty value access and initialization methods.
//!
//! Called from:
//! - The Reflection checker metadata facade and sibling builders.
//!
//! Key details:
//! - Static and instance guards keep materialized storage and dynamic access distinct.

use super::*;

/// Returns `ReflectionProperty::getValue()` for dynamic public instance reflectors.
pub(super) fn builtin_reflection_property_get_value_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let object = variable_expr("object", dummy_span);
    ClassMethod {
        type_params: Vec::new(),
        name: "getValue".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("object".to_string(), Some(mixed_type()), null_expr(), false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(mixed_type()),
        by_ref_return: false,
        body: vec![
            reflection_property_static_get_value_return(dummy_span),
            reflection_property_object_required_guard("getValue", dummy_span),
            Stmt::new(
                StmtKind::Return(Some(reflection_dynamic_object_property(object, dummy_span))),
                dummy_span,
            ),
        ],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns `ReflectionProperty::setValue()` for dynamic public instance reflectors.
pub(super) fn builtin_reflection_property_set_value_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let object = variable_expr("object", dummy_span);
    let value = variable_expr("value", dummy_span);
    ClassMethod {
        type_params: Vec::new(),
        name: "setValue".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("object".to_string(), Some(mixed_type()), None, false),
            ("value".to_string(), Some(mixed_type()), None, false),
        ],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Void),
        by_ref_return: false,
        body: vec![
            reflection_property_static_value_guard("setValue", dummy_span),
            reflection_property_object_required_guard("setValue", dummy_span),
            Stmt::new(
                StmtKind::ExprStmt(Expr::new(
                    ExprKind::Assignment {
                        target: Box::new(reflection_dynamic_object_property(object, dummy_span)),
                        value: Box::new(value),
                        result_target: None,
                        prelude: Vec::new(),
                        conditional_value_temp: None,
                    },
                    dummy_span,
                )),
                dummy_span,
            ),
        ],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns `ReflectionProperty::isInitialized()` for supported materialized reflectors.
pub(super) fn builtin_reflection_property_is_initialized_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let static_return = reflection_property_static_is_initialized_return(dummy_span);
    let dynamic_return = Stmt::new(
        StmtKind::If {
            condition: reflection_this_property("__is_dynamic", dummy_span),
            then_body: vec![Stmt::new(StmtKind::Return(true_bool()), dummy_span)],
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        dummy_span,
    );
    let defaulted_return = Stmt::new(
        StmtKind::If {
            condition: reflection_this_property("__has_default_value", dummy_span),
            then_body: vec![Stmt::new(StmtKind::Return(true_bool()), dummy_span)],
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        dummy_span,
    );
    ClassMethod {
        type_params: Vec::new(),
        name: "isInitialized".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("object".to_string(), Some(mixed_type()), null_expr(), false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(bool_type()),
        by_ref_return: false,
        body: vec![
            static_return,
            reflection_property_object_required_guard("isInitialized", dummy_span),
            dynamic_return,
            defaulted_return,
            throw_new_reflection_exception(
                string_lit(
                    "ReflectionProperty::isInitialized() for typed properties without defaults requires an inline known property",
                    dummy_span,
                ),
                dummy_span,
            ),
        ],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns a static-property `getValue()` branch backed by the declaring ReflectionClass snapshot.
pub(super) fn reflection_property_static_get_value_return(span: crate::span::Span) -> Stmt {
    Stmt::new(
        StmtKind::If {
            condition: reflection_this_property("__is_static", span),
            then_body: vec![Stmt::new(
                StmtKind::Return(Some(method_call_expr(
                    reflection_this_property("__declaring_class", span),
                    "getStaticPropertyValue",
                    vec![reflection_this_property("__name", span)],
                    span,
                ))),
                span,
            )],
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        span,
    )
}

/// Returns a static-property `isInitialized()` branch backed by materialized static values.
pub(super) fn reflection_property_static_is_initialized_return(span: crate::span::Span) -> Stmt {
    Stmt::new(
        StmtKind::If {
            condition: reflection_this_property("__is_static", span),
            then_body: vec![Stmt::new(
                StmtKind::Return(Some(function_call(
                    "array_key_exists",
                    vec![
                        reflection_this_property("__name", span),
                        method_call_expr(
                            reflection_this_property("__declaring_class", span),
                            "getStaticProperties",
                            Vec::new(),
                            span,
                        ),
                    ],
                    span,
                ))),
                span,
            )],
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        span,
    )
}

/// Builds a guard for static property value access that still needs inline lowering.
pub(super) fn reflection_property_static_value_guard(method: &str, span: crate::span::Span) -> Stmt {
    Stmt::new(
        StmtKind::If {
            condition: reflection_this_property("__is_static", span),
            then_body: vec![throw_new_reflection_exception(
                string_lit(
                    &format!(
                        "ReflectionProperty::{}() for static properties requires an inline known static property",
                        method
                    ),
                    span,
                ),
                span,
            )],
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        span,
    )
}

/// Builds a guard requiring an object argument for instance property value access.
pub(super) fn reflection_property_object_required_guard(method: &str, span: crate::span::Span) -> Stmt {
    Stmt::new(
        StmtKind::If {
            condition: binary_expr(
                variable_expr("object", span),
                BinOp::StrictEq,
                null_value(span),
                span,
            ),
            then_body: vec![throw_new_reflection_exception(
                string_lit(
                    &format!(
                        "ReflectionProperty::{}() requires an object for instance properties",
                        method
                    ),
                    span,
                ),
                span,
            )],
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        span,
    )
}

/// Builds `$object->{$this->__name}` for ReflectionProperty value access.
pub(super) fn reflection_dynamic_object_property(object: Expr, span: crate::span::Span) -> Expr {
    Expr::new(
        ExprKind::DynamicPropertyAccess {
            object: Box::new(object),
            property: Box::new(reflection_this_property("__name", span)),
        },
        span,
    )
}

/// Returns `ReflectionProperty::isLazy()` for the non-lazy property model elephc supports.
pub(super) fn builtin_reflection_property_is_lazy_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        type_params: Vec::new(),
        name: "isLazy".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("object".to_string(), Some(object_type()), None, false)],
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

/// Returns `ReflectionProperty::skipLazyInitialization()` as a no-op for non-lazy properties.
pub(super) fn builtin_reflection_property_skip_lazy_initialization_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let static_modifier = binary_expr(
        binary_expr(
            reflection_this_property("__modifiers", dummy_span),
            BinOp::BitAnd,
            Expr::new(ExprKind::IntLiteral(16), dummy_span),
            dummy_span,
        ),
        BinOp::StrictNotEq,
        Expr::new(ExprKind::IntLiteral(0), dummy_span),
        dummy_span,
    );
    ClassMethod {
        type_params: Vec::new(),
        name: "skipLazyInitialization".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("object".to_string(), Some(object_type()), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Void),
        by_ref_return: false,
        body: vec![
            Stmt::new(
                StmtKind::If {
                    condition: static_modifier,
                    then_body: vec![throw_new_reflection_exception(
                        string_lit(
                            "Can not use skipLazyInitialization on static property",
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
                StmtKind::If {
                    condition: reflection_this_property("__is_virtual", dummy_span),
                    then_body: vec![throw_new_reflection_exception(
                        string_lit(
                            "Can not use skipLazyInitialization on virtual property",
                            dummy_span,
                        ),
                        dummy_span,
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                dummy_span,
            ),
        ],
        span: dummy_span,
        attributes: Vec::new(),
    }
}
