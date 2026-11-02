//! Purpose:
//! Interpreter tests for the `parse_str()` builtin: the mandatory by-reference `$result`
//! out-parameter, PHP's query-string grammar (separator, NUL truncation, decode-before-bracket,
//! leading-space stripping, root mangling, bracket append/whitespace/trailing-garbage rules, the
//! two-outcome unterminated-bracket rule, integer-key canonicalization and the negative/overflow
//! append rule, and the `max_input_vars` limit), every reference-target shape php allows, and the
//! catchable `ArgumentCountError`/`TypeError` php raises.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::builtins_parse_str`.
//!
//! Key details:
//! - Every expected string is `php -n` 8.5.6's own output on the same fragment, re-measured in
//!   this session (`scratchpad/verify_ps.php`, `scratchpad/verify_nest.php`) rather than copied
//!   from an unverified spec -- the existing (unregistered) implementation this file replaces
//!   was independently wrong on several of these rules (splits on `;`, drops an unterminated
//!   bracket instead of flat-mangling or truncating it, mangles leading spaces instead of
//!   stripping them, and does not truncate at NUL).
//! - `arg_separator.input` is not modeled (the interpreter has no ini storage anywhere); only the
//!   default separator `&` is supported, which is what every measured case below uses.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse parse_str() fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute parse_str() fragment");
    values.output.clone()
}

/// Verifies the basic grammar: `&` (never `;`) separates fields, an empty subject yields an
/// empty array, a missing `=` yields an empty-string value, only the FIRST `=` splits name from
/// value, and empty fields are skipped without needing to be counted.
///
/// `php -n` 8.5.6: `1=1&2=2:1;b=2:0:a=:a=b=c:1,1`.
#[test]
fn parse_str_splits_only_on_ampersand_and_handles_missing_equals() {
    assert_eq!(
        out(br#"parse_str("a=1&b=2", $r1); echo $r1["a"], "=1&", $r1["b"], "=2:";
parse_str("a=1;b=2", $r2); echo $r2["a"], ":";
parse_str("", $r3); echo count($r3), ":";
parse_str("a", $r4); echo "a=", $r4["a"], ":";
parse_str("a=b=c", $r5); echo "a=", $r5["a"], ":";
parse_str("&&a=1&&", $r6); echo count($r6), ",", $r6["a"];"#),
        "1=1&2=2:1;b=2:0:a=:a=b=c:1,1",
    );
}

/// Verifies the whole parse stops at the FIRST NUL byte in the source -- not just the current
/// field -- so a NUL before any `=` is later treated as a no-`=` token.
///
/// `php -n` 8.5.6: `0` for a subject that is nothing but a leading NUL, and `a=` then `a=1` for
/// the two truncated-mid-token/truncated-after-a-field cases.
#[test]
fn parse_str_truncates_the_whole_parse_at_the_first_nul() {
    // `chr(0)` builds the NUL byte in PHP source rather than embedding a literal NUL in this
    // Rust file, which a raw byte string (`br#"..."#`) would not even decode as an escape.
    assert_eq!(
        out(br#"parse_str(chr(0) . "a=1", $r1); echo count($r1), ":";
parse_str("a" . chr(0) . "b=1", $r2); echo "a=", $r2["a"], ":";
parse_str("a=1" . chr(0) . "b=2", $r3); echo "a=", $r3["a"];"#),
        "0:a=:a=1",
    );
}

/// Verifies decode-before-bracket-parse: percent/`+` decoding happens on name AND value before
/// the bracket grammar runs, so a percent-encoded `[`/`]` in the NAME still opens a real bracket
/// group, and a doubly-encoded percent decodes only once.
///
/// `php -n` 8.5.6: `c d:1:%61=x`.
#[test]
fn parse_str_decodes_before_parsing_brackets() {
    assert_eq!(
        out(br#"parse_str("a+b=c+d", $r1); echo $r1["a_b"], ":";
parse_str("a%5Bb%5D=1", $r2); echo $r2["a"]["b"], ":";
parse_str("%2561=x", $r3); echo key($r3), "=", $r3["%61"];"#),
        "c d:1:%61=x",
    );
}

/// Verifies leading spaces are stripped from the front of the NAME after decoding (not mangled
/// to `_`), tab and newline are left untouched, and a name that is entirely a dropped leading
/// space over an empty root drops the whole field.
///
/// `php -n` 8.5.6: `a=1:a=1:a=1: (empty)`.
#[test]
fn parse_str_strips_leading_spaces_from_the_name_but_not_other_whitespace() {
    let program = parse_fragment(
        br#"parse_str(" a=1", $r1); echo key($r1), "=", $r1["a"], ":";
parse_str("%20a=1", $r2); echo key($r2), "=", $r2["a"], ":";
parse_str("+a=1", $r3); echo key($r3), "=", $r3["a"], ":";
parse_str("\ta=1", $r4); echo isset($r4["\ta"]) ? "kept-tab" : "bad", ":";
parse_str(" [a]=1", $r5); echo count($r5);"#,
    )
    .expect("parse fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute fragment");
    assert_eq!(values.output, "a=1:a=1:a=1:kept-tab:0");
}

/// Verifies root mangling: `' '` and `'.'` before the first `[` become `'_'`, and the SAME two
/// bytes inside a bracket segment are left untouched.
///
/// `php -n` 8.5.6: `a_b=1:a_b=1:a_b=1:b.c=1`.
#[test]
fn parse_str_mangles_space_and_dot_in_the_root_but_not_inside_brackets() {
    assert_eq!(
        out(br#"parse_str("a.b=1", $r1); echo key($r1), "=", $r1["a_b"], ":";
parse_str("a b=1", $r2); echo key($r2), "=", $r2["a_b"], ":";
parse_str("a.b[c.d]=1", $r3); echo key($r3), "=", $r3["a_b"]["c.d"], ":";
parse_str("a[b.c]=1", $r4); echo key($r4["a"]), "=", $r4["a"]["b.c"];"#),
        "a_b=1:a_b=1:a_b=1:b.c=1",
    );
}

/// Verifies bracket-segment append and whitespace rules: an empty segment `[]` appends, exactly
/// ONE whitespace byte (space or tab) inside a segment ALSO appends, but two spaces are a
/// literal key, and content after the last closed `]` is silently ignored.
///
/// `php -n` 8.5.6: `0=1:0=1:1=1:b=1`.
#[test]
fn parse_str_bracket_append_and_trailing_ws_and_garbage_rules() {
    let program = parse_fragment(
        br#"parse_str("a[]=1", $r1); echo key($r1["a"]), "=", $r1["a"][0], ":";
parse_str("a[ ]=1", $r2); echo key($r2["a"]), "=", $r2["a"][0], ":";
parse_str("a[  ]=1", $r3); echo $r3["a"]["  "], "=1", ":";
parse_str("a[b]c=1", $r4); echo key($r4["a"]), "=", $r4["a"]["b"];"#,
    )
    .expect("parse fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute fragment");
    assert_eq!(values.output, "0=1:0=1:1=1:b=1");
}

/// Verifies R9's TWO different unterminated-bracket outcomes: a failure in the FIRST segment
/// flat-mangles the WHOLE name (every space/dot/`[` becomes `_`, no array is produced), while a
/// failure in a LATER segment drops only the unparsed tail and keeps the path built so far.
///
/// `php -n` 8.5.6: `a_b=1:a_b_c=1:a_b_c=1: a=[b=>1] : x=[a=>1]`.
#[test]
fn parse_str_unterminated_bracket_differs_by_which_segment_fails() {
    let program = parse_fragment(
        br#"parse_str("a[b=1", $r1); echo key($r1), "=", $r1["a_b"], ":";
parse_str("a[b.c=1", $r2); echo key($r2), "=", $r2["a_b_c"], ":";
parse_str("a[b[c=1", $r3); echo key($r3), "=", $r3["a_b_c"], ":";
parse_str("a[b][c[d=1", $r4); echo key($r4["a"]), "=", $r4["a"]["b"], ":";
parse_str("x[a][b=1", $r5); echo key($r5["x"]), "=", $r5["x"]["a"];"#,
    )
    .expect("parse fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute fragment");
    assert_eq!(values.output, "a_b=1:a_b_c=1:a_b_c=1:b=1:a=1");
}

/// Verifies key canonicalization -- a canonical decimal integer string becomes an INT array key,
/// a leading-zero or negative-looking-but-non-canonical string stays a STRING key -- and the
/// `[]` append rule: it uses `max(existing int keys) + 1`, INCLUDING negative keys, and starts at
/// `0` when there are none.
///
/// `php -n` 8.5.6: `ix:sx:ix:i1,i2,:i-5,i-4` (the `kind()` helper prefixes the VALUE with the
/// key's `is_int()` verdict, so `x` is the value echoed back, not the key).
#[test]
fn parse_str_canonicalizes_integer_keys_and_appends_past_the_max_including_negative() {
    let program = parse_fragment(
        br#"function kind($arr, $key) { return (is_int($key) ? "i" : "s") . $arr[$key]; }
parse_str("a[1]=x", $r1); echo kind($r1["a"], key($r1["a"])), ":";
parse_str("a[01]=x", $r2); echo kind($r2["a"], key($r2["a"])), ":";
parse_str("a[-1]=x", $r3); echo kind($r3["a"], key($r3["a"])), ":";
parse_str("a[1]=x&a[]=y", $r4);
foreach ($r4["a"] as $k => $v) { echo (is_int($k) ? "i" : "s"), $k, ","; }
echo ":";
parse_str("a[-5]=x&a[]=y", $r5);
$out = [];
foreach ($r5["a"] as $k => $v) { $out[] = (is_int($k) ? "i" : "s") . $k; }
echo implode(",", $out);"#,
    )
    .expect("parse fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute fragment");
    assert_eq!(values.output, "ix:sx:ix:i1,i2,:i-5,i-4");
}

/// Verifies `PHP_INT_MAX` as an existing int key makes a subsequent `[]` append SILENTLY
/// dropped -- no warning, no overwrite -- rather than wrapping or clobbering the existing
/// element.
///
/// `php -n` 8.5.6: `1` element, still keyed `PHP_INT_MAX`.
#[test]
fn parse_str_append_past_php_int_max_is_silently_dropped() {
    let program = parse_fragment(
        br#"parse_str("a[9223372036854775807]=x&a[]=y", $r);
echo count($r["a"]), ":", $r["a"][PHP_INT_MAX];"#,
    )
    .expect("parse fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute fragment");
    assert_eq!(values.output, "1:x");
}

/// Verifies merging rules when a path is written twice: scalar -> array replaces, array ->
/// scalar replaces, and a deeper write on either side replaces the shallower one.
///
/// `php -n` 8.5.6: `b=2:a=2:c=2`.
#[test]
fn parse_str_reusing_a_path_replaces_scalar_and_array_both_ways() {
    let program = parse_fragment(
        br#"parse_str("a=1&a[b]=2", $r1); echo key($r1["a"]), "=", $r1["a"]["b"], ":";
parse_str("a[b]=1&a=2", $r2); echo "a=", $r2["a"], ":";
parse_str("a[b][c]=1&a[b]=2", $r3); echo "c=", $r3["a"]["b"];"#,
    )
    .expect("parse fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute fragment");
    assert_eq!(values.output, "b=2:a=2:c=2");
}

/// Verifies the nesting depth limit (default `max_input_nesting_level` 64): a name with 64
/// bracket segments still parses to its full depth, and 65 silently drops that one variable
/// while every OTHER field in the same query still lands.
///
/// `php -n` 8.5.6, measured with `scratchpad/verify_ps_nest2.php`: starting the depth count from
/// `$r1["a"]` (one level below the root), 64 brackets reach depth 64, and 65 brackets (66
/// segments counting the root) drop the field entirely with no warning.
#[test]
fn parse_str_caps_bracket_nesting_at_the_default_level() {
    let mut ok_brackets = String::new();
    for _ in 0..64 {
        ok_brackets.push_str("[x]");
    }
    let mut deep_brackets = String::new();
    for _ in 0..65 {
        deep_brackets.push_str("[x]");
    }
    let fragment = format!(
        r#"parse_str("ok=1&a{ok_brackets}=deep", $r1);
$d = 0; $cur = $r1["a"];
while (is_array($cur)) {{ $cur = reset($cur); $d++; }}
echo $d, ":", $cur, ":";
parse_str("ok=1&a{deep_brackets}=deep", $r2);
echo isset($r2["a"]) ? "bad" : "dropped", ":", $r2["ok"];"#
    );
    let program = parse_fragment(fragment.as_bytes()).expect("parse fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute fragment");
    assert_eq!(values.output, "64:deep:dropped:1");
}

/// Verifies `max_input_vars` (default 1000): the 1001st field is dropped and every earlier one
/// survives, with empty fields never counting toward the limit at all.
///
/// `php -n` 8.5.6: exactly 1000 elements, `k999` present, `k1000` absent.
#[test]
fn parse_str_caps_the_field_count_at_the_default_limit() {
    let mut fields = Vec::with_capacity(1001);
    for i in 0..1001 {
        fields.push(format!("k{i}={i}"));
    }
    let fragment = format!(
        r#"parse_str("{}", $r);
echo count($r), ":", isset($r["k999"]) ? "y" : "n", ":", isset($r["k1000"]) ? "y" : "n";"#,
        fields.join("&")
    );
    let program = parse_fragment(fragment.as_bytes()).expect("parse fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute fragment");
    assert_eq!(values.output, "1000:y:n");
}

/// Verifies every reference-target shape php allows for `$result`: an undefined variable (it
/// must be CREATED), an object property, and a nested array element -- and that a prior value at
/// the target is discarded rather than merged into.
///
/// `php -n` 8.5.6: `NULL:x=1:x=1:x=1`.
#[test]
fn parse_str_writes_every_reference_target_shape() {
    let program = parse_fragment(
        br#"$dumped = parse_str("a=1", $undefinedVar) === null ? "NULL" : "bad";
echo $dumped, ":";
class Box { public $p; }
$obj = new Box();
parse_str("x=1", $obj->p);
echo "x=", $obj->p["x"], ":";
$arr = ["slot" => "old"];
parse_str("x=1", $arr["slot"]);
echo "x=", $arr["slot"]["x"], ":";
$prior = ["keep" => "me"];
parse_str("x=1", $prior);
echo "x=", $prior["x"];"#,
    )
    .expect("parse fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute fragment");
    assert_eq!(values.output, "NULL:x=1:x=1:x=1");
}

/// Verifies a numeric non-string first argument coerces (an int becomes its decimal string,
/// which then parses normally as a bare token with no `=`), a non-string, non-numeric second
/// argument shape (an array) is a catchable `TypeError`, and the wrong arity is a catchable
/// `ArgumentCountError` -- not an uncatchable fatal for any of the three.
///
/// `php -n` 8.5.6: `i12345=:` then a `TypeError` then an `ArgumentCountError`.
#[test]
fn parse_str_coerces_a_numeric_argument_and_rejects_bad_shapes() {
    assert_eq!(
        out(br#"parse_str(12345, $r);
echo (is_int(key($r)) ? "i" : "s"), key($r), "=", $r[12345], ":";
try {
    parse_str([1], $bad);
} catch (\TypeError $e) {
    echo $e->getMessage(), ":";
}
try {
    parse_str("a=1");
} catch (\ArgumentCountError $e) {
    echo $e->getMessage();
}"#),
        "i12345=:parse_str(): Argument #1 ($string) must be of type string, array given:\
parse_str() expects exactly 2 arguments, 1 given",
    );
}

/// Verifies named arguments reach the same by-reference machinery as a plain positional call.
///
/// `php -n` 8.5.6: `a=1`.
#[test]
fn parse_str_supports_named_arguments() {
    assert_eq!(
        out(br#"parse_str(string: "a=1", result: $out);
echo "a=", $out["a"];"#),
        "a=1",
    );
}

/// Verifies `function_exists("parse_str")` answers true now that the builtin carries a
/// registry entry -- the interpreter could already EXECUTE `parse_str()` through the
/// hard-coded `eval_call` ladder before this fix, but reported itself as not knowing the
/// function, which is exactly the shape that makes it invisible to code (like Symfony) that
/// checks `function_exists()` before calling.
///
/// `php -n` 8.5.6: `bool(true)`.
#[test]
fn parse_str_is_visible_to_function_exists() {
    let program =
        parse_fragment(br#"return function_exists("parse_str");"#).expect("parse fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute fragment");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
