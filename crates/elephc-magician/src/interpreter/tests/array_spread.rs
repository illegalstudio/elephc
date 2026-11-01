//! Purpose:
//! Interpreter tests pinning PHP's array-literal unpacking, `[...$operand]`, which the eval
//! parser accepted in CALL ARGUMENTS and in parameter lists but never inside an array literal.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::array_spread`.
//!
//! Key details:
//! - Every expected string is `php -n` 8.5.6's output on the same fragment.
//! - Unpacking is TWO rules at once and a test that only spreads a list checks neither of them:
//!   INTEGER keys are renumbered from the literal's own running counter, and STRING keys are
//!   carried through untouched. `json_encode` is the reader used here because it is the one that
//!   distinguishes a list from a hash, which is exactly what the two rules decide.
//! - The operand may be any Traversable, not only an array, so a generator, an `Iterator` and an
//!   `IteratorAggregate` are pinned beside the array cases.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse array spread fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute array spread fragment");
    values.output.clone()
}

/// Verifies a list operand unpacks in place and the elements around it keep their order.
///
/// `php -n` 8.5.6 prints `1,2,3,4;4;2`.
#[test]
fn a_list_operand_unpacks_between_its_neighbours() {
    assert_eq!(
        out(
            br#"$a = [2, 3];
$b = [1, ...$a, 4];
echo implode(",", $b), ";", count($b), ";", $b[1];"#
        ),
        "1,2,3,4;4;2",
    );
}

/// Verifies STRING keys survive unpacking and stay in source order.
///
/// `php -n` 8.5.6 prints `{"w":0,"x":1,"y":2,"z":3}`. Since PHP 8.1 a string key is carried
/// through rather than dropped, so the result is a hash and `json_encode` says so.
#[test]
fn string_keys_survive_unpacking_in_source_order() {
    assert_eq!(
        out(
            br#"$named = ["x" => 1, "y" => 2];
$merged = ["w" => 0, ...$named, "z" => 3];
echo json_encode($merged);"#
        ),
        "{\"w\":0,\"x\":1,\"y\":2,\"z\":3}",
    );
}

/// Verifies a duplicate string key is decided by POSITION, later wins.
///
/// `php -n` 8.5.6 prints `{"x":9};{"x":1}`. The two halves differ only in which side the spread
/// is on, so this catches an implementation that merged with a fixed precedence instead of
/// writing in order.
#[test]
fn a_duplicate_string_key_is_decided_by_position() {
    assert_eq!(
        out(
            br#"$dup = ["x" => 1];
$over = [...$dup, "x" => 9];
$under = ["x" => 9, ...$dup];
echo json_encode($over), ";", json_encode($under);"#
        ),
        "{\"x\":9};{\"x\":1}",
    );
}

/// Verifies INTEGER keys are renumbered, and that renumbering keeps the result a list.
///
/// `php -n` 8.5.6 prints `["a","b"]`. The operand's keys are 5 and 9; a spread that preserved
/// them would print `{"5":"a","9":"b"}`, which is the wrong SHAPE and not just the wrong keys.
#[test]
fn integer_keys_are_renumbered_and_the_result_stays_a_list() {
    assert_eq!(
        out(
            br#"$ints = [5 => "a", 9 => "b"];
echo json_encode([...$ints]);"#
        ),
        "[\"a\",\"b\"]",
    );
}

/// Verifies the integer counter is SHARED with the keyed elements around the spread.
///
/// `php -n` 8.5.6 prints `{"z":0,"0":"a","1":"b","y":9,"k":1,"2":7}`. The trailing bare `7` lands
/// at key 2 because the spread already consumed 0 and 1 -- a spread that renumbered from its own
/// private counter would put it at 0 and silently overwrite.
#[test]
fn the_integer_counter_is_shared_with_the_surrounding_elements() {
    assert_eq!(
        out(
            br#"$mixed = ["z" => 0, ...["a", "b"], "y" => 9, ...["k" => 1], 7];
echo json_encode($mixed);"#
        ),
        "{\"z\":0,\"0\":\"a\",\"1\":\"b\",\"y\":9,\"k\":1,\"2\":7}",
    );
}

/// Verifies an EMPTY operand and a conditional one contribute nothing and everything.
///
/// `php -n` 8.5.6 prints `[];0;1,2,3`. The conditional form is Symfony's
/// `FrameworkExtension`: `...(class_exists(X) ? [[...]] : [])`.
#[test]
fn an_empty_operand_and_a_conditional_one_both_unpack() {
    assert_eq!(
        out(
            br#"$only = [...[]];
echo json_encode($only), ";", count($only), ";";
$cond = [1, ...(true ? [2] : []), 3];
echo implode(",", $cond);"#
        ),
        "[];0;1,2,3",
    );
}

/// Verifies a GENERATOR operand is drained, keys included.
///
/// `php -n` 8.5.6 prints `7,8;{"n":5,"m":6}`. A generator yielding string keys keeps them, so
/// the two halves cover both unpacking rules through the traversable path as well.
#[test]
fn a_generator_operand_is_drained_with_its_keys() {
    assert_eq!(
        out(
            br#"function gen() { yield 7; yield 8; }
echo implode(",", [...gen()]), ";";
function skgen() { yield "n" => 5; yield "m" => 6; }
echo json_encode([...skgen()]);"#
        ),
        "7,8;{\"n\":5,\"m\":6}",
    );
}

/// Verifies an `Iterator` operand is driven through the foreach method sequence.
///
/// `php -n` 8.5.6 prints `["a","b","c"]`. The iterator yields integer keys 0, 10 and 20, which
/// renumbering flattens to a list -- so this also says the traversable path applies the same two
/// unpacking rules the array path does.
#[test]
fn an_iterator_operand_unpacks_through_its_methods() {
    assert_eq!(
        out(
            br#"class It implements Iterator {
    private $i = 0;
    private $data = ["a", "b", "c"];
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < count($this->data); }
    public function current(): mixed { return $this->data[$this->i]; }
    public function key(): mixed { return $this->i * 10; }
    public function next(): void { $this->i++; }
}
echo json_encode([...new It()]);"#
        ),
        "[\"a\",\"b\",\"c\"]",
    );
}

/// Verifies an `IteratorAggregate` operand unpacks through what `getIterator()` hands over.
///
/// `php -n` 8.5.6 prints `{"p":1,"q":2};{"g":3,"0":4}`. The second half is the aggregate handing
/// over a GENERATOR rather than an `Iterator`, which is a different arm of the hand-off.
///
/// TWO neighbouring gaps were found while writing this test, and neither is an unpacking
/// defect -- writing the natural version would have blamed unpacking for them.
///
/// `new ArrayIterator([...])` -- the obvious inner iterator -- does NOT work in the eval
/// interpreter: `ArrayIterator` appears in the known-class list in `interpreter/constants.rs`
/// with no implementation behind it, so the object it builds has no `rewind`/`valid`/`current`.
/// The inner iterator here is a user class instead.
///
/// `getIterator(): Iterator` -- php's own covariant narrowing, and the spelling Symfony uses --
/// was REFUSED at class declaration until the commit that follows this one, so this test first
/// landed spelling the return type `Traversable`. Both spellings now work and the narrowed one
/// is what is pinned, because it is the one real code writes.
#[test]
fn an_aggregate_operand_unpacks_through_its_inner_iterator() {
    assert_eq!(
        out(
            br#"class Pairs implements Iterator {
    private $i = 0;
    private $keys = ["p", "q"];
    private $vals = [1, 2];
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
    public function current(): mixed { return $this->vals[$this->i]; }
    public function key(): mixed { return $this->keys[$this->i]; }
    public function next(): void { $this->i++; }
}
class Agg implements IteratorAggregate {
    public function getIterator(): Iterator { return new Pairs(); }
}
echo json_encode([...new Agg()]), ";";
class AggGen implements IteratorAggregate {
    public function getIterator(): Generator { yield "g" => 3; yield 4; }
}
echo json_encode([...new AggGen()]);"#
        ),
        "{\"p\":1,\"q\":2};{\"g\":3,\"0\":4}",
    );
}
