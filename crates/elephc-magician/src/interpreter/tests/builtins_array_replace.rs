//! Purpose:
//! Interpreter tests for the `array_replace()` builtin: variadic arity, int-key matching
//! (never renumbering, unlike `array_merge`), the list-vs-assoc shape of the fold result,
//! independence from its source arrays, and the catchable `TypeError`/`ArgumentCountError`
//! php raises for a non-array argument or the wrong arity.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::builtins_array_replace`.
//!
//! Key details:
//! - Every expected string is `php -n` 8.5.6's own output on the same fragment, re-measured in
//!   this session (`scratchpad/ar_verify.php`) rather than copied from an unverified spec.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse array_replace() fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute array_replace() fragment");
    values.output.clone()
}

/// Serializer used by every row below, verbatim from `scratchpad/ar_verify.php`: prints key
/// int-vs-string-ness explicitly so a mis-renumbered key is visible without `array_is_list()`
/// (itself still `EVAL_IMPLEMENTATION_PENDING`).
const SER_FN: &[u8] = br#"function ser($a) {
    $s = "";
    foreach ($a as $k => $v) {
        $s .= (is_int($k) ? "i" : "s") . ":" . $k . "=";
        if (is_array($v)) { $s .= "[" . ser($v) . "]"; }
        elseif (is_null($v)) { $s .= "NULL"; }
        elseif (is_bool($v)) { $s .= $v ? "true" : "false"; }
        elseif (is_string($v)) { $s .= "'" . $v . "'"; }
        else { $s .= $v; }
        $s .= ";";
    }
    return $s;
}
"#;

/// Verifies the two-argument case: later wins, an overwrite keeps its original slot position,
/// and a brand-new key is appended at the end.
///
/// `php -n` 8.5.6: `s:a=1;s:b=3;s:c=4;`.
#[test]
fn array_replace_overwrites_in_place_and_appends_new_keys() {
    let mut fragment = SER_FN.to_vec();
    fragment.extend_from_slice(
        br#"echo ser(array_replace(["a"=>1,"b"=>2], ["b"=>3,"c"=>4]));"#,
    );
    assert_eq!(out(&fragment), "s:a=1;s:b=3;s:c=4;");
}

/// Verifies int keys are MATCHED, not renumbered -- the opposite of `array_merge` -- and that a
/// sparse int-keyed source keeps its keys verbatim with a brand-new key appended at the end, not
/// sorted into place.
///
/// `php -n` 8.5.6: `i:0=4;i:1=5;i:2=3;` then `i:5='a';i:9='c';i:2='d';`.
#[test]
fn array_replace_matches_int_keys_without_renumbering() {
    let mut fragment = SER_FN.to_vec();
    fragment.extend_from_slice(
        br#"echo ser(array_replace([1,2,3], [4,5])), ":";
echo ser(array_replace([5=>"a",9=>"b"], [9=>"c",2=>"d"]));"#,
    );
    assert_eq!(out(&fragment), "i:0=4;i:1=5;i:2=3;:i:5='a';i:9='c';i:2='d';");
}

/// Verifies a single argument is legal and returns a copy, that all-empty arguments (any count)
/// are legal, and that more than two arguments fold left to right with the last write winning.
///
/// `php -n` 8.5.6: `s:a=1;` then empty then `s:a=3;s:b=2;s:c=4;`.
#[test]
fn array_replace_accepts_one_argument_and_folds_more_than_two() {
    let mut fragment = SER_FN.to_vec();
    fragment.extend_from_slice(
        br#"echo ser(array_replace(["a"=>1])), ":";
echo ser(array_replace([],[])), ":";
echo ser(array_replace(["a"=>1],["b"=>2],["a"=>3],["c"=>4]));"#,
    );
    assert_eq!(out(&fragment), "s:a=1;::s:a=3;s:b=2;s:c=4;");
}

/// Verifies nested arrays are replaced WHOLESALE, not merged recursively, and that a `null`
/// value replaces rather than being treated as a delete/skip.
///
/// `php -n` 8.5.6: `s:a=[s:y=3;];` then `s:a=NULL;s:b=2;`.
#[test]
fn array_replace_is_not_recursive_and_a_null_value_still_replaces() {
    let mut fragment = SER_FN.to_vec();
    fragment.extend_from_slice(
        br#"echo ser(array_replace(["a"=>["x"=>1,"y"=>2]], ["a"=>["y"=>3]])), ":";
echo ser(array_replace(["a"=>1,"b"=>2], ["a"=>null]));"#,
    );
    assert_eq!(out(&fragment), "s:a=[s:y=3;];:s:a=NULL;s:b=2;");
}

/// Verifies mixed key kinds fold correctly (a packed list gaining a string key) and that
/// insertion order survives even when it disagrees with numeric order.
///
/// `php -n` 8.5.6: `i:0=1;i:1=2;i:2=3;s:a=9;` then `i:1='b';i:0='a';`.
#[test]
fn array_replace_mixes_key_kinds_and_preserves_insertion_order() {
    let mut fragment = SER_FN.to_vec();
    fragment.extend_from_slice(
        br#"echo ser(array_replace([1,2,3], ["a"=>9])), ":";
echo ser(array_replace([1=>"b",0=>"a"], []));"#,
    );
    assert_eq!(out(&fragment), "i:0=1;i:1=2;i:2=3;s:a=9;:i:1='b';i:0='a';");
}

/// Verifies the list-shape guard (the sentinel this family lives or dies on): the result's tag
/// must follow the FINAL key sequence, not any input's representation. `array_replace([0=>"p"],
/// [1=>"q"])` and `array_replace([1=>"q"], [0=>"p"])` hold the SAME key set `{0,1}` in opposite
/// orders and must land on opposite sides of `json_encode`'s `[...]` vs `{...}` choice. A test
/// asserting only the list-shaped half passes against a half-fix that always tags the result
/// assoc (`array_merge`'s own latent bug, out of scope here, but not to be copied).
///
/// `php -n` 8.5.6: `[4,5,3]|{"1":"q","0":"p"}`.
#[test]
fn array_replace_result_tag_follows_the_final_key_sequence_not_any_input() {
    assert_eq!(
        out(br#"echo json_encode(array_replace([1,2,3],[4,5])), "|", json_encode(array_replace([1=>"q"],[0=>"p"]));"#),
        "[4,5,3]|{\"1\":\"q\",\"0\":\"p\"}",
    );
}

/// Verifies every value type is carried through unchanged, and that the result is an
/// independent array: mutating the FIRST source array after the call does not change what the
/// result already copied out of it.
///
/// `php -n` 8.5.6: `s:a='s';s:b=1.5;s:c=false;s:d=NULL;s:e=[i:0=1;i:1=2;];` then `1`.
///
/// NOT covered here: `php -n` also keeps a PHP reference held by a source array element LIVE in
/// the result (`$a = ["k"=>&$x]; $r = array_replace($a,["j"=>2]); $x = 99;` makes `$r["k"]` read
/// `99`, not the value at call time). `values.array_get()` reads a plain snapshot value with no
/// concept of "this slot is a reference," so elephc's result holds `1` (the value at call time)
/// instead. This is not specific to `array_replace` -- `array_merge`'s own `array_get`/`array_set`
/// fold has the identical shape -- so it is a pre-existing, shared gap in how every array-folding
/// builtin here handles a referenced element, not a regression this function introduces. Fixing
/// it would mean teaching the fold family about reference-carrying array slots, which is out of
/// this lot's scope.
#[test]
fn array_replace_carries_every_value_type_and_is_independent_of_its_source() {
    let mut fragment = SER_FN.to_vec();
    fragment.extend_from_slice(
        br#"echo ser(array_replace(["a"=>"s"], ["b"=>1.5,"c"=>false,"d"=>null,"e"=>[1,2]])), ":";
$x = 1;
$a = ["k" => &$x];
$r = array_replace($a, ["j" => 2]);
$x = 99;
echo $r["k"];"#,
    );
    assert_eq!(
        out(&fragment),
        "s:a='s';s:b=1.5;s:c=false;s:d=NULL;s:e=[i:0=1;i:1=2;];:1",
    );
}

/// Verifies the failure table: zero arguments is a catchable `ArgumentCountError` worded exactly
/// as php words it (not elephc's derived "takes exactly 2 arguments"), and a non-array argument
/// at every position is a catchable `TypeError` -- position 1 carries its parameter name, every
/// variadic position after it does not, booleans are spelled `true`/`false` (not `bool`), and an
/// object is named by its class. The check is left-to-right and eager: the SECOND bad argument
/// in a call is never reached once the first one throws.
///
/// `php -n` 8.5.6, in order:
/// `ArgumentCountError: array_replace() expects at least 1 argument, 0 given`,
/// `TypeError: array_replace(): Argument #1 ($array) must be of type array, string given`,
/// `TypeError: array_replace(): Argument #1 ($array) must be of type array, null given`,
/// `TypeError: array_replace(): Argument #2 must be of type array, string given`,
/// `TypeError: array_replace(): Argument #2 must be of type array, stdClass given`,
/// `TypeError: array_replace(): Argument #2 must be of type array, true given`,
/// `TypeError: array_replace(): Argument #2 must be of type array, false given`,
/// `TypeError: array_replace(): Argument #2 must be of type array, int given`,
/// `TypeError: array_replace(): Argument #3 must be of type array, string given`,
/// `TypeError: array_replace(): Argument #4 must be of type array, int given`.
#[test]
fn array_replace_rejects_wrong_arity_and_non_array_arguments_left_to_right() {
    let fragment = br#"function probe($fn) {
    try { $fn(); } catch (\Throwable $e) { return get_class($e) . ": " . $e->getMessage(); }
    return "no-throw";
}
echo probe(fn() => array_replace()), ":";
echo probe(fn() => array_replace("x", ["a"=>1])), ":";
echo probe(fn() => array_replace(null)), ":";
echo probe(fn() => array_replace(["a"=>1], "x")), ":";
echo probe(fn() => array_replace([], new stdClass())), ":";
echo probe(fn() => array_replace([], true)), ":";
echo probe(fn() => array_replace([], false)), ":";
echo probe(fn() => array_replace(["k"=>1], 1, [])), ":";
echo probe(fn() => array_replace([], [], "x")), ":";
echo probe(fn() => array_replace([], [], [], 42));"#;
    assert_eq!(
        out(fragment),
        "ArgumentCountError: array_replace() expects at least 1 argument, 0 given:\
TypeError: array_replace(): Argument #1 ($array) must be of type array, string given:\
TypeError: array_replace(): Argument #1 ($array) must be of type array, null given:\
TypeError: array_replace(): Argument #2 must be of type array, string given:\
TypeError: array_replace(): Argument #2 must be of type array, stdClass given:\
TypeError: array_replace(): Argument #2 must be of type array, true given:\
TypeError: array_replace(): Argument #2 must be of type array, false given:\
TypeError: array_replace(): Argument #2 must be of type array, int given:\
TypeError: array_replace(): Argument #3 must be of type array, string given:\
TypeError: array_replace(): Argument #4 must be of type array, int given",
    );
}

/// Verifies named arguments, argument unpacking, `call_user_func`, and a variable-function call
/// all reach the same registry entry.
///
/// `php -n` 8.5.6: `s:a=1;:s:a=9;s:b=2;:s:a=1;`.
#[test]
fn array_replace_supports_named_spread_and_dynamic_call_forms() {
    let mut fragment = SER_FN.to_vec();
    fragment.extend_from_slice(
        br#"echo ser(array_replace(array: ["a"=>1])), ":";
echo ser(array_replace(...[["a"=>1],["b"=>2],["a"=>9]])), ":";
echo ser(call_user_func("array_replace", ["a"=>1]));"#,
    );
    assert_eq!(out(&fragment), "s:a=1;:s:a=9;s:b=2;:s:a=1;");
}

/// Verifies `function_exists("array_replace")` answers true now that the builtin is registered.
///
/// `php -n` 8.5.6: `bool(true)`.
#[test]
fn array_replace_is_visible_to_function_exists() {
    let program = parse_fragment(br#"return function_exists("array_replace");"#)
        .expect("parse fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute fragment");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
