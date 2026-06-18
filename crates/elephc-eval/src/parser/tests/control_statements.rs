//! Purpose:
//! Parser tests for branch, loop, switch, foreach, and function declaration statements.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - These cases verify statement body shapes and ordered EvalIR blocks.

use super::support::*;

/// Verifies if/else fragments lower to branch statements with nested blocks.
#[test]
fn parse_fragment_accepts_if_else_source() {
    let program = parse_fragment(br#"if ($flag) { $x = "yes"; } else { $x = "no"; }"#)
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::If {
            condition: EvalExpr::LoadVar("flag".to_string()),
            then_branch: vec![EvalStmt::StoreVar {
                name: "x".to_string(),
                value: EvalExpr::Const(EvalConst::String("yes".to_string())),
            }],
            else_branch: vec![EvalStmt::StoreVar {
                name: "x".to_string(),
                value: EvalExpr::Const(EvalConst::String("no".to_string())),
            }],
        }]
    );
}
/// Verifies braceless if/else bodies parse as single-statement branch bodies.
#[test]
fn parse_fragment_accepts_braceless_if_else_source() {
    let program = parse_fragment(br#"if ($flag) echo "yes"; else echo "no";"#)
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::If {
            condition: EvalExpr::LoadVar("flag".to_string()),
            then_branch: vec![EvalStmt::Echo(EvalExpr::Const(EvalConst::String(
                "yes".to_string()
            )))],
            else_branch: vec![EvalStmt::Echo(EvalExpr::Const(EvalConst::String(
                "no".to_string()
            )))],
        }]
    );
}
/// Verifies elseif fragments lower to nested if statements in the else branch.
#[test]
fn parse_fragment_accepts_elseif_source() {
    let program = parse_fragment(br#"if ($a) { $x = "a"; } elseif ($b) { $x = "b"; }"#)
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::If {
            condition: EvalExpr::LoadVar("a".to_string()),
            then_branch: vec![EvalStmt::StoreVar {
                name: "x".to_string(),
                value: EvalExpr::Const(EvalConst::String("a".to_string())),
            }],
            else_branch: vec![EvalStmt::If {
                condition: EvalExpr::LoadVar("b".to_string()),
                then_branch: vec![EvalStmt::StoreVar {
                    name: "x".to_string(),
                    value: EvalExpr::Const(EvalConst::String("b".to_string())),
                }],
                else_branch: Vec::new(),
            }],
        }]
    );
}
/// Verifies PHP's `else if` spelling follows the same nested branch shape.
#[test]
fn parse_fragment_accepts_else_if_source() {
    let program = parse_fragment(br#"if ($a) { $x = "a"; } else if ($b) { $x = "b"; }"#)
        .expect("fragment should parse");

    assert!(matches!(
        program.statements(),
        [EvalStmt::If {
            else_branch,
            ..
        }] if matches!(else_branch.as_slice(), [EvalStmt::If { .. }])
    ));
}
/// Verifies for loops lower clauses and body statements separately.
#[test]
fn parse_fragment_accepts_for_source() {
    let program = parse_fragment(br#"for ($i = 2; $i; $i = $i - 1) { echo $i; }"#)
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::For {
            init: vec![EvalStmt::StoreVar {
                name: "i".to_string(),
                value: EvalExpr::Const(EvalConst::Int(2)),
            }],
            condition: Some(EvalExpr::LoadVar("i".to_string())),
            update: vec![EvalStmt::StoreVar {
                name: "i".to_string(),
                value: EvalExpr::Binary {
                    op: EvalBinOp::Sub,
                    left: Box::new(EvalExpr::LoadVar("i".to_string())),
                    right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
                },
            }],
            body: vec![EvalStmt::Echo(EvalExpr::LoadVar("i".to_string()))],
        }]
    );
}
/// Verifies switch fragments preserve ordered case and default bodies.
#[test]
fn parse_fragment_accepts_switch_source() {
    let program =
        parse_fragment(br#"switch ($x) { case 1: echo "one"; break; default: echo "other"; }"#)
            .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Switch {
            expr: EvalExpr::LoadVar("x".to_string()),
            cases: vec![
                EvalSwitchCase {
                    condition: Some(EvalExpr::Const(EvalConst::Int(1))),
                    body: vec![
                        EvalStmt::Echo(EvalExpr::Const(EvalConst::String("one".to_string()))),
                        EvalStmt::Break,
                    ],
                },
                EvalSwitchCase {
                    condition: None,
                    body: vec![EvalStmt::Echo(EvalExpr::Const(EvalConst::String(
                        "other".to_string()
                    )))],
                },
            ],
        }]
    );
}
/// Verifies value-only foreach loops lower to an array expression, value target, and body.
#[test]
fn parse_fragment_accepts_foreach_source() {
    let program = parse_fragment(br#"foreach ($items as $item) { echo $item; }"#).expect("parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Foreach {
            array: EvalExpr::LoadVar("items".to_string()),
            key_name: None,
            value_name: "item".to_string(),
            body: vec![EvalStmt::Echo(EvalExpr::LoadVar("item".to_string()))],
        }]
    );
}
/// Verifies key-value foreach loops preserve both loop target names in EvalIR.
#[test]
fn parse_fragment_accepts_foreach_key_value_source() {
    let program = parse_fragment(br#"foreach ($items as $key => $item) { echo $key . $item; }"#)
        .expect("parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Foreach {
            array: EvalExpr::LoadVar("items".to_string()),
            key_name: Some("key".to_string()),
            value_name: "item".to_string(),
            body: vec![EvalStmt::Echo(EvalExpr::Binary {
                op: EvalBinOp::Concat,
                left: Box::new(EvalExpr::LoadVar("key".to_string())),
                right: Box::new(EvalExpr::LoadVar("item".to_string())),
            })],
        }]
    );
}
