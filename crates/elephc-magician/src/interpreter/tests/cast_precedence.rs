//! Purpose:
//! Interpreter tests pinning what a CAST evaluates to when an operator follows it, and what an
//! int cast makes of a string that only starts with a number.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::cast_precedence`.
//!
//! Key details:
//! - Every expected string is `php -n` 8.5.6's output on the same fragment.
//! - These were SILENT WRONG VALUES rather than refusals, which is why they are worth their own
//!   file: a refusal stops the program and says where, a wrong number reaches the page.
//! - TWO defects that compounded. The parser took the cast's operand with `parse_concat`, so the
//!   cast swallowed the whole concatenation and every additive term in it; and the fixture's
//!   string-to-number conversion parsed the WHOLE string instead of its leading numeric prefix.
//!   `(int) $a . "4x"` with `$a = "3"` printed `0`: the cast took `"34x"` and then the fixture
//!   made 0 of it. Fixing either alone still prints the wrong answer.
//! - The second is a FAKE-RUNTIME defect, not an interpreter one. The compiled backend already
//!   answers php's way -- `tests/ir_backend_smoke_test.rs` pins `intval("42xyz")` at `42` -- so
//!   only the fixture diverged, and it diverged for every numeric use of a string.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse cast precedence fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute cast precedence fragment");
    values.output.clone()
}

/// Verifies a cast applies to its OPERAND and not to the concatenation that follows.
///
/// `php -n` 8.5.6 prints `34x;1x;12x`. The first is the shape this file exists for; the second
/// and third say the same for a `(string)` cast, where taking the concatenation as the operand
/// would have been invisible in one case and wrong in the other.
#[test]
fn a_cast_applies_to_its_operand_not_to_the_concatenation() {
    assert_eq!(
        out(
            br#"$a = "3";
echo (int) $a . "4x", ";";
echo (string) 1 . "x", ";";
echo (string) 12 . "x";"#
        ),
        "34x;1x;12x",
    );
}

/// Verifies a cast binds tighter than arithmetic, so the ARITHMETIC sees the cast result.
///
/// `php -n` 8.5.6 prints `35;5;-3`. `(int) "34x" + 1` is 35 only if the cast runs first AND the
/// leading-numeric rule gives 34; with either half missing it prints something else, which is
/// what makes this one assertion cover both defects.
#[test]
fn a_cast_binds_tighter_than_arithmetic() {
    assert_eq!(
        out(
            br#"$a = "3";
echo (int) "34x" + 1, ";";
echo (int) $a + 2, ";";
echo -(int) "3";"#
        ),
        "35;5;-3",
    );
}

/// Verifies a cast binds tighter than the logical operators.
///
/// `php -n` 8.5.6 prints `no`, because `(bool) $a && $b` is `((bool) $a) && $b` and `$b` is
/// false. Taking the operand further would have cast the whole conjunction and printed the same
/// thing here, so the `$b = false` half is what makes the result depend on the shape.
#[test]
fn a_cast_binds_tighter_than_the_logical_operators() {
    assert_eq!(
        out(
            br#"$a = "3";
$b = false;
echo ((bool) $a && $b) ? "yes" : "no";"#
        ),
        "no",
    );
}

/// Verifies `**` still binds TIGHTER than a cast, which is the one direction unary does not win.
///
/// `php -n` 8.5.6 prints `8`: the power runs first and the cast truncates 8.41. A fix that moved
/// the operand all the way down to the primary level would print 4 here.
#[test]
fn power_still_binds_tighter_than_a_cast() {
    assert_eq!(out(br#"echo (int) "2.9" ** 2;"#), "8");
}

/// Verifies an int cast takes a string's LEADING NUMERIC PREFIX, the way PHP does.
///
/// `php -n` 8.5.6 prints `34;0;12;12;0;1000;0`. The cases are, in order: a numeric prefix; a
/// string that does not start with a number; surrounding whitespace; a fractional prefix
/// truncated toward zero; a HEX literal, which PHP does NOT recognise; an exponent, which it
/// does; and the empty string.
#[test]
fn an_int_cast_takes_the_leading_numeric_prefix() {
    assert_eq!(
        out(
            br#"echo (int) "34x", ";";
echo (int) "x34", ";";
echo (int) "  12  ", ";";
echo (int) "12.9abc", ";";
echo (int) "0x1A", ";";
echo (int) "1e3", ";";
echo (int) "";"#
        ),
        "34;0;12;12;0;1000;0",
    );
}

/// Verifies a FLOAT cast follows the same prefix rule and keeps the fraction.
///
/// `php -n` 8.5.6 prints `3.5`. Pinning the float beside the int says the rule lives in the
/// string-to-number conversion rather than in one cast.
#[test]
fn a_float_cast_takes_the_leading_numeric_prefix_with_its_fraction() {
    assert_eq!(out(br#"echo (float) "3.5rest";"#), "3.5");
}

/// Verifies a cast or prefix operator before an assignment applies to the whole assignment.
///
/// PHP's grammar needs a variable on the left of `=`, so `(int) $pos = strpos($s, ":")` is
/// `(int) ($pos = strpos($s, ":"))`, and the same holds for `-`, `!`, `~`, `@`. The parser
/// refused every one but `!` with "unexpected token `=`"; Symfony's `IconRenderer::renderIcon()`
/// writes `if (0 < (int) $pos = strpos($name, ':'))`, which killed every page rendering an
/// icon. The last case is a compound assignment under a cast. `php -n` 8.5.10 prints
/// `2:2;5:5;-3:3;1:0;-8:7;a:ab`.
#[test]
fn a_prefix_operator_applies_to_the_whole_assignment() {
    assert_eq!(
        out(
            br#"$name = "fa:home";
if (0 < (int) $pos = strpos($name, ":")) { echo $pos, ":", $pos, ";"; }
$s = (string) $y = 5; echo $s, ":", $y, ";";
$n = -$z = 3; echo $n, ":", $z, ";";
$b = !$x = 0; echo (int) $b, ":", $x, ";";
$t = ~$w = 7; echo $t, ":", $w, ";";
$c = "a"; $d = (string) $c .= "b"; echo "a:", $d;"#
        ),
        "2:2;5:5;-3:3;1:0;-8:7;a:ab",
    );
}
