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

/// Every profile refuses reference foreach targets outside the interpreter fragment subset.
#[test]
fn parse_reference_foreach_preserves_fragment_subset_boundary() {
    use elephc_semantics::{SemanticProfile, StrictFeature};
    for profile in [SemanticProfile::PERMISSIVE,
        SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true),
        SemanticProfile::PERMISSIVE.with(StrictFeature::Indexing, true)] {
        for source in [b"foreach ($items as &$item) {}".as_slice(),
            b"foreach ($items as $key => &$item) {}"] {
            let error = crate::parser::parse_fragment_with_semantics(source, profile).unwrap_err();
            assert_eq!(error, EvalParseError::UnsupportedConstruct);
            assert_eq!(error.status(), crate::errors::EvalStatus::UnsupportedConstruct);
        }
    }
}
