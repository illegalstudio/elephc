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
/// constants, and never checks the branch it skips, so a variable there is not an error.
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
}
foreach (E::cases() as $c) { echo $c->name, '=', $c->value, ';'; }
echo "\n";
"#,
    );
    assert_eq!(out, "A=1;B=2;C=3;D=4;F=5;\n");
}
