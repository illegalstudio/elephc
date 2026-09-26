//! Purpose:
//! Builds ReflectionClass constants and scalar/member lookup methods.
//!
//! Called from:
//! - The Reflection checker metadata facade and sibling builders.
//!
//! Key details:
//! - Synthetic bodies read retained private metadata with PHP-compatible fallbacks.

use super::*;

/// Returns the public modifier constants exposed by PHP's `ReflectionClass`.
pub(super) fn reflection_class_constants() -> Vec<ClassConst> {
    vec![
        builtin_class_const("IS_IMPLICIT_ABSTRACT", 16),
        builtin_class_const("IS_FINAL", 32),
        builtin_class_const("IS_EXPLICIT_ABSTRACT", 64),
        builtin_class_const("IS_READONLY", 65_536),
    ]
}

/// Builds a public integer class constant for a synthetic reflection type.
pub(super) fn builtin_class_const(name: &str, value: i64) -> ClassConst {
    ClassConst {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_final: false,
        type_expr: None,
        value: Expr::new(ExprKind::IntLiteral(value), crate::span::Span::dummy()),
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

/// Returns a public `ReflectionClass` string method backed by one private slot.
pub(super) fn builtin_reflection_class_string_method(method_name: &str, property: &str) -> ClassMethod {
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
        return_type: Some(TypeExpr::Str),
        by_ref_return: false,
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(
                ExprKind::PropertyAccess {
                    object: Box::new(Expr::new(ExprKind::This, dummy_span)),
                    property: property.to_string(),
                },
                dummy_span,
            ))),
            dummy_span,
        )],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns a public `ReflectionClass` integer method backed by one private slot.
pub(super) fn builtin_reflection_class_int_method(method_name: &str, property: &str) -> ClassMethod {
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
        return_type: Some(TypeExpr::Int),
        by_ref_return: false,
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(
                ExprKind::PropertyAccess {
                    object: Box::new(Expr::new(ExprKind::This, dummy_span)),
                    property: property.to_string(),
                },
                dummy_span,
            ))),
            dummy_span,
        )],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns a public `ReflectionClass` membership probe backed by a private string array.
pub(super) fn builtin_reflection_class_has_name_method(
    method_name: &str,
    property: &str,
    case_insensitive: bool,
) -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let name_arg = Expr::new(ExprKind::Variable("name".to_string()), dummy_span);
    let needle = if case_insensitive {
        Expr::new(
            ExprKind::FunctionCall {
                name: Name::unqualified("strtolower"),
                args: vec![name_arg],
            },
            dummy_span,
        )
    } else {
        name_arg
    };
    let haystack = Expr::new(
        ExprKind::PropertyAccess {
            object: Box::new(Expr::new(ExprKind::This, dummy_span)),
            property: property.to_string(),
        },
        dummy_span,
    );
    let contains = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("in_array"),
            args: vec![needle, haystack],
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
        params: vec![("name".to_string(), Some(TypeExpr::Str), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Int),
        by_ref_return: false,
        body: vec![Stmt::new(StmtKind::Return(Some(contains)), dummy_span)],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns `ReflectionClass::getConstant()` backed by the private constant-value map.
pub(super) fn builtin_reflection_class_get_constant_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let name_arg = Expr::new(ExprKind::Variable("name".to_string()), dummy_span);
    let value = Expr::new(ExprKind::Variable("value".to_string()), dummy_span);
    let value_read = Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(reflection_this_property("__constants", dummy_span)),
            index: Box::new(name_arg),
        },
        dummy_span,
    );
    let value_is_present = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(value.clone()),
            op: BinOp::StrictNotEq,
            right: Box::new(Expr::new(ExprKind::Null, dummy_span)),
        },
        dummy_span,
    );
    ClassMethod {
        type_params: Vec::new(),
        name: "getConstant".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("name".to_string(), Some(TypeExpr::Str), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(mixed_type()),
        by_ref_return: false,
        body: vec![
            Stmt::new(
                StmtKind::Assign {
                    name: "value".to_string(),
                    value: value_read,
                },
                dummy_span,
            ),
            Stmt::new(
                StmtKind::If {
                    condition: value_is_present,
                    then_body: vec![Stmt::new(StmtKind::Return(Some(value)), dummy_span)],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                dummy_span,
            ),
            Stmt::new(
                StmtKind::Return(Some(Expr::new(ExprKind::BoolLiteral(false), dummy_span))),
                dummy_span,
            ),
        ],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns `ReflectionClass::getStaticPropertyValue()` backed by the static-property map.
pub(super) fn builtin_reflection_class_get_static_property_value_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let name = variable_expr("name", dummy_span);
    let default = variable_expr("default", dummy_span);
    let value_read = Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(reflection_this_property("__static_properties", dummy_span)),
            index: Box::new(name.clone()),
        },
        dummy_span,
    );
    let key_exists = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("array_key_exists"),
            args: vec![
                name,
                reflection_this_property("__static_properties", dummy_span),
            ],
        },
        dummy_span,
    );
    let value_or_default = Expr::new(
        ExprKind::Ternary {
            condition: Box::new(key_exists),
            then_expr: Box::new(value_read),
            else_expr: Box::new(default),
        },
        dummy_span,
    );
    ClassMethod {
        type_params: Vec::new(),
        name: "getStaticPropertyValue".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("name".to_string(), Some(TypeExpr::Str), None, false),
            (
                "default".to_string(),
                Some(mixed_type()),
                null_expr(),
                false,
            ),
        ],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(mixed_type()),
        by_ref_return: false,
        body: vec![Stmt::new(
            StmtKind::Return(Some(value_or_default)),
            dummy_span,
        )],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns the public `ReflectionClass::setStaticPropertyValue()` signature.
pub(super) fn builtin_reflection_class_set_static_property_value_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        type_params: Vec::new(),
        name: "setStaticPropertyValue".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("name".to_string(), Some(TypeExpr::Str), None, false),
            ("value".to_string(), Some(mixed_type()), None, false),
        ],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Void),
        by_ref_return: false,
        body: Vec::new(),
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns `ReflectionClass::getReflectionConstant()` backed by reflected constant objects.
pub(super) fn builtin_reflection_class_get_reflection_constant_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let name = variable_expr("name", dummy_span);
    let member = variable_expr("member", dummy_span);
    let exists = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(method_call_expr(
                member.clone(),
                "getName",
                Vec::new(),
                dummy_span,
            )),
            op: BinOp::StrictEq,
            right: Box::new(name.clone()),
        },
        dummy_span,
    );
    ClassMethod {
        type_params: Vec::new(),
        name: "getReflectionConstant".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("name".to_string(), Some(TypeExpr::Str), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(mixed_type()),
        by_ref_return: false,
        body: vec![
            Stmt::new(
                StmtKind::Foreach {
                    array: reflection_this_property("__reflection_constants", dummy_span),
                    key_var: None,
                    value_var: "member".to_string(),
                    value_by_ref: false,
                    body: vec![Stmt::new(
                        StmtKind::If {
                            condition: exists,
                            then_body: vec![Stmt::new(StmtKind::Return(Some(member)), dummy_span)],
                            elseif_clauses: Vec::new(),
                            else_body: None,
                        },
                        dummy_span,
                    )],
                },
                dummy_span,
            ),
            Stmt::new(StmtKind::Return(false_bool()), dummy_span),
        ],
        span: dummy_span,
        attributes: Vec::new(),
    }
}
