//! Purpose:
//! Covers PHP 8.2 backed enum case values written as constant expressions instead of literals.
//!
//! Called from:
//! - The codegen types integration suite.
//!
//! Key details:
//! - Each value folds to its literal at compile time, so `cases()`, `from()`, `tryFrom()` and
//!   `->value` must print exactly what php does.

use crate::support::*;

/// Applies strict operator domains to resolved enum backing constants before folding.
#[test]
fn test_strict_enum_backing_expressions_preserve_operator_domains() {
    let numeric = r#"<?php
const ENUM_COUNT = 7;
enum StrictNumericEnum: int { case Count = +ENUM_COUNT; }
echo (string) StrictNumericEnum::Count->value;
"#;
    assert_eq!(StrictFixture::new(numeric).run(&["--strict"]), "7");

    let strings = r#"<?php
const ENUM_SUFFIX = 3 + 4;
enum StrictStringEnum: string { case Value = 'n' . ENUM_SUFFIX; }
"#;
    StrictFixture::new(strings).assert_error(&["--strict-strings"], "--strict-strings");

    let bools = r#"<?php
const ENUM_CONDITION = 1;
enum StrictBoolEnum: int { case Count = ENUM_CONDITION ? 7 : 8; }
"#;
    StrictFixture::new(bools).assert_error(&["--strict-bools"], "--strict-bools");
}

/// Class and interface constants, concatenation both ways and negation fold to literals,
/// inside a namespace, so `cases()`, `from()` and `tryFrom()` see the values php does.
#[test]
fn test_enum_case_values_from_constant_expressions() {
    let out = compile_and_run(
        r#"<?php
namespace U;
final class Glyphs { public const BAR = '▌'; public const PREFIX = 'sel-'; public const ONE = 1; }
interface Codes { const BASE = 40; }
enum SelectionBar: string
{
    case Focused = Glyphs::BAR;
    case Prefixed = Glyphs::PREFIX . 'x';
    case Number = 'n' . Glyphs::ONE;
    case None = ' ';
}
enum Level: int
{
    case Low = -Glyphs::ONE;
    case Base = Codes::BASE;
    case Zero = 0;
}
foreach (SelectionBar::cases() as $c) { echo $c->name, '=', $c->value, ';'; }
foreach (Level::cases() as $c) { echo $c->name, '=', $c->value, ';'; }
echo SelectionBar::from('sel-x')->name, '|', Level::tryFrom(40)?->name, "\n";
"#,
    );
    assert_eq!(out, "Focused=▌;Prefixed=sel-x;Number=n1;None= ;Low=-1;Base=40;Zero=0;Prefixed|Base\n");
}

/// Global `const` declarations (also declared after the enum), constants inherited from a parent
/// class or an implemented interface, arithmetic, shifts and a constant ternary all fold, and
/// `tryFrom()` / `match` select the folded cases.
#[test]
fn test_enum_case_values_fold_global_inherited_and_arithmetic_constants() {
    let out = compile_and_run(
        r#"<?php
const BASE = 100;
interface HasTag { const TAG = 'tag'; }
class Base { const FLAG = 2; const PREFIX = self::FLAG . '-'; }
final class Leaf extends Base implements HasTag {}
enum Code: int
{
    case Ok = BASE;
    case Shifted = Leaf::FLAG << 3;
    case Sum = BASE + Leaf::FLAG * 5;
}
enum Label: string
{
    case Tagged = Leaf::PREFIX . Leaf::TAG;
    case Late = LATE . (Leaf::FLAG > 1 ? 'big' : 'small');
}
const LATE = 'late-';
foreach (Code::cases() as $c) { echo $c->name, '=', $c->value, ';'; }
foreach (Label::cases() as $c) { echo $c->name, '=', $c->value, ';'; }
echo match (Code::tryFrom(16)) { Code::Shifted => 'shifted', default => 'other' }, '|';
echo Label::from('late-big')->name, "\n";
"#,
    );
    assert_eq!(out, "Ok=100;Shifted=16;Sum=110;Tagged=2-tag;Late=late-big;shifted|Late\n");
}

/// An int-and-string concatenation nested under a comparison and a ternary is joined before the
/// surrounding operators fold, so the case gets the ternary's branch exactly as php computes it.
#[test]
fn test_enum_case_values_fold_int_concatenation_inside_comparison_and_ternary() {
    let out = compile_and_run(
        r#"<?php
class C { const ONE = 1; }
enum E: string
{
    case A = ('n' . C::ONE) === 'n1' ? 'yes' : 'no';
    case B = (C::ONE . C::ONE . 'x') . '-' . (C::ONE + 1);
}
foreach (E::cases() as $c) { echo $c->name, '=', $c->value, ';'; }
echo "\n";
"#,
    );
    assert_eq!(out, "A=yes;B=11x-2;\n");
}

/// An int-and-string concatenation inside an array literal (as a list element, an assoc value
/// or an assoc key) is joined before the enclosing array read folds, so the case gets the
/// element php reads.
#[test]
fn test_enum_case_values_fold_int_concatenation_inside_constant_array_read() {
    let out = compile_and_run(
        r#"<?php
class C { const ONE = 1; }
enum E: string
{
    case A = ['n' . C::ONE][0];
    case B = ['k' => 'x' . C::ONE]['k'];
    case C = ['k' . C::ONE => 'y']['k1'];
}
foreach (E::cases() as $c) { echo $c->name, '=', $c->value, ';'; }
echo "\n";
"#,
    );
    assert_eq!(out, "A=n1;B=x1;C=y;\n");
}

/// PHP decides a branch at compile time when its operand folds from literals and predefined
/// constants (operators and concatenations included), and never checks the branch it skips,
/// so a variable there is not an error.
#[test]
fn test_enum_case_values_skip_branches_decided_by_literals() {
    let out = compile_and_run(
        r#"<?php
enum E: int
{
    case A = true ? 1 : $x;
    case B = PHP_INT_SIZE ? 2 : strlen('b');
    case C = false || 1 ? 3 : $y;
    case D = null ?? 4 ?? $z;
    case F = 5 ?: $w;
    case G = ('n' . 1) ? 6 : $v;
    case H = ('' . 0) ? $u : 7;
    case I = (2 ** 3 - 8) ? $t : 8;
}
foreach (E::cases() as $c) { echo $c->name, '=', $c->value, ';'; }
echo "\n";
"#,
    );
    assert_eq!(out, "A=1;B=2;C=3;D=4;F=5;G=6;H=7;I=8;\n");
}

/// A non-empty keyed array is a known-true condition, like a list: PHP decides the branch at
/// compile time and skips the variable, under `!`, a `(bool)` cast, `&&` and `||` too.
#[test]
fn test_enum_case_values_decide_branches_on_literal_keyed_arrays() {
    let out = compile_and_run(
        r#"<?php
enum E: int
{
    case A = ['k' => 1] ? 1 : $x;
    case B = [1 => 'a', 'b'] ? 2 : $y;
    case C = !['k' => 1] ? $z : 3;
    case D = (bool) ['k' => [1]] ? 4 : $w;
    case F = ([1] && ['k' => 1]) ? 5 : $v;
    case G = ([] || ['k' => 0]) ? 6 : $u;
    case H = ['k' => 1] ? 7 : 0;
    case I = [2.0 => 'a', true => 'b'] ? 9 : $t;
    case J = [-9223372036854775808.0 => 'a', -0.0 => 'b', 1e3 => 'c'] ? 10 : $s;
    case K = [9223372036854775806 => 'a', 'b', 'k' => 'c'] ? 11 : $r;
    case L = [-5 => 'a', 'b'] ? 12 : $q;
}
foreach (E::cases() as $c) { echo $c->name, '=', $c->value, ';'; }
echo "\n";
"#,
    );
    assert_eq!(out, "A=1;B=2;C=3;D=4;F=5;G=6;H=7;I=9;J=10;K=11;L=12;\n");
}

/// A property default whose literal array appends after the `PHP_INT_MAX` key keeps PHP's
/// catchable "next element is already occupied" Error instead of folding the condition away.
#[test]
fn test_property_default_keeps_next_key_overflow_error() {
    let out = compile_and_run(
        r#"<?php
class C { public $p = [9223372036854775807 => 1, 2] ? 7 : 8; }
try { echo (new C)->p; } catch (Error $e) { echo $e->getMessage(); }
echo "\n";
"#,
    );
    assert_eq!(out, "Cannot add element to the array as the next element is already occupied\n");
}

/// A `const` declared in a `require_once`d file backs an enum case (and a property default,
/// which reads the same collected constants), as it does in php.
#[test]
fn test_enum_case_values_fold_constants_from_required_files() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
require_once __DIR__ . '/consts.php';
enum Route: string
{
    case Orders = PREFIX . 'orders';
    case Users = \Api\BASE . 'users';
}
class Client { public $root = PREFIX . 'v1'; }
foreach (Route::cases() as $c) { echo $c->name, '=', $c->value, ';'; }
echo Route::from('/api/orders')->name, '|', (new Client)->root, "\n";
"#,
            ),
            (
                "consts.php",
                r#"<?php
namespace {
    const PREFIX = '/api/';
}
namespace Api {
    const BASE = '/base/';
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "Orders=/api/orders;Users=/base/users;Orders|/api/v1\n");
}
