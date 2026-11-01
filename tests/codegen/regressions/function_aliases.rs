//! Purpose:
//! Regression tests for PHP's plain function aliases (`sizeof`, `pos`, `key_exists`, ...).
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The aliases had no declaration at all: a call failed at run time (`Call to undefined function
//!   sizeof()`) and `function_exists()` answered `false`. The name resolver now rewrites a call or
//!   a literal callback to the function the alias names (`name_resolver::FUNCTION_ALIASES`), and
//!   introspection answers for the alias.

use crate::support::*;

/// Aliases work as calls (also unqualified inside a namespace), as literal callbacks, and through
/// literal and dynamic `function_exists()` / `is_callable()`.
#[test]
fn test_php_function_aliases_resolve_to_their_functions() {
    let out = compile_and_run(
        r#"<?php
namespace App;
echo sizeof([1, 2, 3]), \sizeof([1]), "\n";
var_dump(key_exists("a", ["a" => 1]), pos([7, 8]), strchr("hello", "l"), doubleval("2.5"));
fputs(STDOUT, "fputs\n");
echo implode(",", array_map('sizeof', [[1], [1, 2]])), " ", call_user_func('strchr', "abc", "b"), "\n";
var_dump(function_exists('sizeof'), function_exists('\pos'), is_callable('sizeof'), function_exists('nosuch_alias'));
$names = ["sizeof", "doubleval", "nope"];
foreach ($names as $n) { echo $n, "=", function_exists($n) ? 1 : 0, " "; }
echo "\n";
"#,
    );
    assert_eq!(out, "31\nbool(true)\nint(7)\nstring(3) \"llo\"\nfloat(2.5)\nfputs\n1,2 bc\nbool(true)\nbool(true)\nbool(true)\nbool(false)\nsizeof=1 doubleval=1 nope=0 \n");
}

/// An alias held in a string variable calls the function it names.
#[test]
fn test_php_function_alias_called_through_a_string_variable() {
    let out = compile_and_run(
        r#"<?php
function a2() { $f = "sizeof"; return $f([1, 2, 3]); }
function a4() { $f = "key_exists"; return var_export($f("a", ["a" => 1]), true); }
function a6() { $f = "strchr"; return $f("abc", "b"); }
function a7() { $f = "doubleval"; return $f("1.5"); }
echo a2(), "|", a4(), "|", a6(), "|", a7(), "\n";
"#,
    );
    assert_eq!(out, "3|true|bc|1.5\n");
}
