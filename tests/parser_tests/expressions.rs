//! Purpose:
//! Groups the expression parsing integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for basics, operators, modern PHP operators, assignments, arrays, string offsets, and match expressions.

use super::*;

#[path = "expressions/basics.rs"]
mod basics;
#[path = "expressions/operators.rs"]
mod operators;
#[path = "expressions/modern_ops/mod.rs"]
mod modern_ops;
#[path = "expressions/assignments.rs"]
mod assignments;
#[path = "expressions/arrays_match.rs"]
mod arrays_match;

/// Verifies that `$obj->$method()` desugars to `call_user_func([$obj, $method])`, reusing the
/// runtime dynamic-dispatch path.
#[test]
fn test_dynamic_method_call_desugars_to_call_user_func() {
    let stmts = parse_source("<?php $obj->$m(7);");
    let expr = match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => expr,
        other => panic!("Expected ExprStmt, got {:?}", other),
    };
    match &expr.kind {
        ExprKind::FunctionCall { name, args } => {
            assert_eq!(name.as_str(), "call_user_func");
            // First arg is the [$obj, $m] callable array, then the forwarded 7.
            assert!(matches!(&args[0].kind, ExprKind::ArrayLiteral(items) if items.len() == 2));
            assert_eq!(args.len(), 2);
            assert!(matches!(args[1].kind, ExprKind::IntLiteral(7)));
        }
        other => panic!("Expected call_user_func FunctionCall, got {:?}", other),
    }
}

/// Verifies `$cls::method()` with a literal method name desugars to the runtime class-name
/// dispatch primitive (expanded by `ir_lower::expr::dynamic_class_access`).
#[test]
fn test_dynamic_static_call_desugars_to_dynamic_static_call_primitive() {
    let stmts = parse_source("<?php $cls::build(1);");
    let expr = match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => expr,
        other => panic!("Expected ExprStmt, got {:?}", other),
    };
    match &expr.kind {
        ExprKind::FunctionCall { name, args } => {
            assert_eq!(name.as_str(), "__elephc_dynamic_static_call");
            assert!(matches!(&args[0].kind, ExprKind::Variable(name) if name == "cls"));
            assert!(matches!(&args[1].kind, ExprKind::StringLiteral(name) if name == "build"));
            assert_eq!(args.len(), 3);
        }
        other => panic!("Expected __elephc_dynamic_static_call FunctionCall, got {:?}", other),
    }
}

/// Verifies a variable method name (`$cls::$m()`) keeps the `call_user_func` desugar.
#[test]
fn test_dynamic_static_call_with_variable_method_desugars_to_call_user_func() {
    let stmts = parse_source("<?php $cls::$m();");
    let expr = match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => expr,
        other => panic!("Expected ExprStmt, got {:?}", other),
    };
    match &expr.kind {
        ExprKind::FunctionCall { name, args } => {
            assert_eq!(name.as_str(), "call_user_func");
            assert!(matches!(&args[0].kind, ExprKind::ArrayLiteral(items) if items.len() == 2));
        }
        other => panic!("Expected call_user_func FunctionCall, got {:?}", other),
    }
}

/// Returns the expression of the first statement when it is an `ExprStmt` or `Echo`.
fn first_stmt_expr(stmts: &[Stmt]) -> &Expr {
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) | StmtKind::Echo(expr) => expr,
        other => panic!("Expected ExprStmt/Echo, got {:?}", other),
    }
}

/// Verifies `self::$method($a)` is a call of the method named by `$method` (marked with the
/// scoped `self::class` receiver), while `self::$prop` stays a static property read.
#[test]
fn test_scoped_variable_method_call_desugars_to_scoped_dynamic_call() {
    let stmts = parse_source("<?php self::$method($a, $b); echo self::$method;");
    match &first_stmt_expr(&stmts).kind {
        ExprKind::FunctionCall { name, args } => {
            assert_eq!(name.as_str(), "__elephc_dynamic_static_call");
            assert!(matches!(
                &args[0].kind,
                ExprKind::ClassConstant { receiver: StaticReceiver::Self_ }
            ));
            assert!(matches!(&args[1].kind, ExprKind::Variable(name) if name == "method"));
            assert_eq!(args.len(), 4);
        }
        other => panic!("Expected scoped dynamic call, got {:?}", other),
    }
    assert!(matches!(
        echoed_expr(&stmts[1..]),
        ExprKind::StaticPropertyAccess { property, .. } if property == "method"
    ));
}

/// Verifies `static::{expr}()`, `parent::{expr}()` and `C::{expr}()` keep their receiver and the
/// computed name; a literal braced name is the ordinary static call.
#[test]
fn test_scoped_computed_method_name_parses() {
    for (src, expected) in [
        ("<?php static::{'a' . $b}();", "static"),
        ("<?php parent::{$m}(1);", "parent"),
        ("<?php Foo::{$m}();", "Foo"),
    ] {
        let stmts = parse_source(src);
        match &first_stmt_expr(&stmts).kind {
            ExprKind::FunctionCall { name, args } => {
                assert_eq!(name.as_str(), "__elephc_dynamic_static_call", "{src}");
                let receiver = match &args[0].kind {
                    ExprKind::ClassConstant { receiver } => receiver,
                    other => panic!("Expected ClassConstant receiver, got {:?}", other),
                };
                let spelled = match receiver {
                    StaticReceiver::Static => "static".to_string(),
                    StaticReceiver::Parent => "parent".to_string(),
                    StaticReceiver::Named(name) => name.as_str().to_string(),
                    other => format!("{other:?}"),
                };
                assert_eq!(spelled, expected, "{src}");
            }
            other => panic!("Expected scoped dynamic call for {src}, got {:?}", other),
        }
    }
    let stmts = parse_source("<?php self::{'run'}(1);");
    assert!(matches!(
        &first_stmt_expr(&stmts).kind,
        ExprKind::StaticMethodCall { receiver: StaticReceiver::Self_, method, args }
            if method == "run" && args.len() == 1
    ));
}

/// Verifies `[$class, 'm']()` calls the array literal and `(expr)(...)`, `$this(...)`,
/// `f()(...)` and `$o->$m(...)` build Closures through `__elephc_closure_from_callable`.
#[test]
fn test_expression_callables_parse() {
    let stmts = parse_source("<?php [self::$s, 'events']();");
    assert!(matches!(
        &first_stmt_expr(&stmts).kind,
        ExprKind::ExprCall { callee, args } if matches!(callee.kind, ExprKind::ArrayLiteral(_)) && args.is_empty()
    ));
    for (src, callee) in [
        ("<?php ($this->callback)(...);", "property"),
        ("<?php $this(...);", "this"),
        ("<?php f()(...);", "call"),
        ("<?php $o->$m(...);", "array"),
    ] {
        let stmts = parse_source(src);
        match &first_stmt_expr(&stmts).kind {
            ExprKind::FunctionCall { name, args } => {
                assert_eq!(name.as_str(), "__elephc_closure_from_callable", "{src}");
                let ok = match callee {
                    "property" => matches!(args[0].kind, ExprKind::PropertyAccess { .. }),
                    "this" => matches!(args[0].kind, ExprKind::This),
                    "call" => matches!(args[0].kind, ExprKind::FunctionCall { .. }),
                    _ => matches!(&args[0].kind, ExprKind::ArrayLiteral(items) if items.len() == 2),
                };
                assert!(ok, "{src}: {:?}", args[0].kind);
            }
            other => panic!("Expected closure conversion for {src}, got {:?}", other),
        }
    }
}

/// Verifies `$obj->{$m}(...$a, name: v)` binds names at run time through
/// `call_user_func_array` (an assignment whose prelude evaluates receiver, name and arguments).
#[test]
fn test_dynamic_method_call_with_named_arguments_parses() {
    let stmts = parse_source("<?php $q->{$m}(...$v, boolean: $b);");
    match &first_stmt_expr(&stmts).kind {
        ExprKind::Assignment { value, prelude, .. } => {
            assert!(matches!(
                &value.kind,
                ExprKind::FunctionCall { name, .. } if name.as_str() == "call_user_func_array"
            ));
            assert!(prelude.len() >= 4, "{:?}", prelude);
        }
        other => panic!("Expected named-argument dynamic call, got {:?}", other),
    }
}

/// Verifies `$a['p'][]['to'][]['email'] = $v;` appends one nested literal to `$a['p']`, after a
/// guard that refuses an object place (PHP would go through `offsetGet`, not `offsetSet`).
#[test]
fn test_mid_chain_append_parses_to_nested_literal_push() {
    let stmts = parse_source("<?php $s[][] = 1;");
    let StmtKind::Synthetic(group) = &stmts[0].kind else {
        panic!("Expected guarded group, got {:?}", stmts[0].kind);
    };
    assert!(matches!(
        &group[0].kind,
        StmtKind::Assign { value, .. } if matches!(&value.kind, ExprKind::Ternary { then_expr, .. } if matches!(then_expr.kind, ExprKind::Throw(_)))
    ));
    match &group[1].kind {
        StmtKind::ArrayPush { array, value } => {
            assert_eq!(array, "s");
            assert!(matches!(&value.kind, ExprKind::ArrayLiteral(items) if items.len() == 1));
        }
        other => panic!("Expected ArrayPush, got {:?}", other),
    }
    assert!(!parse_fails("<?php $body['p'][]['to'][]['email'] = $r;"));
    assert!(parse_fails("<?php $a[]->p = 1;"));
}

/// Verifies `foreach ($a as $o->k => $o->v)` binds hidden temporaries and starts the body by
/// storing the value, then the key.
#[test]
fn test_foreach_place_targets_parse() {
    let stmts = parse_source("<?php foreach ($a as $o->k => $o->v) { echo 1; }");
    match &stmts[0].kind {
        StmtKind::Foreach { key_var, value_var, body, .. } => {
            assert!(key_var.as_deref().is_some_and(|key| key.contains("__elephc_foreach_key")));
            assert!(value_var.contains("__elephc_foreach_value"));
            match &body[0].kind {
                StmtKind::Synthetic(stores) => {
                    assert!(matches!(&stores[0].kind, StmtKind::PropertyAssign { property, .. } if property == "v"));
                    assert!(matches!(&stores[1].kind, StmtKind::PropertyAssign { property, .. } if property == "k"));
                }
                other => panic!("Expected target stores, got {:?}", other),
            }
        }
        other => panic!("Expected Foreach, got {:?}", other),
    }
}

/// Verifies a semi-reserved keyword method name after a dynamic receiver (`$factory::new()`)
/// takes the literal-name dispatch.
#[test]
fn test_dynamic_receiver_keyword_method_name() {
    let stmts = parse_source("<?php $factory::new(1);");
    match &first_stmt_expr(&stmts).kind {
        ExprKind::FunctionCall { name, args } => {
            assert_eq!(name.as_str(), "__elephc_dynamic_static_call");
            assert!(matches!(&args[1].kind, ExprKind::StringLiteral(name) if name == "new"));
        }
        other => panic!("Expected dynamic static call, got {:?}", other),
    }
}

/// Verifies `$obj->{$name}[] = $v;` becomes the runtime-named push primitive, whose lowering
/// dispatches on the name over the receiver's slots.
#[test]
fn test_dynamic_property_append_parses_to_push_primitive() {
    let stmts = parse_source("<?php $this->{$p}[] = $v;");
    match &first_stmt_expr(&stmts).kind {
        ExprKind::FunctionCall { name, args } => {
            assert_eq!(name.as_str(), "__elephc_dynamic_property_push");
            assert!(matches!(args[0].kind, ExprKind::This));
            assert!(matches!(&args[1].kind, ExprKind::Variable(name) if name == "p"));
            assert!(matches!(&args[2].kind, ExprKind::Variable(name) if name == "v"));
        }
        other => panic!("Expected the push primitive, got {:?}", other),
    }
}
