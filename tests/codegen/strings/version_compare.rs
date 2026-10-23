//! Purpose:
//! Codegen tests for PHP's `version_compare()`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The comparison table was measured on PHP 8.5: canonicalization of separators and
//!   digit/letter boundaries, special forms (`dev` < `alpha` < `beta` < `RC` < `#` < `pl`),
//!   trailing separators, the `#` prefix, and saturating numeric tokens.

use super::*;

/// Every pair in the table compares exactly as PHP 8.5 does.
#[test]
fn test_version_compare_matches_php_table() {
    let out = compile_and_run(
        r##"<?php
$pairs = [
    ["5.2", "5.10"],
    ["1.0rc1", "1.0"],
    ["1.0.0", "1.0"],
    ["1.0-dev", "1.0alpha"],
    ["1.0pl1", "1.0"],
    ["1.0", "1.0.0"],
    ["1.0#1", "1.0.1"],
    ["", "1"],
    ["1.0", "1.0"],
    ["1", "2"],
    ["1.0.0-beta2", "1.0.0-beta10"],
    ["1.0a", "1.0b"],
    ["1.0", "1.0a"],
    ["1.0RC1", "1.0rc1"],
    ["7.4.33", "8.0"],
    ["1..2", "1.2"],
    ["1.2.", "1.2"],
    ["abc", "1"],
    ["1.x", "1.0"],
    ["1.0.0.0", "1.0"],
    ["10", "9"],
    [".5", "0.5"],
    ["1.0-p1", "1.0"],
    ["1.10.0", "1.9.9"],
    ["2.0b", "2.0"],
    ["1.0+", "1.0"],
    ["1.2.", "1.2.0"],
    ["1.2.", "1.2.a"],
    ["1.2.", "1.2.pl"],
    ["1.2.", "1.2#"],
    ["1.2", "1.2."],
    ["1.2..", "1.2"],
    ["1.2.", "1.2.dev"],
    ["1.2-", "1.2-0"],
    [".", "1"],
    [".", "a"],
    ["-", "dev"],
    ["1.", "1"],
    ["1.", "1.0"],
    ["a.", "a"],
    ["1.0.", "1.0.#"],
    ["#", "1"],
    ["1.0.0-alpha.1", "1.0.0-alpha.beta"],
    ["v1.2", "1.2"],
    ["1.2a3", "1.2a10"],
    ["8.5.0RC2", "8.5.0"],
    ["1-2-3", "1.2.3"],
    ["1_2", "1.2"],
    ["1.0.0+build", "1.0.0"],
    ["2.0.0-rc.1", "2.0.0-rc.1+build.123"],
    ["01.2", "1.2"],
    ["99999999999999999999", "1"],
    ["1 2", "1.2"],
    ["1.2.3.4.5", "1.2.3.4.5.0"],
    ["a", "b"],
    ["x", "y"],
    ["alpha", "beta"],
    ["rc", "pl"]
];
$results = [];
foreach ($pairs as [$a, $b]) {
    $results[] = version_compare($a, $b);
}
echo implode(" ", $results), "\n";
"##,
    );
    assert_eq!(out, "-1 -1 1 -1 1 -1 1 -1 0 -1 -1 -1 1 0 -1 0 0 -1 -1 1 1 -1 1 1 -1 0 -1 1 -1 0 0 0 1 -1 -1 -1 -1 0 -1 0 0 0 1 -1 -1 -1 0 0 -1 1 0 1 -1 -1 -1 0 -1 -1\n");
}

/// Each operator spelling selects its predicate; an unknown one is a catchable `ValueError`,
/// and a null operator keeps the int form.
#[test]
fn test_version_compare_operators() {
    let out = compile_and_run(
        r##"<?php
foreach (["<", "lt", "<=", "le", ">", "gt", ">=", "ge", "==", "=", "eq", "!=", "<>", "ne"] as $op) {
    echo version_compare("1.0", "1.1", $op) ? 1 : 0;
}
echo "\n";
var_dump(version_compare("1.0", "1.0", null), version_compare("8.1", "8.0", ">="));
foreach (["lt", null, "bogus"] as $op) {
    try {
        var_dump(version_compare("1", "2", $op));
    } catch (ValueError $e) {
        echo $e->getMessage(), "\n";
    }
}
"##,
    );
    assert_eq!(
        out,
        "11110000000111\nint(0)\nbool(true)\nbool(true)\nint(-1)\n\
         version_compare(): Argument #3 ($operator) must be a valid comparison operator\n"
    );
}

/// A `null` version still compiles in PHP's coercive mode and compares as the empty string,
/// directly and through a nullable variable.
#[test]
fn test_version_compare_null_versions_compare_as_empty_string() {
    let out = compile_and_run(
        r##"<?php
var_dump(version_compare(null, "1"), version_compare("1", null), version_compare(null, null, "eq"));
$v = $argc > 5 ? "2.0" : null;
var_dump(version_compare($v, "0"), version_compare($v, "", "=="));
"##,
    );
    assert_eq!(out, "int(-1)\nint(1)\nbool(true)\nint(-1)\nbool(true)\n");
}

/// Scalar versions that are not strings are converted like PHP's coercive `string` parameter,
/// in either position; the second version is the one staged through the SysV `rdx`/`rcx` pair.
#[test]
fn test_version_compare_scalar_versions_are_coerced() {
    let out = compile_and_run(
        r##"<?php
var_dump(version_compare("1", 2), version_compare("1.5", 1.5, "eq"), version_compare(2, true), version_compare("1", false, ">"));
"##,
    );
    assert_eq!(out, "int(-1)\nbool(true)\nint(1)\nbool(true)\n");
}

/// A nullable scalar operator (`?int`, `?bool`) that holds null keeps the int form; a non-null
/// one is converted to a string operator, which is not a valid comparison operator here.
#[test]
fn test_version_compare_nullable_scalar_operator() {
    let out = compile_and_run(
        r##"<?php
function f(?int $op) { var_dump(version_compare("1", "2", $op)); }
f(null);
try { f(5); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
$o = $argc > 5 ? true : null;
var_dump(version_compare("1", "2", $o));
"##,
    );
    assert_eq!(
        out,
        "int(-1)\nversion_compare(): Argument #3 ($operator) must be a valid comparison operator\nint(-1)\n"
    );
}

/// `version_compare()` is an internal function: eval, string callables, first-class callables
/// and namespaced calls reach it, and Reflection reports it as internal.
#[test]
fn test_version_compare_is_an_internal_function() {
    let out = compile_and_run(
        r##"<?php
namespace App;
eval('echo version_compare("5.2", "5.10"), version_compare("1.0rc1", "1.0", "<") ? "y" : "n", "\n";');
$f = 'version_compare';
$g = version_compare(...);
echo $f("2.0", "2.0.0"), call_user_func('version_compare', "1.2", "1.10"), $g("1", "1"), VERSION_COMPARE("1.0-dev", "1.0alpha"), "\n";
echo (new \ReflectionFunction('version_compare'))->isInternal() ? 1 : 0, in_array('version_compare', get_defined_functions()['internal'], true) ? 1 : 0, "\n";
"##,
    );
    assert_eq!(out, "-1y\n-1-10-1\n11\n");
}
