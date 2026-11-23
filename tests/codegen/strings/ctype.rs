//! Purpose:
//! Focused end-to-end tests for the complete `ext/ctype` surface.
//!
//! Called from:
//! - `cargo test` through `tests/codegen/strings.rs`.
//!
//! Key details:
//! - Expected values are php-src 8.5.10 behavior under the C locale: every byte
//!   must satisfy the class, the empty string is false, and high bytes are false.
//! - Non-string arguments exercise the php-src 8.1+ deprecated integer fallback and
//!   the deprecation naming the argument's runtime type.
//! - Diagnostics live on stderr; stdout assertions stay separate so the order of the
//!   two streams cannot make the test flaky.

use crate::codegen::ctype_php_src::{locale_available, test_locales_required};
use crate::support::*;

/// Verifies `setlocale()` tries array elements and additional locale arguments in order.
#[test]
fn test_setlocale_tries_array_and_extra_candidates() {
    let out = compile_and_run(r#"<?php
var_dump(setlocale(LC_CTYPE, ["missing", "C"]));
var_dump(setlocale(LC_CTYPE, ["missing"], "C"));
var_dump(setlocale(LC_CTYPE, "missing", ["C"]));
var_dump(is_string(setlocale(LC_CTYPE, 0, "C")));
var_dump(is_string(setlocale(LC_CTYPE, ["0"])));
var_dump(is_string(setlocale(LC_CTYPE, "missing", "0")));
"#);
    assert_eq!(out, "string(1) \"C\"\nstring(1) \"C\"\nstring(1) \"C\"\nbool(true)\nbool(true)\nbool(true)\n");
}

/// Verifies `nl_langinfo(CODESET)` reports the active libc codeset, not a name guess.
#[test]
fn test_nl_langinfo_reports_the_active_libc_codeset() {
    if !locale_available(&["de_DE.ISO8859-1", "de_DE.ISO-8859-1", "de_DE"]) {
        return;
    }
    let out = compile_and_run(r#"<?php
$locale = setlocale(LC_CTYPE, "de_DE.ISO8859-1");
if ($locale === false) { $locale = setlocale(LC_CTYPE, "de_DE.ISO-8859-1"); }
if ($locale === false) { $locale = setlocale(LC_CTYPE, "de_DE"); }
if ($locale === false) {
    echo "skip";
} else {
    echo str_contains(strtoupper(nl_langinfo(CODESET)), "8859") ? "latin1" : "wrong:" . nl_langinfo(CODESET);
}
"#);
    assert!(out == "latin1" || (out == "skip" && !test_locales_required()), "{out}");
}

/// Verifies eval locale calls share the host locale and the libc-backed ctype predicates.
#[test]
fn test_eval_locale_calls_share_the_host_locale() {
    if !locale_available(&["de_DE.ISO8859-1", "de_DE.ISO-8859-1", "de_DE"]) {
        return;
    }
    let out = compile_and_run(r#"<?php
$locale = setlocale(LC_CTYPE, ["missing", "de_DE.ISO8859-1"]);
if ($locale === false) { $locale = setlocale(LC_CTYPE, ["missing", "de_DE.ISO-8859-1"]); }
if ($locale === false) { $locale = setlocale(LC_CTYPE, ["missing", "de_DE"]); }
if ($locale === false) {
    echo "skip";
} else {
    eval('var_dump(ctype_lower("\xe4")); var_dump(nl_langinfo(CODESET)); var_dump(setlocale(LC_CTYPE, ["missing", "C"]));');
}
"#);
    if out == "skip" && !test_locales_required() {
        return;
    }
    assert!(out.starts_with("bool(true)\n"), "{out}");
    assert!(out.contains("8859"), "{out}");
    assert!(out.ends_with("string(1) \"C\"\n"), "{out}");
}

/// Verifies repeated locale queries return balanced heap ownership.
#[test]
fn test_locale_calls_leave_clean_heap() {
    let out = compile_and_run_with_gc_stats(r#"<?php
for ($i = 0; $i < 32; $i++) {
    setlocale(LC_CTYPE, "C");
    nl_langinfo(CODESET);
}
echo "done";
"#);
    assert!(out.success, "program failed: {}", out.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
    assert_eq!(out.stdout, "done");
}

/// Verifies an iterable holding an object names its concrete class, not `array`.
#[test]
fn test_iterable_object_names_its_concrete_class() {
    let out = compile_and_run(r#"<?php
class It implements Iterator {
    private int $i = 0;
    public function current(): mixed { return 1; }
    public function key(): mixed { return 0; }
    public function next(): void { $this->i++; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return false; }
}
function check(iterable $value): void { var_dump(ctype_alpha($value)); }
set_error_handler(function (int $no, string $message): bool { echo $message; return true; });
check(new It());
"#);
    assert_eq!(out, "ctype_alpha(): Argument of type It will be interpreted as string in the futurebool(false)\n");
}

/// Verifies computed eval does not duplicate a program's own global locale functions.
#[test]
fn test_computed_eval_keeps_global_locale_declarations() {
    let out = compile_and_run(r#"<?php
function setlocale(int $category, mixed $locales, mixed ...$rest): string|false { return "custom"; }
function nl_langinfo(int $item): string|false { return "custom"; }
$code = $argc > 0 ? 'var_dump(setlocale(0, "C"), nl_langinfo(0));' : '';
eval($code);
"#);
    assert_eq!(out, "string(6) \"custom\"\nstring(6) \"custom\"\n");
}

/// Verifies locale constants are available without any locale function call.
#[test]
fn test_locale_constants_inject_without_function_calls() {
    let out = compile_and_run(r#"<?php
var_dump(is_int(LC_CTYPE), is_int(LC_ALL), is_int(CODESET));
"#);
    assert_eq!(out, "bool(true)\nbool(true)\nbool(true)\n");
}

/// Verifies a namespaced declaration does not hide the global locale functions.
#[test]
fn test_namespaced_locale_declaration_keeps_global_prelude() {
    let out = compile_and_run(r#"<?php
namespace App {
    function setlocale() { return 'custom'; }
    var_dump(\setlocale(LC_CTYPE, "C"));
    var_dump(setlocale());
}
"#);
    assert_eq!(out, "string(1) \"C\"\nstring(6) \"custom\"\n");
}

/// Verifies computed eval source can reach the injected locale declarations.
#[test]
fn test_computed_eval_reaches_locale_declarations() {
    let out = compile_and_run(r#"<?php
$code = $argc > 0 ? 'var_dump(setlocale(LC_CTYPE, "C")); var_dump(is_string(nl_langinfo(CODESET)));' : '';
eval($code);
"#);
    assert_eq!(out, "string(1) \"C\"\nbool(true)\n");
}

/// Asserts the exact byte-class membership counts php-src's ctype matrix reports.
#[test]
fn test_ctype_byte_class_counts_match_php_src() {
    let out = compile_and_run(r#"<?php
setlocale(LC_ALL, "C");
$functions = [
    "ctype_lower", "ctype_upper", "ctype_alpha", "ctype_digit", "ctype_alnum",
    "ctype_cntrl", "ctype_graph", "ctype_print", "ctype_punct", "ctype_space",
    "ctype_xdigit",
];
foreach ($functions as $function) {
    $count = 0;
    for ($byte = 0; $byte < 256; $byte++) {
        if ($function(chr($byte))) {
            $count++;
        }
    }
    echo $function, " ", $count, "\n";
}
"#);
    assert_eq!(
        out,
        "ctype_lower 26\n\
         ctype_upper 26\n\
         ctype_alpha 52\n\
         ctype_digit 10\n\
         ctype_alnum 62\n\
         ctype_cntrl 33\n\
         ctype_graph 94\n\
         ctype_print 95\n\
         ctype_punct 32\n\
         ctype_space 6\n\
         ctype_xdigit 22\n"
    );
}

/// Verifies `ctype_*` returns a real PHP boolean for string arguments.
#[test]
fn test_ctype_returns_bool_and_rejects_empty_and_non_ascii_strings() {
    let out = compile_and_run(r#"<?php
var_dump(ctype_alpha("abc"));
var_dump(ctype_alpha("abc1"));
var_dump(ctype_alpha(""));
var_dump(ctype_alpha("\xff"));
var_dump(ctype_cntrl(chr(0)));
var_dump(ctype_cntrl(chr(127)));
var_dump(ctype_cntrl(chr(128)));
var_dump(ctype_space(" \t\n\r\v\f"));
"#);
    assert_eq!(
        out,
        "bool(true)\n\
         bool(false)\n\
         bool(false)\n\
         bool(false)\n\
         bool(true)\n\
         bool(true)\n\
         bool(false)\n\
         bool(true)\n"
    );
}

/// Exercises php-src's deprecated integer window, wrap, and out-of-window fallbacks.
#[test]
fn test_ctype_integer_fallback_matches_php_src() {
    let out = compile_and_run(r#"<?php
var_dump(ctype_alnum(0));
var_dump(ctype_alnum(48));
var_dump(ctype_alnum(12345));
var_dump(ctype_alnum(-2345));
var_dump(ctype_alpha(65));
var_dump(ctype_alpha(-191));
var_dump(ctype_alpha(300));
var_dump(ctype_alpha(-300));
var_dump(ctype_digit(48));
var_dump(ctype_digit(-1));
var_dump(ctype_digit(-199));
var_dump(ctype_graph(300));
var_dump(ctype_graph(-300));
var_dump(ctype_print(-300));
var_dump(ctype_space(-224));
var_dump(ctype_xdigit(-199));
var_dump(ctype_xdigit(300));
var_dump(ctype_upper(9223372036854775807));
var_dump(ctype_upper(-9223372036854775807));
"#);
    assert_eq!(
        out,
        "bool(false)\n\
         bool(true)\n\
         bool(true)\n\
         bool(false)\n\
         bool(true)\n\
         bool(false)\n\
         bool(false)\n\
         bool(false)\n\
         bool(true)\n\
         bool(false)\n\
         bool(false)\n\
         bool(true)\n\
         bool(true)\n\
         bool(true)\n\
         bool(false)\n\
         bool(false)\n\
         bool(true)\n\
         bool(false)\n\
         bool(false)\n"
    );
}

/// Verifies the deprecation names the runtime type and the caller location.
#[test]
fn test_ctype_non_string_deprecation_names_runtime_type() {
    let output = compile_and_run_capture(r#"<?php
var_dump(ctype_digit(12345));
var_dump(ctype_alnum(1.5));
var_dump(ctype_alpha(null));
var_dump(ctype_space(false));
var_dump(ctype_print([]));
class CtypeMarker { public function __toString(): string { return "abc"; } }
var_dump(ctype_graph(new CtypeMarker()));
$resource = fopen("php://memory", "r+");
var_dump(ctype_xdigit($resource));
fclose($resource);
"#);
    assert_eq!(
        output.stdout,
        "bool(true)\n\
         bool(false)\n\
         bool(false)\n\
         bool(false)\n\
         bool(false)\n\
         bool(false)\n\
         bool(false)\n"
    );
    for (function, type_name) in [
        ("ctype_digit", "int"),
        ("ctype_alnum", "float"),
        ("ctype_alpha", "null"),
        ("ctype_space", "bool"),
        ("ctype_print", "array"),
        ("ctype_graph", "CtypeMarker"),
        ("ctype_xdigit", "resource"),
    ] {
        let expected = format!(
            "Deprecated: {function}(): Argument of type {type_name} will be interpreted as string in the future in "
        );
        assert!(
            output.stderr.contains(&expected),
            "missing deprecation for {function}: {expected}\nstderr:\n{}",
            output.stderr
        );
    }
}

/// Verifies an object argument is named by its concrete subclass, not its declared type.
#[test]
fn test_ctype_object_deprecation_uses_concrete_class() {
    let output = compile_and_run_capture(r#"<?php
class CtypeBase {}
class CtypeChild extends CtypeBase {}
function check_ctype_object(CtypeBase $value): void { var_dump(ctype_alnum($value)); }
check_ctype_object(new CtypeChild());
"#);
    assert_eq!(output.stdout, "bool(false)\n");
    assert!(
        output.stderr.contains(
            "Deprecated: ctype_alnum(): Argument of type CtypeChild will be interpreted as string in the future in "
        ),
        "stderr:\n{}",
        output.stderr
    );
}

/// Verifies the deprecation passes `E_DEPRECATED` and the message to a user handler.
#[test]
fn test_ctype_deprecation_reaches_user_error_handler() {
    let output = compile_and_run_capture(r#"<?php
set_error_handler(function (int $errno, string $message): bool {
    var_dump($errno);
    echo $message, "\n";
    return true;
});
var_dump(ctype_digit(49));
"#);
    assert_eq!(
        output.stdout,
        "int(8192)\n\
         ctype_digit(): Argument of type int will be interpreted as string in the future\n\
         bool(true)\n"
    );
    assert_eq!(output.stderr, "");
}

/// Verifies `error_reporting()` suppresses the deprecation while keeping the answer.
#[test]
fn test_ctype_deprecation_respects_error_reporting() {
    let output = compile_and_run_capture(r#"<?php
error_reporting(E_ALL & ~E_DEPRECATED);
var_dump(ctype_digit(12345));
"#);
    assert_eq!(output.stdout, "bool(true)\n");
    assert_eq!(output.stderr, "");
}

/// Verifies first-class and runtime-selected callables reach every ctype predicate.
#[test]
fn test_ctype_first_class_and_dynamic_callables() {
    let out = compile_and_run(r#"<?php
$digit = ctype_digit(...);
var_dump($digit("123"));
var_dump(call_user_func("ctype_alpha", "abc"));
var_dump(call_user_func_array("ctype_xdigit", ["f"]));
var_dump(call_user_func_array("ctype_punct", ["text" => "!?"]));
var_dump(call_user_func("ctype_lower", "ABC"));
"#);
    assert_eq!(
        out,
        "bool(true)\n\
         bool(true)\n\
         bool(true)\n\
         bool(true)\n\
         bool(false)\n"
    );
}

/// Verifies repeated non-string ctype calls leave no live heap blocks.
#[test]
fn test_ctype_deprecation_paths_leave_clean_heap() {
    let out = compile_and_run_with_gc_stats(r#"<?php
for ($i = 0; $i < 3; $i++) {
    ctype_digit(123);
    ctype_alnum(1.5);
    ctype_alpha(null);
    ctype_print([]);
    ctype_graph(new stdClass());
    $handle = fopen("php://memory", "r+");
    ctype_xdigit($handle);
    fclose($handle);
}
echo "done";
"#);
    assert!(out.success, "program failed: {}", out.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
    assert_eq!(out.stdout, "done");
}

/// Verifies both architectures define the branch target used when a locale name exceeds the
/// fixed stack buffer; linux-x86_64 previously referenced a label it never emitted.
#[test]
fn test_setlocale_oversize_locale_label_is_defined_on_every_target() {
    for target in ["linux-x86_64", "linux-aarch64", "macos-aarch64"] {
        let dir = make_cli_test_dir("elephc_locale_oversize_label");
        let php = dir.join("main.php");
        fs::write(&php, "<?php var_dump(setlocale(LC_CTYPE, 'C'));").unwrap();
        let mut command = elephc_cli_command(&dir);
        command.args(["--emit-asm", "--target", target]).arg(&php);
        let output = command.output().unwrap();
        assert!(output.status.success(), "{target}: {}", String::from_utf8_lossy(&output.stderr));
        let assembly = fs::read_to_string(php.with_extension("s")).unwrap();
        assert_eq!(assembly.matches("setlocale_too_long").count(), 2, "{target}: {assembly}");
        fs::remove_dir_all(dir).unwrap();
    }
}

/// Verifies named arguments and static spread reach a ctype call.
#[test]
fn test_ctype_named_and_spread_arguments() {
    let out = compile_and_run(r#"<?php
var_dump(ctype_digit(text: "123"));
var_dump(ctype_alpha(...["text" => "abc"]));
var_dump(ctype_space(text: "\t"));
"#);
    assert_eq!(out, "bool(true)\nbool(true)\nbool(true)\n");
}
