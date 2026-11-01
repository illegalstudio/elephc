//! Purpose:
//! Interpreter tests for `preg_match()` named capture groups
//! (`(?P<name>...)`, `(?<name>...)`, `(?'name'...)`), which the eval bridge
//! previously matched but never surfaced under their names.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Every expected string was captured from `php -n` 8.5.6, not inferred.
//! - PHP's `$matches` writes a group's NAME immediately before its own
//!   numeric key, so assertions check both presence AND order.

use super::super::*;
use super::support::*;

/// Verifies `(?P<name>...)`, `(?<name>...)`, and `(?'name'...)` all populate
/// `$matches['name']`, with the name key written immediately before its own
/// numeric key, matching `php -n`'s `var_export()` order exactly.
#[test]
fn execute_program_dispatches_preg_match_named_groups() {
    let program = parse_fragment(
        br#"$r1 = preg_match('/^\/bar\/(?P<slug>[^\/]++)$/', "/bar/hello", $m1);
$r2 = preg_match('/^\/bar\/(?<slug>[^\/]++)$/', "/bar/hello", $m2);
$r3 = preg_match("/(?'slug'[^\/]++)/", "hello", $m3);
echo $r1 . ":" . $m1["slug"] . ":" . $r2 . ":" . $m2["slug"] . ":" . $r3 . ":" . $m3["slug"] . ":";
$r4 = preg_match('/(a)(?P<mid>b)(c)/', 'abc', $m4);
echo $r4 . ":" . array_keys($m4)[0] . "," . array_keys($m4)[1] . "," . array_keys($m4)[2] . "," . array_keys($m4)[3] . "," . array_keys($m4)[4] . ":";
echo $m4[0] . $m4[1] . $m4["mid"] . $m4[2] . $m4[3] . ":";
$r5 = preg_match('/(?P<x>zzz)/', 'abc', $m5);
echo $r5 . ":" . count($m5) . ":";
$r6 = preg_match('/(?P<slug>[a-z]+)(\d+)/', 'ab12', $m6, PREG_OFFSET_CAPTURE);
echo $r6 . ":" . $m6["slug"][0] . ":" . $m6["slug"][1] . ":" . $m6[1][0] . ":" . $m6[2][0] . ":" . $m6[2][1] . ":";
return function_exists("preg_match");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    // Order verified against `php -n` 8.5.6: a NAMED group's own key sits
    // immediately before its numeric twin, but an UNNAMED group ahead of it
    // (group 1, `(a)`) still keeps its own plain numeric slot first — the
    // name-before-number rule applies per group, not to the whole array.
    assert_eq!(
        values.output,
        "1:hello:1:hello:1:hello:1:0,1,mid,2,3:abcabbc:0:0:1:ab:0:ab:12:2:"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
