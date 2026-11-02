//! Purpose:
//! Parser tests for comparison, equality, logical, ternary, match, and unary expressions.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases focus on PHP precedence and associativity in EvalIR.

use super::support::*;

/// Verifies attributed arrow functions retain metadata and infer by-value captures.
#[test]
fn parse_fragment_accepts_attributed_arrow_closure() {
    let program = parse_fragment(
        br#"return #[\Closure(name: "service", class: "Demo\\Service")] fn ($value) => $container->load($value);"#,
    )
    .expect("attributed arrow function should parse");
    let [EvalStmt::Return(Some(EvalExpr::Closure {
        function,
        captures,
        is_static,
    }))] = program.statements()
    else {
        panic!("expected attributed arrow closure return");
    };
    assert!(!is_static);
    assert_eq!(function.attributes().len(), 1);
    assert_eq!(function.attributes()[0].name(), "Closure");
    assert_eq!(captures, &[EvalClosureCapture::new("container", false)]);
}

/// Verifies the parser leaves a generator body alone instead of rewriting it.
///
/// This used to assert the body became `return new ArrayIterator([...])`. That eager lowering
/// was wrong in kind, not degree: it evaluated every yielded expression at call time, so an
/// infinite generator hung, `send()` had nowhere to land, and the body ran before PHP runs it.
/// The parser now emits the yields as marker calls and the interpreter owns the suspension, so
/// the contract this pins is that BOTH yields survive as statements of the closure body.
#[test]
fn parse_fragment_leaves_straight_line_yields_in_the_generator_body() {
    let program = parse_fragment(
        br#"return function () { yield 2 => "two"; yield 5 => "five"; };"#,
    )
    .expect("straight-line yields should parse");
    let [EvalStmt::Return(Some(EvalExpr::Closure { function, .. }))] = program.statements() else {
        panic!("expected generator closure return");
    };
    let body = function.body();
    assert_eq!(body.len(), 2);
    assert!(body.iter().all(|statement| matches!(
        statement,
        EvalStmt::Expr(EvalExpr::Call { name, args })
            if name == EVAL_YIELD_INTRINSIC && args.len() == 2
    )));
}

/// Verifies PHP's error-suppression prefix preserves the wrapped call expression.
#[test]
fn parse_fragment_accepts_error_suppression_source() {
    let program = parse_fragment(br#"return @trigger_error("deprecated", 16384);"#)
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Unary {
            op: EvalUnaryOp::ErrorSuppress,
            expr: Box::new(EvalExpr::Call {
                name: "trigger_error".to_string(),
                args: vec![
                    EvalCallArg::positional(EvalExpr::Const(EvalConst::String(
                        "deprecated".to_string(),
                    ))),
                    EvalCallArg::positional(EvalExpr::Const(EvalConst::Int(16384))),
                ],
            }),
        }))]
    );
}

/// Verifies a conditionally declared typed variadic function accepts a suppressed call body.
#[test]
fn parse_fragment_accepts_conditional_typed_variadic_deprecation_function() {
    parse_fragment(
        br#"if (!function_exists('trigger_deprecation')) {
function trigger_deprecation(string $package, string $version, string $message, mixed ...$args): void {
    @trigger_error(($package || $version ? "Since $package $version: " : '').($args ? vsprintf($message, $args) : $message), \E_USER_DEPRECATED);
}
}"#,
    )
    .expect("conditional typed variadic function should parse");
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
/// Verifies static `instanceof` parses as a high-precedence EvalIR expression.
#[test]
fn parse_fragment_accepts_static_instanceof_source() {
    let program =
        parse_fragment(br#"return !$object instanceof App\Box;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Unary {
            op: EvalUnaryOp::LogicalNot,
            expr: Box::new(EvalExpr::InstanceOf {
                value: Box::new(EvalExpr::LoadVar("object".to_string())),
                target: EvalInstanceOfTarget::ClassName("App\\Box".to_string()),
            }),
        }))]
    );
}

/// Verifies dynamic `instanceof` targets parse from variables, properties, arrays, and parens.
#[test]
fn parse_fragment_accepts_dynamic_instanceof_targets() {
    let program = parse_fragment(
        br#"return $object instanceof $names[0] . ":" . ($object instanceof ($prefix . $suffix));"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Binary {
            op: EvalBinOp::Concat,
            left: Box::new(EvalExpr::Binary {
                op: EvalBinOp::Concat,
                left: Box::new(EvalExpr::InstanceOf {
                    value: Box::new(EvalExpr::LoadVar("object".to_string())),
                    target: EvalInstanceOfTarget::Expr(Box::new(EvalExpr::ArrayGet {
                        array: Box::new(EvalExpr::LoadVar("names".to_string())),
                        index: Box::new(EvalExpr::Const(EvalConst::Int(0))),
                    })),
                }),
                right: Box::new(EvalExpr::Const(EvalConst::String(":".to_string()))),
            }),
            right: Box::new(EvalExpr::InstanceOf {
                value: Box::new(EvalExpr::LoadVar("object".to_string())),
                target: EvalInstanceOfTarget::Expr(Box::new(EvalExpr::Binary {
                    op: EvalBinOp::Concat,
                    left: Box::new(EvalExpr::LoadVar("prefix".to_string())),
                    right: Box::new(EvalExpr::LoadVar("suffix".to_string())),
                })),
            }),
        }))]
    );
}

/// Verifies a cast binds TIGHTER than concatenation, as PHP's unary precedence says.
///
/// This expectation used to pin the opposite shape under a docblock claiming PHP cast
/// precedence: the operand was parsed with `parse_concat`, so the cast swallowed the whole
/// concatenation. `php -n` 8.5.6 prints `34x` for `(int) $a . "4x"` with `$a = "3"` while elephc
/// printed a number, so the shape below is the one php actually builds.
#[test]
fn parse_fragment_accepts_scalar_cast_source() {
    let program =
        parse_fragment(br#"return (string)$value . "!";"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Binary {
            op: EvalBinOp::Concat,
            left: Box::new(EvalExpr::Cast {
                target: EvalCastType::String,
                expr: Box::new(EvalExpr::LoadVar("value".to_string())),
            }),
            right: Box::new(EvalExpr::Const(EvalConst::String("!".to_string()))),
        }))]
    );
}

/// Verifies `**` still binds tighter than a cast, which unary precedence does NOT change.
///
/// `php -n` 8.5.6 gives `int(8)` for `(int) "2.9" ** 2`: the power runs first and the cast
/// truncates 8.41. Parsing the operand with `parse_unary` keeps that, because `parse_unary`
/// reaches `parse_power` on the way down -- so this is the case that says the operand moved to
/// the right LEVEL rather than merely becoming narrower.
#[test]
fn parse_fragment_keeps_power_tighter_than_a_cast() {
    let program = parse_fragment(br#"return (int) "2.9" ** 2;"#).expect("fragment should parse");
    assert!(matches!(
        program.statements(),
        [EvalStmt::Return(Some(EvalExpr::Cast {
            target: EvalCastType::Int,
            expr,
        }))] if matches!(expr.as_ref(), EvalExpr::Binary { op: EvalBinOp::Pow, .. })
    ));
}

/// Verifies `(array)` casts lower through the dynamic cast expression node.
#[test]
fn parse_fragment_accepts_array_cast_source() {
    let program = parse_fragment(br#"return (array) $value;"#).expect("array cast should parse");
    assert!(matches!(
        program.statements(),
        [EvalStmt::Return(Some(EvalExpr::Cast {
            target: EvalCastType::Array,
            expr,
        }))] if matches!(expr.as_ref(), EvalExpr::LoadVar(name) if name == "value")
    ));
}

/// Verifies `(object)` casts lower through the dynamic cast expression node.
///
/// `object` was missing from the cast keyword list, so `(object) $value` was read as the
/// parenthesised constant `object` and the parser then refused the operand that followed it.
#[test]
fn parse_fragment_accepts_object_cast_source() {
    let program = parse_fragment(br#"return (object) $value;"#).expect("object cast should parse");
    assert!(matches!(
        program.statements(),
        [EvalStmt::Return(Some(EvalExpr::Cast {
            target: EvalCastType::Object,
            expr,
        }))] if matches!(expr.as_ref(), EvalExpr::LoadVar(name) if name == "value")
    ));
}

/// Verifies an ARRAY LITERAL operand casts, which is the spelling that refused at the `=>`.
#[test]
fn parse_fragment_accepts_object_cast_of_an_array_literal() {
    let program = parse_fragment(br#"return (object) ["mark" => 0];"#)
        .expect("object cast of an array literal should parse");
    assert!(matches!(
        program.statements(),
        [EvalStmt::Return(Some(EvalExpr::Cast {
            target: EvalCastType::Object,
            expr,
        }))] if matches!(expr.as_ref(), EvalExpr::Array(elements) if elements.len() == 1)
    ));
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
/// Verifies a `match` arm condition list may end with a trailing comma.
///
/// PHP's `match_arm_cond_list` admits one, and `symfony/cache/Traits/RedisTrait.php` writes
/// `'use-cache', 'client-tracking', …, => $context[$name] = …`; the parser named the `=>` as the
/// unexpected token instead of closing the list. A comma with nothing but the closing brace after
/// it stays a syntax error, as it is in PHP.
#[test]
fn parse_fragment_accepts_a_trailing_comma_in_a_match_arm_condition_list() {
    let program = parse_fragment(br#"return match ($x) { 1, 2, => "small", default => "other" };"#)
        .expect("fragment should parse");
    let [EvalStmt::Return(Some(EvalExpr::Match { arms, .. }))] = program.statements() else {
        panic!("expected one match return, got {:?}", program.statements());
    };
    assert_eq!(arms.len(), 1);
    assert_eq!(arms[0].patterns.len(), 2);
    assert!(parse_fragment(br#"return match ($x) { 1, };"#).is_err());
}
