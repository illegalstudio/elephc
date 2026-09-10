//! Purpose:
//! Builds ReflectionClass collection and member-object accessors.
//!
//! Called from:
//! - The Reflection checker metadata facade and sibling builders.
//!
//! Key details:
//! - Modifier filters and missing-member behavior remain represented in the AST.

use super::*;

/// Returns a public `ReflectionClass` array method backed by one private slot.
pub(super) fn builtin_reflection_class_array_method(
    method_name: &str,
    property: &str,
    return_type: TypeExpr,
) -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
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
        return_type: Some(return_type.clone()),
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

/// Returns a deferred `ReflectionExtension::getClasses()` collection builder.
///
/// The owner stores names rather than nested reflector objects so constructing a
/// class-owned extension cannot recurse through Extension -> Class -> Extension.
pub(super) fn builtin_reflection_extension_classes_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let class_name = variable_expr("class_name", dummy_span);
    let collection = reflection_this_property("__classes", dummy_span);
    let is_enum = function_call(
        "in_array",
        vec![
            class_name.clone(),
            reflection_this_property("__enum_names", dummy_span),
            Expr::new(ExprKind::BoolLiteral(true), dummy_span),
        ],
        dummy_span,
    );
    let new_reflector = |reflector_class: &str| {
        Expr::new(
            ExprKind::NewObject {
                class_name: Name::unqualified(reflector_class),
                args: vec![class_name.clone()],
            },
            dummy_span,
        )
    };
    ClassMethod {
        name: "getClasses".to_string(),
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
        body: vec![
            Stmt::new(
                StmtKind::PropertyAssign {
                    object: Box::new(Expr::new(ExprKind::This, dummy_span)),
                    property: "__classes".to_string(),
                    value: Expr::new(ExprKind::ArrayLiteral(Vec::new()), dummy_span),
                },
                dummy_span,
            ),
            Stmt::new(
                StmtKind::Foreach {
                    array: reflection_this_property("__class_names", dummy_span),
                    key_var: None,
                    value_var: "class_name".to_string(),
                    value_by_ref: false,
                    body: vec![Stmt::new(
                        StmtKind::If {
                            condition: is_enum,
                            then_body: vec![Stmt::new(
                                StmtKind::PropertyArrayAssign {
                                    object: Box::new(Expr::new(ExprKind::This, dummy_span)),
                                    property: "__classes".to_string(),
                                    index: class_name.clone(),
                                    value: new_reflector("ReflectionEnum"),
                                },
                                dummy_span,
                            )],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::new(
                                StmtKind::PropertyArrayAssign {
                                    object: Box::new(Expr::new(ExprKind::This, dummy_span)),
                                    property: "__classes".to_string(),
                                    index: class_name.clone(),
                                    value: new_reflector("ReflectionClass"),
                                },
                                dummy_span,
                            )]),
                        },
                        dummy_span,
                    )],
                },
                dummy_span,
            ),
            Stmt::new(StmtKind::Return(Some(collection)), dummy_span),
        ],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns a deferred `ReflectionExtension::getFunctions()` collection builder.
///
/// Function reflectors are created only when the public collection is requested,
/// preventing the reverse extension link from recursively constructing the same graph.
pub(super) fn builtin_reflection_extension_functions_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let function_name = variable_expr("function_name", dummy_span);
    let collection = reflection_this_property("__functions", dummy_span);
    let new_reflector = Expr::new(
        ExprKind::NewObject {
            class_name: Name::unqualified("ReflectionFunction"),
            args: vec![function_name.clone()],
        },
        dummy_span,
    );
    ClassMethod {
        name: "getFunctions".to_string(),
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
        body: vec![
            Stmt::new(
                StmtKind::PropertyAssign {
                    object: Box::new(Expr::new(ExprKind::This, dummy_span)),
                    property: "__functions".to_string(),
                    value: Expr::new(ExprKind::ArrayLiteral(Vec::new()), dummy_span),
                },
                dummy_span,
            ),
            Stmt::new(
                StmtKind::Foreach {
                    array: reflection_this_property("__function_names", dummy_span),
                    key_var: None,
                    value_var: "function_name".to_string(),
                    value_by_ref: false,
                    body: vec![Stmt::new(
                        StmtKind::PropertyArrayAssign {
                            object: Box::new(Expr::new(ExprKind::This, dummy_span)),
                            property: "__functions".to_string(),
                            index: function_name,
                            value: new_reflector,
                        },
                        dummy_span,
                    )],
                },
                dummy_span,
            ),
            Stmt::new(StmtKind::Return(Some(collection)), dummy_span),
        ],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns a public `ReflectionClass` array method with an optional modifier filter.
pub(super) fn builtin_reflection_class_filtered_array_method(
    method_name: &str,
    property: &str,
    return_type: TypeExpr,
) -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let filter = variable_expr("filter", dummy_span);
    let member = variable_expr("member", dummy_span);
    let source = reflection_this_property(property, dummy_span);
    let filter_is_null = binary_expr(
        filter.clone(),
        BinOp::StrictEq,
        Expr::new(ExprKind::Null, dummy_span),
        dummy_span,
    );
    let filter_is_zero = binary_expr(
        filter.clone(),
        BinOp::StrictEq,
        Expr::new(ExprKind::IntLiteral(0), dummy_span),
        dummy_span,
    );
    let empty_result = Expr::new(ExprKind::ArrayLiteral(Vec::new()), dummy_span);
    let modifier_match = binary_expr(
        binary_expr(
            method_call_expr(member.clone(), "getModifiers", Vec::new(), dummy_span),
            BinOp::BitAnd,
            filter,
            dummy_span,
        ),
        BinOp::StrictNotEq,
        Expr::new(ExprKind::IntLiteral(0), dummy_span),
        dummy_span,
    );
    ClassMethod {
        name: method_name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![(
            "filter".to_string(),
            Some(TypeExpr::Nullable(Box::new(TypeExpr::Int))),
            null_expr(),
            false,
        )],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(return_type.clone()),
        by_ref_return: false,
        body: vec![
            Stmt::new(
                StmtKind::If {
                    condition: filter_is_null,
                    then_body: vec![Stmt::new(
                        StmtKind::Return(Some(source.clone())),
                        dummy_span,
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                dummy_span,
            ),
            Stmt::new(
                StmtKind::If {
                    condition: filter_is_zero,
                    then_body: vec![Stmt::new(
                        StmtKind::Return(Some(empty_result.clone())),
                        dummy_span,
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                dummy_span,
            ),
            Stmt::new(
                StmtKind::TypedAssign {
                    type_expr: return_type.clone(),
                    name: "result".to_string(),
                    value: empty_result,
                },
                dummy_span,
            ),
            Stmt::new(
                StmtKind::Foreach {
                    array: source,
                    key_var: None,
                    value_var: "member".to_string(),
                    value_by_ref: false,
                    body: vec![Stmt::new(
                        StmtKind::If {
                            condition: modifier_match,
                            then_body: vec![Stmt::new(
                                StmtKind::ArrayPush {
                                    array: "result".to_string(),
                                    value: member,
                                },
                                dummy_span,
                            )],
                            elseif_clauses: Vec::new(),
                            else_body: None,
                        },
                        dummy_span,
                    )],
                },
                dummy_span,
            ),
            Stmt::new(
                StmtKind::Return(Some(variable_expr("result", dummy_span))),
                dummy_span,
            ),
        ],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns a public `ReflectionClass::getMethod()` or `getProperty()` lookup method.
pub(super) fn builtin_reflection_class_get_member_method(
    method_name: &str,
    property: &str,
    return_class: &str,
    case_insensitive: bool,
) -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let name = variable_expr("name", dummy_span);
    let member = variable_expr("member", dummy_span);
    let member_name = method_call_expr(member.clone(), "getName", Vec::new(), dummy_span);
    let left = if case_insensitive {
        strtolower_call(member_name, dummy_span)
    } else {
        member_name
    };
    let right = if case_insensitive {
        strtolower_call(name.clone(), dummy_span)
    } else {
        name.clone()
    };
    let exists = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(left),
            op: if case_insensitive {
                BinOp::Eq
            } else {
                BinOp::StrictEq
            },
            right: Box::new(right),
        },
        dummy_span,
    );
    let message = if return_class == "ReflectionMethod" {
        concat_expr(
            concat_expr(
                concat_expr(
                    reflection_this_property("__name", dummy_span),
                    string_lit("::", dummy_span),
                    dummy_span,
                ),
                name.clone(),
                dummy_span,
            ),
            string_lit("() does not exist", dummy_span),
            dummy_span,
        )
    } else {
        concat_expr(
            concat_expr(
                concat_expr(
                    reflection_this_property("__name", dummy_span),
                    string_lit("::$", dummy_span),
                    dummy_span,
                ),
                name.clone(),
                dummy_span,
            ),
            string_lit(" does not exist", dummy_span),
            dummy_span,
        )
    };
    let message = concat_expr(
        string_lit(
            if return_class == "ReflectionMethod" {
                "Method "
            } else {
                "Property "
            },
            dummy_span,
        ),
        message,
        dummy_span,
    );
    ClassMethod {
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
        return_type: Some(TypeExpr::Named(Name::unqualified(return_class))),
        by_ref_return: false,
        body: vec![
            Stmt::new(
                StmtKind::Foreach {
                    array: reflection_this_property(property, dummy_span),
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
            throw_new_reflection_exception(message, dummy_span),
        ],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns a public nullable object `ReflectionClass` method backed by one private slot.
pub(super) fn builtin_reflection_class_nullable_object_method(
    method_name: &str,
    property: &str,
    class_name: &str,
) -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
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
        return_type: Some(nullable_object_type(class_name)),
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

/// Returns a public object-valued `ReflectionClass` method backed by one private slot.
pub(super) fn builtin_reflection_class_object_method(
    method_name: &str,
    property: &str,
    class_name: &str,
) -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
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
        return_type: Some(TypeExpr::Named(Name::unqualified(class_name))),
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

/// Returns a public mixed `ReflectionClass` method backed by one private slot.
pub(super) fn builtin_reflection_class_mixed_method(method_name: &str, property: &str) -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
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

/// Returns a public `ReflectionClass` boolean method backed by one private slot.
pub(super) fn builtin_reflection_class_bool_method(method_name: &str, property: &str) -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
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
