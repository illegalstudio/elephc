//! Purpose:
//! Parser tests for print, strings, calls, includes, constants, named args, spread args, isset, and empty.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases cover function-like and call-expression parsing.

use super::support::*;

/// Verifies print fragments lower to expression-form print with the printed value.
#[test]
fn parse_fragment_accepts_print_source() {
    let program = parse_fragment(br#"print "hi";"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Expr(EvalExpr::Print(Box::new(EvalExpr::Const(
            EvalConst::String("hi".to_string())
        ))))]
    );
}
/// Verifies single- and double-quoted strings keep PHP-compatible simple escapes.
#[test]
fn parse_fragment_preserves_php_string_escape_semantics() {
    let program = parse_fragment(br#"return ['A\nB', "A\qB", "A\v\e\fB", 'It\'s'];"#)
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Array(vec![
            EvalArrayElement::Value(EvalExpr::Const(EvalConst::String("A\\nB".to_string()))),
            EvalArrayElement::Value(EvalExpr::Const(EvalConst::String("A\\qB".to_string()))),
            EvalArrayElement::Value(EvalExpr::Const(EvalConst::String(
                "A\x0b\x1b\x0cB".to_string()
            ))),
            EvalArrayElement::Value(EvalExpr::Const(EvalConst::String("It's".to_string()))),
        ])))]
    );
}
/// Verifies call expressions preserve their callee name and source-order arguments.
#[test]
fn parse_fragment_accepts_call_expression_source() {
    let program = parse_fragment(br#"return eval("return 1;");"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Call {
            name: "eval".to_string(),
            args: vec![EvalCallArg::positional(EvalExpr::Const(EvalConst::String(
                "return 1;".to_string()
            )))],
        }))]
    );
}
/// Verifies include and require constructs parse as expressions with path metadata.
#[test]
fn parse_fragment_accepts_include_require_expression_source() {
    let program = parse_fragment(br#"return include "a" . ".php"; require_once("b.php");"#)
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::Return(Some(EvalExpr::Include {
                path: Box::new(EvalExpr::Binary {
                    op: EvalBinOp::Concat,
                    left: Box::new(EvalExpr::Const(EvalConst::String("a".to_string()))),
                    right: Box::new(EvalExpr::Const(EvalConst::String(".php".to_string()))),
                }),
                required: false,
                once: false,
            })),
            EvalStmt::Expr(EvalExpr::Include {
                path: Box::new(EvalExpr::Const(EvalConst::String("b.php".to_string()))),
                required: true,
                once: true,
            }),
        ]
    );
}
/// Verifies explicitly qualified call expressions normalize away the leading slash.
#[test]
fn parse_fragment_accepts_qualified_call_expression_source() {
    let program = parse_fragment(br#"return \strlen("abcd");"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Call {
            name: "strlen".to_string(),
            args: vec![EvalCallArg::positional(EvalExpr::Const(EvalConst::String(
                "abcd".to_string()
            )))],
        }))]
    );
}
/// Verifies first-class function callable syntax lowers to runtime callable resolution metadata.
#[test]
fn parse_fragment_accepts_first_class_function_callable_source() {
    let program = parse_fragment(br#"namespace App; return strlen(...);"#)
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::FunctionCallable {
                name: "app\\strlen".to_string(),
                fallback_name: Some("strlen".to_string()),
            }))]
    );
}
/// Verifies variable callable expressions lower to dynamic calls with source-order args.
#[test]
fn parse_fragment_accepts_dynamic_call_expression_source() {
    let program =
        parse_fragment(br#"return $fn(first: "a", ...$rest);"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::DynamicCall {
            callee: Box::new(EvalExpr::LoadVar("fn".to_string())),
            args: vec![
                EvalCallArg::named("first", EvalExpr::Const(EvalConst::String("a".to_string())),),
                EvalCallArg::spread(EvalExpr::LoadVar("rest".to_string())),
            ],
        }))]
    );
}
/// Verifies dynamic calls can be applied after another postfix expression.
#[test]
fn parse_fragment_accepts_postfix_dynamic_call_source() {
    let program =
        parse_fragment(br#"return $callbacks[0]("abcd");"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::DynamicCall {
            callee: Box::new(EvalExpr::ArrayGet {
                array: Box::new(EvalExpr::LoadVar("callbacks".to_string())),
                index: Box::new(EvalExpr::Const(EvalConst::Int(0))),
            }),
            args: vec![EvalCallArg::positional(EvalExpr::Const(EvalConst::String(
                "abcd".to_string()
            )))],
        }))]
    );
}
/// Verifies bare constant names lower to dynamic constant-fetch expressions.
#[test]
fn parse_fragment_accepts_constant_fetch_source() {
    let program = parse_fragment(br#"return \Dyn\EvalConst;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::ConstFetch(
            "Dyn\\EvalConst".to_string()
        )))]
    );
}
/// Verifies function calls preserve named arguments in source order.
#[test]
fn parse_fragment_accepts_named_call_argument_source() {
    let program = parse_fragment(br#"return add(y: 2, x: 1);"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Call {
            name: "add".to_string(),
            args: vec![
                EvalCallArg::named("y", EvalExpr::Const(EvalConst::Int(2))),
                EvalCallArg::named("x", EvalExpr::Const(EvalConst::Int(1))),
            ],
        }))]
    );
}
/// Verifies function calls preserve spread arguments in source order.
#[test]
fn parse_fragment_accepts_spread_call_argument_source() {
    let program = parse_fragment(br#"return add(...$args);"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Call {
            name: "add".to_string(),
            args: vec![EvalCallArg::spread(EvalExpr::LoadVar("args".to_string()))],
        }))]
    );
}
/// Verifies `isset` parses as a case-insensitive function-like expression.
#[test]
fn parse_fragment_accepts_isset_source() {
    let program =
        parse_fragment(br#"return ISSET($x, $items["k"]);"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Call {
            name: "isset".to_string(),
            args: vec![
                EvalCallArg::positional(EvalExpr::LoadVar("x".to_string())),
                EvalCallArg::positional(EvalExpr::ArrayGet {
                    array: Box::new(EvalExpr::LoadVar("items".to_string())),
                    index: Box::new(EvalExpr::Const(EvalConst::String("k".to_string()))),
                }),
            ],
        }))]
    );
}
/// Verifies `empty` parses as a case-insensitive function-like expression.
#[test]
fn parse_fragment_accepts_empty_source() {
    let program = parse_fragment(br#"return EMPTY($items["k"]);"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Call {
            name: "empty".to_string(),
            args: vec![EvalCallArg::positional(EvalExpr::ArrayGet {
                array: Box::new(EvalExpr::LoadVar("items".to_string())),
                index: Box::new(EvalExpr::Const(EvalConst::String("k".to_string()))),
            })],
        }))]
    );
}
