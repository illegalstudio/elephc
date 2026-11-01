//! Purpose:
//! Interpreter tests for the `substr_compare()` builtin: php's raw-byte-difference return, the
//! `ZEND_THREEWAY_COMPARE` tiebreak, the coerce-then-`$length`-then-`$offset` validation order,
//! the negative-offset CLAMP (not a throw), the run-time-null `$length`, ASCII-only case folding,
//! named/spread/dynamic call forms, and the catchable `ValueError`/`TypeError`/
//! `ArgumentCountError` php raises.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::builtins_substr_compare`.
//!
//! Key details:
//! - Every expected string is php 8.5.10's own output on the same fragment, re-measured in this
//!   session (`scratchpad/verify_scmp_tests.php`) rather than copied from an unverified spec.
//! - The one place these DELIBERATELY do not follow php is a raw Latin-1 high byte under
//!   `$case_insensitive`: php's fold there is the C library's locale-dependent `tolower()`, and
//!   elephc pins the deterministic ASCII answer php itself gives under `setlocale(LC_CTYPE,"C")`.
//!   See the module comment on `builtins/string/substr_compare.rs`; no test here asserts a
//!   Latin-1 fold in either direction, so neither behavior is silently frozen by accident.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse substr_compare() fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute substr_compare() fragment");
    values.output.clone()
}

/// Verifies the two halves of php's return value, which callers read differently and which a
/// single normalized sign would destroy: the FIRST differing byte returns `memcmp()`'s raw
/// UNSIGNED difference (`"a"` vs `"z"` is `-25`, not `-1`), while an equal prefix falls back to
/// `ZEND_THREEWAY_COMPARE` on the truncated lengths and yields exactly `-1`/`0`/`1`.
///
/// php 8.5.10: `0:-1:-25:25:-32:-1:1:3`.
#[test]
fn substr_compare_returns_a_byte_difference_but_a_threeway_length_tiebreak() {
    assert_eq!(
        out(br#"echo substr_compare("abcde","abcde",0), ":";
echo substr_compare("abcde","abcdf",0), ":";
echo substr_compare("a","z",0), ":";
echo substr_compare("z","a",0), ":";
echo substr_compare("A","a",0), ":";
echo substr_compare("abc","abcdef",0), ":";
echo substr_compare("abcdef","abc",0), ":";
echo substr_compare("abcdef","abc",3,3);"#),
        "0:-1:-25:25:-32:-1:1:3",
    );
}

/// Verifies a negative `$offset` counts back from the haystack end and CLAMPS to zero when its
/// magnitude overruns the haystack. This is the single sharpest difference from `substr_count()`,
/// which raises a `ValueError` for the same input: `substr_compare("abcdef","def",-100)` is `-3`
/// ('a' minus 'd'), proving the compare really restarted at offset zero.
///
/// php 8.5.10: `0:0:0:0:-3:1`.
#[test]
fn substr_compare_clamps_an_underflowing_negative_offset_instead_of_throwing() {
    assert_eq!(
        out(br#"echo substr_compare("abcdef","def",-3), ":";
echo substr_compare("abcdef","ef",-2), ":";
echo substr_compare("abcdef","abcdef",-6), ":";
echo substr_compare("abcdef","abcdef",-7), ":";
echo substr_compare("abcdef","def",-100), ":";
echo substr_compare("abc","a",PHP_INT_MIN);"#),
        "0:0:0:0:-3:1",
    );
}

/// Verifies `$offset === strlen($haystack)` is legal (it compares an empty window) while an
/// offset one past the end is a catchable `ValueError` naming argument #3.
///
/// php 8.5.10: `-1:0:0:` then the two `ValueError`s.
#[test]
fn substr_compare_accepts_an_offset_at_the_haystack_end_and_rejects_one_past_it() {
    assert_eq!(
        out(br#"echo substr_compare("abc","a",3), ":";
echo substr_compare("abc","",3), ":";
echo substr_compare("","",0), ":";
try {
    substr_compare("abc","a",4);
} catch (\ValueError $e) {
    echo $e->getMessage(), ":";
}
try {
    substr_compare("","",1);
} catch (\ValueError $e) {
    echo $e->getMessage();
}"#),
        "-1:0:0:substr_compare(): Argument #3 ($offset) must be contained in argument #1 ($haystack):\
substr_compare(): Argument #3 ($offset) must be contained in argument #1 ($haystack)",
    );
}

/// Verifies a `null` `$length` compares `MAX(strlen($needle), strlen($haystack) - $offset)` bytes
/// -- the LONGER operand -- which is what makes a prefix answer `-1`/`1` where an explicit
/// `$length` covering only the prefix answers `0`. A `$length` past both operands changes
/// nothing, and `$length = 0` compares equal.
///
/// php 8.5.10: `-1:0:-1:-1:0:1:0`.
#[test]
fn substr_compare_with_a_null_length_compares_the_longer_operand() {
    assert_eq!(
        out(br#"echo substr_compare("abcde","abcdef",0,null), ":";
echo substr_compare("abcde","abcdef",0,5), ":";
echo substr_compare("abcde","abcdef",0,6), ":";
echo substr_compare("abcde","abcdef",0,100), ":";
echo substr_compare("abcdef","abc",0,3), ":";
echo substr_compare("abcdef","abc",0), ":";
echo substr_compare("abcdef","abc",0,0);"#),
        "-1:0:-1:-1:0:1:0",
    );
}

/// Verifies a `?int $length` that is `null` at RUN TIME behaves exactly like an omitted argument
/// rather than like `0`. Folding it to zero would answer `0` for every call, which is precisely
/// the `0 === substr_compare(...)` shape real callers test.
///
/// php 8.5.10: `1:0:1`.
#[test]
fn substr_compare_a_runtime_null_length_behaves_like_omitted() {
    assert_eq!(
        out(br#"function f(?int $l) { return substr_compare("abcdef","abc",0,$l); }
echo f(null), ":";
echo f(3), ":";
$n = null;
echo substr_compare("abcdef","abc",0,$n);"#),
        "1:0:1",
    );
}

/// Verifies php 8 refuses a negative `$length` outright (it is NOT measured back from the end the
/// way `substr_count()`'s is) and checks it BEFORE `$offset`: a call with both arguments bad
/// reports the `$length` error, and only a call with a valid `$length` reaches the `$offset` one.
///
/// php 8.5.10: two `$length` `ValueError`s then one `$offset` `ValueError`.
#[test]
fn substr_compare_refuses_a_negative_length_before_it_bounds_the_offset() {
    assert_eq!(
        out(br#"try {
    substr_compare("abcdef","abc",0,-1);
} catch (\ValueError $e) {
    echo $e->getMessage(), ":";
}
try {
    substr_compare("abc","a",100,-1);
} catch (\ValueError $e) {
    echo $e->getMessage(), ":";
}
try {
    substr_compare("abc","a",100,1);
} catch (\ValueError $e) {
    echo $e->getMessage();
}"#),
        "substr_compare(): Argument #4 ($length) must be greater than or equal to 0:\
substr_compare(): Argument #4 ($length) must be greater than or equal to 0:\
substr_compare(): Argument #3 ($offset) must be contained in argument #1 ($haystack)",
    );
}

/// Verifies `$case_insensitive` folds ASCII letters only: `[`/`{` and `_`/`?` differ by the same
/// `0x20` an ASCII letter pair does, so a fold that tested the bit instead of the `A`-`Z` range
/// would answer `0` for them. The UTF-8 pair shows a multi-byte sequence is compared byte for
/// byte -- the lead byte cancels and the difference comes from the continuation byte.
///
/// php 8.5.10: `0:0:-32:-32:32:-32:-32`.
#[test]
fn substr_compare_case_insensitive_folds_ascii_letters_only() {
    assert_eq!(
        out(br#"echo substr_compare("Hello","hello",0,5,true), ":";
echo substr_compare("Hello","hello",0,null,true), ":";
echo substr_compare("HELLO","hello",0,5,false), ":";
echo substr_compare("[","{",0,1,true), ":";
echo substr_compare("_","?",0,1,true), ":";
echo substr_compare("\xC3\x89","\xC3\xA9",0,2,true), ":";
echo substr_compare("\xC3\x89","\xC3\xA9",0,2,false);"#),
        "0:0:-32:-32:32:-32:-32",
    );
}

/// Verifies every empty-operand combination, where only the truncated-length tiebreak can decide
/// and an empty haystack must still lose to a non-empty needle rather than compare equal.
///
/// php 8.5.10: `-1:1:1:0:0:-1:1`.
#[test]
fn substr_compare_orders_empty_operands_by_the_length_tiebreak() {
    assert_eq!(
        out(br#"echo substr_compare("","abc",0), ":";
echo substr_compare("abc","",0), ":";
echo substr_compare("abc","",1), ":";
echo substr_compare("","",0,null,true), ":";
echo substr_compare("","",0,5), ":";
echo substr_compare("","a",0,null,true), ":";
echo substr_compare("a","",0,null,true);"#),
        "-1:1:1:0:0:-1:1",
    );
}

/// Verifies bytes above `0x7F` are compared as UNSIGNED values, the way `memcmp` does. Reading
/// them as signed `char` would make `"\xFF"` sort BELOW `"\x01"` and flip the sign of every
/// comparison involving non-ASCII text.
///
/// php 8.5.10: `94:-94:254:-254`.
#[test]
fn substr_compare_compares_high_bytes_as_unsigned() {
    assert_eq!(
        out(br#"echo substr_compare("\xC3\xA9","e",0,1), ":";
echo substr_compare("e","\xC3\xA9",0,1), ":";
echo substr_compare("\xFF","\x01",0,1), ":";
echo substr_compare("\x01","\xFF",0,1);"#),
        "94:-94:254:-254",
    );
}

/// Verifies php coerces EVERY argument before the body validates any of them, so a `TypeError`
/// always wins over the `ValueError`s -- including the `?int` spelling php uses for `$length`,
/// which is NOT the plain `int` a non-nullable parameter reports.
///
/// php 8.5.10: the four `TypeError`s below, in declaration order.
#[test]
fn substr_compare_rejects_bad_types_before_it_validates_values() {
    assert_eq!(
        out(br#"try {
    substr_compare([1],"a",0);
} catch (\TypeError $e) {
    echo $e->getMessage(), ":";
}
try {
    substr_compare("a","b","x",-1);
} catch (\TypeError $e) {
    echo $e->getMessage(), ":";
}
try {
    substr_compare("a","b",99,"y");
} catch (\TypeError $e) {
    echo $e->getMessage(), ":";
}
try {
    substr_compare("a","b",0,null,[1]);
} catch (\TypeError $e) {
    echo $e->getMessage();
}"#),
        "substr_compare(): Argument #1 ($haystack) must be of type string, array given:\
substr_compare(): Argument #3 ($offset) must be of type int, string given:\
substr_compare(): Argument #4 ($length) must be of type ?int, string given:\
substr_compare(): Argument #5 ($case_insensitive) must be of type bool, array given",
    );
}

/// Verifies too-few and too-many arguments are catchable `ArgumentCountError`s worded exactly as
/// php words them, not an uncatchable fatal. `$offset` is REQUIRED, so the floor is three.
///
/// php 8.5.10: `substr_compare() expects at least 3 arguments, 2 given` then
/// `substr_compare() expects at most 5 arguments, 6 given`.
#[test]
fn substr_compare_rejects_wrong_arity_with_a_catchable_argument_count_error() {
    assert_eq!(
        out(br#"try {
    substr_compare("abc","a");
} catch (\ArgumentCountError $e) {
    echo $e->getMessage(), ":";
}
try {
    substr_compare("a","b",0,1,true,9);
} catch (\ArgumentCountError $e) {
    echo $e->getMessage();
}"#),
        "substr_compare() expects at least 3 arguments, 2 given:\
substr_compare() expects at most 5 arguments, 6 given",
    );
}

/// Verifies named arguments (in and out of source order), a partial named override that skips
/// `$length` and leaves it at its `null` default, argument unpacking, `call_user_func`, the
/// case-insensitive spelling of the function name, and the root-namespace spelling all reach the
/// same registry entry.
///
/// php 8.5.10: `0:0:0:0:0:0:0`.
#[test]
fn substr_compare_supports_named_spread_and_dynamic_call_forms() {
    assert_eq!(
        out(br#"echo substr_compare(haystack: "Hello", needle: "hello", offset: 0, length: 5, case_insensitive: true), ":";
echo substr_compare(needle: "hello", haystack: "Hello", offset: 0, case_insensitive: true), ":";
echo substr_compare("Hello","hello",0,case_insensitive: true), ":";
echo substr_compare(...["abcdef","def",-3]), ":";
echo call_user_func("substr_compare","abcdef","def",-3), ":";
echo SUBSTR_COMPARE("abcdef","def",-3), ":";
echo \substr_compare("abcdef","def",-3);"#),
        "0:0:0:0:0:0:0",
    );
}

/// Verifies the weak-typing boundary every declared parameter has: a NUMERIC string and a float
/// `$offset` coerce silently, a non-string `$haystack` stringifies, and any scalar
/// `$case_insensitive` folds through PHP truthiness.
///
/// php 8.5.10: `1:1:0:1:0`.
#[test]
fn substr_compare_applies_the_weak_scalar_coercions() {
    assert_eq!(
        out(br#"echo substr_compare("abc","b","1"), ":";
echo substr_compare("abc","b",1.0), ":";
echo substr_compare(123,"23",1), ":";
echo substr_compare("abc","b",1,null,1), ":";
echo substr_compare("abcde","abcdef",0,"5");"#),
        "1:1:0:1:0",
    );
}
