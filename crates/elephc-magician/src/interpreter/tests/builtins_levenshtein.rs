//! Purpose:
//! Interpreter tests for the `levenshtein()` builtin: the default unit-cost edit distance, the
//! three optional per-operation cost parameters, byte-wise (not codepoint-wise) comparison,
//! named/spread/dynamic call forms, and the catchable `TypeError`/`ArgumentCountError` php
//! raises.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::builtins_levenshtein`.
//!
//! Key details:
//! - `levenshtein()` has no shared `BuiltinContract`, no AOT registry binding, and no interpreter
//!   home file anywhere in this tree before this commit -- the compiled backend serves only the
//!   plain two-argument call shape as an injected PHP-source prelude function
//!   (`src/backend_gap_prelude.rs`), which the eval bridge cannot reach, so this whole table was
//!   built fresh against `php -n` 8.5.6 rather than adapted from an existing spec.
//! - Every expected string is `php -n` 8.5.6's own output on the same fragment, measured in this
//!   session (`scratchpad/verify_lev.php`).

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse levenshtein() fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute levenshtein() fragment");
    values.output.clone()
}

/// Verifies the default unit-cost edit distance: the canonical `"kitten"`/`"sitting"` example,
/// an empty string against a non-empty one (pure insertion/deletion), identical strings (zero),
/// and single-edit shapes (substitution, and a pure length difference).
///
/// `php -n` 8.5.6: `3:3:3:0:0:1:1:1`.
#[test]
fn levenshtein_computes_the_default_unit_cost_distance() {
    assert_eq!(
        out(br#"echo levenshtein("kitten","sitting"), ":";
echo levenshtein("","abc"), ":";
echo levenshtein("abc",""), ":";
echo levenshtein("",""), ":";
echo levenshtein("same","same"), ":";
echo levenshtein("abc","abd"), ":";
echo levenshtein("abc","ab"), ":";
echo levenshtein("ab","abc");"#),
        "3:3:3:0:0:1:1:1",
    );
}

/// Verifies the three optional cost parameters change the answer independently: a higher
/// insertion cost, a higher replacement cost (which can make substituting worse than
/// delete+insert), and a higher deletion cost.
///
/// `php -n` 8.5.6: `4:5:3`.
#[test]
fn levenshtein_applies_the_insertion_replacement_and_deletion_costs() {
    assert_eq!(
        out(br#"echo levenshtein("kitten","sitting",2,1,1), ":";
echo levenshtein("kitten","sitting",1,2,1), ":";
echo levenshtein("kitten","sitting",1,1,2);"#),
        "4:5:3",
    );
}

/// Verifies scalar coercion at the string boundary (int, float, bool all cast the way PHP casts
/// them for a `string` parameter) and byte-wise (not codepoint-wise) comparison: a 2-byte UTF-8
/// character costs 2 edits against a single ASCII byte.
///
/// `php -n` 8.5.6: `2:0:0:0:2`.
#[test]
fn levenshtein_coerces_scalars_and_compares_bytes_not_codepoints() {
    assert_eq!(
        out(br#"echo levenshtein(12321,"123"), ":";
echo levenshtein(1.5,"1.5"), ":";
echo levenshtein(true,"1"), ":";
echo levenshtein(false,""), ":";
echo levenshtein("h" . chr(0xc3) . chr(0xa9) . "llo","hello");"#),
        "2:0:0:0:2",
    );
}

/// Verifies a non-string second argument is a catchable `TypeError` naming the given type, and
/// that a negative or zero cost is accepted arithmetically (php never validates the cost
/// parameters' range).
///
/// `php -n` 8.5.6: `levenshtein(): Argument #2 ($string2) must be of type string, array given`
/// then `2:2`.
#[test]
fn levenshtein_rejects_a_non_string_argument_but_accepts_any_cost_value() {
    assert_eq!(
        out(br#"try {
    levenshtein("abc", [1]);
} catch (\TypeError $e) {
    echo $e->getMessage(), ":";
}
echo levenshtein("abc","b",-1), ":";
echo levenshtein("abc","b",0);"#),
        "levenshtein(): Argument #2 ($string2) must be of type string, array given:2:2",
    );
}

/// Verifies both arity failures are a catchable `ArgumentCountError`, worded exactly as php
/// words it, not an uncatchable fatal.
///
/// `php -n` 8.5.6: `levenshtein() expects at least 2 arguments, 1 given` then
/// `levenshtein() expects at most 5 arguments, 6 given`.
#[test]
fn levenshtein_rejects_wrong_arity_with_a_catchable_argument_count_error() {
    assert_eq!(
        out(br#"try {
    levenshtein("abc");
} catch (\ArgumentCountError $e) {
    echo $e->getMessage(), ":";
}
try {
    levenshtein("kitten","sitting",0,0,0,0);
} catch (\ArgumentCountError $e) {
    echo $e->getMessage();
}"#),
        "levenshtein() expects at least 2 arguments, 1 given:\
levenshtein() expects at most 5 arguments, 6 given",
    );
}

/// Verifies named arguments, argument unpacking, and `call_user_func` all reach the same
/// registry entry.
///
/// `php -n` 8.5.6: `3:3:3`.
#[test]
fn levenshtein_supports_named_spread_and_dynamic_call_forms() {
    assert_eq!(
        out(br#"echo levenshtein(string1: "kitten", string2: "sitting"), ":";
echo levenshtein(...["kitten","sitting"]), ":";
echo call_user_func("levenshtein","kitten","sitting");"#),
        "3:3:3",
    );
}

/// Verifies `function_exists("levenshtein")` answers true now that the builtin is registered in
/// the eval bridge (it was previously reachable ONLY through the compiled backend's own
/// two-argument prelude wrapper, which `eval()` cannot see at all).
///
/// `php -n` 8.5.6: `bool(true)`.
#[test]
fn levenshtein_is_visible_to_function_exists() {
    let program =
        parse_fragment(br#"return function_exists("levenshtein");"#).expect("parse fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute fragment");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
