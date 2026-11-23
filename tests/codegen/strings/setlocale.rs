//! Purpose:
//! Focused end-to-end tests for `setlocale()` argument handling against php-src 8.5.
//!
//! Called from:
//! - `cargo test` through `tests/codegen/strings.rs`.
//!
//! Key details:
//! - php-src checks every top-level argument against `array|string|null` before the first
//!   libc call, expands arrays in place, takes `null`/`""` from the environment, queries
//!   only on `"0"`, and warns about names of 255 bytes or more before trying the next one.
//! - Tests that need a real non-C locale go through `locale_available`, which fails
//!   instead of skipping when CI sets `ELEPHC_REQUIRE_TEST_LOCALES`.

use crate::codegen::ctype_php_src::{locale_available, test_locales_required};
use crate::support::*;
use std::ffi::OsStr;

/// Verifies an array of candidates returns the first accepted locale (issue #2379).
#[test]
fn test_setlocale_array_returns_the_first_valid_locale() {
    let out = compile_and_run(r#"<?php
var_dump(setlocale(LC_ALL, ["xx_XX", "C"]));
var_dump(setlocale(LC_ALL, ["xx_XX"], ["yy_YY", "C"]));
var_dump(setlocale(LC_ALL, [], "xx_XX", ["C"]));
var_dump(setlocale(LC_ALL, ["xx_XX", "yy_YY"]));
var_dump(setlocale(LC_ALL, ["key" => "C"]));
"#);
    assert_eq!(
        out,
        "string(1) \"C\"\nstring(1) \"C\"\nstring(1) \"C\"\nbool(false)\nstring(1) \"C\"\n"
    );
}

/// Verifies `null`, a `null` array element and `false` take the locale from the
/// environment instead of querying the current one (issue #2402).
#[test]
fn test_setlocale_null_takes_the_environment_locale() {
    if !locale_available(&["de_DE.UTF-8", "de_DE.utf8"]) {
        return;
    }
    let out = compile_and_run_with_env(r#"<?php
var_dump(setlocale(LC_CTYPE, "C"));
var_dump(setlocale(LC_CTYPE, null));
var_dump(setlocale(LC_CTYPE, "0"));
var_dump(setlocale(LC_CTYPE, "C"));
var_dump(setlocale(LC_CTYPE, [null]));
var_dump(setlocale(LC_CTYPE, "C"));
var_dump(setlocale(LC_CTYPE, false));
var_dump(setlocale(LC_CTYPE, 0));
"#, &[("LC_ALL", OsStr::new("de_DE.UTF-8"))]);
    let german = "string(11) \"de_DE.UTF-8\"\n";
    let c = "string(1) \"C\"\n";
    assert_eq!(out, [c, german, german, c, german, c, german, german].concat());
}

/// Verifies names of 255 bytes or more warn and fall through to the next candidate, while
/// a 254-byte name still reaches libc (issue #2403).
#[test]
fn test_setlocale_rejects_locale_names_of_255_bytes() {
    let out = compile_and_run(r#"<?php
set_error_handler(function (int $no, string $message): bool {
    echo $no === E_WARNING ? "warning: " : "other: ", $message, "\n";
    return true;
});
var_dump(setlocale(LC_ALL, str_repeat("a", 255), "C"));
var_dump(setlocale(LC_ALL, [str_repeat("b", 300)]));
var_dump(setlocale(LC_ALL, str_repeat("c", 254)));
eval('var_dump(setlocale(LC_ALL, [str_repeat("d", 255), "C"]));');
"#);
    let warning = "warning: setlocale(): Specified locale name is too long\n";
    assert_eq!(
        out,
        [
            warning,
            "string(1) \"C\"\n",
            warning,
            "bool(false)\n",
            "bool(false)\n",
            warning,
            "string(1) \"C\"\n",
        ]
        .concat()
    );
}

/// Verifies top-level arguments outside `array|string|null` throw PHP's TypeError before any
/// locale is tried, while weak mode still coerces scalars and Stringable objects (issue #2404).
#[test]
fn test_setlocale_rejects_non_string_arguments_with_type_error() {
    let out = compile_and_run(r#"<?php
class LocaleName { public function __toString(): string { return "C"; } }
try { setlocale(LC_ALL, new stdClass()); } catch (TypeError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { setlocale(LC_ALL, "C", new stdClass()); } catch (TypeError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { setlocale(LC_ALL, "C", "C", fopen("php://memory", "r")); } catch (TypeError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
var_dump(setlocale(LC_ALL, new LocaleName()));
var_dump(setlocale(LC_ALL, 1.5, true, "C"));
eval('try { setlocale(LC_ALL, "C", new stdClass()); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }');
"#);
    assert_eq!(
        out,
        "TypeError: setlocale(): Argument #2 ($locales) must be of type array|string|null, stdClass given\n\
         TypeError: setlocale(): Argument #3 must be of type array|string|null, stdClass given\n\
         TypeError: setlocale(): Argument #4 must be of type array|string|null, resource given\n\
         string(1) \"C\"\n\
         string(1) \"C\"\n\
         setlocale(): Argument #3 must be of type array|string|null, stdClass given\n"
    );
}

/// Verifies an array element that is an object without `__toString()` throws PHP's `Error`
/// when its turn comes instead of taking the environment locale, while a Stringable element
/// still converts.
#[test]
fn test_setlocale_array_object_element_without_to_string_throws() {
    let out = compile_and_run(r#"<?php
class LocaleName { public function __toString(): string { return "C"; } }
try {
    setlocale(LC_ALL, ["xx_XX", new stdClass(), "C"]);
} catch (Error $e) {
    echo get_class($e), ": ", $e->getMessage(), "\n";
}
var_dump(setlocale(LC_ALL, ["C", new stdClass()]));
var_dump(setlocale(LC_ALL, ["xx_XX", new LocaleName()]));
eval('try { setlocale(LC_ALL, [new stdClass()]); } catch (Error $e) { echo $e->getMessage(), "\n"; }');
"#);
    assert_eq!(
        out,
        "Error: Object of class stdClass could not be converted to string\n\
         string(1) \"C\"\n\
         string(1) \"C\"\n\
         Object of class stdClass could not be converted to string\n"
    );
}

/// Verifies an invalid later argument throws without applying an earlier valid candidate.
#[test]
fn test_setlocale_validates_every_argument_before_changing_the_locale() {
    if !locale_available(&["de_DE.UTF-8", "de_DE.utf8"]) {
        return;
    }
    let out = compile_and_run(r#"<?php
var_dump(setlocale(LC_CTYPE, "C"));
try {
    setlocale(LC_CTYPE, ["de_DE.UTF-8", "de_DE.utf8"], new stdClass());
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
var_dump(setlocale(LC_CTYPE, "0"));
"#);
    assert_eq!(
        out,
        "string(1) \"C\"\n\
         setlocale(): Argument #3 must be of type array|string|null, stdClass given\n\
         string(1) \"C\"\n"
    );
}

/// Verifies `declare(strict_types=1)` rejects scalar locale arguments, including the extra
/// variadic candidates, literal argument unpacks, calls through a captured `setlocale(...)`
/// and `call_user_func()` / `call_user_func_array()` callbacks, against the
/// `array|string|null` contract (issue #2404).
#[test]
fn test_setlocale_strict_rejects_scalar_locale_arguments() {
    for source in [
        "<?php declare(strict_types=1); setlocale(LC_ALL, 0);",
        "<?php declare(strict_types=1); setlocale(LC_ALL, 1.5);",
        "<?php declare(strict_types=1); setlocale(LC_ALL, true);",
        "<?php declare(strict_types=1); $f = setlocale(...); $f(LC_ALL, 0);",
        "<?php declare(strict_types=1); $f = setlocale(...); ($f)(LC_ALL, 0);",
        "<?php declare(strict_types=1); (setlocale(...))(LC_ALL, 0);",
        "<?php declare(strict_types=1); call_user_func('setlocale', LC_ALL, 0);",
        "<?php declare(strict_types=1); call_user_func(setlocale(...), LC_ALL, 0);",
        "<?php declare(strict_types=1); $f = setlocale(...); call_user_func($f, LC_ALL, 0);",
        "<?php declare(strict_types=1); call_user_func_array('setlocale', [LC_ALL, 0]);",
        "<?php declare(strict_types=1); call_user_func_array(setlocale(...), [LC_ALL, 0]);",
        "<?php declare(strict_types=1); $f = setlocale(...); call_user_func_array($f, [LC_ALL, 0]);",
        "<?php declare(strict_types=1); setlocale(LC_ALL, ...[0]);",
        "<?php declare(strict_types=1); setlocale(...[LC_ALL, 0]);",
        "<?php declare(strict_types=1); $f = setlocale(...); $f(LC_ALL, ...[0]);",
        "<?php declare(strict_types=1); call_user_func('setlocale', LC_ALL, ...[0]);",
        "<?php declare(strict_types=1); setlocale(LC_ALL, ...[false]);",
    ] {
        let error = compile_expect_type_error(source);
        assert!(error.contains("$locales"), "{source}: {error}");
    }
    for source in [
        "<?php declare(strict_types=1); setlocale(LC_ALL, \"C\", 0);",
        "<?php declare(strict_types=1); setlocale(LC_ALL, \"C\", \"C\", false);",
        "<?php declare(strict_types=1); $f = setlocale(...); $f(LC_ALL, \"C\", 0);",
        "<?php declare(strict_types=1); call_user_func('setlocale', LC_ALL, \"C\", 0);",
        "<?php declare(strict_types=1); setlocale(LC_ALL, \"C\", ...[0]);",
        "<?php declare(strict_types=1); $f = setlocale(...); ($f)(LC_ALL, \"C\", ...[0]);",
        "<?php declare(strict_types=1); setlocale(LC_ALL, \"C\", ...[false]);",
    ] {
        let error = compile_expect_type_error(source);
        assert!(error.contains("$rest"), "{source}: {error}");
    }
}

/// Verifies a literal unpack keeps the strict check a directly written argument gets for
/// every declared function, not only the `setlocale()` adapter: `null` into `string` is
/// refused, while `null` into a nullable or implicitly nullable slot is still accepted.
#[test]
fn test_strict_literal_unpack_checks_null_against_declared_types() {
    let error = compile_expect_type_error(
        "<?php declare(strict_types=1); function f(string $s): void {} f(...[null]);",
    );
    assert!(error.contains("$s"), "{error}");
    let out = compile_and_run(r#"<?php
declare(strict_types=1);
function g(?string $s): void { var_dump($s); }
function h(string $s = null): void { var_dump($s); }
function k(int|false $v, float ...$rest): void { var_dump($v, $rest); }
g(...[null]);
h(...[null]);
k(...[false, 1.5]);
"#);
    assert_eq!(out, "NULL\nNULL\nbool(false)\narray(1) {\n  [0]=>\n  float(1.5)\n}\n");
}

/// Verifies strict files keep accepting every `array|string|null` locale argument, directly
/// and through a captured `setlocale(...)`; `null` takes the environment locale, pinned to
/// `C` here.
#[test]
fn test_setlocale_strict_accepts_contract_arguments() {
    let out = compile_and_run_with_env(r#"<?php
declare(strict_types=1);
var_dump(setlocale(LC_ALL, ["xx_XX", null]));
var_dump(setlocale(LC_ALL, "xx_XX", ["C"]));
var_dump(setlocale(LC_ALL, "xx_XX", null));
var_dump(setlocale(LC_ALL, "0"));
$f = setlocale(...);
var_dump($f(LC_ALL, "xx_XX", ["C"]));
var_dump(setlocale(LC_ALL, ...["xx_XX", ["C"]]));
var_dump(setlocale(LC_ALL, ...[null]));
var_dump(setlocale(LC_ALL, ...[[false]]));
"#, &[("LC_ALL", OsStr::new("C"))]);
    assert_eq!(out, "string(1) \"C\"\n".repeat(8));
}

/// Verifies compiled PCRE character classes follow `setlocale(LC_CTYPE)` (issue #2380).
#[test]
fn test_setlocale_drives_pcre_word_characters() {
    if !locale_available(&["de_DE.ISO8859-1", "de_DE.iso88591", "de_DE"]) {
        return;
    }
    let out = compile_and_run(r#"<?php
var_dump(preg_match('/^\w$/', "\xe9"));
if (setlocale(LC_CTYPE, "de_DE.ISO8859-1", "de_DE.ISO-8859-1", "de_DE") === false) {
    echo "skip";
} else {
    var_dump(preg_match('/^\w$/', "\xe9"));
}
"#);
    if out.ends_with("skip") && !test_locales_required() {
        return;
    }
    assert_eq!(out, "int(0)\nint(1)\n");
}
