//! Purpose:
//! Builds ReflectionClass relation predicates and shared AST expressions.
//!
//! Called from:
//! - The Reflection checker metadata facade and sibling builders.
//!
//! Key details:
//! - Interface, subclass, and instance checks keep case-insensitive PHP name semantics.

use super::*;

/// Returns `ReflectionClass::implementsInterface()` backed by interface-name metadata.
pub(super) fn builtin_reflection_class_implements_interface_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let interface_var = Expr::new(ExprKind::Variable("interface".to_string()), dummy_span);
    let candidate_var = Expr::new(ExprKind::Variable("interfaceName".to_string()), dummy_span);
    let missing_interface_check = Stmt::new(
        StmtKind::If {
            condition: Expr::new(
                ExprKind::Not(Box::new(function_call(
                    "interface_exists",
                    vec![interface_var.clone()],
                    dummy_span,
                ))),
                dummy_span,
            ),
            then_body: vec![
                throw_if_class_like_exists(
                    "class_exists",
                    interface_var.clone(),
                    concat_expr(
                        interface_var.clone(),
                        string_lit(" is not an interface", dummy_span),
                        dummy_span,
                    ),
                    dummy_span,
                ),
                throw_if_class_like_exists(
                    "trait_exists",
                    interface_var.clone(),
                    concat_expr(
                        interface_var.clone(),
                        string_lit(" is not an interface", dummy_span),
                        dummy_span,
                    ),
                    dummy_span,
                ),
                throw_if_class_like_exists(
                    "enum_exists",
                    interface_var.clone(),
                    concat_expr(
                        interface_var.clone(),
                        string_lit(" is not an interface", dummy_span),
                        dummy_span,
                    ),
                    dummy_span,
                ),
                throw_new_reflection_exception(
                    concat_expr(
                        concat_expr(
                            string_lit("Interface \"", dummy_span),
                            interface_var.clone(),
                            dummy_span,
                        ),
                        string_lit("\" does not exist", dummy_span),
                        dummy_span,
                    ),
                    dummy_span,
                ),
            ],
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        dummy_span,
    );
    let lowered_interface = strtolower_call(interface_var.clone(), dummy_span);
    let lowered_candidate = strtolower_call(candidate_var, dummy_span);
    let candidate_matches = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(lowered_candidate),
            op: BinOp::Eq,
            right: Box::new(lowered_interface.clone()),
        },
        dummy_span,
    );
    let reflected_name_matches = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(strtolower_call(
                reflection_this_property("__name", dummy_span),
                dummy_span,
            )),
            op: BinOp::Eq,
            right: Box::new(lowered_interface),
        },
        dummy_span,
    );
    let interface_self_matches = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(reflection_this_property("__is_interface", dummy_span)),
            op: BinOp::And,
            right: Box::new(reflected_name_matches),
        },
        dummy_span,
    );
    ClassMethod {
        type_params: Vec::new(),
        name: "implementsInterface".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("interface".to_string(), Some(TypeExpr::Str), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(bool_type()),
        by_ref_return: false,
        body: vec![
            missing_interface_check,
            Stmt::new(
                StmtKind::Foreach {
                    array: reflection_this_property("__interface_names", dummy_span),
                    key_var: None,
                    value_var: "interfaceName".to_string(),
                    value_by_ref: false,
                    body: vec![Stmt::new(
                        StmtKind::If {
                            condition: candidate_matches,
                            then_body: vec![Stmt::new(StmtKind::Return(true_bool()), dummy_span)],
                            elseif_clauses: Vec::new(),
                            else_body: None,
                        },
                        dummy_span,
                    )],
                },
                dummy_span,
            ),
            Stmt::new(
                StmtKind::If {
                    condition: interface_self_matches,
                    then_body: vec![Stmt::new(StmtKind::Return(true_bool()), dummy_span)],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                dummy_span,
            ),
            Stmt::new(StmtKind::Return(false_bool()), dummy_span),
        ],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns `ReflectionClass::isSubclassOf()` backed by parent and interface metadata.
pub(super) fn builtin_reflection_class_is_subclass_of_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let class_var = variable_expr("class", dummy_span);
    let target_var = variable_expr("target", dummy_span);
    let parent_name_var = variable_expr("parentName", dummy_span);
    let interface_name_var = variable_expr("interfaceName", dummy_span);
    let target_missing = binary_expr(
        binary_expr(
            Expr::new(
                ExprKind::Not(Box::new(function_call(
                    "class_exists",
                    vec![class_var.clone()],
                    dummy_span,
                ))),
                dummy_span,
            ),
            BinOp::And,
            Expr::new(
                ExprKind::Not(Box::new(function_call(
                    "interface_exists",
                    vec![class_var.clone()],
                    dummy_span,
                ))),
                dummy_span,
            ),
            dummy_span,
        ),
        BinOp::And,
        binary_expr(
            Expr::new(
                ExprKind::Not(Box::new(function_call(
                    "trait_exists",
                    vec![class_var.clone()],
                    dummy_span,
                ))),
                dummy_span,
            ),
            BinOp::And,
            Expr::new(
                ExprKind::Not(Box::new(function_call(
                    "enum_exists",
                    vec![class_var.clone()],
                    dummy_span,
                ))),
                dummy_span,
            ),
            dummy_span,
        ),
        dummy_span,
    );
    let missing_target_check = Stmt::new(
        StmtKind::If {
            condition: target_missing,
            then_body: vec![throw_new_reflection_exception(
                concat_expr(
                    concat_expr(
                        string_lit("Class \"", dummy_span),
                        class_var.clone(),
                        dummy_span,
                    ),
                    string_lit("\" does not exist", dummy_span),
                    dummy_span,
                ),
                dummy_span,
            )],
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        dummy_span,
    );
    let parent_matches = binary_expr(
        strtolower_call(parent_name_var, dummy_span),
        BinOp::Eq,
        target_var.clone(),
        dummy_span,
    );
    let interface_matches = binary_expr(
        strtolower_call(interface_name_var, dummy_span),
        BinOp::Eq,
        target_var.clone(),
        dummy_span,
    );
    ClassMethod {
        type_params: Vec::new(),
        name: "isSubclassOf".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("class".to_string(), Some(TypeExpr::Str), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(bool_type()),
        by_ref_return: false,
        body: vec![
            missing_target_check,
            Stmt::new(
                StmtKind::Assign {
                    name: "target".to_string(),
                    value: strtolower_call(class_var, dummy_span),
                },
                dummy_span,
            ),
            Stmt::new(
                StmtKind::Foreach {
                    array: reflection_this_property("__parent_names", dummy_span),
                    key_var: None,
                    value_var: "parentName".to_string(),
                    value_by_ref: false,
                    body: vec![Stmt::new(
                        StmtKind::If {
                            condition: parent_matches,
                            then_body: vec![Stmt::new(StmtKind::Return(true_bool()), dummy_span)],
                            elseif_clauses: Vec::new(),
                            else_body: None,
                        },
                        dummy_span,
                    )],
                },
                dummy_span,
            ),
            Stmt::new(
                StmtKind::Foreach {
                    array: reflection_this_property("__interface_names", dummy_span),
                    key_var: None,
                    value_var: "interfaceName".to_string(),
                    value_by_ref: false,
                    body: vec![Stmt::new(
                        StmtKind::If {
                            condition: interface_matches,
                            then_body: vec![Stmt::new(StmtKind::Return(true_bool()), dummy_span)],
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

/// Returns `ReflectionClass::isInstance()` backed by PHP's class relation predicate.
pub(super) fn builtin_reflection_class_is_instance_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        type_params: Vec::new(),
        name: "isInstance".to_string(),
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
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(
                ExprKind::InstanceOf {
                    value: Box::new(variable_expr("object", dummy_span)),
                    target: InstanceOfTarget::Expr(Box::new(reflection_this_property(
                        "__name", dummy_span,
                    ))),
                },
                dummy_span,
            ))),
            dummy_span,
        )],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Builds `if (<predicate>($interface)) throw new ReflectionException($message);`.
pub(super) fn throw_if_class_like_exists(
    predicate_name: &str,
    interface_var: Expr,
    message: Expr,
    span: crate::span::Span,
) -> Stmt {
    Stmt::new(
        StmtKind::If {
            condition: function_call(predicate_name, vec![interface_var], span),
            then_body: vec![throw_new_reflection_exception(message, span)],
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        span,
    )
}

/// Builds a normal function call expression for synthetic Reflection method bodies.
pub(super) fn function_call(name: &str, args: Vec<Expr>, span: crate::span::Span) -> Expr {
    Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified(name),
            args,
        },
        span,
    )
}

/// Builds a binary expression with the given operator and operands.
pub(super) fn binary_expr(left: Expr, op: BinOp, right: Expr, span: crate::span::Span) -> Expr {
    Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(left),
            op,
            right: Box::new(right),
        },
        span,
    )
}

/// Builds a PHP string literal expression for synthetic method bodies.
pub(super) fn string_lit(value: &str, span: crate::span::Span) -> Expr {
    Expr::new(ExprKind::StringLiteral(value.to_string()), span)
}

/// Builds a PHP string concatenation expression.
pub(super) fn concat_expr(left: Expr, right: Expr, span: crate::span::Span) -> Expr {
    binary_expr(left, BinOp::Concat, right, span)
}

/// Builds `throw new ReflectionException($message)`.
pub(super) fn throw_new_reflection_exception(message: Expr, span: crate::span::Span) -> Stmt {
    Stmt::new(
        StmtKind::Throw(Expr::new(
            ExprKind::NewObject {
                class_name: Name::unqualified("ReflectionException"),
                args: vec![message],
            },
            span,
        )),
        span,
    )
}

/// Builds a `null` expression for synthetic Reflection method bodies.
pub(super) fn null_value(span: crate::span::Span) -> Expr {
    Expr::new(ExprKind::Null, span)
}

/// Builds `$this->{$property}` for synthetic ReflectionClass method bodies.
pub(super) fn reflection_this_property(property: &str, span: crate::span::Span) -> Expr {
    Expr::new(
        ExprKind::PropertyAccess {
            object: Box::new(Expr::new(ExprKind::This, span)),
            property: property.to_string(),
        },
        span,
    )
}

/// Builds a `strtolower()` call around an expression for case-insensitive class names.
pub(super) fn strtolower_call(expr: Expr, span: crate::span::Span) -> Expr {
    function_call("strtolower", vec![expr], span)
}

/// Builds a variable expression for synthetic Reflection method bodies.
pub(super) fn variable_expr(name: &str, span: crate::span::Span) -> Expr {
    Expr::new(ExprKind::Variable(name.to_string()), span)
}

/// Builds a method call expression for synthetic Reflection method bodies.
pub(super) fn method_call_expr(object: Expr, method: &str, args: Vec<Expr>, span: crate::span::Span) -> Expr {
    Expr::new(
        ExprKind::MethodCall {
            object: Box::new(object),
            method: method.to_string(),
            args,
        },
        span,
    )
}
