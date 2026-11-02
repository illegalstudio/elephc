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

/// Returns a `ReflectionClass` method that builds one reflector per name held in `names_property`.
///
/// php hands back fresh reflector objects on every call, keyed by name, so building them here is
/// the documented behavior rather than a workaround. It also removes a dependency on the object
/// slot (`__interfaces` / `__traits`) being populated: a reflector built from a RUNTIME class name
/// goes through the eval bridge's `reflection_owner_layout`, which fills the name arrays and never
/// the object arrays. Symfony's `AutowirePass::populateAvailableType()` iterates
/// `getInterfaces()` on exactly such reflectors, got null, and warned for every service.
pub(super) fn builtin_reflection_class_objects_from_names_method(
    method_name: &str,
    names_property: &str,
    return_type: TypeExpr,
) -> ClassMethod {
    use crate::synthetic_class::{e_array, e_new, e_var, s_array_assign, s_assign, s_foreach, s_return};
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
        return_type: Some(return_type),
        by_ref_return: false,
        body: vec![
            s_assign("__elephc_reflectors", e_array(Vec::new())),
            s_foreach(
                reflection_this_property(names_property, dummy_span),
                None,
                "__elephc_reflector_name",
                vec![s_array_assign(
                    "__elephc_reflectors",
                    e_var("__elephc_reflector_name"),
                    e_new("ReflectionClass", vec![e_var("__elephc_reflector_name")]),
                )],
            ),
            s_return(e_var("__elephc_reflectors")),
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
        type_params: Vec::new(),
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
/// `ReflectionClass::getConstants(?int $filter = null)`: the full name => value map, or only the
/// constants whose modifiers match `$filter` (league/uri's `Idna\Option` asks for
/// `ReflectionClassConstant::IS_PUBLIC`). The filtered form reuses `getReflectionConstants()`.
pub(super) fn builtin_reflection_class_get_constants_method() -> ClassMethod {
    use crate::synthetic_class::{e_method_call, e_null, e_this, e_var, s_array_assign, s_assign, s_foreach, s_if, s_return};
    let mut method = builtin_reflection_class_mixed_method("getConstants", "__constants");
    method.params = vec![(
        "filter".to_string(),
        Some(TypeExpr::Nullable(Box::new(TypeExpr::Int))),
        null_expr(),
        false,
    )];
    let all = method.body.clone();
    // A closed-world reflector is always table-built: its constants' modifiers are a plain list,
    // so the filtered form needs no ReflectionClassConstant at all.
    if crate::closed_world::is_enabled() {
        use crate::synthetic_class::{e_binop, e_cast, e_int, e_this_prop, s_prop_assign};
        let _ = s_prop_assign;
        method.body = vec![
            s_if(
                binary_expr(e_var("filter"), BinOp::StrictEq, e_null(), crate::span::Span::dummy()),
                all,
                Vec::new(),
                None,
            ),
            s_assign("result", Expr::new(ExprKind::ArrayLiteral(Vec::new()), crate::span::Span::dummy())),
            s_assign("position", e_int(0)),
            s_foreach(
                e_this_prop("__constant_names"),
                None,
                "constant_name",
                vec![
                    s_if(
                        e_binop(
                            e_binop(
                                e_cast(
                                    crate::parser::ast::CastType::Int,
                                    crate::synthetic_class::e_index(
                                        e_this_prop("__constant_modifiers"),
                                        e_var("position"),
                                    ),
                                ),
                                BinOp::BitAnd,
                                e_var("filter"),
                            ),
                            BinOp::StrictNotEq,
                            e_int(0),
                        ),
                        vec![s_array_assign(
                            "result",
                            e_var("constant_name"),
                            crate::synthetic_class::e_index(
                                e_this_prop("__constants"),
                                e_var("constant_name"),
                            ),
                        )],
                        Vec::new(),
                        None,
                    ),
                    s_assign("position", e_binop(e_var("position"), BinOp::Add, e_int(1))),
                ],
            ),
            s_return(e_var("result")),
        ];
        return method;
    }
    method.body = vec![
        s_if(
            binary_expr(e_var("filter"), BinOp::StrictEq, e_null(), crate::span::Span::dummy()),
            all,
            Vec::new(),
            None,
        ),
        s_assign("result", Expr::new(ExprKind::ArrayLiteral(Vec::new()), crate::span::Span::dummy())),
        s_foreach(
            e_method_call(e_this(), "getReflectionConstants", vec![e_var("filter")]),
            None,
            "constant",
            vec![s_array_assign(
                "result",
                e_method_call(e_var("constant"), "getName", Vec::new()),
                e_method_call(e_var("constant"), "getValue", Vec::new()),
            )],
        ),
        s_return(e_var("result")),
    ];
    method
}

pub(super) fn builtin_reflection_class_mixed_method(method_name: &str, property: &str) -> ClassMethod {
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
