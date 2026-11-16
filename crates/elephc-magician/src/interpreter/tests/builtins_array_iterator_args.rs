//! Purpose:
//! Interpreter tests for `ArrayIterator`'s constructor argument check and its offset writes.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::builtins_array_iterator_args`.
//!
//! Key details:
//! - Expected strings are `php -n` 8.5.10's own output on the same fragments.

use super::super::*;
use super::support::*;

/// Verifies a `null` or scalar backing array is php's catchable TypeError, not an empty iterator.
#[test]
fn array_iterator_constructor_rejects_null_and_scalars_with_a_type_error() {
    let program = parse_fragment(
        br#"try { new ArrayIterator(null); } catch (TypeError $e) { echo $e->getMessage() . "|"; }
try { new ArrayIterator("x"); } catch (TypeError $e) { echo $e->getMessage(); }
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "ArrayIterator::__construct(): Argument #1 ($array) must be of type array, null given|\
         ArrayIterator::__construct(): Argument #1 ($array) must be of type array, string given"
    );
}

/// Verifies `offsetSet()` and `offsetUnset()` keep the entries they are not changing.
///
/// Both rebuild or extend the backing array while holding their own references to the key and
/// value; running them under the fake runtime's counting proves they give those back.
#[test]
fn array_iterator_offset_writes_balance_their_references() {
    let program = parse_fragment(
        br#"$it = new ArrayIterator(["a" => 1, "b" => 2]);
$it["c"] = 3;
$it[] = 4;
$it->offsetUnset("a");
echo count($it) . ":" . $it["b"] . $it["c"] . $it[0];
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "3:234");
    assert!(values.over_releases().is_empty());
}
