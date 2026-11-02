//! Purpose:
//! Parser tests for assignment, compound assignment, increment/decrement, and echo statements.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases assert direct statement lowering into EvalIR stores and echoes.

use super::support::*;

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

/// Verifies an append in RETURN position lowers to the append EXPRESSION node.
///
/// `parse_postfix` stops in front of an empty `[]` so the statement parser can claim
/// `$a[] = 1;`, which left every other position with no rule at all: this refused at the `[`
/// with `ExpectedSemicolon`.
#[test]
fn parse_fragment_accepts_an_array_append_in_return_position() {
    let program = parse_fragment(br#"return $this->before[] = $name;"#)
        .expect("an append in return position should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::ArrayAppendAssign {
            target: Box::new(EvalExpr::PropertyGet {
                object: Box::new(EvalExpr::LoadVar("this".to_string())),
                property: "before".to_string(),
            }),
            value: Box::new(EvalExpr::LoadVar("name".to_string())),
        }))]
    );
}

/// Verifies a CHAINED append keeps the statement lowering outside and the expression inside.
///
/// The outer append is still a whole statement, so it keeps `ArrayAppendVar`; only the nested
/// one needs the expression node. Pinning both halves in one expectation is what says the new
/// rule did not swallow the statement form.
#[test]
fn parse_fragment_accepts_a_chained_array_append() {
    let program = parse_fragment(br#"$dirs[] = $paths[] = "/res";"#)
        .expect("a chained append should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::ArrayAppendVar {
            name: "dirs".to_string(),
            value: EvalExpr::ArrayAppendAssign {
                target: Box::new(EvalExpr::LoadVar("paths".to_string())),
                value: Box::new(EvalExpr::Const(EvalConst::String("/res".to_string()))),
            },
        }]
    );
}

/// Verifies an append through an ARRAY ELEMENT of a property parses as the expression node.
///
/// `$this->rows["k"][] = 1;` has no dedicated statement -- the write goes through an element,
/// not through a property -- and used to be refused outright. It is Symfony's
/// `EventDispatcher::addListener()` and `DebugClassLoader`'s `self::$method[$class][] = ...`.
#[test]
fn parse_fragment_accepts_an_append_through_a_property_element() {
    let program = parse_fragment(br#"$this->rows["k"][] = 1;"#)
        .expect("an append through a property element should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Expr(EvalExpr::ArrayAppendAssign {
            target: Box::new(EvalExpr::ArrayGet {
                array: Box::new(EvalExpr::PropertyGet {
                    object: Box::new(EvalExpr::LoadVar("this".to_string())),
                    property: "rows".to_string(),
                }),
                index: Box::new(EvalExpr::Const(EvalConst::String("k".to_string()))),
            }),
            value: Box::new(EvalExpr::Const(EvalConst::Int(1))),
        })]
    );
}

/// Verifies a REFERENCE append is still refused rather than accepted as a copy.
///
/// `php -n` 8.5.6 ACCEPTS `return $this->rows["k"][] = &$b;`, so this is a gap, not a rule --
/// but `$a[] = &$b` BINDS, and taking it into the append expression node would silently copy
/// instead. The append rule declines when a `&` follows the `=`, which leaves the pre-existing
/// `ExpectedSemicolon` at the `[` in place. A truthful refusal is worth more than a wrong
/// answer; the `&` family is its own gap and this is pinned so a later fix has to change it
/// deliberately.
#[test]
fn parse_fragment_refuses_a_reference_append_in_expression_position() {
    let error = parse_fragment(br#"return $this->rows["k"][] = &$b;"#)
        .expect_err("a reference append should not be accepted as a copy");
    assert_eq!(error.error(), EvalParseError::ExpectedSemicolon);
}

/// Verifies a by-reference declaration and a bind to its result both parse.
///
/// The gap this test was written to pin -- the declaration parsing while the BIND stayed refused
/// -- is closed. Both halves parse now, and what the bind DOES is a behavioural question
/// answered against `php -n` in `interpreter::tests::by_ref_return`.
#[test]
fn parse_fragment_accepts_a_by_ref_declaration_and_a_bind_to_its_result() {
    parse_fragment(br#"function &counter() { static $n = 1; return $n; }"#)
        .expect("a by-reference declaration should parse");
    parse_fragment(br#"function &counter() { static $n = 1; return $n; } $r = &counter();"#)
        .expect("binding to a by-reference call result should parse");
}

/// Verifies null-coalescing assignment is an expression with a writable array target.
#[test]
fn parse_fragment_accepts_null_coalesce_assignment_expression() {
    let program = parse_fragment(br#"return $_SERVER["option"] ??= [];"#)
        .expect("null-coalescing assignment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::NullCoalesceAssign {
            target: Box::new(EvalExpr::ArrayGet {
                array: Box::new(EvalExpr::LoadVar("_SERVER".to_string())),
                index: Box::new(EvalExpr::Const(EvalConst::String("option".to_string()))),
            }),
            default: Box::new(EvalExpr::Array(Vec::new())),
        }))]
    );
}

/// Verifies null-coalescing assignment remains right-associative for variable targets.
#[test]
fn parse_fragment_keeps_null_coalesce_assignment_right_associative() {
    let program =
        parse_fragment(b"return $left ??= $right ??= 7;").expect("assignment chain should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::NullCoalesceAssign {
            target: Box::new(EvalExpr::LoadVar("left".to_string())),
            default: Box::new(EvalExpr::NullCoalesceAssign {
                target: Box::new(EvalExpr::LoadVar("right".to_string())),
                default: Box::new(EvalExpr::Const(EvalConst::Int(7))),
            }),
        }))]
    );
}

/// Verifies `??=` binds to the rightmost lvalue of a concatenation expression.
#[test]
fn parse_fragment_accepts_null_coalesce_assignment_after_concat() {
    let program = parse_fragment(
        br#"return $directory.$tmpSuffix ??= str_replace('/', '-', 'x/y');"#,
    )
    .expect("concatenation RHS null-coalesce assignment should parse");
    assert!(matches!(
        program.statements(),
        [EvalStmt::Return(Some(EvalExpr::Binary {
            op: EvalBinOp::Concat,
            right,
            ..
        }))]
            if matches!(right.as_ref(), EvalExpr::NullCoalesceAssign { target, .. }
                if matches!(target.as_ref(), EvalExpr::LoadVar(name) if name == "tmpSuffix"))
    ));
}

/// Verifies a negated static property keeps `??=` bound to that property target.
#[test]
fn parse_fragment_accepts_negated_static_property_null_coalesce_assignment() {
    let program = parse_fragment(
        br#"return !self::$ready ??= StaticAssignmentProbe::resolve();"#,
    )
    .expect("negated static-property null-coalesce assignment should parse");
    assert!(matches!(
        program.statements(),
        [EvalStmt::Return(Some(EvalExpr::Unary {
            op: EvalUnaryOp::LogicalNot,
            expr,
        }))]
            if matches!(expr.as_ref(), EvalExpr::NullCoalesceAssign { target, .. }
                if matches!(target.as_ref(), EvalExpr::StaticPropertyGet { class_name, property }
                    if class_name == "self" && property == "ready"))
    ));
}

/// Verifies a statement-level nested array target remains one writable `??=` expression.
#[test]
fn parse_fragment_accepts_nested_array_null_coalesce_assignment() {
    let program = parse_fragment(br#"$items["outer"]["inner"] ??= 4;"#)
        .expect("nested null-coalescing assignment should parse");
    assert!(matches!(
        program.statements(),
        [EvalStmt::Expr(EvalExpr::NullCoalesceAssign { target, .. })]
            if matches!(target.as_ref(), EvalExpr::ArrayGet { array, .. }
                if matches!(array.as_ref(), EvalExpr::ArrayGet { .. }))
    ));
}

/// Verifies a static-property store can contain a right-associative variable assignment.
#[test]
fn parse_fragment_accepts_chained_assignment_expression() {
    let program = parse_fragment(br#"self::$loader = $loader = new \stdClass();"#)
        .expect("chained assignment should parse");
    assert!(matches!(
        program.statements(),
        [EvalStmt::StaticPropertySet {
            value: EvalExpr::Assign { target, .. },
            ..
        }] if matches!(target.as_ref(), EvalExpr::LoadVar(name) if name == "loader")
    ));
}

/// Verifies short-array destructuring assignments remain expressions in conditions.
#[test]
fn parse_fragment_accepts_array_destructure_assignment_expression() {
    let program = parse_fragment(
        br#"if ([$scope, $name] = $propertyScopes[$property] ?? null) { echo $scope . $name; }"#,
    )
    .expect("destructuring condition should parse");
    assert!(matches!(
        program.statements(),
        [EvalStmt::If { condition: EvalExpr::ArrayDestructureAssign { targets, value }, .. }]
            if destructure_variable_names(targets) == vec![Some("scope".to_string()), Some("name".to_string())]
                && matches!(value.as_ref(), EvalExpr::NullCoalesce { .. })
    ));
}

/// Verifies a leading logical negation applies after its nested assignment expression.
#[test]
fn parse_fragment_accepts_negated_assignment_expression() {
    let program = parse_fragment(br#"return !$valueIsStatic = $values[0] !== $sentinel;"#)
        .expect("negated assignment should parse");
    assert!(matches!(
        program.statements(),
        [EvalStmt::Return(Some(EvalExpr::Unary {
            op: EvalUnaryOp::LogicalNot,
            expr,
        }))] if matches!(expr.as_ref(), EvalExpr::Assign { target, value }
            if matches!(target.as_ref(), EvalExpr::LoadVar(name) if name == "valueIsStatic")
                && matches!(value.as_ref(), EvalExpr::Binary { op: EvalBinOp::StrictNotEq, .. }))
    ));
}

/// Verifies a comparison assigns its right operand at PHP assignment precedence.
#[test]
fn parse_fragment_accepts_comparison_right_hand_assignment() {
    let program = parse_fragment(br#"return null !== $ref = 1;"#)
        .expect("comparison assignment should parse");
    assert!(matches!(
        program.statements(),
        [EvalStmt::Return(Some(EvalExpr::Binary {
            op: EvalBinOp::StrictNotEq,
            right,
            ..
        }))] if matches!(right.as_ref(), EvalExpr::Assign { target, value }
            if matches!(target.as_ref(), EvalExpr::LoadVar(name) if name == "ref")
                && matches!(value.as_ref(), EvalExpr::Const(EvalConst::Int(1))))
    ));
}

/// Verifies a logical branch preserves a terminal comparison assignment on its right side.
#[test]
fn parse_fragment_accepts_logical_comparison_right_hand_assignment() {
    let program = parse_fragment(br#"return $enabled && null !== $ref = 1;"#)
        .expect("logical comparison assignment should parse");
    assert!(matches!(
        program.statements(),
        [EvalStmt::Return(Some(EvalExpr::Binary {
            op: EvalBinOp::LogicalAnd,
            right,
            ..
        }))] if matches!(right.as_ref(), EvalExpr::Binary {
            op: EvalBinOp::StrictNotEq,
            right,
            ..
        } if matches!(right.as_ref(), EvalExpr::Assign { target, .. }
            if matches!(target.as_ref(), EvalExpr::LoadVar(name) if name == "ref")))
    ));
}

/// Verifies a logical branch preserves a terminal negated assignment on its right side.
#[test]
fn parse_fragment_accepts_logical_negated_right_hand_assignment() {
    let program = parse_fragment(
        br#"return $receiver->isAnonymous() || !$class = $receiver->getClosureCalledClass();"#,
    )
    .expect("logical negated assignment should parse");
    assert!(matches!(
        program.statements(),
        [EvalStmt::Return(Some(EvalExpr::Binary {
            op: EvalBinOp::LogicalOr,
            right,
            ..
        }))] if matches!(right.as_ref(), EvalExpr::Unary {
            op: EvalUnaryOp::LogicalNot,
            expr,
        } if matches!(expr.as_ref(), EvalExpr::Assign { target, value }
            if matches!(target.as_ref(), EvalExpr::LoadVar(name) if name == "class")
                && matches!(value.as_ref(), EvalExpr::MethodCall { object, method, args }
                    if method == "getClosureCalledClass" && args.is_empty()
                        && matches!(object.as_ref(), EvalExpr::LoadVar(name) if name == "receiver"))))
    ));
}

/// Verifies a variable array read can continue into an instance-method postfix expression.
#[test]
fn parse_fragment_accepts_array_element_method_call_statement() {
    let program = parse_fragment(br#"$objects[$state]->__wakeup();"#)
        .expect("array element method call should parse");
    assert!(matches!(
        program.statements(),
        [EvalStmt::Expr(EvalExpr::MethodCall { object, method, args })]
            if method == "__wakeup" && args.is_empty()
                && matches!(object.as_ref(), EvalExpr::ArrayGet { array, index }
                    if matches!(array.as_ref(), EvalExpr::LoadVar(name) if name == "objects")
                        && matches!(index.as_ref(), EvalExpr::LoadVar(name) if name == "state"))
    ));
}

/// Verifies a prefix increment can supply an array index expression.
#[test]
fn parse_fragment_accepts_prefix_increment_array_index_expression() {
    let program = parse_fragment(br#"return $tokens[++$i];"#).expect("prefix index should parse");
    assert!(matches!(
        program.statements(),
        [EvalStmt::Return(Some(EvalExpr::ArrayGet { index, .. }))]
            if matches!(index.as_ref(), EvalExpr::CompoundAssign {
                target,
                op: EvalBinOp::Add,
                value,
            } if matches!(target.as_ref(), EvalExpr::LoadVar(name) if name == "i")
                && matches!(value.as_ref(), EvalExpr::Const(EvalConst::Int(1))))
    ));
}

/// Verifies a postfix increment remains an expression while returning an array element index.
#[test]
fn parse_fragment_accepts_postfix_increment_expression() {
    let program = parse_fragment(br#"return $i++;"#).expect("postfix increment should parse");
    assert!(matches!(
        program.statements(),
        [EvalStmt::Return(Some(EvalExpr::PostfixIncDec { target, increment: true }))]
            if matches!(target.as_ref(), EvalExpr::LoadVar(name) if name == "i")
    ));
}

/// Verifies a nested array append lowers to a generic writable array target.
#[test]
fn parse_fragment_accepts_nested_array_append_statement() {
    let program = parse_fragment(br#"$index[$key][] = $value;"#)
        .expect("nested append should parse");
    assert!(matches!(
        program.statements(),
        [EvalStmt::ArrayAppend {
            target: EvalExpr::ArrayGet { array, index },
            value: EvalExpr::LoadVar(value),
        }] if value == "value"
            && matches!(array.as_ref(), EvalExpr::LoadVar(name) if name == "index")
            && matches!(index.as_ref(), EvalExpr::LoadVar(name) if name == "key")
    ));
}

/// Verifies callable signatures accept PHP's trailing comma after the final parameter.
#[test]
fn parse_fragment_accepts_trailing_parameter_comma() {
    parse_fragment(br#"function collectValues(int $first, string $second,) {}"#)
        .expect("trailing parameter comma should parse");
}

/// Verifies a generated runtime bootstrap shape parses as one complete fragment.
#[test]
fn parse_fragment_accepts_generated_runtime_bootstrap_shape() {
    parse_fragment(
        br#"
if (true === (require_once __DIR__.'/autoload.php') || empty($_SERVER['SCRIPT_FILENAME'])) {
    return;
}
$app = require $_SERVER['SCRIPT_FILENAME'];
if (!is_object($app)) {
    throw new TypeError(sprintf('invalid %s from %s', get_debug_type($app), $_SERVER['SCRIPT_FILENAME']));
}
if (is_string($_SERVER['APP_RUNTIME_OPTIONS'] ??= $_ENV['APP_RUNTIME_OPTIONS'] ?? [])) {
    $_SERVER['APP_RUNTIME_OPTIONS'] = json_decode($_SERVER['APP_RUNTIME_OPTIONS'], true, 512, JSON_THROW_ON_ERROR);
}
$_SERVER['APP_RUNTIME'] ??= $_ENV['APP_RUNTIME'] ?? 'Runtime\\DefaultRuntime';
$runtime = new $_SERVER['APP_RUNTIME']($_SERVER['APP_RUNTIME_OPTIONS'] += [
    'project_dir' => dirname(__DIR__, 1),
]);
[$app, $args] = $runtime->getResolver($app)->resolve();
$app = $app(...$args);
exit($runtime->getRunner($app)->run());
"#,
    )
    .expect("generated runtime bootstrap should parse");
}

/// Verifies a generated autoloader initialization shape parses completely.
#[test]
fn parse_fragment_accepts_generated_autoloader_shape() {
    parse_fragment(
        br#"
class GeneratedAutoloaderInitFixture
{
    private static $loader;
    public static function loadClassLoader($class)
    {
        if ('Vendor\Autoload\ClassLoader' === $class) {
            require __DIR__ . '/ClassLoader.php';
        }
    }
    public static function getLoader()
    {
        if (null !== self::$loader) {
            return self::$loader;
        }
        require __DIR__ . '/platform_check.php';
        spl_autoload_register(array('GeneratedAutoloaderInitFixture', 'loadClassLoader'), true, true);
        self::$loader = $loader = new \Vendor\Autoload\ClassLoader(\dirname(__DIR__));
        spl_autoload_unregister(array('GeneratedAutoloaderInitFixture', 'loadClassLoader'));
        require __DIR__ . '/autoload_static.php';
        call_user_func(\Vendor\Autoload\StaticInitFixture::getInitializer($loader));
        $loader->register(true);
        $filesToLoad = \Vendor\Autoload\StaticInitFixture::$files;
        $requireFile = \Closure::bind(static function ($fileIdentifier, $file) {
            if (empty($GLOBALS['__autoload_files'][$fileIdentifier])) {
                $GLOBALS['__autoload_files'][$fileIdentifier] = true;
                require $file;
            }
        }, null, null);
        foreach ($filesToLoad as $fileIdentifier => $file) {
            $requireFile($fileIdentifier, $file);
        }
        return $loader;
    }
}
"#,
    )
    .expect("generated autoloader shape should parse");
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

/// Verifies a property source lowers to the general lvalue reference binding.
///
/// `$knownTagVersions = &$this->knownTagVersions;` is the Symfony `TagAwareAdapter` line that
/// this grammar refused; the plain-variable case above must stay on `ReferenceAssign`, because
/// only that statement aliases two scope names symmetrically.
#[test]
fn parse_fragment_accepts_a_property_as_a_reference_source() {
    let program = parse_fragment(b"$shared = &$this->known;").expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::VarReferenceBind {
            target: "shared".to_string(),
            source: EvalExpr::PropertyGet {
                object: Box::new(EvalExpr::LoadVar("this".to_string())),
                property: "known".to_string(),
            },
        }]
    );
}

/// Verifies a static-property source lowers to the general lvalue reference binding.
#[test]
fn parse_fragment_accepts_a_static_property_as_a_reference_source() {
    let program = parse_fragment(b"$shared = &Registry::$items;").expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::VarReferenceBind {
            target: "shared".to_string(),
            source: EvalExpr::StaticPropertyGet {
                class_name: "Registry".to_string(),
                property: "items".to_string(),
            },
        }]
    );
}

/// Verifies a CALL is a legal reference source while a literal is still refused.
///
/// This expectation used to refuse both, on the ground that the interpreter could neither alias
/// a by-reference return nor warn and copy. It can now do exactly that, so a call parses and the
/// RUNTIME decides: a callee declared to return by reference hands its reference over, and one
/// that is not raises php's `Only variable references should be returned by reference` and binds
/// a copy. A literal names no storage under any callee and stays a refusal.
#[test]
fn parse_fragment_accepts_a_call_reference_source_and_refuses_a_literal() {
    parse_fragment(b"$shared = &make_it();").expect("a call is a legal reference source");
    assert_eq!(
        parse_fragment_error(b"$shared = &1;"),
        Err(EvalParseError::UnsupportedConstruct)
    );
}

/// Verifies a plain-variable source still lowers a property target to exactly one statement.
///
/// The hidden-binding desugaring must apply only to a source that is not a bare variable;
/// `$box->value =& $source;` has to keep producing the single `PropertyReferenceBind` whose
/// source is the caller's own scope name.
#[test]
fn parse_fragment_keeps_one_statement_for_a_plain_variable_property_bind() {
    let program = parse_fragment(b"$box->value =& $source;").expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::PropertyReferenceBind {
            object: EvalExpr::LoadVar("box".to_string()),
            property: "value".to_string(),
            source: "source".to_string(),
        }]
    );
}

/// Verifies a property target accepts a general lvalue source through a hidden binding.
///
/// `PropertyReferenceBind` names its source by scope name and resolves it through
/// `scope.reference_target()`, so binding the lvalue to a hidden name first hands it the exact
/// target without teaching that statement to evaluate an expression.
#[test]
fn parse_fragment_binds_a_property_target_to_a_property_source() {
    let program =
        parse_fragment(b"$clone->known = &$this->known;").expect("fragment should parse");
    let statements = program.statements();
    assert_eq!(statements.len(), 2, "expected a hidden binding then the bind");
    let EvalStmt::VarReferenceBind { target, source } = &statements[0] else {
        panic!("first statement should bind the source to a hidden name: {statements:?}");
    };
    assert!(
        target.starts_with('\0'),
        "the hidden binding must be invisible to user code: {target:?}"
    );
    assert_eq!(
        source,
        &EvalExpr::PropertyGet {
            object: Box::new(EvalExpr::LoadVar("this".to_string())),
            property: "known".to_string(),
        }
    );
    assert_eq!(
        &statements[1],
        &EvalStmt::PropertyReferenceBind {
            object: EvalExpr::LoadVar("clone".to_string()),
            property: "known".to_string(),
            source: target.clone(),
        }
    );
}

/// Verifies nested array elements can bind to an existing variable reference.
#[test]
fn parse_fragment_accepts_array_reference_assignment_source() {
    let program = parse_fragment(b"$refs[$group][$name] =& $value;").expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::ArrayReferenceBind {
            target: EvalExpr::ArrayGet {
                array: Box::new(EvalExpr::ArrayGet {
                    array: Box::new(EvalExpr::LoadVar("refs".to_string())),
                    index: Box::new(EvalExpr::LoadVar("group".to_string())),
                }),
                index: Box::new(EvalExpr::LoadVar("name".to_string())),
            },
            source: EvalExpr::LoadVar("value".to_string()),
        }]
    );
}

/// Verifies an array reference assignment accepts a nested array element source lvalue.
#[test]
fn parse_fragment_accepts_nested_array_reference_assignment_source() {
    let program = parse_fragment(b"$value[$key] =& $refs[$rid];").expect("fragment should parse");
    assert!(matches!(
        program.statements(),
        [EvalStmt::ArrayReferenceBind {
            target: EvalExpr::ArrayGet { .. },
            source: EvalExpr::ArrayGet { array, index },
        }] if matches!(array.as_ref(), EvalExpr::LoadVar(name) if name == "refs")
            && matches!(index.as_ref(), EvalExpr::LoadVar(name) if name == "rid")
    ));
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
/// Verifies a write through a chain deeper than one property or one index parses.
///
/// `$this->listeners[$name][$priority][] = $listener;` and its relatives cost twenty-one Symfony
/// files. The four builders in `property_builders` enumerated property shapes and refused anything
/// else, and because the refusal surfaced only after the whole statement had been read it was
/// reported at the NEXT statement's first token — which is why the sweep saw `unexpected "}"`,
/// `unexpected "return"` and `unexpected "continue"` for what is one construct.
#[test]
fn parse_fragment_accepts_writes_through_a_deeper_chain() {
    for source in [
        br#"$this->listeners[$name][$priority][] = $listener;"# as &[u8],
        br#"$this->rows["k"]["n"] += 2;"#,
        br#"++$sanitizedLogs[$errorId]["errorCount"];"#,
        br#"$this->errorCount[$key] ??= 0;"#,
        br#"$this->data[$key] = &$rows;"#,
    ] {
        parse_fragment(source).unwrap_or_else(|error| {
            panic!(
                "should parse {}: {error:?}",
                String::from_utf8_lossy(source)
            )
        });
    }
}
/// Verifies a by-reference binding parses where the assignment's value is used.
///
/// `if (null !== $exists = &self::$existsCache[$this->resource])` sits at
/// `symfony/config/Resource/ClassExistenceResource.php:62`, in the file that holds
/// `throwOnRequiredClass` — the loader every Symfony `class_exists()` reaches, which is why this
/// one line stopped the whole request.
#[test]
fn parse_fragment_accepts_a_reference_binding_used_as_a_value() {
    let program =
        parse_fragment(br#"$e = &$rows["k"];"#).expect("the plain form should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::VarReferenceBind {
            target: "e".to_string(),
            source: EvalExpr::ArrayGet {
                array: Box::new(EvalExpr::LoadVar("rows".to_string())),
                index: Box::new(EvalExpr::Const(EvalConst::String("k".to_string()))),
            },
        }]
    );
    let program = parse_fragment(br#"return null !== $exists = &self::$cache[$key];"#)
        .expect("the value form should parse");
    let [EvalStmt::Return(Some(EvalExpr::Binary { right, .. }))] = program.statements() else {
        panic!("expected one comparison return, got {:?}", program.statements());
    };
    assert!(matches!(
        right.as_ref(),
        EvalExpr::ReferenceBind { target, .. }
            if matches!(target.as_ref(), EvalExpr::LoadVar(name) if name == "exists")
    ));
}
/// Verifies a `&` before a declaration name is the by-reference return marker, not a syntax error.
#[test]
fn parse_fragment_accepts_by_reference_return_declarations() {
    for source in [
        br#"function &usageIndex() { return 1; }"# as &[u8],
        br#"class DynEvalByRef { public function &getUsageIndex(): int { return 1; } }"#,
        br#"$f = function &() { return 1; };"#,
        br#"$f = fn &() => 1;"#,
    ] {
        parse_fragment(source).unwrap_or_else(|error| {
            panic!(
                "should parse {}: {error:?}",
                String::from_utf8_lossy(source)
            )
        });
    }
}
/// Verifies an assignment is accepted as the right operand of `??`, as PHP's grammar has it.
///
/// PHP's `=` binds looser than `??`, so `$a ?? $a = 5` could only mean `($a ?? $a) = 5`, which is
/// not derivable because the left of `=` must be a variable; bison reduces `$a ?? ($a = 5)` and
/// php prints `55`. Four Symfony files write that, and `dependency-injection/Container.php` writes
/// the `??=` form inside the same shape.
#[test]
fn parse_fragment_accepts_an_assignment_after_null_coalesce() {
    let program = parse_fragment(br#"return $a ?? $a = 5;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::NullCoalesce {
            value: Box::new(EvalExpr::LoadVar("a".to_string())),
            default: Box::new(EvalExpr::Assign {
                target: Box::new(EvalExpr::LoadVar("a".to_string())),
                value: Box::new(EvalExpr::Const(EvalConst::Int(5))),
            }),
        }))]
    );
    parse_fragment(br#"return $this->factories[$id] ?? self::$make ??= self::make(...);"#)
        .expect("the coalesce-assign form should parse");
}
/// Verifies `...` unpacks inside an array literal, keyed and unkeyed.
#[test]
fn parse_fragment_accepts_a_spread_inside_an_array_literal() {
    let program = parse_fragment(br#"return [1, ...$tail, 4];"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Array(vec![
            EvalArrayElement::Value(EvalExpr::Const(EvalConst::Int(1))),
            EvalArrayElement::Spread(EvalExpr::LoadVar("tail".to_string())),
            EvalArrayElement::Value(EvalExpr::Const(EvalConst::Int(4))),
        ])))]
    );
    parse_fragment(br#"return ["a" => 1, ...$rest];"#).expect("the keyed form should parse");
}
/// Verifies a destructuring pattern PHP accepts parses, whatever its targets.
///
/// A pattern's slots are LVALUES, not scope names: `[$this->keys, $this->values] = $values;` sits
/// at `symfony/cache/Adapter/PhpArrayAdapter.php:357` and the keyed form at
/// `http-kernel/DataCollector/DumpDataCollector.php:91`, and a hole carries no slot at all.
#[test]
fn parse_fragment_accepts_every_destructuring_target_shape() {
    let program = parse_fragment(br#"[$a, , $b] = $v;"#).expect("the plain form should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::ArrayDestructure {
            targets: vec![
                Some(EvalDestructureTarget {
                    key: None,
                    slot: EvalDestructureSlot::Lvalue(EvalExpr::LoadVar("a".to_string())),
                }),
                None,
                Some(EvalDestructureTarget {
                    key: None,
                    slot: EvalDestructureSlot::Lvalue(EvalExpr::LoadVar("b".to_string())),
                }),
            ],
            value: EvalExpr::LoadVar("v".to_string()),
        }]
    );
    for source in [
        br#"[$this->keys, $this->values] = $values;"# as &[u8],
        br#"["name" => $name, "line" => $line] = $context;"#,
        br#"[[$a, $b], $c] = $v;"#,
        br#"[$headers["u"], $headers["p"]] = $exploded;"#,
        br#"[$first, , $this->third] = $v;"#,
    ] {
        parse_fragment(source).unwrap_or_else(|error| {
            panic!("should parse {}: {error:?}", String::from_utf8_lossy(source))
        });
    }
    parse_fragment(br#"[f(), $b] = $v;"#).expect_err("a call is not an assignable target");
}
/// Verifies a `foreach` key or value target may be any lvalue or a destructuring pattern.
///
/// `foreach ($container->getDefinitions() as $this->currentId => $definition)` sits at
/// `dependency-injection/Compiler/ResolveInvalidReferencesPass.php:45`, and
/// `foreach ($infos as ['info' => $info, 'count' => $count])` at
/// `event-dispatcher/Debug/TraceableEventDispatcher.php:160`.
#[test]
fn parse_fragment_accepts_foreach_targets_beyond_a_plain_variable() {
    for source in [
        br#"foreach ($m as $this->currentId => $definition) { echo $definition; }"# as &[u8],
        br#"foreach ($m as ["info" => $info, "count" => $count]) { echo $info; }"#,
        br#"foreach ($m as $k => [$a, $b]) { echo $a; }"#,
        br#"foreach ($m as $rows["k"]) { echo 1; }"#,
    ] {
        parse_fragment(source).unwrap_or_else(|error| {
            panic!("should parse {}: {error:?}", String::from_utf8_lossy(source))
        });
    }
    parse_fragment(br#"foreach ($m as [$a] => $v) { echo $v; }"#)
        .expect_err("a destructuring pattern is not a key target in PHP");
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

/// Verifies an assignment expression inside a call argument stores and yields its value.
#[test]
fn parse_fragment_accepts_assignment_inside_call_argument() {
    let program = parse_fragment(b"var_dump($n = 42);").expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Expr(EvalExpr::Call {
            name: "var_dump".to_string(),
            args: vec![EvalCallArg::positional(EvalExpr::Assign {
                target: Box::new(EvalExpr::LoadVar("n".to_string())),
                value: Box::new(EvalExpr::Const(EvalConst::Int(42))),
            })],
        })]
    );
}

/// Verifies assignment expressions stay right-associative and bind tighter than `or`.
#[test]
fn parse_fragment_assignment_expression_precedence_matches_php() {
    let program = parse_fragment(b"return $a = $b = 1; return $a = $b or $c;")
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::Return(Some(EvalExpr::Assign {
                target: Box::new(EvalExpr::LoadVar("a".to_string())),
                value: Box::new(EvalExpr::Assign {
                    target: Box::new(EvalExpr::LoadVar("b".to_string())),
                    value: Box::new(EvalExpr::Const(EvalConst::Int(1))),
                }),
            })),
            EvalStmt::Return(Some(EvalExpr::Binary {
                op: EvalBinOp::LogicalOr,
                left: Box::new(EvalExpr::Assign {
                    target: Box::new(EvalExpr::LoadVar("a".to_string())),
                    value: Box::new(EvalExpr::LoadVar("b".to_string())),
                }),
                right: Box::new(EvalExpr::LoadVar("c".to_string())),
            })),
        ]
    );
}

/// Returns the variable name each destructuring slot writes to, or None for a hole.
///
/// A pattern slot is a general lvalue now, so a test that only cares which VARIABLES a pattern
/// fills says so here rather than spelling the whole node out.
fn destructure_variable_names(
    targets: &[Option<EvalDestructureTarget>],
) -> Vec<Option<String>> {
    targets
        .iter()
        .map(|target| match target {
            Some(EvalDestructureTarget {
                key: None,
                slot: EvalDestructureSlot::Lvalue(EvalExpr::LoadVar(name)),
            }) => Some(name.clone()),
            _ => None,
        })
        .collect()
}
