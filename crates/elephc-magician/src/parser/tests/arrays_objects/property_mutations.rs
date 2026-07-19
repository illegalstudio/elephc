//! Purpose:
//! Parser tests for object property assignment, references, array writes,
//! compound operations, increment/decrement, and unset.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Named and dynamic property mutations retain distinct EvalIR statements.

use super::super::support::*;

/// Verifies object property writes parse as dedicated EvalIR statements.
#[test]
fn parse_fragment_accepts_property_write_source() {
    let program = parse_fragment(br#"$this->x = $this->x + 1;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::PropertySet {
            object: EvalExpr::LoadVar("this".to_string()),
            property: "x".to_string(),
            value: EvalExpr::Binary {
                op: EvalBinOp::Add,
                left: Box::new(EvalExpr::PropertyGet {
                    object: Box::new(EvalExpr::LoadVar("this".to_string())),
                    property: "x".to_string(),
                }),
                right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
            },
        }]
    );
}

/// Verifies object property reference bindings parse as dedicated EvalIR statements.
#[test]
fn parse_fragment_accepts_property_reference_bind_source() {
    let program =
        parse_fragment(br#"$this->x =& $source; $this->{$name} =& $other;"#)
            .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::PropertyReferenceBind {
                object: EvalExpr::LoadVar("this".to_string()),
                property: "x".to_string(),
                source: "source".to_string(),
            },
            EvalStmt::DynamicPropertyReferenceBind {
                object: EvalExpr::LoadVar("this".to_string()),
                property: EvalExpr::LoadVar("name".to_string()),
                source: "other".to_string(),
            },
        ]
    );
}

/// Verifies object property array writes and appends parse as dedicated EvalIR statements.
#[test]
fn parse_fragment_accepts_property_array_write_source() {
    let program = parse_fragment(br#"$this->items[0] = "x"; $this->items[] = "y";"#)
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::PropertyArraySet {
                object: EvalExpr::LoadVar("this".to_string()),
                property: "items".to_string(),
                index: EvalExpr::Const(EvalConst::Int(0)),
                op: None,
                value: EvalExpr::Const(EvalConst::String("x".to_string())),
            },
            EvalStmt::PropertyArrayAppend {
                object: EvalExpr::LoadVar("this".to_string()),
                property: "items".to_string(),
                value: EvalExpr::Const(EvalConst::String("y".to_string())),
            },
        ]
    );
}

/// Verifies property array compound assignment retains the indexed property target.
#[test]
fn parse_fragment_accepts_property_array_compound_assignment_source() {
    let program = parse_fragment(br#"$this->items[0] += 2;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::PropertyArraySet {
            object: EvalExpr::LoadVar("this".to_string()),
            property: "items".to_string(),
            index: EvalExpr::Const(EvalConst::Int(0)),
            op: Some(EvalBinOp::Add),
            value: EvalExpr::Const(EvalConst::Int(2)),
        }]
    );
}

/// Verifies object property increment/decrement parses as dedicated member updates.
#[test]
fn parse_fragment_accepts_property_inc_dec_source() {
    let program = parse_fragment(br#"$this->x++; --$this->x;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::PropertyIncDec {
                object: EvalExpr::LoadVar("this".to_string()),
                property: "x".to_string(),
                increment: true,
            },
            EvalStmt::PropertyIncDec {
                object: EvalExpr::LoadVar("this".to_string()),
                property: "x".to_string(),
                increment: false,
            },
        ]
    );
}

/// Verifies dynamic object property writes parse as runtime-name EvalIR statements.
#[test]
fn parse_fragment_accepts_dynamic_property_write_source() {
    let program = parse_fragment(br#"$this->{$name} = 7;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::DynamicPropertySet {
            object: EvalExpr::LoadVar("this".to_string()),
            property: EvalExpr::LoadVar("name".to_string()),
            value: EvalExpr::Const(EvalConst::Int(7)),
        }]
    );
}

/// Verifies dynamic property array writes keep the runtime property expression.
#[test]
fn parse_fragment_accepts_dynamic_property_array_write_source() {
    let program = parse_fragment(br#"$this->{$name}[0] = "x"; $this->{$name}[] = "y";"#)
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::DynamicPropertyArraySet {
                object: EvalExpr::LoadVar("this".to_string()),
                property: EvalExpr::LoadVar("name".to_string()),
                index: EvalExpr::Const(EvalConst::Int(0)),
                op: None,
                value: EvalExpr::Const(EvalConst::String("x".to_string())),
            },
            EvalStmt::DynamicPropertyArrayAppend {
                object: EvalExpr::LoadVar("this".to_string()),
                property: EvalExpr::LoadVar("name".to_string()),
                value: EvalExpr::Const(EvalConst::String("y".to_string())),
            },
        ]
    );
}

/// Verifies object property compound assignment parses as a dedicated member update.
#[test]
fn parse_fragment_accepts_property_compound_assignment_source() {
    let program = parse_fragment(br#"$this->x += 2; $this->label .= "ok";"#)
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::PropertyCompoundAssign {
                object: EvalExpr::LoadVar("this".to_string()),
                property: "x".to_string(),
                op: EvalBinOp::Add,
                value: EvalExpr::Const(EvalConst::Int(2)),
            },
            EvalStmt::PropertyCompoundAssign {
                object: EvalExpr::LoadVar("this".to_string()),
                property: "label".to_string(),
                op: EvalBinOp::Concat,
                value: EvalExpr::Const(EvalConst::String("ok".to_string())),
            },
        ]
    );
}

/// Verifies dynamic object property compound assignment keeps the runtime property expression.
#[test]
fn parse_fragment_accepts_dynamic_property_compound_assignment_source() {
    let program =
        parse_fragment(br#"$this->{$name} += 2; $this->{$label} .= "ok";"#)
            .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::DynamicPropertyCompoundAssign {
                object: EvalExpr::LoadVar("this".to_string()),
                property: EvalExpr::LoadVar("name".to_string()),
                op: EvalBinOp::Add,
                value: EvalExpr::Const(EvalConst::Int(2)),
            },
            EvalStmt::DynamicPropertyCompoundAssign {
                object: EvalExpr::LoadVar("this".to_string()),
                property: EvalExpr::LoadVar("label".to_string()),
                op: EvalBinOp::Concat,
                value: EvalExpr::Const(EvalConst::String("ok".to_string())),
            },
        ]
    );
}

/// Verifies dynamic object property increment/decrement keeps the runtime property expression.
#[test]
fn parse_fragment_accepts_dynamic_property_inc_dec_source() {
    let program =
        parse_fragment(br#"$this->{$name}++; --$this->{$name};"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::DynamicPropertyIncDec {
                object: EvalExpr::LoadVar("this".to_string()),
                property: EvalExpr::LoadVar("name".to_string()),
                increment: true,
            },
            EvalStmt::DynamicPropertyIncDec {
                object: EvalExpr::LoadVar("this".to_string()),
                property: EvalExpr::LoadVar("name".to_string()),
                increment: false,
            },
        ]
    );
}

/// Verifies dynamic object property unsets parse as runtime-name EvalIR statements.
#[test]
fn parse_fragment_accepts_dynamic_property_unset_source() {
    let program = parse_fragment(br#"unset($this->{$name});"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::UnsetDynamicProperty {
            object: EvalExpr::LoadVar("this".to_string()),
            property: EvalExpr::LoadVar("name".to_string()),
        }]
    );
}

/// Verifies unsetting object-property array elements keeps the property expression target.
#[test]
fn parse_fragment_accepts_property_array_unset_source() {
    let program = parse_fragment(br#"unset($this->items[0], $this->{$name}[1]);"#)
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::UnsetArrayElement {
                array: EvalExpr::PropertyGet {
                    object: Box::new(EvalExpr::LoadVar("this".to_string())),
                    property: "items".to_string(),
                },
                index: EvalExpr::Const(EvalConst::Int(0)),
            },
            EvalStmt::UnsetArrayElement {
                array: EvalExpr::DynamicPropertyGet {
                    object: Box::new(EvalExpr::LoadVar("this".to_string())),
                    property: Box::new(EvalExpr::LoadVar("name".to_string())),
                },
                index: EvalExpr::Const(EvalConst::Int(1)),
            },
        ]
    );
}
