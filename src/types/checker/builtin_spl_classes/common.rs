//! Purpose:
//! Provides small AST builder helpers used by synthetic SPL class metadata modules.
//! Keeps method/property/body construction in one place so individual SPL files own only class behavior.
//!
//! Called from:
//! - Sibling modules under `crate::types::checker::builtin_spl_classes`.
//!
//! Key details:
//! - Helpers create dummy-span AST nodes for checker-injected synthetic methods.
//! - Visibility is restricted to the parent module to avoid becoming a general checker API.

use crate::names::Name;
use crate::parser::ast::{
    BinOp, CastType, ClassConst, ClassMethod, ClassProperty, Expr, ExprKind,
    InstanceOfTarget, PropertyHooks, StaticReceiver, Stmt, StmtKind, TypeExpr, Visibility,
};

/// Provides the Method helper used by the common module.
pub(super) fn method(
    name: &str,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    return_type: Option<TypeExpr>,
) -> ClassMethod {
    class_method(name, false, params, return_type)
}

/// Builds the synthetic method body for method with.
pub(super) fn method_with_body(
    name: &str,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    return_type: Option<TypeExpr>,
    body: Vec<Stmt>,
) -> ClassMethod {
    class_method_with_body(name, false, params, return_type, body)
}

/// Provides the Abstract method helper used by the common module.
pub(super) fn abstract_method(
    name: &str,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    return_type: Option<TypeExpr>,
) -> ClassMethod {
    let mut method = class_method_with_body(name, false, params, return_type, Vec::new());
    method.is_abstract = true;
    method.has_body = false;
    method
}

/// Computes method for the PHP class-introspection builtin.
pub(super) fn class_method(
    name: &str,
    is_static: bool,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    return_type: Option<TypeExpr>,
) -> ClassMethod {
    class_method_with_body(
        name,
        is_static,
        params,
        return_type.clone(),
        dummy_body_for(return_type.as_ref()),
    )
}

/// Computes method with body for the PHP class-introspection builtin.
pub(super) fn class_method_with_body(
    name: &str,
    is_static: bool,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    return_type: Option<TypeExpr>,
    body: Vec<Stmt>,
) -> ClassMethod {
    ClassMethod {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_static,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params,
        variadic: None,
        variadic_type: None,
        return_type,
        by_ref_return: false,
        body,
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the property metadata for storage.
pub(super) fn storage_property(name: &str, type_expr: TypeExpr) -> ClassProperty {
    storage_property_with_default(name, type_expr, None)
}

/// Builds the property metadata for protected storage.
pub(super) fn protected_storage_property(name: &str, type_expr: TypeExpr) -> ClassProperty {
    storage_property_with_visibility(name, Some(type_expr), None, Visibility::Protected)
}

/// Provides the Storage property default helper used by the common module.
pub(super) fn storage_property_default(name: &str, type_expr: TypeExpr, default: Expr) -> ClassProperty {
    storage_property_with_default(name, type_expr, Some(default))
}

/// Provides the Protected storage property untyped helper used by the common module.
pub(super) fn protected_storage_property_untyped(name: &str) -> ClassProperty {
    storage_property_with_visibility(name, None, None, Visibility::Protected)
}

/// Provides the Storage property with default helper used by the common module.
pub(super) fn storage_property_with_default(
    name: &str,
    type_expr: TypeExpr,
    default: Option<Expr>,
) -> ClassProperty {
    storage_property_with_visibility(name, Some(type_expr), default, Visibility::Private)
}

/// Provides the Storage property with visibility helper used by the common module.
pub(super) fn storage_property_with_visibility(
    name: &str,
    type_expr: Option<TypeExpr>,
    default: Option<Expr>,
    visibility: Visibility,
) -> ClassProperty {
    ClassProperty {
        name: name.to_string(),
        visibility,
        set_visibility: None,
        type_expr,
        hooks: PropertyHooks::none(),
        readonly: false,
        is_final: false,
        is_static: false,
        is_abstract: false,
        by_ref: false,
        default,
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

/// Provides the Dummy body for helper used by the common module.
pub(super) fn dummy_body_for(return_type: Option<&TypeExpr>) -> Vec<Stmt> {
    match return_type {
        Some(TypeExpr::Void) | None => Vec::new(),
        Some(TypeExpr::Int) => return_body(int_expr(0)),
        Some(TypeExpr::Bool) => return_body(bool_expr(false)),
        Some(TypeExpr::Str) => return_body(Expr::new(
            ExprKind::StringLiteral(String::new()),
            crate::span::Span::dummy(),
        )),
        Some(TypeExpr::Array(_)) => {
            return_body(Expr::new(ExprKind::ArrayLiteral(Vec::new()), crate::span::Span::dummy()))
        }
        Some(TypeExpr::Named(name)) if name.as_canonical() == "array" => {
            return_body(Expr::new(ExprKind::ArrayLiteral(Vec::new()), crate::span::Span::dummy()))
        }
        _ => return_body(Expr::new(ExprKind::Null, crate::span::Span::dummy())),
    }
}

/// Builds the synthetic method body for return.
pub(super) fn return_body(value: Expr) -> Vec<Stmt> {
    vec![return_stmt(value)]
}

/// Builds the synthetic method body for null return.
pub(super) fn null_return_body() -> Vec<Stmt> {
    return_body(expr(ExprKind::Null))
}

/// Builds the AST statement for return.
pub(super) fn return_stmt(value: Expr) -> Stmt {
    Stmt::new(StmtKind::Return(Some(value)), crate::span::Span::dummy())
}

/// Builds the AST statement for return void.
pub(super) fn return_void_stmt() -> Stmt {
    Stmt::new(StmtKind::Return(None), crate::span::Span::dummy())
}

/// Builds the AST statement for throw.
pub(super) fn throw_stmt(value: Expr) -> Stmt {
    Stmt::new(StmtKind::Throw(value), crate::span::Span::dummy())
}

/// Provides the Param helper used by the common module.
pub(super) fn param(name: &str, ty: TypeExpr) -> (String, Option<TypeExpr>, Option<Expr>, bool) {
    (name.to_string(), Some(ty), None, false)
}

/// Provides the Param default helper used by the common module.
pub(super) fn param_default(
    name: &str,
    ty: TypeExpr,
    default: Expr,
) -> (String, Option<TypeExpr>, Option<Expr>, bool) {
    (name.to_string(), Some(ty), Some(default), false)
}

/// Computes const for the PHP class-introspection builtin.
pub(super) fn class_const(name: &str, value: i64) -> ClassConst {
    ClassConst {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_final: false,
        value: int_expr(value),
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the AST expression for integer.
pub(super) fn int_expr(value: i64) -> Expr {
    Expr::new(ExprKind::IntLiteral(value), crate::span::Span::dummy())
}

/// Builds the AST expression for boolean.
pub(super) fn bool_expr(value: bool) -> Expr {
    Expr::new(ExprKind::BoolLiteral(value), crate::span::Span::dummy())
}

/// Builds the AST expression for empty array.
pub(super) fn empty_array_expr() -> Expr {
    expr(ExprKind::ArrayLiteral(Vec::new()))
}

/// Builds the AST expression for empty assoc array.
pub(super) fn empty_assoc_array_expr() -> Expr {
    expr(ExprKind::ArrayLiteralAssoc(Vec::new()))
}

/// Computes the type metadata for mixed.
pub(super) fn mixed_type() -> TypeExpr {
    named_type("mixed")
}

/// Computes the type metadata for array.
pub(super) fn array_type() -> TypeExpr {
    named_type("array")
}

/// Computes the type metadata for array<string>.
pub(super) fn string_array_type() -> TypeExpr {
    TypeExpr::Array(Box::new(TypeExpr::Str))
}

/// Computes the type metadata for named.
pub(super) fn named_type(name: &str) -> TypeExpr {
    TypeExpr::Named(Name::unqualified(name))
}

/// Provides the Expr helper used by the common module.
pub(super) fn expr(kind: ExprKind) -> Expr {
    Expr::new(kind, crate::span::Span::dummy())
}

/// Builds the AST expression for string.
pub(super) fn string_expr(value: &str) -> Expr {
    expr(ExprKind::StringLiteral(value.to_string()))
}

/// Builds the AST expression for var.
pub(super) fn var_expr(name: &str) -> Expr {
    expr(ExprKind::Variable(name.to_string()))
}

/// Builds the AST expression for this.
pub(super) fn this_expr() -> Expr {
    expr(ExprKind::This)
}

/// Builds the AST expression for null.
pub(super) fn null_expr() -> Expr {
    expr(ExprKind::Null)
}

/// Builds the AST expression for binary.
pub(super) fn binary_expr(left: Expr, op: BinOp, right: Expr) -> Expr {
    expr(ExprKind::BinaryOp {
        left: Box::new(left),
        op,
        right: Box::new(right),
    })
}

/// Builds the AST expression for null coalescing.
pub(super) fn null_coalesce_expr(value: Expr, default: Expr) -> Expr {
    expr(ExprKind::NullCoalesce {
        value: Box::new(value),
        default: Box::new(default),
    })
}

/// Builds the AST expression for not.
pub(super) fn not_expr(value: Expr) -> Expr {
    expr(ExprKind::Not(Box::new(value)))
}

/// Builds the AST expression for cast.
pub(super) fn cast_expr(target: CastType, value: Expr) -> Expr {
    expr(ExprKind::Cast {
        target,
        expr: Box::new(value),
    })
}

/// Provides the Method call helper used by the common module.
pub(super) fn method_call(object: Expr, method: &str, args: Vec<Expr>) -> Expr {
    expr(ExprKind::MethodCall {
        object: Box::new(object),
        method: method.to_string(),
        args,
    })
}

/// Provides the Function call helper used by the common module.
pub(super) fn function_call(name: &str, args: Vec<Expr>) -> Expr {
    expr(ExprKind::FunctionCall {
        name: Name::unqualified(name),
        args,
    })
}

/// Builds the AST expression for new object.
pub(super) fn new_object_expr(class_name: &str, args: Vec<Expr>) -> Expr {
    expr(ExprKind::NewObject {
        class_name: Name::unqualified(class_name),
        args,
    })
}

/// Builds the AST expression for a runtime class-string object factory.
pub(super) fn new_dynamic_object_expr(
    class_name: Expr,
    fallback_class: &str,
    required_parent: &str,
    args: Vec<Expr>,
) -> Expr {
    expr(ExprKind::NewDynamicObject {
        class_name: Box::new(class_name),
        fallback_class: Name::unqualified(fallback_class),
        required_parent: Name::unqualified(required_parent),
        args,
    })
}

/// Builds the AST expression for new static.
pub(super) fn new_static_expr(args: Vec<Expr>) -> Expr {
    expr(ExprKind::NewScopedObject {
        receiver: StaticReceiver::Static,
        args,
    })
}

/// Builds the AST expression for instanceof.
pub(super) fn instanceof_expr(value: Expr, class_name: &str) -> Expr {
    expr(ExprKind::InstanceOf {
        value: Box::new(value),
        target: InstanceOfTarget::Name(Name::unqualified(class_name)),
    })
}

/// Provides the Property access helper used by the common module.
pub(super) fn property_access(object: Expr, property: &str) -> Expr {
    expr(ExprKind::PropertyAccess {
        object: Box::new(object),
        property: property.to_string(),
    })
}

/// Provides the Array access helper used by the common module.
pub(super) fn array_access(array: Expr, index: Expr) -> Expr {
    expr(ExprKind::ArrayAccess {
        array: Box::new(array),
        index: Box::new(index),
    })
}

/// Builds the AST statement for assign.
pub(super) fn assign_stmt(name: &str, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::Assign {
            name: name.to_string(),
            value,
        },
        crate::span::Span::dummy(),
    )
}

/// Builds the AST statement for typed assign.
pub(super) fn typed_assign_stmt(name: &str, type_expr: TypeExpr, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::TypedAssign {
            type_expr,
            name: name.to_string(),
            value,
        },
        crate::span::Span::dummy(),
    )
}

/// Builds the AST statement for expr.
pub(super) fn expr_stmt(value: Expr) -> Stmt {
    Stmt::new(StmtKind::ExprStmt(value), crate::span::Span::dummy())
}

/// Builds the AST statement for property assign.
pub(super) fn property_assign_stmt(object: Expr, property: &str, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::PropertyAssign {
            object: Box::new(object),
            property: property.to_string(),
            value,
        },
        crate::span::Span::dummy(),
    )
}

/// Builds the AST statement for property array push.
pub(super) fn property_array_push_stmt(object: Expr, property: &str, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::PropertyArrayPush {
            object: Box::new(object),
            property: property.to_string(),
            value,
        },
        crate::span::Span::dummy(),
    )
}

/// Builds the AST statement for property array assign.
pub(super) fn property_array_assign_stmt(object: Expr, property: &str, index: Expr, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::PropertyArrayAssign {
            object: Box::new(object),
            property: property.to_string(),
            index,
            value,
        },
        crate::span::Span::dummy(),
    )
}

/// Builds the AST statement for array assign.
pub(super) fn array_assign_stmt(array: &str, index: Expr, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::ArrayAssign {
            array: array.to_string(),
            index,
            value,
        },
        crate::span::Span::dummy(),
    )
}

/// Builds the AST statement for array push.
pub(super) fn array_push_stmt(array: &str, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::ArrayPush {
            array: array.to_string(),
            value,
        },
        crate::span::Span::dummy(),
    )
}

/// Builds the AST statement for while.
pub(super) fn while_stmt(condition: Expr, body: Vec<Stmt>) -> Stmt {
    Stmt::new(
        StmtKind::While { condition, body },
        crate::span::Span::dummy(),
    )
}

/// Builds the AST statement for if.
pub(super) fn if_stmt(condition: Expr, then_body: Vec<Stmt>, else_body: Option<Vec<Stmt>>) -> Stmt {
    Stmt::new(
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses: Vec::new(),
            else_body,
        },
        crate::span::Span::dummy(),
    )
}

/// Builds the AST statement for foreach.
pub(super) fn foreach_stmt(array: Expr, key_var: Option<&str>, value_var: &str, body: Vec<Stmt>) -> Stmt {
    Stmt::new(
        StmtKind::Foreach {
            array,
            key_var: key_var.map(str::to_string),
            value_var: value_var.to_string(),
            value_by_ref: false,
            body,
        },
        crate::span::Span::dummy(),
    )
}

/// Builds the AST statement for increment.
pub(super) fn increment_stmt(name: &str) -> Stmt {
    assign_stmt(name, binary_expr(var_expr(name), BinOp::Add, int_expr(1)))
}

/// Builds the AST expression for count.
pub(super) fn count_expr(value: Expr) -> Expr {
    function_call("count", vec![value])
}
