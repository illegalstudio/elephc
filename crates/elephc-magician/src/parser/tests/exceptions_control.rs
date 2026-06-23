//! Purpose:
//! Parser tests for while, do-while, break, continue, return, throw, try/catch/finally, and unset.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases cover control-transfer and exception statement parsing.

use super::support::*;

/// Verifies while fragments lower to loop statements with a nested block.
#[test]
fn parse_fragment_accepts_while_source() {
    let program = parse_fragment(br#"while ($flag) { echo $flag; $flag = false; }"#)
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::While {
            condition: EvalExpr::LoadVar("flag".to_string()),
            body: vec![
                EvalStmt::Echo(EvalExpr::LoadVar("flag".to_string())),
                EvalStmt::StoreVar {
                    name: "flag".to_string(),
                    value: EvalExpr::Const(EvalConst::Bool(false)),
                },
            ],
        }]
    );
}
/// Verifies do/while fragments lower to body-first loop statements.
#[test]
fn parse_fragment_accepts_do_while_source() {
    let program = parse_fragment(br#"do { echo $flag; $flag = false; } while ($flag);"#)
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::DoWhile {
            body: vec![
                EvalStmt::Echo(EvalExpr::LoadVar("flag".to_string())),
                EvalStmt::StoreVar {
                    name: "flag".to_string(),
                    value: EvalExpr::Const(EvalConst::Bool(false)),
                },
            ],
            condition: EvalExpr::LoadVar("flag".to_string()),
        }]
    );
}
/// Verifies loop control statements parse inside while blocks.
#[test]
fn parse_fragment_accepts_break_and_continue_source() {
    let program =
        parse_fragment(br#"while ($flag) { continue; break; }"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::While {
            condition: EvalExpr::LoadVar("flag".to_string()),
            body: vec![EvalStmt::Continue, EvalStmt::Break],
        }]
    );
}
/// Verifies return fragments parse optional return expressions.
#[test]
fn parse_fragment_accepts_return_source() {
    let program = parse_fragment(b"return ($x - 1) * 4;").expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Binary {
            op: EvalBinOp::Mul,
            left: Box::new(EvalExpr::Binary {
                op: EvalBinOp::Sub,
                left: Box::new(EvalExpr::LoadVar("x".to_string())),
                right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
            }),
            right: Box::new(EvalExpr::Const(EvalConst::Int(4))),
        }))]
    );
}
/// Verifies throw statements lower to a Throwable expression carried by EvalIR.
#[test]
fn parse_fragment_accepts_throw_source() {
    let program =
        parse_fragment(br#"throw new Exception("eval boom");"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Throw(EvalExpr::NewObject {
            class_name: "Exception".to_string(),
            args: vec![EvalCallArg::positional(EvalExpr::Const(EvalConst::String(
                "eval boom".to_string()
            )))],
        })]
    );
}
/// Verifies try/catch statements lower supported Throwable clauses into EvalIR.
#[test]
fn parse_fragment_accepts_try_catch_throwable_source() {
    let program = parse_fragment(
        br#"try {
    throw new Exception("eval boom");
} catch (Throwable $caught) {
    return 1;
}"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Try {
            body: vec![EvalStmt::Throw(EvalExpr::NewObject {
                class_name: "Exception".to_string(),
                args: vec![EvalCallArg::positional(EvalExpr::Const(EvalConst::String(
                    "eval boom".to_string()
                )))],
            })],
            catches: vec![EvalCatch {
                class_names: vec!["Throwable".to_string()],
                var_name: Some("caught".to_string()),
                body: vec![EvalStmt::Return(Some(EvalExpr::Const(EvalConst::Int(1))))],
            }],
            finally_body: Vec::new(),
        }]
    );
}
/// Verifies class imports can alias the supported Throwable catch type.
#[test]
fn parse_fragment_accepts_try_catch_imported_throwable_alias() {
    let program = parse_fragment(
        br#"use Throwable as T;
try {
    throw $e;
} catch (T $caught) {
    echo "caught";
}"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Try {
            body: vec![EvalStmt::Throw(EvalExpr::LoadVar("e".to_string()))],
            catches: vec![EvalCatch {
                class_names: vec!["Throwable".to_string()],
                var_name: Some("caught".to_string()),
                body: vec![EvalStmt::Echo(EvalExpr::Const(EvalConst::String(
                    "caught".to_string()
                )))],
            }],
            finally_body: Vec::new(),
        }]
    );
}
/// Verifies Throwable catch clauses can omit the catch variable like PHP.
#[test]
fn parse_fragment_accepts_try_catch_without_variable() {
    let program = parse_fragment(
        br#"try {
    throw new Exception("eval boom");
} catch (Throwable) {
    return 1;
}"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Try {
            body: vec![EvalStmt::Throw(EvalExpr::NewObject {
                class_name: "Exception".to_string(),
                args: vec![EvalCallArg::positional(EvalExpr::Const(EvalConst::String(
                    "eval boom".to_string()
                )))],
            })],
            catches: vec![EvalCatch {
                class_names: vec!["Throwable".to_string()],
                var_name: None,
                body: vec![EvalStmt::Return(Some(EvalExpr::Const(EvalConst::Int(1))))],
            }],
            finally_body: Vec::new(),
        }]
    );
}
/// Verifies single catch type narrowing lowers into EvalIR.
#[test]
fn parse_fragment_accepts_specific_eval_catch_type() {
    let program = parse_fragment(
        br#"try {
    throw new Exception("eval boom");
} catch (Exception $caught) {
    return 1;
}"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Try {
            body: vec![EvalStmt::Throw(EvalExpr::NewObject {
                class_name: "Exception".to_string(),
                args: vec![EvalCallArg::positional(EvalExpr::Const(EvalConst::String(
                    "eval boom".to_string()
                )))],
            })],
            catches: vec![EvalCatch {
                class_names: vec!["Exception".to_string()],
                var_name: Some("caught".to_string()),
                body: vec![EvalStmt::Return(Some(EvalExpr::Const(EvalConst::Int(1))))],
            }],
            finally_body: Vec::new(),
        }]
    );
}
/// Verifies union catch type narrowing lowers all source-order types into one clause.
#[test]
fn parse_fragment_accepts_union_eval_catch_type() {
    let program = parse_fragment(
        br#"try {
    throw new Exception("eval boom");
} catch (Throwable|Exception $caught) {
    return 1;
}"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Try {
            body: vec![EvalStmt::Throw(EvalExpr::NewObject {
                class_name: "Exception".to_string(),
                args: vec![EvalCallArg::positional(EvalExpr::Const(EvalConst::String(
                    "eval boom".to_string()
                )))],
            })],
            catches: vec![EvalCatch {
                class_names: vec!["Throwable".to_string(), "Exception".to_string()],
                var_name: Some("caught".to_string()),
                body: vec![EvalStmt::Return(Some(EvalExpr::Const(EvalConst::Int(1))))],
            }],
            finally_body: Vec::new(),
        }]
    );
}
/// Verifies try/finally statements lower the finalizer block into EvalIR.
#[test]
fn parse_fragment_accepts_eval_finally_source() {
    let program = parse_fragment(br#"try { return 1; } finally { echo "finally"; }"#)
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Try {
            body: vec![EvalStmt::Return(Some(EvalExpr::Const(EvalConst::Int(1))))],
            catches: Vec::new(),
            finally_body: vec![EvalStmt::Echo(EvalExpr::Const(EvalConst::String(
                "finally".to_string()
            )))],
        }]
    );
}
/// Verifies unset fragments expand variable, array-access, and object-property operands.
#[test]
fn parse_fragment_accepts_unset_source() {
    let program = parse_fragment(br#"unset($x, $this->name, $box["k"], $y);"#)
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::UnsetVar {
                name: "x".to_string()
            },
            EvalStmt::UnsetProperty {
                object: EvalExpr::LoadVar("this".to_string()),
                property: "name".to_string(),
            },
            EvalStmt::UnsetArrayElement {
                array: EvalExpr::LoadVar("box".to_string()),
                index: EvalExpr::Const(EvalConst::String("k".to_string())),
            },
            EvalStmt::UnsetVar {
                name: "y".to_string()
            },
        ]
    );
}
/// Verifies eval fragments reject PHP opening tags.
#[test]
fn parse_fragment_rejects_opening_tag() {
    assert_eq!(
        parse_fragment(b"<?php echo 1;"),
        Err(EvalParseError::PhpOpenTag)
    );
}
