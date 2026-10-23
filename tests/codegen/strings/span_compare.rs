//! Purpose:
//! Codegen tests for `strspn`, `strcspn`, `strrchr`, `substr_compare` and `preg_quote`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Expected outputs were measured on PHP 8.5, including the offset/length edge cases,
//!   the raw byte difference `substr_compare()` returns, and `preg_quote()`'s `\000`.
//! - Every builtin is also called through eval, a string callable and a namespaced or
//!   case-insensitive name, and checked as an internal function.

use super::*;

/// `strspn()` / `strcspn()` follow php-src's offset and length window rules.
#[test]
fn test_strspn_and_strcspn_windows() {
    let out = compile_and_run(
        r#"<?php
echo strspn("42 is the answer", "1234567890"), ",", strspn("foo", "o", 1, 2), ",",
    strspn("foo", "o", -2), ",", strspn("foo", "o", 5), ",", strspn("foo", "o", 1, -1), ",",
    strspn("abc", ""), ",", strspn("", "a"), "\n";
echo strcspn("abcd", "cd"), ",", strcspn("abcd", ""), ",", strcspn("hello", "l", -4), ",",
    strcspn("hello", "l", 10), ",", strcspn("abcdhello", "l", -5, 3), ",",
    strcspn("abc", "x", 1, -5), "\n";
"#,
    );
    assert_eq!(out, "2,2,2,0,1,0,0\n2,4,1,0,2,0\n");
}

/// A runtime `null` length behaves like an omitted one.
#[test]
fn test_strspn_runtime_null_length() {
    let out = compile_and_run(
        r#"<?php
foreach ([null, 2] as $length) {
    echo strspn("aaab", "a", 0, $length), ",", strcspn("abcdef", "z", 1, $length), ";";
}
echo "\n";
"#,
    );
    assert_eq!(out, "3,5;2,2;\n");
}

/// `strrchr()` searches the needle's first byte, returns `false` on a miss, and supports
/// PHP 8.3's `$before_needle`.
#[test]
fn test_strrchr_results() {
    let out = compile_and_run(
        r#"<?php
var_dump(strrchr("a/b/c", "/"), strrchr("abc", "x"), strrchr("abc", "bz"), strrchr("a/b/c", "/", true));
var_dump(strrchr("abc", ""), strlen(strrchr("a\0b", "")), strrchr("", "a"));
var_dump(strrchr("x.y.z", ".") !== false);
"#,
    );
    assert_eq!(
        out,
        "string(2) \"/c\"\nbool(false)\nstring(2) \"bc\"\nstring(3) \"a/b\"\nbool(false)\nint(2)\nbool(false)\nbool(true)\n"
    );
}

/// `substr_compare()` returns php-src's byte difference or length comparison.
#[test]
fn test_substr_compare_results() {
    let out = compile_and_run(
        r#"<?php
echo substr_compare("Hello", "ello", 1), ",", substr_compare("Hello", "ELLO", 1, null, true), ",",
    substr_compare("abcde", "bc", 1, 2), ",", substr_compare("abcde", "de", -2, 2), ",",
    substr_compare("abcde", "bd", 1, 2), ",", substr_compare("abcde", "BC", 1, 3, true), ",",
    substr_compare("abcde", "cd", 1, 2), ",", substr_compare("abcde", "abc", 5, 1), ",",
    substr_compare("abcde", "zzzzz", 0), ",", substr_compare("abc", "a", -10), ",",
    substr_compare("abcde", "bc", 1, 0), "\n";
"#,
    );
    assert_eq!(out, "0,0,0,0,-1,1,-1,-1,-25,1,0\n");
}

/// An `$offset` past the haystack or a negative `$length` is a catchable `ValueError`.
#[test]
fn test_substr_compare_value_errors() {
    let out = compile_and_run(
        r#"<?php
foreach ([[6, null], [1, -1]] as [$offset, $length]) {
    try {
        substr_compare("abcde", "bc", $offset, $length);
    } catch (ValueError $e) {
        echo get_class($e), ": ", $e->getMessage(), "\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        "ValueError: substr_compare(): Argument #3 ($offset) must be contained in argument #1 ($haystack)\n\
         ValueError: substr_compare(): Argument #4 ($length) must be greater than or equal to 0\n"
    );
}

/// `preg_quote()` escapes php-src's metacharacter set, the delimiter's first byte, and NUL.
#[test]
fn test_preg_quote_escapes() {
    let out = compile_and_run(
        r##"<?php
echo preg_quote("Hello.World?(1+1=2)[a]{b}^$|\\/#:-<>!\0 x"), "\n";
echo preg_quote("a/b#c", "/"), "|", preg_quote("a/b#c", "#"), "|", preg_quote("abc", ""), "|",
    preg_quote("a=b", "ab"), "|", preg_quote("a/b", null), "\n";
"##,
    );
    assert_eq!(
        out,
        "Hello\\.World\\?\\(1\\+1\\=2\\)\\[a\\]\\{b\\}\\^\\$\\|\\\\/\\#\\:\\-\\<\\>\\!\\000 x\n\
         a\\/b\\#c|a/b\\#c|abc|\\a\\=b|a/b\n"
    );
}

/// The builtins are internal functions: callable from eval, by string callable and from a
/// namespace, listed by `get_defined_functions()`, and `isInternal()` for Reflection.
#[test]
fn test_span_compare_builtins_are_internal_functions() {
    let out = compile_and_run(
        r#"<?php
namespace App;
eval('echo strspn("42 is", "0123456789"), strcspn("abcd", "cd"), strrchr("a/b", "/"), substr_compare("Hello", "ELLO", 1, null, true), preg_quote("a.b"), "\n";');
$f = 'strspn';
echo $f("aab", "a"), call_user_func('strrchr', "x.y.z", "."), STRCSPN("hello", "l"), \preg_quote("1+1"), "\n";
$defined = get_defined_functions()['internal'];
foreach (['strspn', 'strcspn', 'strrchr', 'substr_compare', 'preg_quote'] as $name) {
    echo in_array($name, $defined, true) ? 1 : 0;
}
echo (new \ReflectionFunction('strspn'))->isInternal() ? 1 : 0,
    (new \ReflectionFunction('strcspn'))->isInternal() ? 1 : 0,
    (new \ReflectionFunction('strrchr'))->isInternal() ? 1 : 0,
    (new \ReflectionFunction('substr_compare'))->isInternal() ? 1 : 0,
    (new \ReflectionFunction('preg_quote'))->isInternal() ? 1 : 0, "\n";
"#,
    );
    assert_eq!(out, "22/b0a\\.b\n2.z21\\+1\n1111111111\n");
}

/// Inside eval, an array or a non-`Stringable` object passed to an integer `$offset` / `$length`
/// or to `preg_quote()`'s `$delimiter` throws PHP's `TypeError` instead of being cast to `0` /
/// `""`; null, numeric strings and `Stringable` objects are still coerced.
#[test]
fn test_span_compare_eval_rejects_container_arguments() {
    let out = compile_and_run(
        r#"<?php
class Slash { public function __toString(): string { return "/"; } }
$calls = [
    'return strspn("aaa", "a", 0, []);',
    'return strcspn("abc", "c", []);',
    'return substr_compare("aaa", "a", 0, []);',
    'return substr_compare("aaa", "a", new stdClass());',
    'return preg_quote("a.b", []);',
];
foreach ($calls as $code) {
    try {
        var_dump(eval($code));
    } catch (TypeError $e) {
        echo $e->getMessage(), "\n";
    }
}
echo eval('return preg_quote("a/b", new Slash());'), " ", eval('return strspn("aaab", "a", 0, null);'),
    " ", eval('return strspn("aaab", "a", 0, "2");'), "\n";
"#,
    );
    assert_eq!(
        out,
        "strspn(): Argument #4 ($length) must be of type ?int, array given\n\
strcspn(): Argument #3 ($offset) must be of type int, array given\n\
substr_compare(): Argument #4 ($length) must be of type ?int, array given\n\
substr_compare(): Argument #3 ($offset) must be of type int, stdClass given\n\
preg_quote(): Argument #2 ($delimiter) must be of type ?string, array given\n\
a\\/b 3 2\n"
    );
}

/// A second string argument that is not already a string (int, float, null, bool) is converted
/// before the helper call; on x86_64 it is staged into `rdx`/`rcx`, where the converted pointer
/// must not overwrite the converted length.
#[test]
fn test_span_compare_converted_second_strings() {
    let out = compile_and_run(
        r#"<?php
echo strcspn("abc1", 1), strspn("1123x", 12), strcspn("ab", null), strspn("11.5", 1.5), "\n";
var_dump(strrchr("a1b1c", 1), strrchr("xtrue", true), substr_compare("abc12", 12, 3), substr_compare("abc", null, 3), substr_compare("x1.5", 1.5, 1));
"#,
    );
    assert_eq!(out, "3324\nstring(2) \"1c\"\nbool(false)\nint(0)\nint(0)\nint(0)\n");
}
