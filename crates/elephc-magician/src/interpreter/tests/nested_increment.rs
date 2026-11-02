//! Purpose:
//! Interpreter tests pinning `++` and `--` applied to an ARRAY ELEMENT rather than to a plain
//! variable or a property -- `++$collectedLogs[$message]['count'];`, which is Symfony's
//! `KernelTrait` deprecation collector.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::nested_increment`.
//!
//! Key details:
//! - Every expected string is `php -n` 8.5.6's output on the same fragment.
//! - The PREFIX statement form had no lowering: `parse_prefix_inc_dec_stmt` and
//!   `parse_prefixed_property_inc_dec_stmt` handed their target to `property_inc_dec_stmt`,
//!   which only knows property shapes, and an element target was refused. The POSTFIX spelling
//!   already worked, which is why the gap looked like nothing at all from the test suite.
//! - The semicolon had already been consumed when the refusal happened, so the diagnostic named
//!   the NEXT statement's first token -- `echo`, `return`, `if` -- and because the parser keeps
//!   only its FIRST failure position across backtracking, the reported line could belong to a
//!   different statement entirely. Both together point a reader at innocent code.
//! - The neighbouring key is asserted alongside the incremented one: a lowering that rebuilt the
//!   inner array instead of writing one element would still get the counter right.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse nested increment fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute nested increment fragment");
    values.output.clone()
}

/// Verifies prefix `++` and `--` on a nested element accumulate.
///
/// `php -n` 8.5.6 prints `2`.
#[test]
fn prefix_increment_and_decrement_reach_a_nested_element() {
    assert_eq!(
        out(
            br#"$a = ["k" => ["c" => 1]];
++$a["k"]["c"];
++$a["k"]["c"];
--$a["k"]["c"];
echo $a["k"]["c"];"#
        ),
        "2",
    );
}

/// Verifies the Symfony shape: a COMPUTED key, then a literal one, with a sibling key intact.
///
/// `php -n` 8.5.6 prints `2;n`. `KernelTrait` writes exactly this --
/// `++$collectedLogs[$message]['count'];` -- and the sibling `note` is what says the write went
/// to one element rather than replacing the inner array.
#[test]
fn a_computed_outer_key_increments_without_disturbing_a_sibling() {
    assert_eq!(
        out(
            br#"$logs = [];
$m = "msg";
$logs[$m] = ["count" => 0, "note" => "n"];
++$logs[$m]["count"];
++$logs[$m]["count"];
echo $logs[$m]["count"], ";", $logs[$m]["note"];"#
        ),
        "2;n",
    );
}

/// Verifies the POSTFIX statement spelling reaches the same element.
///
/// `php -n` 8.5.6 prints `4`. This half ALREADY worked -- measured, not assumed: with the
/// prefix fix disabled this test is the one that stays green while the other four go red. It is
/// pinned as the reference the prefix spelling has to match, since as a statement the returned
/// value is discarded and the two must land on the same number.
#[test]
fn postfix_increment_and_decrement_reach_a_nested_element() {
    assert_eq!(
        out(
            br#"$b = ["k" => ["c" => 5]];
$b["k"]["c"]++;
$b["k"]["c"]--;
$b["k"]["c"]--;
echo $b["k"]["c"];"#
        ),
        "4",
    );
}

/// Verifies an element of a PROPERTY and of a STATIC property both increment.
///
/// `php -n` 8.5.6 prints `13;2;11`. These are element writes reached through a property, which
/// is what separates them from `++$this->count;` -- that one has had a dedicated statement all
/// along.
#[test]
fn a_property_element_and_a_static_property_element_both_increment() {
    assert_eq!(
        out(
            br#"class Bag {
    public array $m = ["k" => 1];
    public static array $t = ["s" => 10];
    public function bump(): int {
        ++$this->m["k"];
        ++self::$t["s"];
        return $this->m["k"] + self::$t["s"];
    }
}
$bag = new Bag();
echo $bag->bump(), ";", $bag->m["k"], ";", Bag::$t["s"];"#
        ),
        "13;2;11",
    );
}

/// Verifies THREE levels of index, and that a missing key autovivifies to 1.
///
/// `php -n` 8.5.6 prints `1` for each. The missing-key case also emits two
/// `Warning: Undefined array key` notices on the warning channel, which is not stdout and so is
/// not part of this expectation; the VALUE is, because a lowering that refused to create the
/// intermediate array would print nothing at all.
#[test]
fn three_index_levels_and_an_absent_key_both_increment() {
    assert_eq!(
        out(
            br#"$deep = ["a" => ["b" => ["c" => 0]]];
++$deep["a"]["b"]["c"];
echo $deep["a"]["b"]["c"], ";";
$missing = [];
++$missing["new"]["deep"];
echo $missing["new"]["deep"];"#
        ),
        "1;1",
    );
}
