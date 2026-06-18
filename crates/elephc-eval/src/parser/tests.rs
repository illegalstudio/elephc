//! Purpose:
//! Parser unit tests for runtime PHP eval fragments.
//! The cases assert that supported syntax lowers into EvalIR and unsupported
//! syntax reports stable parse statuses.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - Fixtures intentionally use eval fragments without PHP opening tags.
//! - Expected values compare against EvalIR so grammar regressions are visible.

use super::parse_fragment;
use super::state::inc_dec_store;
use crate::errors::EvalParseError;
use crate::eval_ir::*;

/// Verifies assignment fragments lower to by-name StoreVar statements.
#[test]
fn parse_fragment_accepts_assignment_source() {
    let program = parse_fragment(b"$x = 1;").expect("fragment should parse");
    assert_eq!(program.source_len(), 7);
    assert_eq!(
        program.statements(),
        &[EvalStmt::StoreVar {
            name: "x".to_string(),
            value: EvalExpr::Const(EvalConst::Int(1)),
        }]
    );
}

/// Verifies reference assignments lower to by-name ReferenceAssign statements.
#[test]
fn parse_fragment_accepts_reference_assignment_source() {
    let program = parse_fragment(b"$left =& $right;").expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::ReferenceAssign {
            target: "left".to_string(),
            source: "right".to_string(),
        }]
    );
}

/// Verifies multiplicative operators preserve PHP precedence and associativity.
#[test]
fn parse_fragment_accepts_division_and_modulo_source() {
    let program = parse_fragment(b"return 10 / 4 % 3;").expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Binary {
            op: EvalBinOp::Mod,
            left: Box::new(EvalExpr::Binary {
                op: EvalBinOp::Div,
                left: Box::new(EvalExpr::Const(EvalConst::Int(10))),
                right: Box::new(EvalExpr::Const(EvalConst::Int(4))),
            }),
            right: Box::new(EvalExpr::Const(EvalConst::Int(3))),
        }))]
    );
}

/// Verifies exponentiation is right-associative and binds tighter than unary negation.
#[test]
fn parse_fragment_accepts_power_source() {
    let program =
        parse_fragment(b"return -2 ** 2; return 2 ** 3 ** 2;").expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::Return(Some(EvalExpr::Unary {
                op: EvalUnaryOp::Negate,
                expr: Box::new(EvalExpr::Binary {
                    op: EvalBinOp::Pow,
                    left: Box::new(EvalExpr::Const(EvalConst::Int(2))),
                    right: Box::new(EvalExpr::Const(EvalConst::Int(2))),
                }),
            })),
            EvalStmt::Return(Some(EvalExpr::Binary {
                op: EvalBinOp::Pow,
                left: Box::new(EvalExpr::Const(EvalConst::Int(2))),
                right: Box::new(EvalExpr::Binary {
                    op: EvalBinOp::Pow,
                    left: Box::new(EvalExpr::Const(EvalConst::Int(3))),
                    right: Box::new(EvalExpr::Const(EvalConst::Int(2))),
                }),
            })),
        ]
    );
}

/// Verifies bitwise operators preserve PHP precedence.
#[test]
fn parse_fragment_accepts_bitwise_source() {
    let program = parse_fragment(b"return ~0 | 2 ^ 3 & 4;").expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Binary {
            op: EvalBinOp::BitOr,
            left: Box::new(EvalExpr::Unary {
                op: EvalUnaryOp::BitNot,
                expr: Box::new(EvalExpr::Const(EvalConst::Int(0))),
            }),
            right: Box::new(EvalExpr::Binary {
                op: EvalBinOp::BitXor,
                left: Box::new(EvalExpr::Const(EvalConst::Int(2))),
                right: Box::new(EvalExpr::Binary {
                    op: EvalBinOp::BitAnd,
                    left: Box::new(EvalExpr::Const(EvalConst::Int(3))),
                    right: Box::new(EvalExpr::Const(EvalConst::Int(4))),
                }),
            }),
        }))]
    );
}

/// Verifies shift operators bind lower than additive expressions.
#[test]
fn parse_fragment_accepts_shift_source() {
    let program = parse_fragment(b"return 1 + 2 << 3;").expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Binary {
            op: EvalBinOp::ShiftLeft,
            left: Box::new(EvalExpr::Binary {
                op: EvalBinOp::Add,
                left: Box::new(EvalExpr::Const(EvalConst::Int(1))),
                right: Box::new(EvalExpr::Const(EvalConst::Int(2))),
            }),
            right: Box::new(EvalExpr::Const(EvalConst::Int(3))),
        }))]
    );
}

/// Verifies simple variable compound assignments lower to StoreVar with binary expressions.
#[test]
fn parse_fragment_accepts_compound_assignment_source() {
    let program = parse_fragment(br#"$x += 2; $x -= 1; $x *= 3; $x /= 2; $x %= 5; $s .= "ok";"#)
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::StoreVar {
                name: "x".to_string(),
                value: EvalExpr::Binary {
                    op: EvalBinOp::Add,
                    left: Box::new(EvalExpr::LoadVar("x".to_string())),
                    right: Box::new(EvalExpr::Const(EvalConst::Int(2))),
                },
            },
            EvalStmt::StoreVar {
                name: "x".to_string(),
                value: EvalExpr::Binary {
                    op: EvalBinOp::Sub,
                    left: Box::new(EvalExpr::LoadVar("x".to_string())),
                    right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
                },
            },
            EvalStmt::StoreVar {
                name: "x".to_string(),
                value: EvalExpr::Binary {
                    op: EvalBinOp::Mul,
                    left: Box::new(EvalExpr::LoadVar("x".to_string())),
                    right: Box::new(EvalExpr::Const(EvalConst::Int(3))),
                },
            },
            EvalStmt::StoreVar {
                name: "x".to_string(),
                value: EvalExpr::Binary {
                    op: EvalBinOp::Div,
                    left: Box::new(EvalExpr::LoadVar("x".to_string())),
                    right: Box::new(EvalExpr::Const(EvalConst::Int(2))),
                },
            },
            EvalStmt::StoreVar {
                name: "x".to_string(),
                value: EvalExpr::Binary {
                    op: EvalBinOp::Mod,
                    left: Box::new(EvalExpr::LoadVar("x".to_string())),
                    right: Box::new(EvalExpr::Const(EvalConst::Int(5))),
                },
            },
            EvalStmt::StoreVar {
                name: "s".to_string(),
                value: EvalExpr::Binary {
                    op: EvalBinOp::Concat,
                    left: Box::new(EvalExpr::LoadVar("s".to_string())),
                    right: Box::new(EvalExpr::Const(EvalConst::String("ok".to_string()))),
                },
            },
        ]
    );
}

/// Verifies exponentiation compound assignment lowers through the binary power operator.
#[test]
fn parse_fragment_accepts_power_compound_assignment_source() {
    let program = parse_fragment(br#"$x **= 3;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::StoreVar {
            name: "x".to_string(),
            value: EvalExpr::Binary {
                op: EvalBinOp::Pow,
                left: Box::new(EvalExpr::LoadVar("x".to_string())),
                right: Box::new(EvalExpr::Const(EvalConst::Int(3))),
            },
        }]
    );
}

/// Verifies bitwise compound assignments lower to StoreVar with binary expressions.
#[test]
fn parse_fragment_accepts_bitwise_compound_assignment_source() {
    let program = parse_fragment(br#"$x &= 3; $x |= 1; $x ^= 2; $x <<= 4; $x >>= 1;"#)
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::StoreVar {
                name: "x".to_string(),
                value: EvalExpr::Binary {
                    op: EvalBinOp::BitAnd,
                    left: Box::new(EvalExpr::LoadVar("x".to_string())),
                    right: Box::new(EvalExpr::Const(EvalConst::Int(3))),
                },
            },
            EvalStmt::StoreVar {
                name: "x".to_string(),
                value: EvalExpr::Binary {
                    op: EvalBinOp::BitOr,
                    left: Box::new(EvalExpr::LoadVar("x".to_string())),
                    right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
                },
            },
            EvalStmt::StoreVar {
                name: "x".to_string(),
                value: EvalExpr::Binary {
                    op: EvalBinOp::BitXor,
                    left: Box::new(EvalExpr::LoadVar("x".to_string())),
                    right: Box::new(EvalExpr::Const(EvalConst::Int(2))),
                },
            },
            EvalStmt::StoreVar {
                name: "x".to_string(),
                value: EvalExpr::Binary {
                    op: EvalBinOp::ShiftLeft,
                    left: Box::new(EvalExpr::LoadVar("x".to_string())),
                    right: Box::new(EvalExpr::Const(EvalConst::Int(4))),
                },
            },
            EvalStmt::StoreVar {
                name: "x".to_string(),
                value: EvalExpr::Binary {
                    op: EvalBinOp::ShiftRight,
                    left: Box::new(EvalExpr::LoadVar("x".to_string())),
                    right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
                },
            },
        ]
    );
}

/// Verifies simple variable increment and decrement statements lower to StoreVar.
#[test]
fn parse_fragment_accepts_inc_dec_statement_source() {
    let program = parse_fragment(br#"$i++; ++$j; $k--; --$m;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            inc_dec_store("i".to_string(), true),
            inc_dec_store("j".to_string(), true),
            inc_dec_store("k".to_string(), false),
            inc_dec_store("m".to_string(), false),
        ]
    );
}

/// Verifies echo fragments preserve expression source order.
#[test]
fn parse_fragment_accepts_echo_source() {
    let program = parse_fragment(br#"echo "hi" . $name;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Echo(EvalExpr::Binary {
            op: EvalBinOp::Concat,
            left: Box::new(EvalExpr::Const(EvalConst::String("hi".to_string()))),
            right: Box::new(EvalExpr::LoadVar("name".to_string())),
        })]
    );
}

/// Verifies PHP echo comma lists lower to one EvalIR echo statement per expression.
#[test]
fn parse_fragment_accepts_echo_comma_list_source() {
    let program = parse_fragment(br#"echo "a", $b, "c";"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::Echo(EvalExpr::Const(EvalConst::String("a".to_string()))),
            EvalStmt::Echo(EvalExpr::LoadVar("b".to_string())),
            EvalStmt::Echo(EvalExpr::Const(EvalConst::String("c".to_string()))),
        ]
    );
}

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

/// Verifies dynamic function declarations preserve name, parameters, and body.
#[test]
fn parse_fragment_accepts_function_declaration_source() {
    let program =
        parse_fragment(br#"function dyn($x) { return $x + 1; }"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::FunctionDecl {
            name: "dyn".to_string(),
            params: vec!["x".to_string()],
            body: vec![EvalStmt::Return(Some(EvalExpr::Binary {
                op: EvalBinOp::Add,
                left: Box::new(EvalExpr::LoadVar("x".to_string())),
                right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
            }))],
        }]
    );
}

/// Verifies semicolon namespace declarations qualify functions and unqualified calls.
#[test]
fn parse_fragment_accepts_semicolon_namespace_source() {
    let program = parse_fragment(
        br#"namespace Eval\Ns;
function dyn() { return __NAMESPACE__; }
return dyn();"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::FunctionDecl {
                name: "Eval\\Ns\\dyn".to_string(),
                params: Vec::new(),
                body: vec![EvalStmt::Return(Some(EvalExpr::Const(EvalConst::String(
                    "Eval\\Ns".to_string()
                ))))],
            },
            EvalStmt::Return(Some(EvalExpr::NamespacedCall {
                name: "eval\\ns\\dyn".to_string(),
                fallback_name: "dyn".to_string(),
                args: Vec::new(),
            })),
        ]
    );
}

/// Verifies braced namespace declarations restore the previous namespace afterward.
#[test]
fn parse_fragment_accepts_braced_namespace_source() {
    let program = parse_fragment(
        br#"namespace Eval\Block {
    class Box {}
    return new Box();
}
return Box;"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::ClassDecl(EvalClass::new("Eval\\Block\\Box", Vec::new(), Vec::new())),
            EvalStmt::Return(Some(EvalExpr::NewObject {
                class_name: "Eval\\Block\\Box".to_string(),
                args: Vec::new(),
            })),
            EvalStmt::Return(Some(EvalExpr::ConstFetch("Box".to_string()))),
        ]
    );
}

/// Verifies namespace import declarations resolve functions, constants, and class aliases.
#[test]
fn parse_fragment_accepts_namespace_use_imports() {
    let program = parse_fragment(
        br#"namespace Eval\UseNs;
use function Lib\strlen as Alias;
use const Lib\VALUE as LocalValue;
use Lib\Box as BoxAlias;
return Alias(LocalValue, new BoxAlias\Inner());"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Call {
            name: "lib\\strlen".to_string(),
            args: vec![
                EvalCallArg::positional(EvalExpr::ConstFetch("Lib\\VALUE".to_string())),
                EvalCallArg::positional(EvalExpr::NewObject {
                    class_name: "Lib\\Box\\Inner".to_string(),
                    args: Vec::new(),
                }),
            ],
        }))]
    );
}

/// Verifies grouped namespace imports resolve functions, constants, and class aliases.
#[test]
fn parse_fragment_accepts_grouped_namespace_use_imports() {
    let program = parse_fragment(
        br#"namespace Eval\UseNs;
use Lib\{Box as BoxAlias, Sub\Thing, function imported_func as Alias};
use const Lib\{VALUE as LocalValue, OTHER};
return Alias(LocalValue, OTHER, new BoxAlias\Inner(), new Thing());"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Call {
            name: "lib\\imported_func".to_string(),
            args: vec![
                EvalCallArg::positional(EvalExpr::ConstFetch("Lib\\VALUE".to_string())),
                EvalCallArg::positional(EvalExpr::ConstFetch("Lib\\OTHER".to_string())),
                EvalCallArg::positional(EvalExpr::NewObject {
                    class_name: "Lib\\Box\\Inner".to_string(),
                    args: Vec::new(),
                }),
                EvalCallArg::positional(EvalExpr::NewObject {
                    class_name: "Lib\\Sub\\Thing".to_string(),
                    args: Vec::new(),
                }),
            ],
        }))]
    );
}

/// Verifies typed grouped namespace imports reject mixed per-entry kinds.
#[test]
fn parse_fragment_rejects_mixed_kind_typed_grouped_use_imports() {
    assert_eq!(
        parse_fragment(br#"use function Lib\{target, const VALUE};"#),
        Err(EvalParseError::UnexpectedToken)
    );
}

/// Verifies namespace blocks restore imports when control returns to the outer namespace.
#[test]
fn parse_fragment_restores_use_imports_after_namespace_block() {
    let program = parse_fragment(
        br#"namespace Eval\Outer;
use function Lib\outer_func;
namespace Eval\Block {
    use function Lib\inner_func as alias;
    return alias();
}
return outer_func();"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::Return(Some(EvalExpr::Call {
                name: "lib\\inner_func".to_string(),
                args: Vec::new(),
            })),
            EvalStmt::Return(Some(EvalExpr::Call {
                name: "lib\\outer_func".to_string(),
                args: Vec::new(),
            })),
        ]
    );
}

/// Verifies imported aliases remain visible while parsing eval-declared function bodies.
#[test]
fn parse_fragment_applies_use_imports_inside_function_body() {
    let program = parse_fragment(
        br#"namespace Eval\UseNs;
use function Lib\target as alias;
function dyn() { return alias(); }"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::FunctionDecl {
            name: "Eval\\UseNs\\dyn".to_string(),
            params: Vec::new(),
            body: vec![EvalStmt::Return(Some(EvalExpr::Call {
                name: "lib\\target".to_string(),
                args: Vec::new(),
            }))],
        }]
    );
}

/// Verifies import declarations are rejected inside eval-declared function bodies.
#[test]
fn parse_fragment_rejects_use_import_inside_function_body() {
    assert_eq!(
        parse_fragment(br#"function dyn() { use function Lib\target; }"#),
        Err(EvalParseError::UnsupportedConstruct)
    );
}

/// Verifies static local declarations preserve the target name and initializer expression.
#[test]
fn parse_fragment_accepts_static_var_source() {
    let program = parse_fragment(br#"static $n = 1 + 1;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::StaticVar {
            name: "n".to_string(),
            init: EvalExpr::Binary {
                op: EvalBinOp::Add,
                left: Box::new(EvalExpr::Const(EvalConst::Int(1))),
                right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
            },
        }]
    );
}

/// Verifies global declarations preserve source-order variable names.
#[test]
fn parse_fragment_accepts_global_source() {
    let program = parse_fragment(br#"global $left, $right;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Global {
            vars: vec!["left".to_string(), "right".to_string()],
        }]
    );
}

/// Verifies eval magic constants lower to explicit EvalIR nodes with fragment line metadata.
#[test]
fn parse_fragment_accepts_magic_constants() {
    let program =
        parse_fragment(b"\nreturn __line__ . __FUNCTION__;").expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Binary {
            op: EvalBinOp::Concat,
            left: Box::new(EvalExpr::Magic(EvalMagicConst::Line(2))),
            right: Box::new(EvalExpr::Magic(EvalMagicConst::Function)),
        }))]
    );
}

/// Verifies file-dependent eval magic constants lower to EvalIR nodes.
#[test]
fn parse_fragment_accepts_file_magic_constants() {
    let program = parse_fragment(b"return __FILE__ . __dir__;").expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Binary {
            op: EvalBinOp::Concat,
            left: Box::new(EvalExpr::Magic(EvalMagicConst::File)),
            right: Box::new(EvalExpr::Magic(EvalMagicConst::Dir)),
        }))]
    );
}

/// Verifies eval scope magic constants lower with namespace resolved at parse time.
#[test]
fn parse_fragment_accepts_scope_magic_constants() {
    let program = parse_fragment(b"return __CLASS__ . __NAMESPACE__ . __TRAIT__ . __METHOD__;")
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Binary {
            op: EvalBinOp::Concat,
            left: Box::new(EvalExpr::Binary {
                op: EvalBinOp::Concat,
                left: Box::new(EvalExpr::Binary {
                    op: EvalBinOp::Concat,
                    left: Box::new(EvalExpr::Magic(EvalMagicConst::Class)),
                    right: Box::new(EvalExpr::Const(EvalConst::String(String::new()))),
                }),
                right: Box::new(EvalExpr::Magic(EvalMagicConst::Trait)),
            }),
            right: Box::new(EvalExpr::Magic(EvalMagicConst::Method)),
        }))]
    );
}

/// Verifies PHP comments are skipped while preserving fragment line numbers.
#[test]
fn parse_fragment_skips_comments_and_preserves_line_metadata() {
    let program = parse_fragment(b"// leading\n# hash\n/* block\ncomment */ return __LINE__;")
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Magic(
            EvalMagicConst::Line(4)
        )))]
    );
}

/// Verifies unterminated block comments fail before partial EvalIR is returned.
#[test]
fn parse_fragment_rejects_unterminated_block_comment() {
    assert_eq!(
        parse_fragment(b"/* open").unwrap_err(),
        EvalParseError::UnterminatedComment
    );
}

/// Verifies comparison operators parse with lower precedence than arithmetic.
#[test]
fn parse_fragment_accepts_comparison_source() {
    let program = parse_fragment(br#"return $i + 1 < 3;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Binary {
            op: EvalBinOp::Lt,
            left: Box::new(EvalExpr::Binary {
                op: EvalBinOp::Add,
                left: Box::new(EvalExpr::LoadVar("i".to_string())),
                right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
            }),
            right: Box::new(EvalExpr::Const(EvalConst::Int(3))),
        }))]
    );
}

/// Verifies the spaceship operator parses at ordered-comparison precedence.
#[test]
fn parse_fragment_accepts_spaceship_source() {
    let program = parse_fragment(br#"return $i + 1 <=> 3;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Binary {
            op: EvalBinOp::Spaceship,
            left: Box::new(EvalExpr::Binary {
                op: EvalBinOp::Add,
                left: Box::new(EvalExpr::LoadVar("i".to_string())),
                right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
            }),
            right: Box::new(EvalExpr::Const(EvalConst::Int(3))),
        }))]
    );
}

/// Verifies loose equality operators parse as binary EvalIR expressions.
#[test]
fn parse_fragment_accepts_loose_equality_source() {
    let program = parse_fragment(br#"return "a" != "b";"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Binary {
            op: EvalBinOp::LooseNotEq,
            left: Box::new(EvalExpr::Const(EvalConst::String("a".to_string()))),
            right: Box::new(EvalExpr::Const(EvalConst::String("b".to_string()))),
        }))]
    );
}

/// Verifies strict equality operators parse as distinct EvalIR comparisons.
#[test]
fn parse_fragment_accepts_strict_equality_source() {
    let program =
        parse_fragment(br#"return "10" === "10" && "10" !== 10;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Binary {
            op: EvalBinOp::LogicalAnd,
            left: Box::new(EvalExpr::Binary {
                op: EvalBinOp::StrictEq,
                left: Box::new(EvalExpr::Const(EvalConst::String("10".to_string()))),
                right: Box::new(EvalExpr::Const(EvalConst::String("10".to_string()))),
            }),
            right: Box::new(EvalExpr::Binary {
                op: EvalBinOp::StrictNotEq,
                left: Box::new(EvalExpr::Const(EvalConst::String("10".to_string()))),
                right: Box::new(EvalExpr::Const(EvalConst::Int(10))),
            }),
        }))]
    );
}

/// Verifies logical operators parse with `&&` binding tighter than `||`.
#[test]
fn parse_fragment_accepts_short_circuit_logical_source() {
    let program = parse_fragment(br#"return $a && $b || false;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Binary {
            op: EvalBinOp::LogicalOr,
            left: Box::new(EvalExpr::Binary {
                op: EvalBinOp::LogicalAnd,
                left: Box::new(EvalExpr::LoadVar("a".to_string())),
                right: Box::new(EvalExpr::LoadVar("b".to_string())),
            }),
            right: Box::new(EvalExpr::Const(EvalConst::Bool(false))),
        }))]
    );
}

/// Verifies PHP logical keywords parse case-insensitively with their own precedence.
#[test]
fn parse_fragment_accepts_keyword_logical_source() {
    let program =
        parse_fragment(br#"return false || true AnD false;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Binary {
            op: EvalBinOp::LogicalAnd,
            left: Box::new(EvalExpr::Binary {
                op: EvalBinOp::LogicalOr,
                left: Box::new(EvalExpr::Const(EvalConst::Bool(false))),
                right: Box::new(EvalExpr::Const(EvalConst::Bool(true))),
            }),
            right: Box::new(EvalExpr::Const(EvalConst::Bool(false))),
        }))]
    );
}

/// Verifies PHP `xor` binds between `or` and `and` in eval expressions.
#[test]
fn parse_fragment_accepts_keyword_xor_source() {
    let program =
        parse_fragment(br#"return true XoR false or false;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Binary {
            op: EvalBinOp::LogicalOr,
            left: Box::new(EvalExpr::Binary {
                op: EvalBinOp::LogicalXor,
                left: Box::new(EvalExpr::Const(EvalConst::Bool(true))),
                right: Box::new(EvalExpr::Const(EvalConst::Bool(false))),
            }),
            right: Box::new(EvalExpr::Const(EvalConst::Bool(false))),
        }))]
    );
}

/// Verifies ternary expressions parse below logical OR and preserve both branches.
#[test]
fn parse_fragment_accepts_ternary_source() {
    let program =
        parse_fragment(br#"return $a || $b ? "yes" : "no";"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Ternary {
            condition: Box::new(EvalExpr::Binary {
                op: EvalBinOp::LogicalOr,
                left: Box::new(EvalExpr::LoadVar("a".to_string())),
                right: Box::new(EvalExpr::LoadVar("b".to_string())),
            }),
            then_branch: Some(Box::new(EvalExpr::Const(EvalConst::String(
                "yes".to_string()
            )))),
            else_branch: Box::new(EvalExpr::Const(EvalConst::String("no".to_string()))),
        }))]
    );
}

/// Verifies PHP's short ternary form omits the explicit then branch in EvalIR.
#[test]
fn parse_fragment_accepts_short_ternary_source() {
    let program = parse_fragment(br#"return $name ?: "fallback";"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Ternary {
            condition: Box::new(EvalExpr::LoadVar("name".to_string())),
            then_branch: None,
            else_branch: Box::new(EvalExpr::Const(EvalConst::String("fallback".to_string()))),
        }))]
    );
}

/// Verifies null coalescing parses as a right-associative expression.
#[test]
fn parse_fragment_accepts_null_coalesce_source() {
    let program =
        parse_fragment(br#"return $a ?? $b ?? "fallback";"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::NullCoalesce {
            value: Box::new(EvalExpr::LoadVar("a".to_string())),
            default: Box::new(EvalExpr::NullCoalesce {
                value: Box::new(EvalExpr::LoadVar("b".to_string())),
                default: Box::new(EvalExpr::Const(EvalConst::String("fallback".to_string()))),
            }),
        }))]
    );
}

/// Verifies match expressions preserve subject, patterns, and default expression.
#[test]
fn parse_fragment_accepts_match_source() {
    let program = parse_fragment(br#"return match ($x) { 1, 2 => "small", default => "other" };"#)
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Match {
            subject: Box::new(EvalExpr::LoadVar("x".to_string())),
            arms: vec![EvalMatchArm {
                patterns: vec![
                    EvalExpr::Const(EvalConst::Int(1)),
                    EvalExpr::Const(EvalConst::Int(2)),
                ],
                value: EvalExpr::Const(EvalConst::String("small".to_string())),
            }],
            default: Some(Box::new(EvalExpr::Const(EvalConst::String(
                "other".to_string()
            )))),
        }))]
    );
}

/// Verifies null coalescing binds tighter than PHP ternary expressions.
#[test]
fn parse_fragment_null_coalesce_binds_tighter_than_ternary() {
    let program =
        parse_fragment(br#"return $a ?? $b ? "yes" : "no";"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Ternary {
            condition: Box::new(EvalExpr::NullCoalesce {
                value: Box::new(EvalExpr::LoadVar("a".to_string())),
                default: Box::new(EvalExpr::LoadVar("b".to_string())),
            }),
            then_branch: Some(Box::new(EvalExpr::Const(EvalConst::String(
                "yes".to_string()
            )))),
            else_branch: Box::new(EvalExpr::Const(EvalConst::String("no".to_string()))),
        }))]
    );
}

/// Verifies logical negation parses as a unary expression before comparisons.
#[test]
fn parse_fragment_accepts_logical_not_source() {
    let program = parse_fragment(br#"return !$flag == true;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Binary {
            op: EvalBinOp::LooseEq,
            left: Box::new(EvalExpr::Unary {
                op: EvalUnaryOp::LogicalNot,
                expr: Box::new(EvalExpr::LoadVar("flag".to_string())),
            }),
            right: Box::new(EvalExpr::Const(EvalConst::Bool(true))),
        }))]
    );
}

/// Verifies unary numeric operators bind tighter than multiplication.
#[test]
fn parse_fragment_accepts_unary_numeric_source() {
    let program = parse_fragment(br#"return -$x * +2;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Binary {
            op: EvalBinOp::Mul,
            left: Box::new(EvalExpr::Unary {
                op: EvalUnaryOp::Negate,
                expr: Box::new(EvalExpr::LoadVar("x".to_string())),
            }),
            right: Box::new(EvalExpr::Unary {
                op: EvalUnaryOp::Plus,
                expr: Box::new(EvalExpr::Const(EvalConst::Int(2))),
            }),
        }))]
    );
}

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

/// Verifies indexed array literals and reads parse as runtime array expressions.
#[test]
fn parse_fragment_accepts_indexed_array_read_source() {
    let program = parse_fragment(br#"return [1, 2][0];"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::ArrayGet {
            array: Box::new(EvalExpr::Array(vec![
                EvalArrayElement::Value(EvalExpr::Const(EvalConst::Int(1))),
                EvalArrayElement::Value(EvalExpr::Const(EvalConst::Int(2))),
            ])),
            index: Box::new(EvalExpr::Const(EvalConst::Int(0))),
        }))]
    );
}

/// Verifies legacy `array(...)` literals parse through the same EvalIR array node.
#[test]
fn parse_fragment_accepts_legacy_array_literal_source() {
    let program =
        parse_fragment(br#"return array(1, "name" => "Ada",)[1];"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::ArrayGet {
            array: Box::new(EvalExpr::Array(vec![
                EvalArrayElement::Value(EvalExpr::Const(EvalConst::Int(1))),
                EvalArrayElement::KeyValue {
                    key: EvalExpr::Const(EvalConst::String("name".to_string())),
                    value: EvalExpr::Const(EvalConst::String("Ada".to_string())),
                },
            ])),
            index: Box::new(EvalExpr::Const(EvalConst::Int(1))),
        }))]
    );
}

/// Verifies associative array literals preserve explicit key/value expressions.
#[test]
fn parse_fragment_accepts_assoc_array_literal_source() {
    let program = parse_fragment(br#"return ["name" => "Ada"];"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Array(vec![
            EvalArrayElement::KeyValue {
                key: EvalExpr::Const(EvalConst::String("name".to_string())),
                value: EvalExpr::Const(EvalConst::String("Ada".to_string())),
            }
        ])))]
    );
}

/// Verifies indexed array writes parse as variable-target array set statements.
#[test]
fn parse_fragment_accepts_indexed_array_write_source() {
    let program = parse_fragment(br#"$items[1] = "x";"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::ArraySetVar {
            name: "items".to_string(),
            index: EvalExpr::Const(EvalConst::Int(1)),
            value: EvalExpr::Const(EvalConst::String("x".to_string())),
        }]
    );
}

/// Verifies indexed array append syntax parses as a variable-target append statement.
#[test]
fn parse_fragment_accepts_indexed_array_append_source() {
    let program = parse_fragment(br#"$items[] = "x";"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::ArrayAppendVar {
            name: "items".to_string(),
            value: EvalExpr::Const(EvalConst::String("x".to_string())),
        }]
    );
}

/// Verifies array append syntax is accepted inside `for` update clauses.
#[test]
fn parse_fragment_accepts_array_append_in_for_update_source() {
    let program = parse_fragment(br#"for ($i = 0; $i < 2; $items[] = $i) { $i += 1; }"#)
        .expect("fragment should parse");
    let [EvalStmt::For { update, .. }] = program.statements() else {
        panic!("expected for statement");
    };
    assert_eq!(
        update,
        &vec![EvalStmt::ArrayAppendVar {
            name: "items".to_string(),
            value: EvalExpr::LoadVar("i".to_string()),
        }]
    );
}

/// Verifies object property reads parse as postfix EvalIR expressions.
#[test]
fn parse_fragment_accepts_property_read_source() {
    let program = parse_fragment(br#"return $this->x;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::PropertyGet {
            object: Box::new(EvalExpr::LoadVar("this".to_string())),
            property: "x".to_string(),
        }))]
    );
}

/// Verifies property names preserve source case while keywords remain case-insensitive.
#[test]
fn parse_fragment_preserves_property_case_source() {
    let program = parse_fragment(br#"RETURN $this->camelName;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::PropertyGet {
            object: Box::new(EvalExpr::LoadVar("this".to_string())),
            property: "camelName".to_string(),
        }))]
    );
}

/// Verifies object method calls parse as postfix EvalIR call expressions.
#[test]
fn parse_fragment_accepts_method_call_source() {
    let program = parse_fragment(br#"return $this->Answer();"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::MethodCall {
            object: Box::new(EvalExpr::LoadVar("this".to_string())),
            method: "answer".to_string(),
            args: Vec::new(),
        }))]
    );
}

/// Verifies object construction parses as a named EvalIR expression.
#[test]
fn parse_fragment_accepts_new_object_source() {
    let program = parse_fragment(br#"return new Box();"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::NewObject {
            class_name: "Box".to_string(),
            args: Vec::new(),
        }))]
    );
}

/// Verifies object construction accepts explicitly qualified class names.
#[test]
fn parse_fragment_accepts_qualified_new_object_source() {
    let program = parse_fragment(br#"return new \EvalNs\Box();"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::NewObject {
            class_name: "EvalNs\\Box".to_string(),
            args: Vec::new(),
        }))]
    );
}

/// Verifies object method calls preserve source-order argument expressions.
#[test]
fn parse_fragment_accepts_method_call_args_source() {
    let program = parse_fragment(br#"return $this->add($x + 1);"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::MethodCall {
            object: Box::new(EvalExpr::LoadVar("this".to_string())),
            method: "add".to_string(),
            args: vec![EvalCallArg::positional(EvalExpr::Binary {
                op: EvalBinOp::Add,
                left: Box::new(EvalExpr::LoadVar("x".to_string())),
                right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
            })],
        }))]
    );
}

/// Verifies object method calls parse multiple argument expressions in source order.
#[test]
fn parse_fragment_accepts_method_call_multiple_args_source() {
    let program =
        parse_fragment(br#"return $this->label($x, "ok");"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::MethodCall {
            object: Box::new(EvalExpr::LoadVar("this".to_string())),
            method: "label".to_string(),
            args: vec![
                EvalCallArg::positional(EvalExpr::LoadVar("x".to_string())),
                EvalCallArg::positional(EvalExpr::Const(EvalConst::String("ok".to_string()))),
            ],
        }))]
    );
}

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

/// Verifies unset fragments expand to one by-name unset statement per variable.
#[test]
fn parse_fragment_accepts_unset_source() {
    let program = parse_fragment(b"unset($x, $y);").expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::UnsetVar {
                name: "x".to_string()
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

/// Verifies empty class declarations lower to dynamic class-registration statements.
#[test]
fn parse_fragment_accepts_empty_class_declaration_source() {
    let program = parse_fragment(b"class DynEvalClass {};").expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::ClassDecl(EvalClass::new(
            "DynEvalClass",
            Vec::new(),
            Vec::new()
        ))]
    );
}

/// Verifies public property and method class members lower into dynamic class metadata.
#[test]
fn parse_fragment_accepts_public_class_members() {
    let program = parse_fragment(
            b"class DynEvalSupported { public int $x = 1; public function read() { return $this->x; } }",
        )
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::ClassDecl(EvalClass::new(
            "DynEvalSupported",
            vec![EvalClassProperty::new(
                "x",
                Some(EvalExpr::Const(EvalConst::Int(1)))
            )],
            vec![EvalClassMethod::new(
                "read",
                Vec::new(),
                vec![EvalStmt::Return(Some(EvalExpr::PropertyGet {
                    object: Box::new(EvalExpr::LoadVar("this".to_string())),
                    property: "x".to_string(),
                }))]
            )]
        ))]
    );
}

/// Verifies malformed object construction reports an unexpected token.
#[test]
fn parse_fragment_rejects_new_without_class_name() {
    assert_eq!(
        parse_fragment(b"return new ();"),
        Err(EvalParseError::UnexpectedToken)
    );
}

/// Verifies unsupported expression keywords report the unsupported construct status.
#[test]
fn parse_fragment_rejects_expression_keywords_as_unsupported_constructs() {
    for source in [
        b"return clone $value;" as &[u8],
        b"return yield 1;" as &[u8],
    ] {
        assert_eq!(
            parse_fragment(source),
            Err(EvalParseError::UnsupportedConstruct)
        );
    }
}

/// Verifies malformed statements report parse errors instead of partial IR.
#[test]
fn parse_fragment_rejects_missing_semicolon() {
    assert_eq!(
        parse_fragment(b"$x = 1"),
        Err(EvalParseError::ExpectedSemicolon)
    );
}
