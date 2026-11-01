//! Purpose:
//! Interpreter tests for the six PCRE2 modifiers `x`, `n`, `S`, `D`, `J`, `X`,
//! which the eval bridge previously refused outright with
//! `unsupported Call expression` before it could even attempt the match.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Every expected string was captured from `php -n` 8.5.6, not inferred.
//! - The `J`/`n` cases here also exercise named-group support, matching the
//!   task's own guidance to probe naming and these modifiers together.

use super::super::*;
use super::support::*;

/// Verifies the six previously-refused modifiers (`x`, `n`, `S`, `D`, `J`,
/// `X`) compile and match instead of raising `unsupported Call expression`.
#[test]
fn execute_program_accepts_previously_refused_preg_modifiers() {
    let program = parse_fragment(
        br#"echo (preg_match('/^he(l)lo$/x', 'hello', $mx)) . ":" . ($mx[1] ?? "-") . ":";
echo (preg_match('/^he(l)lo$/n', 'hello', $mn)) . ":" . ($mn[1] ?? "-") . ":";
echo (preg_match('/^he(l)lo$/S', 'hello', $mS)) . ":" . ($mS[1] ?? "-") . ":";
echo (preg_match('/^he(l)lo$/D', 'hello', $mD)) . ":" . ($mD[1] ?? "-") . ":";
echo (preg_match('/^he(l)lo$/J', 'hello', $mJ)) . ":" . ($mJ[1] ?? "-") . ":";
echo (preg_match('/^he(l)lo$/X', 'hello', $mX)) . ":" . ($mX[1] ?? "-") . ":";
return function_exists("preg_match");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "1:l:1:-:1:l:1:l:1:l:1:l:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies each modifier's REAL semantic effect, not just acceptance:
/// `x` strips pattern whitespace and `#` comments, `D` pins `$` to the
/// strict subject end, `J` allows duplicate group names, and `n` disables
/// auto-capture for unnamed groups while named groups keep capturing.
#[test]
fn execute_program_applies_real_preg_modifier_semantics() {
    let program = parse_fragment(
        b"$rx = preg_match('/  h e l l o  # trailing comment\n/x', 'hello', $mxr);\necho \"xreal:\" . $rx . \":\";\n$rd1 = preg_match('/hello$/', \"hello\\n\");\n$rd2 = preg_match('/hello$/D', \"hello\\n\");\necho \"dollar:\" . $rd1 . \":\" . $rd2 . \":\";\n$rj = preg_match('/(?J)(?<dup>a)|(?<dup>b)/', 'b', $mj);\necho \"dup:\" . $rj . \":\" . $mj[\"dup\"] . \":\";\n$rn = preg_match('/(a)(?<w>b)/n', 'ab', $mn2);\necho \"n2:\" . $rn . \":\" . count($mn2) . \":\" . $mn2[0] . \":\" . $mn2[\"w\"] . \":\" . array_key_exists(1, $mn2) . \":\";\nreturn function_exists(\"preg_match\");",
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    // With `n`, the unnamed group `(a)` stops capturing entirely, so the
    // named group `w` becomes PCRE2's capture group 1 (renumbered) instead
    // of 2 — `$mn2` ends up with THREE entries (0, "w", 1), not two, and
    // `array_key_exists(1, ...)` is true. Verified against `php -n` 8.5.6;
    // PHP echoes `true` as "1".
    assert_eq!(
        values.output,
        "xreal:1:dollar:1:0:dup:1:b:n2:1:3:ab:b:1:"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
