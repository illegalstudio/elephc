//! Purpose:
//! Interpreter tests for the value sorts' `$flags` handling and cell ownership.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::builtins_sort_flags`.
//!
//! Key details:
//! - Every expected string is `php -n` 8.5.10's own output on the same fragment.

use super::super::*;
use super::support::*;

/// Verifies the value sorts bind `$flags` by name and treat an unknown flag value as a regular sort.
///
/// `SORT_NATURAL` orders `a007` before `a1` (leading zeros never break a tie), `SORT_DESC` and
/// `99` are not comparison domains so `[10, 2, 9]` sorts as numbers, and `flags:` may precede
/// `array:`.
#[test]
fn execute_program_value_sorts_follow_php_flags() {
    let program = parse_fragment(
        br#"$a = ["a7", "a007", "a1"]; sort($a, SORT_NATURAL); echo implode(",", $a) . ":";
$b = [10, 2, 9]; sort($b, SORT_DESC); echo implode(",", $b) . ":";
$c = [10, 2, 9]; sort($c, 99); echo implode(",", $c) . ":";
$d = [10, 2, 9]; sort(flags: SORT_STRING, array: $d); echo implode(",", $d) . ":";
$e = ["x" => "b10", "y" => "b9"]; asort($e, SORT_NATURAL); echo implode(",", array_keys($e)) . ":";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "a007,a1,a7:2,9,10:2,9,10:10,2,9:y,x:");
}

/// Verifies a value sort gives back the key and value reads it took from the source array.
///
/// `array_get` hands out one reference per element, so the builder must release that read and
/// leave each element at the count it started with.
#[test]
fn value_sort_releases_the_reads_it_took() {
    let mut values = FakeOps::default();
    let first = values.int(3).unwrap();
    let second = values.int(1).unwrap();
    let array = values.alloc(FakeValue::Array(vec![first, second]));
    let before = (values.refcount(first), values.refcount(second));

    let sorted =
        crate::interpreter::builtins::eval_array_sort_replacement("sort", array, 0, &mut values)
            .expect("sort");

    assert_eq!(values.get(sorted), FakeValue::Array(vec![second, first]));
    assert_eq!((values.refcount(first), values.refcount(second)), before);
}
