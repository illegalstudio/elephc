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

/// Returns `getAttributes(?string $name = null, int $flags = 0)` with php-src filtering.
pub(super) fn builtin_reflection_owner_get_attributes_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let expr = |kind| Expr::new(kind, dummy_span);
    let variable = |name: &str| expr(ExprKind::Variable(name.to_string()));
    let reflection_attribute_flag = || expr(ExprKind::IntLiteral(2));
    let binary = |left: Expr, op, right: Expr| {
        expr(ExprKind::BinaryOp {
            left: Box::new(left),
            op,
            right: Box::new(right),
        })
    };
    let attribute_name = || {
        expr(ExprKind::MethodCall {
            object: Box::new(variable("attribute")),
            method: "getName".to_string(),
            args: Vec::new(),
        })
    };
    let clone_attribute = || expr(ExprKind::Clone(Box::new(variable("attribute"))));
    let push_clone = || {
        Stmt::new(
            StmtKind::ArrayPush {
                array: "result".to_string(),
                value: clone_attribute(),
            },
            dummy_span,
        )
    };
    let body = vec![
        Stmt::new(
            StmtKind::If {
                condition: binary(
                    binary(
                        variable("flags"),
                        BinOp::StrictNotEq,
                        expr(ExprKind::IntLiteral(0)),
                    ),
                    BinOp::And,
                    binary(
                        variable("flags"),
                        BinOp::StrictNotEq,
                        reflection_attribute_flag(),
                    ),
                ),
                then_body: vec![Stmt::new(
                    StmtKind::Throw(expr(ExprKind::NewObject {
                        class_name: Name::from("ValueError"),
                        args: vec![expr(ExprKind::StringLiteral(
                            "Argument #2 ($flags) must be a valid attribute filter flag"
                                .to_string(),
                        ))],
                    })),
                    dummy_span,
                )],
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            dummy_span,
        ),
        Stmt::new(
            StmtKind::If {
                condition: binary(variable("name"), BinOp::StrictEq, expr(ExprKind::Null)),
                then_body: vec![Stmt::new(
                    StmtKind::Return(Some(expr(ExprKind::PropertyAccess {
                        object: Box::new(expr(ExprKind::This)),
                        property: "__attrs".to_string(),
                    }))),
                    dummy_span,
                )],
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            dummy_span,
        ),
        Stmt::new(
            StmtKind::Assign {
                name: "result".to_string(),
                value: expr(ExprKind::ArrayLiteral(Vec::new())),
            },
            dummy_span,
        ),
        Stmt::new(
            StmtKind::Foreach {
                array: expr(ExprKind::PropertyAccess {
                    object: Box::new(expr(ExprKind::This)),
                    property: "__attrs".to_string(),
                }),
                key_var: None,
                value_var: "attribute".to_string(),
                value_by_ref: false,
                body: vec![Stmt::new(
                    StmtKind::If {
                        condition: binary(
                            variable("flags"),
                            BinOp::StrictEq,
                            reflection_attribute_flag(),
                        ),
                        then_body: vec![Stmt::new(
                            StmtKind::If {
                                condition: expr(ExprKind::FunctionCall {
                                    name: Name::from("is_a"),
                                    args: vec![
                                        attribute_name(),
                                        variable("name"),
                                        expr(ExprKind::BoolLiteral(true)),
                                    ],
                                }),
                                then_body: vec![push_clone()],
                                elseif_clauses: Vec::new(),
                                else_body: None,
                            },
                            dummy_span,
                        )],
                        elseif_clauses: vec![(
                            binary(attribute_name(), BinOp::StrictEq, variable("name")),
                            vec![push_clone()],
                        )],
                        else_body: None,
                    },
                    dummy_span,
                )],
            },
            dummy_span,
        ),
        Stmt::new(StmtKind::Return(Some(variable("result"))), dummy_span),
    ];
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
                Some(Expr::new(ExprKind::Null, dummy_span)),
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
        body,
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
