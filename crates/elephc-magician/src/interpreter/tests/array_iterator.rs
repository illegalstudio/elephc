//! Purpose:
//! Interpreter tests pinning `ArrayIterator`, which the eval interpreter knew only as a NAME in
//! `interpreter/constants.rs` -- so `new ArrayIterator([...])` built an object with no members
//! at all.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::array_iterator`.
//!
//! Key details:
//! - Every expected string is `php -n` 8.5.6's output on the same fragment.
//! - Symfony's header, parameter and attribute bags all `return new \ArrayIterator($this->…)`
//!   from `getIterator()`, so an interpreted container reaches this on the request path.
//! - The four interfaces are asserted alongside the members, because they are what makes
//!   `foreach` reach the Iterator protocol and `count()` reach `Countable::count()` at all: a
//!   complete set of members answers nothing if `instanceof` says the object is neither.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse ArrayIterator fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute ArrayIterator fragment");
    values.output.clone()
}

/// Runs one fragment and returns what it echoed plus the warnings it raised.
fn out_with_warnings(fragment: &[u8]) -> (String, Vec<String>) {
    let program = parse_fragment(fragment).expect("parse ArrayIterator fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute ArrayIterator fragment");
    (values.output.clone(), values.warnings.clone())
}

/// Verifies `count()`, `foreach`, `iterator_to_array()` and `getArrayCopy()` on one instance.
///
/// `php -n` 8.5.6 prints `2;a=1,b=2,;{"a":1,"b":2};{"a":1,"b":2}`. The four readers are pinned
/// together because each reaches the object by a different route -- `Countable`, the Iterator
/// protocol, the traversable drain, and a plain method -- and any one of them could work while
/// the others did not.
#[test]
fn count_foreach_iterator_to_array_and_get_array_copy_all_read_it() {
    assert_eq!(
        out(
            br#"$it = new ArrayIterator(["a" => 1, "b" => 2]);
echo count($it), ";";
foreach ($it as $k => $v) { echo $k, "=", $v, ","; }
echo ";";
echo json_encode(iterator_to_array($it)), ";";
echo json_encode($it->getArrayCopy());"#
        ),
        "2;a=1,b=2,;{\"a\":1,\"b\":2};{\"a\":1,\"b\":2}",
    );
}

/// Verifies the four `ArrayAccess` members, including that a write is visible to `count()`.
///
/// `php -n` 8.5.6 prints `1;set;unset;2`. The last number is the one that says `offsetSet` and
/// `offsetUnset` changed the BACKING array rather than a copy of it.
#[test]
fn array_access_reads_writes_and_unsets_the_backing_array() {
    assert_eq!(
        out(
            br#"$it = new ArrayIterator(["a" => 1, "b" => 2]);
echo $it["a"], ";";
$it["c"] = 3;
echo isset($it["c"]) ? "set" : "unset", ";";
unset($it["c"]);
echo isset($it["c"]) ? "set" : "unset", ";";
echo count($it);"#
        ),
        "1;set;unset;2",
    );
}

/// Verifies the Iterator members drive a cursor, and that it runs out.
///
/// `php -n` 8.5.6 prints `10,20,30;0=10;1=20;valid;done`. Stepping past the end has to answer
/// `valid() === false` rather than wrapping or erroring, which is what `foreach` relies on.
#[test]
fn the_iterator_members_drive_a_cursor_that_runs_out() {
    assert_eq!(
        out(
            br#"$list = new ArrayIterator([10, 20, 30]);
echo implode(",", iterator_to_array($list)), ";";
$list->rewind();
echo $list->key(), "=", $list->current(), ";";
$list->next();
echo $list->key(), "=", $list->current(), ";";
echo $list->valid() ? "valid" : "done", ";";
$list->next();
$list->next();
echo $list->valid() ? "valid" : "done";"#
        ),
        "10,20,30;0=10;1=20;valid;done",
    );
}

/// Verifies construction from an OBJECT and from nothing, and php's deprecation for the former.
///
/// `php -n` 8.5.6 prints `{"x":1,"y":2};2;0` and raises
/// `ArrayIterator::__construct(): Using an object as a backing array for ArrayIterator is
/// deprecated, as it allows violating class constraints and invariants`.
#[test]
fn construction_from_an_object_is_deprecated_and_from_nothing_is_empty() {
    let (output, warnings) = out_with_warnings(
        br#"class Bag { public $x = 1; public $y = 2; }
$obj = new ArrayIterator(new Bag());
echo json_encode($obj->getArrayCopy()), ";";
echo count($obj), ";";
$empty = new ArrayIterator();
echo count($empty);"#,
    );
    assert_eq!(output, "{\"x\":1,\"y\":2};2;0");
    assert_eq!(
        warnings,
        vec![
            "ArrayIterator::__construct(): Using an object as a backing array for ArrayIterator \
             is deprecated, as it allows violating class constraints and invariants"
                .to_string(),
        ]
    );
}

/// Verifies the four interfaces php reports, which is what every reader above depends on.
///
/// `php -n` 8.5.6 prints `ITCA`.
#[test]
fn it_reports_the_four_interfaces_php_reports() {
    assert_eq!(
        out(
            br#"$it = new ArrayIterator(["a" => 1]);
echo ($it instanceof Iterator) ? "I" : "i";
echo ($it instanceof Traversable) ? "T" : "t";
echo ($it instanceof Countable) ? "C" : "c";
echo ($it instanceof ArrayAccess) ? "A" : "a";"#
        ),
        "ITCA",
    );
}

/// Verifies the shape Symfony's bags use: an `IteratorAggregate` returning one.
///
/// `php -n` 8.5.6 prints `h=1,k=2,;2`. This is `HeaderBag::getIterator()` in miniature, and it
/// exercises the aggregate hand-off and the Iterator protocol in one call.
#[test]
fn an_aggregate_returning_one_iterates_and_counts() {
    assert_eq!(
        out(
            br#"class Bag implements IteratorAggregate, Countable {
    private array $items = ["h" => 1, "k" => 2];
    public function getIterator(): Traversable { return new ArrayIterator($this->items); }
    public function count(): int { return count($this->items); }
}
$bag = new Bag();
foreach ($bag as $k => $v) { echo $k, "=", $v, ","; }
echo ";", count($bag);"#
        ),
        "h=1,k=2,;2",
    );
}
