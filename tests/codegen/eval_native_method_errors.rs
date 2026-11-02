//! Purpose:
//! Regressions for errors php raises when interpreted code calls a missing method on a compiled
//! object.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval_native_method_errors` through Rust's test harness.
//!
//! Key details:
//! - The generated method helper answered a name the object's class lacks with a bare failure and
//!   no pending throwable. The interpreter turned that into an uncatchable `RuntimeFatal`
//!   ("eval callback dispatch failed"), so `try { $o->nope(); } catch (Error $e) { ... }` in
//!   interpreted code could not catch what php throws as a plain `Error`.
//! - Expected output is real php 8.5 output.

use crate::support::compile_and_run;

/// Verifies a missing method on a compiled object is a catchable `Error` in eval code, while a
/// real method and a `__call()` fallback still answer.
#[test]
fn test_eval_missing_native_method_is_a_catchable_error() {
    let out = compile_and_run(
        r#"<?php
class Plain { public function known() { return "k"; } }
class Magic { public function __call($n, $a) { return "magic:$n"; } }
$p = new Plain;
$m = new Magic;
$s = new stdClass;
$ao = new ArrayObject([1]);
echo eval('
$out = [];
foreach ([[$p, "nope"], [$s, "nope"], [$ao, "nope"], [$m, "anything"], [$p, "known"]] as [$o, $name]) {
    try { $out[] = $o->$name(); } catch (Error $e) { $out[] = get_class($e) . ":" . $e->getMessage(); }
}
return implode("|", $out);
'), "\n";
"#,
    );
    assert_eq!(
        out,
        "Error:Call to undefined method Plain::nope()|Error:Call to undefined method stdClass::nope()|Error:Call to undefined method ArrayObject::nope()|magic:anything|k\n"
    );
}
