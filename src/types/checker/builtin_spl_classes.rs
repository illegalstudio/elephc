//! Purpose:
//! Injects SPL container class metadata into the checker.
//! Provides nominal class/interface/signature contracts for runtime-backed SPL containers.
//!
//! Called from:
//! - `crate::types::checker::driver`
//!
//! Key details:
//! - Direct storage and legacy serialization methods use runtime `IntrinsicCall` backing.
//! - Structured serialization/debug helpers keep small synthetic PHP bodies.

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{
    BinOp, ClassConst, ClassMethod, Expr, ExprKind, Stmt, StmtKind, TypeExpr, Visibility,
};
use crate::types::traits::FlattenedClass;

use super::builtin_types::InterfaceDeclInfo;

pub(crate) fn inject_builtin_spl_classes(
    interface_map: &mut HashMap<String, InterfaceDeclInfo>,
    class_map: &mut HashMap<String, FlattenedClass>,
) -> Result<(), CompileError> {
    for class_name in PHASE4_SPL_CLASS_NAMES {
        let class_key = php_symbol_key(class_name);
        if interface_map
            .keys()
            .any(|name| php_symbol_key(name) == class_key)
            || class_map
                .keys()
                .any(|name| php_symbol_key(name) == class_key)
        {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!("Cannot redeclare built-in SPL class: {}", class_name),
            ));
        }
    }

    class_map.insert(
        "SplDoublyLinkedList".to_string(),
        FlattenedClass {
            name: "SplDoublyLinkedList".to_string(),
            extends: None,
            implements: vec![
                "Iterator".to_string(),
                "Countable".to_string(),
                "ArrayAccess".to_string(),
            ],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: spl_doubly_linked_list_methods(),
            attributes: Vec::new(),
            constants: spl_doubly_linked_list_constants(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "SplStack".to_string(),
        FlattenedClass {
            name: "SplStack".to_string(),
            extends: Some("SplDoublyLinkedList".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: Vec::new(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "SplQueue".to_string(),
        FlattenedClass {
            name: "SplQueue".to_string(),
            extends: Some("SplDoublyLinkedList".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: vec![
                method("enqueue", vec![param("value", mixed_type())], Some(TypeExpr::Void)),
                method("dequeue", Vec::new(), Some(mixed_type())),
            ],
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "SplFixedArray".to_string(),
        FlattenedClass {
            name: "SplFixedArray".to_string(),
            extends: None,
            implements: vec![
                "ArrayAccess".to_string(),
                "Countable".to_string(),
                "JsonSerializable".to_string(),
            ],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: spl_fixed_array_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    Ok(())
}

const PHASE4_SPL_CLASS_NAMES: &[&str] = &[
    "SplDoublyLinkedList",
    "SplStack",
    "SplQueue",
    "SplFixedArray",
];

fn spl_doubly_linked_list_methods() -> Vec<ClassMethod> {
    vec![
        method(
            "add",
            vec![param("index", TypeExpr::Int), param("value", mixed_type())],
            Some(TypeExpr::Void),
        ),
        method("pop", Vec::new(), Some(mixed_type())),
        method("shift", Vec::new(), Some(mixed_type())),
        method("push", vec![param("value", mixed_type())], Some(TypeExpr::Void)),
        method(
            "unshift",
            vec![param("value", mixed_type())],
            Some(TypeExpr::Void),
        ),
        method("top", Vec::new(), Some(mixed_type())),
        method("bottom", Vec::new(), Some(mixed_type())),
        method("count", Vec::new(), Some(TypeExpr::Int)),
        method("isEmpty", Vec::new(), Some(TypeExpr::Bool)),
        method(
            "setIteratorMode",
            vec![param("mode", TypeExpr::Int)],
            Some(TypeExpr::Void),
        ),
        method("getIteratorMode", Vec::new(), Some(TypeExpr::Int)),
        method(
            "offsetExists",
            vec![param("index", mixed_type())],
            Some(TypeExpr::Bool),
        ),
        method(
            "offsetGet",
            vec![param("index", mixed_type())],
            Some(mixed_type()),
        ),
        method(
            "offsetSet",
            vec![param("index", mixed_type()), param("value", mixed_type())],
            Some(TypeExpr::Void),
        ),
        method(
            "offsetUnset",
            vec![param("index", mixed_type())],
            Some(TypeExpr::Void),
        ),
        method("rewind", Vec::new(), Some(TypeExpr::Void)),
        method("current", Vec::new(), Some(mixed_type())),
        method("key", Vec::new(), Some(mixed_type())),
        method("prev", Vec::new(), Some(TypeExpr::Void)),
        method("next", Vec::new(), Some(TypeExpr::Void)),
        method("valid", Vec::new(), Some(TypeExpr::Bool)),
        method("serialize", Vec::new(), Some(TypeExpr::Str)),
        method(
            "unserialize",
            vec![param("data", TypeExpr::Str)],
            Some(TypeExpr::Void),
        ),
        method_with_body(
            "__serialize",
            Vec::new(),
            Some(array_type()),
            dll_serialize_array_body(),
        ),
        method_with_body(
            "__unserialize",
            vec![param("data", array_type())],
            Some(TypeExpr::Void),
            dll_unserialize_body(),
        ),
        method_with_body(
            "__debugInfo",
            Vec::new(),
            Some(array_type()),
            dll_debug_info_body(),
        ),
    ]
}

fn spl_fixed_array_methods() -> Vec<ClassMethod> {
    vec![
        method(
            "__construct",
            vec![param_default("size", TypeExpr::Int, int_expr(0))],
            Some(TypeExpr::Void),
        ),
        method_with_body("__wakeup", Vec::new(), Some(TypeExpr::Void), Vec::new()),
        class_method(
            "fromArray",
            true,
            vec![
                param("array", array_type()),
                param_default("preserveKeys", TypeExpr::Bool, bool_expr(true)),
            ],
            Some(named_type("SplFixedArray")),
        ),
        method_with_body(
            "__serialize",
            Vec::new(),
            Some(array_type()),
            vec![return_stmt(method_call(this_expr(), "toArray", Vec::new()))],
        ),
        method("__unserialize", vec![param("data", array_type())], Some(TypeExpr::Void)),
        method("count", Vec::new(), Some(TypeExpr::Int)),
        method("toArray", Vec::new(), Some(array_type())),
        method("getSize", Vec::new(), Some(TypeExpr::Int)),
        method(
            "setSize",
            vec![param("size", TypeExpr::Int)],
            Some(TypeExpr::Void),
        ),
        method(
            "offsetExists",
            vec![param("index", mixed_type())],
            Some(TypeExpr::Bool),
        ),
        method(
            "offsetGet",
            vec![param("index", mixed_type())],
            Some(mixed_type()),
        ),
        method(
            "offsetSet",
            vec![param("index", mixed_type()), param("value", mixed_type())],
            Some(TypeExpr::Void),
        ),
        method(
            "offsetUnset",
            vec![param("index", mixed_type())],
            Some(TypeExpr::Void),
        ),
        method("jsonSerialize", Vec::new(), Some(array_type())),
    ]
}

fn spl_doubly_linked_list_constants() -> Vec<ClassConst> {
    vec![
        class_const("IT_MODE_LIFO", 2),
        class_const("IT_MODE_FIFO", 0),
        class_const("IT_MODE_DELETE", 1),
        class_const("IT_MODE_KEEP", 0),
    ]
}

fn method(
    name: &str,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    return_type: Option<TypeExpr>,
) -> ClassMethod {
    class_method(name, false, params, return_type)
}

fn method_with_body(
    name: &str,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    return_type: Option<TypeExpr>,
    body: Vec<Stmt>,
) -> ClassMethod {
    class_method_with_body(name, false, params, return_type, body)
}

fn class_method(
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

fn class_method_with_body(
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
        return_type,
        body,
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

fn dummy_body_for(return_type: Option<&TypeExpr>) -> Vec<Stmt> {
    match return_type {
        Some(TypeExpr::Void) | None => Vec::new(),
        Some(TypeExpr::Int) => return_body(int_expr(0)),
        Some(TypeExpr::Bool) => return_body(bool_expr(false)),
        Some(TypeExpr::Str) => return_body(Expr::new(
            ExprKind::StringLiteral(String::new()),
            crate::span::Span::dummy(),
        )),
        Some(TypeExpr::Named(name)) if name.as_canonical() == "array" => {
            return_body(Expr::new(ExprKind::ArrayLiteral(Vec::new()), crate::span::Span::dummy()))
        }
        _ => return_body(Expr::new(ExprKind::Null, crate::span::Span::dummy())),
    }
}

fn return_body(value: Expr) -> Vec<Stmt> {
    vec![return_stmt(value)]
}

fn return_stmt(value: Expr) -> Stmt {
    Stmt::new(StmtKind::Return(Some(value)), crate::span::Span::dummy())
}

fn param(name: &str, ty: TypeExpr) -> (String, Option<TypeExpr>, Option<Expr>, bool) {
    (name.to_string(), Some(ty), None, false)
}

fn param_default(
    name: &str,
    ty: TypeExpr,
    default: Expr,
) -> (String, Option<TypeExpr>, Option<Expr>, bool) {
    (name.to_string(), Some(ty), Some(default), false)
}

fn class_const(name: &str, value: i64) -> ClassConst {
    ClassConst {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_final: false,
        value: int_expr(value),
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

fn int_expr(value: i64) -> Expr {
    Expr::new(ExprKind::IntLiteral(value), crate::span::Span::dummy())
}

fn bool_expr(value: bool) -> Expr {
    Expr::new(ExprKind::BoolLiteral(value), crate::span::Span::dummy())
}

fn mixed_type() -> TypeExpr {
    named_type("mixed")
}

fn array_type() -> TypeExpr {
    named_type("array")
}

fn named_type(name: &str) -> TypeExpr {
    TypeExpr::Named(Name::unqualified(name))
}

fn expr(kind: ExprKind) -> Expr {
    Expr::new(kind, crate::span::Span::dummy())
}

fn string_expr(value: &str) -> Expr {
    expr(ExprKind::StringLiteral(value.to_string()))
}

fn var_expr(name: &str) -> Expr {
    expr(ExprKind::Variable(name.to_string()))
}

fn this_expr() -> Expr {
    expr(ExprKind::This)
}

fn binary_expr(left: Expr, op: BinOp, right: Expr) -> Expr {
    expr(ExprKind::BinaryOp {
        left: Box::new(left),
        op,
        right: Box::new(right),
    })
}

fn not_expr(value: Expr) -> Expr {
    expr(ExprKind::Not(Box::new(value)))
}

fn method_call(object: Expr, method: &str, args: Vec<Expr>) -> Expr {
    expr(ExprKind::MethodCall {
        object: Box::new(object),
        method: method.to_string(),
        args,
    })
}

fn array_access(array: Expr, index: Expr) -> Expr {
    expr(ExprKind::ArrayAccess {
        array: Box::new(array),
        index: Box::new(index),
    })
}

fn assign_stmt(name: &str, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::Assign {
            name: name.to_string(),
            value,
        },
        crate::span::Span::dummy(),
    )
}

fn expr_stmt(value: Expr) -> Stmt {
    Stmt::new(StmtKind::ExprStmt(value), crate::span::Span::dummy())
}

fn array_push_stmt(array: &str, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::ArrayPush {
            array: array.to_string(),
            value,
        },
        crate::span::Span::dummy(),
    )
}

fn while_stmt(condition: Expr, body: Vec<Stmt>) -> Stmt {
    Stmt::new(
        StmtKind::While { condition, body },
        crate::span::Span::dummy(),
    )
}

fn foreach_stmt(array: Expr, key_var: Option<&str>, value_var: &str, body: Vec<Stmt>) -> Stmt {
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

fn increment_stmt(name: &str) -> Stmt {
    assign_stmt(name, binary_expr(var_expr(name), BinOp::Add, int_expr(1)))
}

fn dll_items_snapshot_prelude() -> Vec<Stmt> {
    vec![
        assign_stmt("items", expr(ExprKind::ArrayLiteral(Vec::new()))),
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", method_call(this_expr(), "count", Vec::new())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                array_push_stmt("items", method_call(this_expr(), "offsetGet", vec![var_expr("i")])),
                increment_stmt("i"),
            ],
        ),
    ]
}

fn dll_serialize_array_body() -> Vec<Stmt> {
    let mut body = dll_items_snapshot_prelude();
    body.push(return_stmt(expr(ExprKind::ArrayLiteral(vec![
        method_call(this_expr(), "getIteratorMode", Vec::new()),
        var_expr("items"),
        expr(ExprKind::ArrayLiteral(Vec::new())),
    ]))));
    body
}

fn dll_debug_info_body() -> Vec<Stmt> {
    let mut body = vec![
        assign_stmt("mode", method_call(this_expr(), "getIteratorMode", Vec::new())),
        expr_stmt(method_call(this_expr(), "setIteratorMode", vec![int_expr(0)])),
    ];
    body.extend(dll_items_snapshot_prelude());
    body.push(expr_stmt(method_call(
        this_expr(),
        "setIteratorMode",
        vec![var_expr("mode")],
    )));
    body.push(return_stmt(expr(ExprKind::ArrayLiteralAssoc(vec![
        (
            string_expr("\0SplDoublyLinkedList\0flags"),
            var_expr("mode"),
        ),
        (
            string_expr("\0SplDoublyLinkedList\0dllist"),
            var_expr("items"),
        ),
    ]))));
    body
}

fn dll_unserialize_body() -> Vec<Stmt> {
    vec![
        expr_stmt(method_call(
            this_expr(),
            "setIteratorMode",
            vec![array_access(var_expr("data"), int_expr(0))],
        )),
        while_stmt(
            not_expr(method_call(this_expr(), "isEmpty", Vec::new())),
            vec![expr_stmt(method_call(this_expr(), "pop", Vec::new()))],
        ),
        foreach_stmt(
            array_access(var_expr("data"), int_expr(1)),
            None,
            "value",
            vec![expr_stmt(method_call(this_expr(), "push", vec![var_expr("value")]))],
        ),
    ]
}
