//! Purpose:
//! Interpreter tests for PHP constructs that eval fragments used to reject: the `@`
//! operator, `throw` expressions, `const` statements, by-reference function declarations
//! and trailing commas in parameter, `use` and `unset` lists.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Expected outputs were checked against PHP 8.5 and PHP 8.4.
//! - The fake runtime has no native reporting mask, so `@` is observed through
//!   `error_reporting()`. Silenced diagnostics are covered end to end by codegen tests.

use super::super::*;
use super::support::*;

/// Runs one eval fragment and returns its output and recorded warnings.
fn run(source: &str) -> (String, Vec<String>) {
    let program = parse_fragment(source.as_bytes())
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute eval fragment");
    (values.output, values.warnings)
}

/// Verifies `@` keeps only the fatal levels while its operand runs, then restores the mask.
#[test]
fn error_suppression_masks_non_fatal_levels_for_its_operand() {
    let (output, warnings) = run(r#"error_reporting(E_ALL);
$inside = @error_reporting();
echo $inside, ':', error_reporting(), ':', @strlen('abc');"#);

    assert_eq!(output, "4437:30719:3");
    assert!(warnings.is_empty(), "{warnings:?}");
}

/// Verifies `@` restores the mask when its operand throws, as php-src's live-range cleanup does.
#[test]
fn error_suppression_restores_the_mask_after_a_throw() {
    let (output, _) = run(r#"error_reporting(E_ALL);
function eval_suppressed_thrower() { throw new Exception('boom'); }
try { @eval_suppressed_thrower(); } catch (Exception $e) { echo $e->getMessage(), ':'; }
echo error_reporting();"#);

    assert_eq!(output, "boom:30719");
}

/// Verifies a mask installed by the `@` operand survives, and a fatal-only mask is left alone.
#[test]
fn error_suppression_keeps_a_mask_set_by_its_operand() {
    let (output, _) = run(r#"error_reporting(E_WARNING);
@error_reporting(E_NOTICE);
echo error_reporting(), ':';
error_reporting(E_ERROR);
echo @error_reporting(), ':', error_reporting();"#);

    assert_eq!(output, "8:1:1");
}

/// Verifies `@` applies to a variable read and to a statement-level expression.
#[test]
fn error_suppression_applies_to_reads_and_statements() {
    let (output, _) = run(r#"$value = 'set';
$copy = @$value;
@print($copy);"#);

    assert_eq!(output, "set");
}

/// Verifies `throw` works as an operand of `||`, `??` and the ternary operator.
#[test]
fn throw_expressions_raise_catchable_exceptions() {
    let (output, _) = run(r#"$ok = false;
try { $ok || throw new Exception('or'); } catch (Exception $e) { echo $e->getMessage(), ':'; }
$missing = null;
try { $value = $missing ?? throw new Exception('coalesce'); } catch (Exception $e) { echo $e->getMessage(), ':'; }
$positive = function ($x) { return $x > 0 ? $x : throw new Exception('negative'); };
try { echo $positive(2), ':'; $positive(-1); } catch (Exception $e) { echo $e->getMessage(), ':'; }
$ok = true;
$ok || throw new Exception('not reached');
$ok && $assigned = 5;
echo $assigned;"#);

    assert_eq!(output, "or:coalesce:2:negative:5");
}

/// Verifies `const` statements define global and namespaced constants in source order.
#[test]
fn const_statements_define_constants() {
    let (output, warnings) = run(r#"const EVAL_CONST_FIRST = 1, EVAL_CONST_SECOND = EVAL_CONST_FIRST + 1;
echo EVAL_CONST_FIRST, EVAL_CONST_SECOND, ':', defined('EVAL_CONST_SECOND') ? 'y' : 'n', ':';
echo constant('EVAL_CONST_FIRST');"#);

    assert_eq!(output, "12:y:1");
    assert!(warnings.is_empty(), "{warnings:?}");

    let (output, _) = run(r#"namespace EvalConstNs;
const LIMIT = 3;
echo LIMIT, ':', \EvalConstNs\LIMIT, ':', constant('EvalConstNs\LIMIT');"#);

    assert_eq!(output, "3:3:3");
}

/// Verifies a repeated `const` keeps the first value and raises PHP 8.5's warning.
#[test]
fn const_statement_redefinition_warns_and_keeps_the_first_value() {
    let (output, warnings) = run("const EVAL_CONST_TWICE = 1;\nconst EVAL_CONST_TWICE = 2;\necho EVAL_CONST_TWICE;");

    assert_eq!(output, "1");
    assert_eq!(
        warnings,
        ["Warning: Constant EVAL_CONST_TWICE already defined, this will be an error in PHP 9\n"],
    );
}

/// Verifies profiles before PHP 8.5 omit the PHP 9 notice from the redefinition warning.
#[test]
fn const_statement_redefinition_warning_follows_the_php_profile() {
    let _profile = crate::eval_php_profile::scoped_profile(80400);
    let (output, warnings) = run("const EVAL_CONST_OLD = 1;\nconst EVAL_CONST_OLD = 2;\necho EVAL_CONST_OLD;");

    assert_eq!(output, "1");
    assert_eq!(warnings, ["Warning: Constant EVAL_CONST_OLD already defined\n"]);
}

/// Verifies by-reference functions, closures and methods are declared and reflected as such.
#[test]
fn by_reference_declarations_execute_and_reflect_their_return_mode() {
    let (output, _) = run(r#"function &eval_ref_counter() { static $count = 0; $count++; return $count; }
$closure = function &() { static $n = 10; return $n; };
class EvalRefHolder { public function &item() { static $v = 7; return $v; } }
echo eval_ref_counter(), eval_ref_counter(), ':', $closure(), ':', (new EvalRefHolder)->item(), ':';
echo (new ReflectionFunction('eval_ref_counter'))->returnsReference() ? 'R' : 'r';
echo (new ReflectionFunction($closure))->returnsReference() ? 'R' : 'r';
echo (new ReflectionMethod('EvalRefHolder', 'item'))->returnsReference() ? 'R' : 'r';
echo (new ReflectionFunction(function () {}))->returnsReference() ? 'R' : 'r';"#);

    assert_eq!(output, "12:10:7:RRRr");
}

/// Verifies trailing commas in parameter, closure `use` and `unset()` lists, as PHP 8 allows.
#[test]
fn trailing_commas_in_parameter_use_and_unset_lists() {
    let (output, _) = run(r#"$a = 1; $b = 2;
$sum = function ($x, $y,) use ($a, $b,) { return $x + $y + $a + $b; };
function eval_trailing_params($p, $q,) { return $p . $q; }
class EvalTrailingParams { public function join($p, $q,) { return $p . $q; } }
$u = 5; $v = 6;
unset($u, $v,);
echo $sum(3, 4,), ':', eval_trailing_params('a', 'b'), ':', (new EvalTrailingParams)->join('c', 'd'), ':';
echo isset($u) || isset($v) ? 'set' : 'unset';"#);

    assert_eq!(output, "10:ab:cd:unset");
}

/// Verifies `const` accepts PHP's constant-expression forms, folding literal-decided branches.
#[test]
fn const_statement_accepts_constant_expression_initializers() {
    let (output, warnings) = run(r#"const CE_A = (int) "5", CE_B = true ? 1 : $x, CE_C = 0 && $x, CE_D = "a" ?: $x, CE_F = null ?? 5;
const CE_G = [1, 2][1], CE_H = -PHP_INT_MAX, CE_J = 1 <=> 2;
const CE_K = strlen(...);
const CE_L = static function () { return 7; };
const CE_M = new stdClass;
echo CE_A, CE_B, CE_C ? "t" : "f", CE_D, CE_F, CE_G, CE_H, CE_J, (CE_K)("ab"), (CE_L)(), CE_M instanceof stdClass ? "o" : "-";"#);

    assert_eq!(output, "51fa52-9223372036854775807-127o");
    assert!(warnings.is_empty(), "{warnings:?}");
}

/// Verifies an initializer PHP refuses to compile makes the whole fragment invalid before it runs.
#[test]
fn const_statement_rejects_non_constant_initializers() {
    for source in [
        "echo 'A'; $a = 1; const CE_BAD = $a; echo CE_BAD;",
        "function ce_bad_call() { echo 'called'; return 1; } echo 'A'; const CE_BAD = ce_bad_call();",
        "const CE_BAD = @PHP_EOL;",
        "const CE_BAD = 1 instanceof stdClass;",
        "const CE_BAD = function () { return 1; };",
        "$a = 1; const CE_BAD = static function () use ($a) { return $a; };",
        "const CE_BAD = static::class;",
        "const CE_BAD = new ArrayObject(...[[1]]);",
        "const CE_BAD = print 1;",
        "$i = 0; const CE_BAD = [1, 2][$i];",
        "const CE_BAD = null ?? throw new Exception('x');",
        "echo 'A'; const CE_BAD = new stdClass(a: 1, 2);",
        "const CE_BAD = (1 + 0) ? $bad : 1;",
    ] {
        assert!(parse_fragment(source.as_bytes()).is_err(), "{source}");
    }
}

/// Verifies branches decided by folded literal operands are discarded before validation, as PHP does.
#[test]
fn const_statement_folds_literal_operands_that_decide_a_branch() {
    let (output, warnings) = run(r#"const CF_A = (1 + 1) ? 1 : $bad, CF_B = (0 + 0) && $bad, CF_C = [] ? $bad : 7, CF_D = (int) "0" ? $bad : 3;
const CF_E = -1 ? 1 : $bad, CF_F = (1 + 0) ?? $bad, CF_G = __LINE__ ? 1 : $bad;
echo CF_A, CF_B ? "t" : "f", CF_C, CF_D, CF_E, CF_F, CF_G;"#);

    assert_eq!(output, "1f73111");
    assert!(warnings.is_empty(), "{warnings:?}");
}
