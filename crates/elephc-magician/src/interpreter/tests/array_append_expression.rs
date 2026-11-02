//! Purpose:
//! Interpreter tests pinning `$a[] = v` used where a VALUE is wanted rather than as a whole
//! statement -- `return $this->before[] = $name;`, the chained `$dirs[] = $paths[] = $d;`, and
//! the parenthesised `($log[] = 7)`.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::array_append_expression`.
//!
//! Key details:
//! - Every expected string is `php -n` 8.5.6's output on the same fragment.
//! - The append had a statement lowering only. `parse_postfix` stops in front of an empty `[]`
//!   so the statement parser can see it, which left every expression position refusing at the
//!   `[` with `ExpectedSemicolon` -- the shape Symfony's `NormalizationBuilder`,
//!   `ValidationBuilder`, `Extension` and `FrameworkExtension` all use.
//! - What the expression EVALUATES TO is the point: the assigned value, not the array and not
//!   the new length. A chain proves it, because the inner append's value is what the outer one
//!   stores.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse array append expression fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values)
        .expect("execute array append expression fragment");
    values.output.clone()
}

/// Verifies `return $this->prop[] = $v;` returns the assigned value and still appends.
///
/// `php -n` 8.5.6 prints `first;second;2;second`. This is
/// `NormalizationBuilder::before()` and `ValidationBuilder::rule()` verbatim in shape: they
/// return the object they just pushed, and a lowering that returned the array or the new count
/// would break every caller that chains off the result.
#[test]
fn a_property_append_in_return_position_yields_the_assigned_value() {
    assert_eq!(
        out(
            br#"class Builder {
    public array $before = [];
    public function add(string $name): string {
        return $this->before[] = $name;
    }
}
$b = new Builder();
echo $b->add("first"), ";", $b->add("second"), ";", count($b->before), ";", $b->before[1];"#
        ),
        "first;second;2;second",
    );
}

/// Verifies a CHAINED append appends the same value to both arrays.
///
/// `php -n` 8.5.6 prints `1;1;/res;/res`. This is `FrameworkExtension`'s
/// `$dirs[] = $transPaths[] = \dirname(...)`. The chain is the assertion that carries the
/// weight: the outer append can only store the right thing if the inner one evaluated to the
/// assigned value.
#[test]
fn a_chained_append_stores_the_same_value_in_both_arrays() {
    assert_eq!(
        out(
            br#"$dirs = [];
$paths = [];
$dirs[] = $paths[] = "/res";
echo count($dirs), ";", count($paths), ";", $dirs[0], ";", $paths[0];"#
        ),
        "1;1;/res;/res",
    );
}

/// Verifies a parenthesised append is an ordinary expression with the assigned value.
///
/// `php -n` 8.5.6 prints `7;7;1`.
#[test]
fn a_parenthesised_append_evaluates_to_the_assigned_value() {
    assert_eq!(
        out(
            br#"$log = [];
$x = ($log[] = 7);
echo $x, ";", $log[0], ";", count($log);"#
        ),
        "7;7;1",
    );
}

/// Verifies an OBJECT value is the same instance in the array and in the expression result.
///
/// `php -n` 8.5.6 prints `same;4`. `Extension::getProcessedConfig()` and
/// `NormalizationBuilder::before()` both return a freshly constructed object they also stored,
/// and their callers then configure it -- so the two have to be one object, not two.
#[test]
fn an_appended_object_is_the_same_instance_the_array_holds() {
    assert_eq!(
        out(
            br#"class Node { public $id = 0; }
$store = [];
$n = ($store[] = new Node());
$n->id = 4;
echo ($store[0] === $n) ? "same" : "copy", ";", $store[0]->id;"#
        ),
        "same;4",
    );
}

/// Verifies the append expression works on a STATIC property and on a NESTED element.
///
/// `php -n` 8.5.6 prints `r1;1;3;3;1`. Both targets reach the append through the same general
/// lvalue path a statement append uses, so pinning them together says the expression form did
/// not quietly narrow to plain variables.
#[test]
fn a_static_property_and_a_nested_element_append_as_expressions() {
    assert_eq!(
        out(
            br#"class Holder { public static array $rows = []; }
echo Holder::$rows[] = "r1", ";", count(Holder::$rows), ";";
$nested = [];
echo $nested["k"][] = 3, ";", $nested["k"][0], ";", count($nested["k"]);"#
        ),
        "r1;1;3;3;1",
    );
}

/// Verifies an append expression through a BY-REFERENCE parameter reaches the caller's array.
///
/// `php -n` 8.5.6 prints `a;b;a,b`. The value the function returns and the element the caller
/// can see are two separate obligations, and a lowering that satisfied only one would still
/// print half of this.
#[test]
fn an_append_expression_through_a_by_reference_parameter_reaches_the_caller() {
    assert_eq!(
        out(
            br#"function collect(array &$sink, $v) { return $sink[] = $v; }
$sink = [];
echo collect($sink, "a"), ";", collect($sink, "b"), ";", implode(",", $sink);"#
        ),
        "a;b;a,b",
    );
}
