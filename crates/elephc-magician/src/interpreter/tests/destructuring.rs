//! Purpose:
//! Interpreter tests pinning PHP list-destructuring beyond the plain `[$a, $b] = $v;` form:
//! holes, keys, nesting, and slots that are properties, elements or static properties.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::destructuring`.
//!
//! Key details:
//! - Every expected string is `php -n` 8.5.6's output on the same fragment.
//! - A destructuring pattern is NOT an array literal, and the old model -- a list of optional
//!   variable NAMES recovered from an already-parsed literal -- could express only the simplest
//!   quarter of it. A literal cannot even hold the hole in `[, , , $x]`, so the expression
//!   position had to stop converting one and parse the pattern outright.
//! - A non-array right-hand side is NOT fatal. That mattered: `var-exporter`'s
//!   `LazyDecoratorTrait` writes `[, , , $access] = $propertyScopes[$name] ?? null` precisely so
//!   the null case is reachable, and the statement form used to refuse it.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse destructuring fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute destructuring fragment");
    values.output.clone()
}

/// Verifies a pattern writes to PROPERTIES, ELEMENTS and STATIC properties, not only variables.
///
/// `php -n` 8.5.6 prints `12;ab;stat`. The first is `PhpDumper`'s
/// `[$this->definitionVariables, $this->referenceVariables] = $scope;`.
#[test]
fn a_pattern_writes_to_every_kind_of_lvalue() {
    assert_eq!(
        out(
            br#"class C { public $x; public $y; public function f($v): void { [$this->x, $this->y] = $v; } }
$c = new C();
$c->f([1, 2]);
echo $c->x, $c->y, ";";
$h = [];
[$h["u"], $h["p"]] = ["a", "b"];
echo $h["u"], $h["p"], ";";
class S { public static $sx; }
[S::$sx] = ["stat"];
echo S::$sx;"#
        ),
        "12;ab;stat",
    );
}

/// Verifies HOLES skip their position rather than shifting the ones after them.
///
/// `php -n` 8.5.6 prints `w;pr`. The second case has the hole in the MIDDLE, which is what
/// separates skipping from truncating: a pattern that dropped holes instead of counting them
/// would put `q` in `$third`.
#[test]
fn holes_skip_their_position_without_shifting_the_rest() {
    assert_eq!(
        out(
            br#"[, , , $access] = [1, 2, 3, "w"];
echo $access, ";";
[$first, , $third] = ["p", "q", "r"];
echo $first, $third;"#
        ),
        "w;pr",
    );
}

/// Verifies KEYED slots read by key, and that nesting works.
///
/// `php -n` 8.5.6 prints `12;123`.
#[test]
fn keyed_slots_read_by_key_and_patterns_nest() {
    assert_eq!(
        out(
            br#"["a" => $x, "b" => $y] = ["a" => 1, "b" => 2];
echo $x, $y, ";";
[[$p, $q], $r] = [[1, 2], 3];
echo $p, $q, $r;"#
        ),
        "12;123",
    );
}

/// Verifies a destructuring assignment used as a CONDITION evaluates to its right-hand side.
///
/// `php -n` 8.5.6 prints `acc;no`. This is `LazyDecoratorTrait` exactly: the `if` tests the
/// array, not the last slot, and a null right-hand side gives every slot null WITHOUT a fatal --
/// the statement form used to refuse a non-array outright, which would have made the second
/// branch unreachable.
#[test]
fn a_destructuring_condition_yields_the_right_hand_side() {
    assert_eq!(
        out(
            br#"$scopes = ["n" => [1, 2, 3, "acc"]];
if ([, , , $a2] = $scopes["n"] ?? null) { echo $a2, ";"; } else { echo "no;"; }
if ([, , , $a3] = $scopes["missing"] ?? null) { echo $a3; } else { echo "no"; }"#
        ),
        "acc;no",
    );
}

/// Verifies a KEYED pattern as a foreach value, and a PROPERTY as a foreach key.
///
/// `php -n` 8.5.6 prints `12;a1`. Both are bound through a hidden name and copied into place by
/// a statement prepended to the loop body, which is how the plain pattern form already worked.
#[test]
fn foreach_binds_a_keyed_pattern_and_a_property_key() {
    assert_eq!(
        out(
            br#"foreach ([["k" => 1], ["k" => 2]] as ["k" => $v]) { echo $v; }
echo ";";
class K { public $k; public function f($m): void { foreach ($m as $this->k => $v) { echo $this->k, $v; } } }
(new K())->f(["a" => 1]);"#
        ),
        "12;a1",
    );
}

/// Verifies the `list(...)` spelling still destructures, which it did before.
///
/// `php -n` 8.5.6 prints `12`. Pinned beside the rest so a disabled-fix run shows which shapes
/// the change bought.
#[test]
fn the_list_spelling_still_destructures() {
    assert_eq!(
        out(
            br#"$lst = [1, 2];
list($l1, $l2) = $lst;
echo $l1, $l2;"#
        ),
        "12",
    );
}
