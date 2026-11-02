//! Purpose:
//! Parser tests for branch, loop, switch, foreach, and function declaration statements.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
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
                        EvalStmt::Break(1),
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
            value_by_ref: false,
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
            value_by_ref: false,
            body: vec![EvalStmt::Echo(EvalExpr::Binary {
                op: EvalBinOp::Concat,
                left: Box::new(EvalExpr::LoadVar("key".to_string())),
                right: Box::new(EvalExpr::LoadVar("item".to_string())),
            })],
        }]
    );
}

/// Verifies foreach short-array targets lower through the existing destructuring statement.
#[test]
fn parse_fragment_accepts_foreach_array_destructure_target() {
    let program = parse_fragment(br#"foreach ($items as [$id, $class]) { echo $id . $class; }"#)
        .expect("parse");
    // The hidden binding name is generated per pattern, so the test reads it back rather than
    // spelling it: what matters is that the loop binds to it and the destructure reads it.
    let [EvalStmt::Foreach {
        array,
        key_name,
        value_name,
        value_by_ref,
        body,
    }] = program.statements()
    else {
        panic!("expected one foreach statement");
    };
    assert_eq!(array, &EvalExpr::LoadVar("items".to_string()));
    assert_eq!(key_name, &None);
    assert!(value_name.starts_with('\0'));
    assert!(!value_by_ref);
    assert_eq!(
        body,
        &vec![
            EvalStmt::ArrayDestructure {
                targets: vec![
                    Some(EvalDestructureTarget {
                        key: None,
                        slot: EvalDestructureSlot::Lvalue(EvalExpr::LoadVar("id".to_string())),
                    }),
                    Some(EvalDestructureTarget {
                        key: None,
                        slot: EvalDestructureSlot::Lvalue(EvalExpr::LoadVar("class".to_string())),
                    }),
                ],
                value: EvalExpr::LoadVar(value_name.clone()),
            },
            EvalStmt::Echo(EvalExpr::Binary {
                op: EvalBinOp::Concat,
                left: Box::new(EvalExpr::LoadVar("id".to_string())),
                right: Box::new(EvalExpr::LoadVar("class".to_string())),
            }),
        ]
    );
}

/// Verifies foreach key-value loops preserve an explicit by-reference value target.
#[test]
fn parse_fragment_accepts_foreach_by_reference_value_target() {
    let program = parse_fragment(br#"foreach ($items as $key => &$item) { $item = $key; }"#)
        .expect("parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Foreach {
            array: EvalExpr::LoadVar("items".to_string()),
            key_name: Some("key".to_string()),
            value_name: "item".to_string(),
            value_by_ref: true,
            body: vec![EvalStmt::StoreVar {
                name: "item".to_string(),
                value: EvalExpr::LoadVar("key".to_string()),
            }],
        }]
    );
}

/// Verifies goto statements and labels lower to explicit EvalIR control nodes.
#[test]
fn parse_fragment_accepts_goto_and_label_source() {
    let program = parse_fragment(br#"goto done; echo "skip"; done: echo "ok";"#).expect("parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::Goto("done".to_string()),
            EvalStmt::Echo(EvalExpr::Const(EvalConst::String("skip".to_string()))),
            EvalStmt::Label("done".to_string()),
            EvalStmt::Echo(EvalExpr::Const(EvalConst::String("ok".to_string()))),
        ]
    );
}

// --- Alternative control-structure syntax ---

/// Verifies an alternative-syntax `if`/`else` fragment lowers to the same branch statement as
/// the braced form, so a runtime `eval()` string behaves like the AOT-compiled one.
#[test]
fn parse_fragment_accepts_alternative_if_else_source() {
    let alternative =
        parse_fragment(br#"if ($flag): $x = "yes"; else: $x = "no"; endif;"#)
            .expect("alternative fragment should parse");
    let braced = parse_fragment(br#"if ($flag) { $x = "yes"; } else { $x = "no"; }"#)
        .expect("braced fragment should parse");
    assert_eq!(alternative.statements(), braced.statements());
}

/// Verifies `elseif:` segments nest exactly like the braced `elseif` chain.
#[test]
fn parse_fragment_accepts_alternative_elseif_chain() {
    let alternative =
        parse_fragment(br#"if ($a): $x = 1; elseif ($b): $x = 2; else: $x = 3; endif;"#)
            .expect("alternative fragment should parse");
    let braced =
        parse_fragment(br#"if ($a) { $x = 1; } elseif ($b) { $x = 2; } else { $x = 3; }"#)
            .expect("braced fragment should parse");
    assert_eq!(alternative.statements(), braced.statements());
}

/// Verifies the alternative `while` body lowers to the same loop as the braced form.
#[test]
fn parse_fragment_accepts_alternative_while_source() {
    let alternative = parse_fragment(br#"while ($i): $i = $i - 1; endwhile;"#)
        .expect("alternative fragment should parse");
    let braced = parse_fragment(br#"while ($i) { $i = $i - 1; }"#)
        .expect("braced fragment should parse");
    assert_eq!(alternative.statements(), braced.statements());
}

/// Verifies the alternative `for` body lowers to the same loop as the braced form.
#[test]
fn parse_fragment_accepts_alternative_for_source() {
    let alternative = parse_fragment(br#"for ($i = 0; $i < 3; $i++): $x = $i; endfor;"#)
        .expect("alternative fragment should parse");
    let braced = parse_fragment(br#"for ($i = 0; $i < 3; $i++) { $x = $i; }"#)
        .expect("braced fragment should parse");
    assert_eq!(alternative.statements(), braced.statements());
}

/// Verifies the alternative `foreach` body lowers to the same loop as the braced form,
/// including the `$key => $value` binding.
#[test]
fn parse_fragment_accepts_alternative_foreach_source() {
    let alternative = parse_fragment(br#"foreach ($items as $k => $v): $x = $v; endforeach;"#)
        .expect("alternative fragment should parse");
    let braced = parse_fragment(br#"foreach ($items as $k => $v) { $x = $v; }"#)
        .expect("braced fragment should parse");
    assert_eq!(alternative.statements(), braced.statements());
}

/// Verifies the alternative `switch` case list lowers to the same arms as the braced form.
#[test]
fn parse_fragment_accepts_alternative_switch_source() {
    let alternative = parse_fragment(
        br#"switch ($x): case 1: $y = "one"; break; default: $y = "other"; endswitch;"#,
    )
    .expect("alternative fragment should parse");
    let braced = parse_fragment(
        br#"switch ($x) { case 1: $y = "one"; break; default: $y = "other"; }"#,
    )
    .expect("braced fragment should parse");
    assert_eq!(alternative.statements(), braced.statements());
}

/// Verifies alternative bodies may be empty and that the two forms nest in either direction.
#[test]
fn parse_fragment_accepts_empty_and_nested_alternative_bodies() {
    parse_fragment(br#"if ($a): endif;"#).expect("empty alternative if should parse");
    parse_fragment(br#"while ($a): endwhile;"#).expect("empty alternative while should parse");
    parse_fragment(br#"foreach ($a as $v): if ($v) { $x = 1; } endforeach;"#)
        .expect("braced body nested in alternative loop should parse");
    parse_fragment(br#"foreach ($a as $v) { if ($v): $x = 1; endif; }"#)
        .expect("alternative body nested in braced loop should parse");
}

/// Verifies an unterminated alternative body is rejected rather than silently consuming
/// the rest of the fragment.
#[test]
fn parse_fragment_rejects_unterminated_alternative_body() {
    assert!(parse_fragment(br#"if ($a): $x = 1;"#).is_err());
    assert!(parse_fragment(br#"while ($a): $x = 1;"#).is_err());
    assert!(parse_fragment(br#"switch ($a): case 1: $x = 1;"#).is_err());
}

/// Verifies each `for` clause is the comma-separated expression list PHP's grammar defines.
///
/// `for ($i = 0, $count = \count($trace); $i < $count; ++$i)` appears ten times across the Symfony
/// tree; only the first element of the init and update lists was parsed before, so the loop refused
/// the file at the comma.
#[test]
fn parse_fragment_accepts_comma_separated_for_clauses() {
    let program = parse_fragment(br#"for ($i = 0, $n = 3; $i < $n; ++$i, --$n) { $x = $i; }"#)
        .expect("fragment should parse");
    let [EvalStmt::For { init, update, .. }] = program.statements() else {
        panic!("expected one for statement, got {:?}", program.statements());
    };
    assert_eq!(init.len(), 2);
    assert_eq!(update.len(), 2);
}

/// Verifies `declare(strict_types=1)` parses and reaches the interpreter as a statement.
///
/// Four PSR packages open with the directive and every one of them was once refused at the `=`.
/// It is no longer discarded: strict mode is ENFORCED, so the directive has to arrive as
/// `EvalStmt::DeclareStrictTypes` for the coercion gate to read.
///
/// STILL OPEN, and named here rather than asserted away: php accepts `declare(ticks=1)` and the
/// block and alternative-syntax bodies -- `declare(...) { ... }` and `declare(...): ... enddeclare;`
/// -- and this parser now refuses all three, because the gate that enforces `strict_types` also
/// decided an unmodelled directive is better refused than silently ignored. Both halves came from
/// the same lane and it owns the reconciliation.
#[test]
fn parse_fragment_accepts_declare_directives() {
    let program =
        parse_fragment(br#"declare(strict_types=1); $x = 1;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::DeclareStrictTypes(true),
            EvalStmt::StoreVar {
                name: "x".to_string(),
                value: EvalExpr::Const(EvalConst::Int(1)),
            }
        ]
    );
    assert!(parse_fragment(br#"declare(1); $x = 1;"#).is_err());
}
