//! Purpose:
//! Integration tests for list destructuring from narrowed and associative array values.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Null guards ending in `continue` or `break` must preserve the non-null complement.
//! - Associative-array storage remains a valid RHS when positional integer keys are present.

use crate::support::*;

/// Verifies a null guard ending in `continue` narrows `?array` to `Array` before list unpacking.
#[test]
fn test_null_guard_continue_narrows_list_unpack_rhs() {
    let out = compile_and_run(
        r#"<?php
final class R {
    private function mk(int $n): ?array {
        if ($n < 0) { return null; }
        return ["k" . $n, "v" . $n];
    }
    public function run(): string {
        $out = "";
        foreach ([1, -1, 2] as $n) {
            $entry = $this->mk($n);
            if ($entry === null) { continue; }
            [$key, $value] = $entry;
            $out .= $key . "=" . $value . ";";
        }
        return $out;
    }
}
echo (new R())->run();
"#,
    );
    assert_eq!(out, "k1=v1;k2=v2;");
}

/// Verifies a null guard ending in `break` narrows `?array` to `Array` before list unpacking.
#[test]
fn test_null_guard_break_narrows_list_unpack_rhs() {
    let out = compile_and_run(
        r#"<?php
function row(int $n): ?array {
    if ($n < 0) { return null; }
    return ["k" . $n, "v" . $n];
}
$out = "";
foreach ([1, -1, 2] as $n) {
    $entry = row($n);
    if ($entry === null) { break; }
    [$key, $value] = $entry;
    $out .= $key . "=" . $value . ";";
}
echo $out;
"#,
    );
    assert_eq!(out, "k1=v1;");
}

/// Verifies positional list unpacking accepts associative storage with integer keys.
#[test]
fn test_list_unpack_assoc_array_rhs() {
    let out = compile_and_run(
        r#"<?php
$row = [0 => "left", 1 => "right", "label" => "ignored"];
[$left, $right] = $row;
echo $left . ":" . $right;
"#,
    );
    assert_eq!(out, "left:right");
}
