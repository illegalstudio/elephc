//! Purpose:
//! Builds WeakMap's PHP collection behavior and a live iterator over weak object keys.
//!
//! Called from:
//! - `super::core_declarations()` before normal checking and EIR lowering.
//!
//! Key details:
//! - Entry records own WeakReference wrappers and values, never their key objects.
//! - The native object-retirement callback removes values before key storage is reclaimed.
//! - Registry entries contain weak map references and are unlinked before map property cleanup.

use crate::parser::ast::{BinOp, Expr, ExprKind, Program, StaticReceiver, Stmt, StmtKind, TypeExpr};
use crate::synthetic_class::*;

/// Builds the collection and its internal iterator using ordinary PHP declaration nodes.
pub(super) fn declarations() -> Program {
    let mut map = class("WeakMap").final_()
        .implements("ArrayAccess").implements("Countable").implements("IteratorAggregate")
        .private_prop("__elephc_entries", t_array(), Some(e_array(vec![])))
        .private_prop("__elephc_sequence", TypeExpr::Int, Some(e_int(0)))
        .private_static_prop("__elephc_maps", t_array(), Some(e_array(vec![])))
        .method(method("__construct").body(vec![
            // This real no-op call also roots the native callback's EIR method body.
            s_expr(e_static_call("WeakMap", "__elephc_retire", vec![e_int(-1)])),
        ]));
    map = map.method(method("__elephc_key").static_().private().param("object", t_mixed())
        .returns(t_mixed()).body(vec![validate_key(), s_throw(e_new_fq("Error", vec![
            e_str("WeakMap key extraction was not lowered"),
        ]))]));
    map = map.method(method("__elephc_id").static_().private().param("object", t_mixed())
        .returns(TypeExpr::Int).body(vec![validate_key(), s_return(e_int(0))]));
    map = map.method(method("__elephc_array").static_().private().param("value", t_mixed())
        .returns(t_array()).body(vec![
            s_if(e_not(e_call("is_array", vec![e_var("value")])), vec![
                s_throw(e_new_fq("TypeError", vec![e_str("WeakMap internal array expected")])),
            ], vec![], None),
            s_return(e_array(vec![])),
        ]));
    map = map.method(method("__elephc_append").private().param("value", t_mixed())
        .returns(TypeExpr::Void).body(vec![s_throw(e_new_fq("Error", vec![
            e_str("Cannot append to WeakMap"),
        ]))]));
    map = map.method(method("offsetSet").param("object", t_mixed()).param("value", t_mixed())
        .returns(TypeExpr::Void).body(vec![
            validate_key(), key_id(),
            s_if(present(entry()), vec![
                s_assign("__entry", entry()),
                s_assign("__sequence", e_index(e_var("__entry"), e_int(2))),
            ], vec![], Some(vec![
                s_assign("__sequence", e_this_prop("__elephc_sequence")),
                s_prop_assign(e_this(), "__elephc_sequence", e_binop(e_var("__sequence"), BinOp::Add, e_int(1))),
            ])),
            s_prop_array_assign(e_this(), "__elephc_entries", e_var("__id"), e_array(vec![
                e_static_call("WeakReference", "create", vec![e_var("__object")]),
                e_var("value"), e_var("__sequence"),
            ])),
            s_typed_assign(t_array(), "__maps", e_static_call("WeakMap", "__elephc_array", vec![e_null_coalesce(registry(), e_array(vec![]))])),
            s_array_assign("__maps", object_id(e_this()), e_static_call("WeakReference", "create", vec![e_this()])),
            s_static_prop_array_assign("WeakMap", "__elephc_maps", e_var("__id"), e_var("__maps")),
        ]));
    map = map.method(method("offsetGet").param("object", t_mixed()).returns(t_mixed()).body(vec![
        validate_key(), key_id(),
        s_if(e_not(present(entry())), vec![s_throw(e_new_fq("Error", vec![e_binop(
            e_binop(e_binop(e_str("Object "), BinOp::Concat, e_call("get_class", vec![e_var("object")])),
                BinOp::Concat, e_binop(e_str("#"), BinOp::Concat, e_var("__id"))),
            BinOp::Concat, e_str(" not contained in WeakMap"),
        )]))], vec![], None),
        s_assign("__entry", entry()), s_return(e_index(e_var("__entry"), e_int(1))),
    ]));
    map = map.method(method("__elephc_reference").private().param("object", t_mixed())
        .returns(t_mixed()).body(vec![
            s_expr(e_method_call(e_this(), "offsetGet", vec![e_var("object")])),
            key_id(),
            entry_value_reference("__value"),
            s_return(e_var("__value")),
        ]));
    map = map.method(method("offsetExists").param("object", t_mixed()).returns(TypeExpr::Bool).body(vec![
        validate_key(), key_id(),
        s_if(e_not(present(entry())), vec![s_return(e_bool(false))], vec![], None),
        s_assign("__entry", entry()),
        s_return(e_binop(e_index(e_var("__entry"), e_int(1)), BinOp::StrictNotEq, e_null())),
    ]));
    map = map.method(method("offsetUnset").param("object", t_mixed()).returns(TypeExpr::Void).body(vec![
        validate_key(), key_id(),
        s_expr(e_method_call(e_this(), "__elephc_unregister", vec![e_var("__id")])),
        remove(entry()),
    ]));
    map = map.method(method("count").returns(TypeExpr::Int).body(vec![
        s_assign("__entries", e_this_prop("__elephc_entries")),
        s_return(e_call("count", vec![e_var("__entries")])),
    ]));
    map = map.method(method("getIterator").returns(t_class("Iterator"))
        .returning(e_new_fq("__ElephcWeakMapIterator", vec![e_this()])));
    map = map.method(method("__elephc_unregister").private().param("__id", TypeExpr::Int)
        .returns(TypeExpr::Void).body(vec![
            s_typed_assign(t_array(), "__maps", e_static_call("WeakMap", "__elephc_array", vec![e_null_coalesce(registry(), e_array(vec![]))])),
            remove(e_index(e_var("__maps"), object_id(e_this()))),
            s_if(e_binop(e_call("count", vec![e_var("__maps")]), BinOp::StrictEq, e_int(0)),
                vec![remove(registry())], vec![], Some(vec![
                    s_static_prop_array_assign("WeakMap", "__elephc_maps", e_var("__id"), e_var("__maps")),
                ])),
        ]));
    map = map.method(method("__elephc_forget").private().param("__id", TypeExpr::Int)
        .returns(TypeExpr::Void).body(vec![remove(entry())]));
    map = map.method(method("__elephc_retire").static_().private().param("__id", TypeExpr::Int)
        .returns(TypeExpr::Void).body(vec![
            s_if(e_not(present(registry())), vec![s_return_void()], vec![], None),
            s_typed_assign(t_array(), "__maps", e_static_call("WeakMap", "__elephc_array", vec![registry()])), remove(registry()),
            s_typed_assign(t_nullable(t_class("Throwable")), "__pending", e_null()),
            s_foreach(e_var("__maps"), None, "__weak", vec![
                s_assign("__map", e_method_call(e_var("__weak"), "get", vec![])),
                s_if(e_binop(e_var("__map"), BinOp::StrictNotEq, e_null()), vec![
                    s_try(vec![s_expr(e_method_call(e_var("__map"), "__elephc_forget", vec![e_var("__id")]))],
                        vec![(vec!["Throwable"], Some("__error"), vec![s_assign("__pending", e_var("__error"))])], None),
                ], vec![], None),
            ]),
            s_if(e_binop(e_var("__pending"), BinOp::StrictNotEq, e_null()),
                vec![s_throw(e_var("__pending"))], vec![], None),
        ]));
    map = map.method(method("__destruct").body(vec![
        s_assign("__entries", e_this_prop("__elephc_entries")),
        s_foreach(e_var("__entries"), Some("__id"), "__entry", vec![
            s_expr(e_method_call(e_this(), "__elephc_unregister", vec![e_var("__id")])),
        ]),
    ]));
    map = map.method(method("__clone").body(vec![
        s_assign("__entries", e_this_prop("__elephc_entries")),
        s_foreach(e_var("__entries"), Some("__id"), "__entry", vec![
            entry_value_reference("__value"),
            s_typed_assign(t_array(), "__maps", e_static_call("WeakMap", "__elephc_array", vec![e_null_coalesce(registry(), e_array(vec![]))])),
            s_array_assign("__maps", object_id(e_this()), e_static_call("WeakReference", "create", vec![e_this()])),
            s_static_prop_array_assign("WeakMap", "__elephc_maps", e_var("__id"), e_var("__maps")),
        ]),
    ]));
    map = map.method(method("__elephc_next").param("__after", TypeExpr::Int).returns(t_array()).body(vec![
        s_assign("__entries", e_this_prop("__elephc_entries")),
        s_foreach(e_var("__entries"), None, "__entry", vec![
            s_if(e_binop(e_index(e_var("__entry"), e_int(2)), BinOp::Gt, e_var("__after")), vec![
                s_assign("__weak", e_index(e_var("__entry"), e_int(0))),
                s_assign("__key", e_method_call(e_var("__weak"), "get", vec![])),
                s_if(e_binop(e_var("__key"), BinOp::StrictNotEq, e_null()), vec![
                    s_return(e_array(vec![e_var("__key"), e_index(e_var("__entry"), e_int(2))])),
                ], vec![], None),
            ], vec![], None),
        ]),
        s_return(e_array(vec![e_null(), e_var("__after")])),
    ]));
    let mut map = map.build();
    if let StmtKind::ClassDecl { methods, .. } = &mut map.kind {
        // This boundary accepts both PHP array layouts, unlike raw synthetic SPL arrays.
        if let Some(method) = methods.iter_mut().find(|method| method.name == "__elephc_array") {
            method.span.line = 1;
        }
        if let Some(method) = methods.iter_mut().find(|method| method.name == "__elephc_reference") {
            method.by_ref_return = true;
        }
    }
    vec![map, iterator()]
}

/// Separates the entry's storage and binds its value through the shared managed array cell.
fn entry_value_reference(target: &str) -> Stmt {
    let span = crate::span::Span::dummy();
    Stmt::new(StmtKind::Synthetic(vec![
        Stmt::new(StmtKind::RefAssign {
            target: "__entries_reference".into(),
            source: e_this_prop("__elephc_entries"),
        }, span),
        Stmt::new(StmtKind::RefAssign {
            target: target.into(),
            source: e_index(e_index(e_var("__entries_reference"), e_var("__id")), e_int(1)),
        }, span),
    ]), span)
}

/// Rejects scalar and null WeakMap keys before object identity lookup.
fn validate_key() -> Stmt {
    s_if(e_not(e_call("is_object", vec![e_var("object")])), vec![
        s_throw(e_new_fq("TypeError", vec![e_str("WeakMap key must be an object")])),
    ], vec![], None)
}

/// Binds the current object's stable live PHP identity.
fn key_id() -> Stmt {
    Stmt::new(StmtKind::Synthetic(vec![
        s_assign("__object", e_static_call("WeakMap", "__elephc_key", vec![e_var("object")])),
        s_assign("__id", object_id(e_var("__object"))),
    ]), crate::span::Span::dummy())
}

/// Looks up one map entry by the current key identity.
fn entry() -> Expr { e_index(e_this_prop("__elephc_entries"), e_var("__id")) }

/// Looks up weak map observers for the current key identity.
fn registry() -> Expr { e_index(e_static_prop("WeakMap", "__elephc_maps"), e_var("__id")) }

/// Queries PHP's live object identity without retaining the object.
fn object_id(object: Expr) -> Expr { e_static_call("WeakMap", "__elephc_id", vec![object]) }

/// Tests whether a map/registry entry exists without fetching a missing offset.
fn present(value: Expr) -> Expr { e_call("isset", vec![value]) }

/// Removes a real lvalue through the common language-construct lowering.
fn remove(value: Expr) -> Stmt {
    if let ExprKind::ArrayAccess { array, index } = &value.kind {
        let replacement = match &array.kind {
            ExprKind::PropertyAccess { object, property } => Some((
                "__remove_entries",
                s_prop_assign((**object).clone(), property, e_var("__remove_entries")),
            )),
            ExprKind::StaticPropertyAccess { receiver: StaticReceiver::Named(name), property } => Some((
                "__remove_registry",
                s_static_prop_assign(name.as_str(), property, e_var("__remove_registry")),
            )),
            _ => None,
        };
        if let Some((local, store)) = replacement {
            let property = matches!(&array.kind, ExprKind::PropertyAccess { .. });
            let mut statements = Vec::new();
            if property {
                // Publish the updated map before a removed value's destructor can inspect it.
                statements.push(s_typed_assign(t_array(), "__retired_entries", (**array).clone()));
            }
            statements.extend([
                s_typed_assign(t_array(), local, (**array).clone()),
                s_expr(e_call("unset", vec![e_index(e_var(local), (**index).clone())])),
                store,
            ]);
            if property { statements.push(s_expr(e_call("unset", vec![e_var("__retired_entries")]))); }
            return Stmt::new(StmtKind::Synthetic(statements), crate::span::Span::dummy());
        }
    }
    s_expr(e_call("unset", vec![value]))
}

/// Builds an iterator that pins only its current key while following live insertion order.
fn iterator() -> Stmt {
    class("__ElephcWeakMapIterator").final_().implements("Iterator")
        .private_prop("__elephc_map", t_class("WeakMap"), None)
        .private_prop("__elephc_position", TypeExpr::Int, Some(e_int(-1)))
        .private_prop("__elephc_key", t_mixed(), Some(e_null()))
        .method(method("__construct").param("map", t_class("WeakMap")).body(vec![
            s_prop_assign(e_this(), "__elephc_map", e_var("map")),
            s_expr(e_method_call(e_this(), "next", vec![])),
        ]))
        .method(method("rewind").returns(TypeExpr::Void).body(vec![
            s_prop_assign(e_this(), "__elephc_position", e_int(-1)),
            s_expr(e_method_call(e_this(), "next", vec![])),
        ]))
        .method(method("next").returns(TypeExpr::Void).body(vec![
            s_prop_assign(e_this(), "__elephc_key", e_null()),
            s_assign("__map", e_this_prop("__elephc_map")),
            s_assign("__pair", e_method_call(e_var("__map"), "__elephc_next", vec![e_this_prop("__elephc_position")])),
            s_prop_assign(e_this(), "__elephc_key", e_index(e_var("__pair"), e_int(0))),
            s_prop_assign(e_this(), "__elephc_position", e_index(e_var("__pair"), e_int(1))),
        ]))
        .method(method("valid").returns(TypeExpr::Bool)
            .returning(e_binop(e_this_prop("__elephc_key"), BinOp::StrictNotEq, e_null())))
        .method(method("key").returns(t_mixed()).returning(e_this_prop("__elephc_key")))
        .method(method("current").returns(t_mixed()).body(vec![
            s_assign("__key", e_this_prop("__elephc_key")),
            s_if(e_binop(e_var("__key"), BinOp::StrictEq, e_null()), vec![s_return(e_null())], vec![], None),
            s_assign("__map", e_this_prop("__elephc_map")),
            s_return(e_method_call(e_var("__map"), "offsetGet", vec![e_var("__key")])),
        ]))
        .build()
}
