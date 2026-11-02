//! Purpose:
//! Interpreter tests for the `substr_count()` builtin: non-overlapping byte counting, the
//! needle -> offset -> length validation order, offset/length normalization against the
//! subject's own length, the run-time-null `$length` regression the compiled backend gets
//! wrong, named/spread/dynamic call forms, and the catchable `ValueError`/`TypeError`/
//! `ArgumentCountError` php raises.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::builtins_substr_count`.
//!
//! Key details:
//! - Every expected string is `php -n` 8.5.6's own output on the same fragment, re-measured in
//!   this session (`scratchpad/verify_sc.php`) rather than copied from an unverified spec.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse substr_count() fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute substr_count() fragment");
    values.output.clone()
}

/// Verifies plain non-overlapping counting: an empty haystack, an empty tail, and overlap that
/// must NOT be double-counted (`"aaaa"` contains `"aa"` twice, not three times).
///
/// `php -n` 8.5.6: `2:3:2:0`.
#[test]
fn substr_count_counts_non_overlapping_occurrences() {
    assert_eq!(
        out(br#"echo substr_count("hello world","o"), ":";
echo substr_count("aaa","a"), ":";
echo substr_count("aaaa","aa"), ":";
echo substr_count("","a");"#),
        "2:3:2:0",
    );
}

/// Verifies an empty needle is a catchable `ValueError`, checked before offset/length, worded
/// exactly as php words it, and that the haystack is otherwise unreferenced (`substr_count`
/// never partially executes).
///
/// `php -n` 8.5.6: `substr_count(): Argument #2 ($needle) must not be empty`.
#[test]
fn substr_count_rejects_an_empty_needle_before_offset_or_length() {
    assert_eq!(
        out(br#"try {
    substr_count("abc", "", 99, 99);
} catch (\ValueError $e) {
    echo $e->getMessage();
}"#),
        "substr_count(): Argument #2 ($needle) must not be empty",
    );
}

/// Verifies `$offset` normalization: negative wraps against `strlen($haystack)`, `$offset ===
/// strlen($haystack)` is legal and counts an empty window, and out-of-range in either direction
/// is a catchable `ValueError` naming argument #3 -- checked BEFORE `$length` (offset wins).
///
/// `php -n` 8.5.6: `1:1:2:0:` then the two `ValueError`s.
#[test]
fn substr_count_normalizes_offset_and_bounds_it_before_length() {
    assert_eq!(
        out(br#"echo substr_count("hello world","o",5), ":";
echo substr_count("hello world","o",-5), ":";
echo substr_count("hello world","o",-11), ":";
echo substr_count("hello world","o",11), ":";
try {
    substr_count("hello world","o",12);
} catch (\ValueError $e) {
    echo $e->getMessage(), ":";
}
try {
    substr_count("hello world","o",-12);
} catch (\ValueError $e) {
    echo $e->getMessage();
}"#),
        "1:1:2:0:substr_count(): Argument #3 ($offset) must be contained in argument #1 ($haystack):\
substr_count(): Argument #3 ($offset) must be contained in argument #1 ($haystack)",
    );
}

/// Verifies `$length` normalization: `null` (explicit or omitted) means "to the end", a negative
/// length is measured from the SUBJECT'S END (not from the offset), and both directions of
/// out-of-range are a catchable `ValueError` naming argument #4.
///
/// `php -n` 8.5.6: `1:0:2:0:` then the two `ValueError`s.
#[test]
fn substr_count_normalizes_length_from_the_subject_end_and_bounds_it() {
    assert_eq!(
        out(br#"echo substr_count("hello world","o",0,5), ":";
echo substr_count("hello world","o",0,0), ":";
echo substr_count("hello world","o",0,-1), ":";
echo substr_count("hello world","o",0,-11), ":";
try {
    substr_count("hello world","o",0,-12);
} catch (\ValueError $e) {
    echo $e->getMessage(), ":";
}
try {
    substr_count("hello world","o",0,12);
} catch (\ValueError $e) {
    echo $e->getMessage();
}"#),
        "1:0:2:0:substr_count(): Argument #4 ($length) must be contained in argument #1 ($haystack):\
substr_count(): Argument #4 ($length) must be contained in argument #1 ($haystack)",
    );
}

/// Verifies combined non-zero offset and negative length windows, including the case that
/// lands exactly on the subject boundary, and that a positive-length overrun is still a
/// catchable `ValueError`.
///
/// `php -n` 8.5.6: `1:0:` then a `ValueError`.
#[test]
fn substr_count_windows_a_nonzero_offset_with_a_negative_length() {
    assert_eq!(
        out(br#"echo substr_count("hello world","o",5,-1), ":";
echo substr_count("hello world","o",5,-6), ":";
try {
    substr_count("hello world","o",5,7);
} catch (\ValueError $e) {
    echo $e->getMessage();
}"#),
        "1:0:substr_count(): Argument #4 ($length) must be contained in argument #1 ($haystack)",
    );
}

/// Verifies `$offset === strlen($haystack)` with an explicit zero length counts nothing rather
/// than throwing -- the empty window at the very end of the subject is legal.
///
/// `php -n` 8.5.6: `0`.
#[test]
fn substr_count_offset_at_the_subject_end_with_zero_length_is_legal() {
    assert_eq!(out(br#"echo substr_count("hello world","o",11,0);"#), "0");
}

/// Verifies the run-time-null `$length` regression the COMPILED backend gets wrong (it decides
/// "was a length given" statically and folds a run-time `null` to zero): a `?int $length` that
/// is `null` at run time must behave exactly like an omitted argument, not like `0`.
///
/// `php -n` 8.5.6: `2:1:2`.
#[test]
fn substr_count_a_runtime_null_length_behaves_like_omitted() {
    assert_eq!(
        out(br#"function f(?int $l) { return substr_count("hello world","o",0,$l); }
echo f(null), ":";
echo f(5), ":";
$n = null;
echo substr_count("hello world","o",0,$n);"#),
        "2:1:2",
    );
}

/// Verifies a non-string needle is a catchable `TypeError` naming the given type, and a
/// non-numeric string offset is likewise a catchable `TypeError` -- not a silent zero.
///
/// `php -n` 8.5.6: `substr_count(): Argument #2 ($needle) must be of type string, array given`
/// then `substr_count(): Argument #3 ($offset) must be of type int, string given`.
#[test]
fn substr_count_rejects_a_non_string_needle_and_a_non_numeric_offset() {
    assert_eq!(
        out(br#"try {
    substr_count("aXbXc", [1]);
} catch (\TypeError $e) {
    echo $e->getMessage(), ":";
}
try {
    substr_count("abc", "b", "x");
} catch (\TypeError $e) {
    echo $e->getMessage();
}"#),
        "substr_count(): Argument #2 ($needle) must be of type string, array given:\
substr_count(): Argument #3 ($offset) must be of type int, string given",
    );
}

/// Verifies too-few and too-many arguments are catchable `ArgumentCountError`s worded exactly
/// as php words them, not an uncatchable fatal.
///
/// `php -n` 8.5.6: `substr_count() expects at least 2 arguments, 1 given` then
/// `substr_count() expects at most 4 arguments, 5 given`.
#[test]
fn substr_count_rejects_wrong_arity_with_a_catchable_argument_count_error() {
    assert_eq!(
        out(br#"try {
    substr_count("abc");
} catch (\ArgumentCountError $e) {
    echo $e->getMessage(), ":";
}
try {
    substr_count("abc","b",0,0,0);
} catch (\ArgumentCountError $e) {
    echo $e->getMessage();
}"#),
        "substr_count() expects at least 2 arguments, 1 given:\
substr_count() expects at most 4 arguments, 5 given",
    );
}

/// Verifies named arguments (in and out of source order), a partial named override that leaves
/// an earlier optional parameter at its default, argument unpacking, `call_user_func`, and
/// `function_exists` all reach the same registry entry.
///
/// `php -n` 8.5.6: `2:2:1:1:2:2`.
#[test]
fn substr_count_supports_named_spread_and_dynamic_call_forms() {
    assert_eq!(
        out(br#"echo substr_count(haystack: "hello world", needle: "o"), ":";
echo substr_count(needle: "o", haystack: "hello world"), ":";
echo substr_count("hello world","o",length: 5), ":";
echo substr_count("hello world","o",0,length: 5), ":";
echo substr_count(...["hello world","o"]), ":";
echo call_user_func("substr_count","hello world","o");"#),
        "2:2:1:1:2:2",
    );
}

/// Verifies `function_exists("substr_count")` answers true now that the builtin is registered.
///
/// `php -n` 8.5.6: `bool(true)`.
#[test]
fn substr_count_is_visible_to_function_exists() {
    let program = parse_fragment(br#"return function_exists("substr_count");"#)
        .expect("parse fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute fragment");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
