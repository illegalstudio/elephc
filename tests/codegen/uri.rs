//! Purpose:
//! Focused end-to-end coverage for PHP 8.5's `ext/uri` surface: RFC 3986 reads,
//! normalization and mutation, WHATWG parsing and validation, opaque handle introspection,
//! serialization, and the `extension_loaded("uri")` surface.
//!
//! Called from:
//! - `cargo test --test codegen_tests_ext uri_` through Rust's test harness.
//!
//! Key details:
//! - Expected outputs were measured on PHP 8.5.10 with the bundled `uri` extension.
//! - The exhaustive php-src case table lives in `tests/codegen/uri_php_src.rs`; these
//!   fixtures are the fast regression subset for day-to-day work.

use crate::support::*;

/// Verifies RFC 3986 raw/normalized component reads, dot-segment removal, host case
/// normalization, percent-encoding repair, and path mutation.
#[test]
fn test_uri_rfc3986_components_and_normalization() {
    let out = compile_and_run(
        r#"<?php
$u = new Uri\Rfc3986\Uri("HTTPS://user:pass@EXAMPLE.com:0443/a/../b?x=%7e#f");
var_dump($u->getScheme(), $u->getRawScheme(), $u->getHost(), $u->getRawHost(), $u->getPort(), $u->getPath(), $u->getRawPath(), $u->getQuery(), $u->getFragment());
echo $u->toString(), "\n", $u->toRawString(), "\n";
echo $u->withPath("/new")->toString(), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "string(5) \"https\"\n",
            "string(5) \"HTTPS\"\n",
            "string(11) \"example.com\"\n",
            "string(11) \"EXAMPLE.com\"\n",
            "int(443)\n",
            "string(2) \"/b\"\n",
            "string(7) \"/a/../b\"\n",
            "string(3) \"x=~\"\n",
            "string(1) \"f\"\n",
            "https://user:pass@example.com:443/b?x=~#f\n",
            "HTTPS://user:pass@EXAMPLE.com:0443/a/../b?x=%7e#f\n",
            "https://user:pass@example.com:443/new?x=~#f\n",
        )
    );
}

/// Verifies WHATWG file-URL drive handling, warning types, and the fatal port error's
/// message and validation entry.
#[test]
fn test_uri_whatwg_validation_and_file_drive() {
    let out = compile_and_run(
        r#"<?php
$errors = [];
$url = Uri\WhatWg\Url::parse("c|/x", new Uri\WhatWg\Url("file:///d:/a/b"), $errors);
var_dump($url->getPath());
var_dump(array_map(static fn (Uri\WhatWg\UrlValidationError $e): string => $e->type->name, (array) $errors));
echo $url->withPath("/zz")->getPath(), "\n";
try { new Uri\WhatWg\Url("http://localhost:99999"); } catch (Uri\WhatWg\InvalidUrlException $e) { echo $e->getMessage(), "|", count($e->errors), "|", $e->errors[0]->type->name, "\n"; }
"#,
    );
    assert_eq!(
        out,
        concat!(
            "string(5) \"/c:/x\"\n",
            "array(2) {\n",
            "  [0]=>\n",
            "  string(14) \"InvalidUrlUnit\"\n",
            "  [1]=>\n",
            "  string(29) \"FileInvalidWindowsDriveLetter\"\n",
            "}\n",
            "/c:/x\n",
            "The specified URI is malformed (PortOutOfRange)|1|PortOutOfRange\n",
        )
    );
}

/// Verifies the native handle is invisible to casts, `var_export`, and serialization while
/// `__debugInfo()` still renders the eight URI components.
#[test]
fn test_uri_opaque_state_and_serialization() {
    let out = compile_and_run(
        r#"<?php
$u = new Uri\Rfc3986\Uri("https://example.com/a");
var_dump((array)$u);
var_export($u); echo "\n";
var_dump($u);
echo serialize($u), "\n";
$v = unserialize(serialize($u));
echo $v->toString(), "|", $v->getHost(), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "array(0) {\n",
            "}\n",
            "\\Uri\\Rfc3986\\Uri::__set_state(array(\n",
            "))\n",
            "object(Uri\\Rfc3986\\Uri)#1 (8) {\n",
            "  [\"scheme\"]=>\n",
            "  string(5) \"https\"\n",
            "  [\"username\"]=>\n",
            "  NULL\n",
            "  [\"password\"]=>\n",
            "  NULL\n",
            "  [\"host\"]=>\n",
            "  string(11) \"example.com\"\n",
            "  [\"port\"]=>\n",
            "  NULL\n",
            "  [\"path\"]=>\n",
            "  string(2) \"/a\"\n",
            "  [\"query\"]=>\n",
            "  NULL\n",
            "  [\"fragment\"]=>\n",
            "  NULL\n",
            "}\n",
            "O:15:\"Uri\\Rfc3986\\Uri\":2:{i:0;a:1:{s:3:\"uri\";s:21:\"https://example.com/a\";}i:1;a:0:{}}\n",
            "https://example.com/a|example.com\n",
        )
    );
}

/// Verifies `?Class::class` is accepted for a nullable object value (php-src ext/uri relies
/// on it for `getPrevious()::class`).
#[test]
fn test_nullable_object_class_name() {
    let out = compile_and_run(
        r#"<?php
function previous(): ?Throwable { return new Exception("x"); }
var_dump(previous()::class);
"#,
    );
    assert_eq!(out, "string(9) \"Exception\"\n");
}

/// Verifies the injected URI surface reports `extension_loaded("uri")` both when the program
/// only probes it and after the classes are used.
#[test]
fn test_uri_extension_loaded() {
    let out = compile_and_run(
        r#"<?php
var_dump(extension_loaded("uri"));
$u = Uri\Rfc3986\Uri::parse("https://example.com");
var_dump(extension_loaded("uri"));
"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\n");
}

/// Verifies `--with-uri` keeps the complete class surface even when the program never names
/// a URI class directly, so declaration reachability cannot erase the forced prelude.
#[test]
fn test_uri_with_flag_keeps_the_forced_surface() {
    let out = compile_cli_file_and_run_with_flags(
        r#"<?php
var_dump(class_exists("Uri\\Rfc3986\\Uri"), class_exists("Uri\\WhatWg\\Url"), extension_loaded("uri"));
"#,
        &["--with-uri"],
    );
    assert_eq!(out, "bool(true)\nbool(true)\nbool(true)\n");
}

/// Verifies `ext/uri` is declared for the PHP 8.5 profile and absent for 8.4.
#[test]
fn test_uri_surface_is_php85_only() {
    let source = r#"<?php var_dump(class_exists("Uri\Rfc3986\Uri"));"#;
    assert_eq!(
        compile_and_run_with_php_version(source, elephc::php_version::PhpVersion::Php84),
        "bool(false)\n"
    );
    assert_eq!(
        compile_and_run_with_php_version(source, elephc::php_version::PhpVersion::Php85),
        "bool(true)\n"
    );
}

/// Verifies php-src's successful-parse warnings: credentials, a special scheme missing its
/// second solidus, non-decimal IPv4 parts, and removed path whitespace.
#[test]
fn test_uri_whatwg_success_warnings() {
    let out = compile_and_run(
        r#"<?php
$cases = [
    "https://user@x.test",
    "https:/x",
    "http://01.02.03.04",
    "http://x.test/a b",
    "http://x.test/a\tb",
];
foreach ($cases as $case) {
    $errors = [];
    $url = new Uri\WhatWg\Url($case, null, $errors);
    echo json_encode($case), ": ", $url->toAsciiString(), "\n";
    foreach ($errors as $error) {
        echo "  ", $error->type->name, " ", json_encode($error->context), " ", var_export($error->failure, true), "\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "\"https:\\/\\/user@x.test\": https://user@x.test/\n",
            "  InvalidCredentials \"@x.test\" false\n",
            "\"https:\\/x\": https://x/\n",
            "  SpecialSchemeMissingFollowingSolidus \"\\/x\" false\n",
            "  SpecialSchemeMissingFollowingSolidus \"\\/x\" false\n",
            "\"http:\\/\\/01.02.03.04\": http://1.2.3.4/\n",
            "  Ipv4NonDecimalPart \"04\" false\n",
            "  Ipv4NonDecimalPart \"03.04\" false\n",
            "  Ipv4NonDecimalPart \"02.03.04\" false\n",
            "  Ipv4NonDecimalPart \"01.02.03.04\" false\n",
            "\"http:\\/\\/x.test\\/a b\": http://x.test/a%20b\n",
            "  InvalidUrlUnit \" b\" false\n",
            "\"http:\\/\\/x.test\\/a\\tb\": http://x.test/ab\n",
            "  InvalidUrlUnit \"\\tb\" false\n",
        )
    );
}

/// Verifies a network-relative reference inherits the base scheme for lexbor's host
/// warnings: credentials and IPv4 non-decimal parts are reported even though the reference
/// text has no scheme of its own.
#[test]
fn test_uri_whatwg_network_relative_warnings_use_the_base_scheme() {
    let out = compile_and_run(
        r#"<?php
$base = new Uri\WhatWg\Url("https://example.com/base");
foreach (["//user@example.com/", "//01.02.03.04/"] as $case) {
    $errors = [];
    $url = new Uri\WhatWg\Url($case, $base, $errors);
    echo json_encode($case), " => ", $url->toAsciiString(), "\n";
    foreach ($errors as $error) {
        echo "  ", $error->type->name, " ", var_export($error->failure, true), " ", json_encode($error->context), "\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        r#""\/\/user@example.com\/" => https://user@example.com/
  InvalidCredentials false "@example.com\/"
"\/\/01.02.03.04\/" => https://1.2.3.4/
  Ipv4NonDecimalPart false "04"
  Ipv4NonDecimalPart false "03.04"
  Ipv4NonDecimalPart false "02.03.04"
  Ipv4NonDecimalPart false "\/"
"#
    );
}

/// Verifies rejected IPv6 hosts report php-src's specific error kind instead of one generic
/// invalid-code-point reason.
#[test]
fn test_uri_whatwg_ipv6_failure_kinds() {
    let out = compile_and_run(
        r#"<?php
foreach (["http://[bad", "http://[::1::2]", "http://[::1:2:3:4:5:6:7:8]"] as $case) {
    try {
        new Uri\WhatWg\Url($case);
    } catch (Uri\WhatWg\InvalidUrlException $error) {
        echo $error->getMessage(), "\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "The specified URI is malformed (Ipv6Unclosed)\n",
            "The specified URI is malformed (Ipv6MultipleCompression)\n",
            "The specified URI is malformed (Ipv6TooManyPieces)\n",
        )
    );
}

/// Verifies a forbidden domain code point inside a non-ASCII label is rejected rather than
/// accepted with a space in the punycode host.
#[test]
fn test_uri_whatwg_forbidden_domain_code_point() {
    let out = compile_and_run(
        r#"<?php
try {
    new Uri\WhatWg\Url("https://EXAMPLE bücher.example:443/");
} catch (Uri\WhatWg\InvalidUrlException $error) {
    echo $error->getMessage(), "\n";
    echo $error->errors[0]->type->name, "\n";
}
"#,
    );
    assert_eq!(
        out,
        "The specified URI is malformed (DomainInvalidCodePoint)\nDomainInvalidCodePoint\n"
    );
}

/// Verifies an authority-only reference resolves to an empty path, without the spurious
/// trailing solidus.
#[test]
fn test_uri_rfc3986_authority_only_resolution() {
    let out = compile_and_run(
        r#"<?php
$base = new Uri\Rfc3986\Uri("http://a/b/c/d;p?q");
foreach (["//g", "//g?x", "//", "//g/"] as $reference) {
    echo $reference, " => ", $base->resolve($reference)->toString(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "//g => http://g\n//g?x => http://g?x\n// => http://\n//g/ => http://g/\n"
    );
}

/// Verifies a user class may declare a property named like the URI prelude's opaque handle
/// without any object renderer or the serializer hiding it.
#[test]
fn test_uri_handle_slot_name_does_not_hide_user_property() {
    let out = compile_and_run(
        r#"<?php
class UserUriNamedProperty {
    public string $__elephc_uri_handle = "mine";
    public string $other = "o";
}
$value = new UserUriNamedProperty();
var_dump($value);
var_dump((array) $value);
echo serialize($value), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "object(UserUriNamedProperty)#1 (2) {\n",
            "  [\"__elephc_uri_handle\"]=>\n",
            "  string(4) \"mine\"\n",
            "  [\"other\"]=>\n",
            "  string(1) \"o\"\n",
            "}\n",
            "array(2) {\n",
            "  [\"__elephc_uri_handle\"]=>\n",
            "  string(4) \"mine\"\n",
            "  [\"other\"]=>\n",
            "  string(1) \"o\"\n",
            "}\n",
            "O:20:\"UserUriNamedProperty\":2:{s:19:\"__elephc_uri_handle\";s:4:\"mine\";s:5:\"other\";s:1:\"o\";}\n",
        )
    );
}

/// Verifies opaque paths keep interior spaces and `^` literal, while the trailing space before
/// a query is the lexbor `%20` case pinned by the embedded case table.
#[test]
fn test_uri_whatwg_opaque_path_encoding() {
    let out = compile_and_run(
        r#"<?php
foreach (["urn:a b", "urn:x^y", "foo:a b ?c"] as $case) {
    $errors = [];
    $url = new Uri\WhatWg\Url($case, null, $errors);
    echo json_encode($case), " => ", $url->toAsciiString(), " path=", json_encode($url->getPath()), "\n";
    foreach ($errors as $error) {
        echo "  ", $error->type->name, " ", json_encode($error->context), "\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "\"urn:a b\" => urn:a b path=\"a b\"\n",
            "  InvalidUrlUnit \" b\"\n",
            "\"urn:x^y\" => urn:x^y path=\"x^y\"\n",
            "  InvalidUrlUnit \"^y\"\n",
            "\"foo:a b ?c\" => foo:a b ?c path=\"a b \"\n",
            "  InvalidUrlUnit \" ?c\"\n",
            "  InvalidUrlUnit \" b ?c\"\n",
        )
    );
}

/// Verifies the empty-host `file:` solidi and Windows drive-pipe repairs, plus
/// `InvalidReverseSoldius` for backslashes lexbor treats as solidi.
#[test]
fn test_uri_whatwg_file_and_backslash_repairs() {
    let out = compile_and_run(
        r#"<?php
foreach (["file:////x", "file://///x", "file:///c|/x", "file:\\x", "https://a\\b/c"] as $case) {
    $errors = [];
    $url = new Uri\WhatWg\Url($case, null, $errors);
    echo json_encode($case), " => ", $url->toAsciiString(), " path=", json_encode($url->getPath()), "\n";
    foreach ($errors as $error) {
        echo "  ", $error->type->name, " ", json_encode($error->context), "\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "\"file:\\/\\/\\/\\/x\" => file:////x path=\"\\/\\/x\"\n",
            "\"file:\\/\\/\\/\\/\\/x\" => file://///x path=\"\\/\\/\\/x\"\n",
            "\"file:\\/\\/\\/c|\\/x\" => file:///c:/x path=\"\\/c:\\/x\"\n",
            "  InvalidUrlUnit \"|\\/x\"\n",
            "\"file:\\\\x\" => file:///x path=\"\\/x\"\n",
            "  InvalidReverseSoldius \"\\\\x\"\n",
            "  SpecialSchemeMissingFollowingSolidus \"\\\\x\"\n",
            "\"https:\\/\\/a\\\\b\\/c\" => https://a/b/c path=\"\\/b\\/c\"\n",
            "  InvalidReverseSoldius \"\\\\b\\/c\"\n",
        )
    );
}

/// Verifies the URL-code-point warning set for brackets, braces, pipes, carets, and
/// non-special backslashes.
#[test]
fn test_uri_whatwg_invalid_url_unit_warnings() {
    let out = compile_and_run(
        r#"<?php
foreach (["urn:[x", "urn:}", "non:x\\y", "https://example.com/x|y", "http://x.test/a^b"] as $case) {
    $errors = [];
    $url = new Uri\WhatWg\Url($case, null, $errors);
    echo json_encode($case), " => ", $url->toAsciiString(), "\n";
    foreach ($errors as $error) {
        echo "  ", $error->type->name, " ", json_encode($error->context), "\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "\"urn:[x\" => urn:[x\n",
            "  InvalidUrlUnit \"[x\"\n",
            "\"urn:}\" => urn:}\n",
            "  InvalidUrlUnit \"}\"\n",
            "\"non:x\\\\y\" => non:x\\y\n",
            "  InvalidUrlUnit \"\\\\y\"\n",
            "\"https:\\/\\/example.com\\/x|y\" => https://example.com/x|y\n",
            "  InvalidUrlUnit \"|y\"\n",
            "\"http:\\/\\/x.test\\/a^b\" => http://x.test/a%5Eb\n",
            "  InvalidUrlUnit \"^b\"\n",
        )
    );
}

/// Verifies whitespace-only input (including an all-whitespace trim range) is a normal parse
/// failure rather than a slice panic at the C ABI boundary.
#[test]
fn test_uri_whatwg_whitespace_only_input() {
    let out = compile_and_run(
        r#"<?php
foreach ([" ", "\t", "   ", "\x1f ", ""] as $input) {
    try {
        new Uri\WhatWg\Url($input);
    } catch (Uri\WhatWg\InvalidUrlException $error) {
        echo json_encode($input), ": ", $error->getMessage(), "\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "\" \": The specified URI is malformed (MissingSchemeNonRelativeUrl)\n",
            "\"\\t\": The specified URI is malformed (MissingSchemeNonRelativeUrl)\n",
            "\"   \": The specified URI is malformed (MissingSchemeNonRelativeUrl)\n",
            "\"\\u001f \": The specified URI is malformed (MissingSchemeNonRelativeUrl)\n",
            "\"\": The specified URI is malformed (MissingSchemeNonRelativeUrl)\n",
        )
    );
}

/// Verifies WHATWG parsing keeps invalid UTF-8 bytes binary-safe: invalid path bytes are
/// percent-encoded, incomplete trailing sequences are dropped, and RFC mutations reject them.
#[test]
fn test_uri_invalid_utf8_bytes_are_binary_safe() {
    let out = compile_and_run(
        r#"<?php
$cases = [
    "http://example.com/\xff\xfe",
    "http://example.com/?q=\xc3",
    "http://x/#\xc3",
    "http://x/a\xc3\xa9b",
    "http://x/?q=a\xc3b",
    "http://x/a\xc3b",
    "urn:a\xc3b",
];
foreach ($cases as $input) {
    echo bin2hex($input), " => ";
    $url = new Uri\WhatWg\Url($input);
    echo $url->toAsciiString(), " path=", bin2hex($url->getPath()), " q=", bin2hex($url->getQuery() ?? ""), " f=", bin2hex($url->getFragment() ?? ""), "\n";
}
$rfc = new Uri\Rfc3986\Uri("https://x/");
try {
    $rfc->withPath("\xff");
} catch (Uri\InvalidUriException $error) {
    echo "rfc path: ", $error->getMessage(), "\n";
}
$whatwg = new Uri\WhatWg\Url("https://x/");
echo "whatwg path: ", $whatwg->withPath("/\xff")->toAsciiString(), "\n";
echo "whatwg path seq: ", $whatwg->withPath("/\xc3b")->toAsciiString(), "\n";
echo "whatwg query: ", $whatwg->withQuery("q=\xc3")->toAsciiString(), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "687474703a2f2f6578616d706c652e636f6d2ffffe =>",
            " http://example.com/%FF%FE path=2f254646254645 q= f=\n",
            "687474703a2f2f6578616d706c652e636f6d2f3f713dc3 =>",
            " http://example.com/?q= path=2f q=713d f=\n",
            "687474703a2f2f782f23c3 => http://x/# path=2f q= f=\n",
            "687474703a2f2f782f61c3a962 => http://x/a%C3%A9b path=2f6125433325413962 q= f=\n",
            "687474703a2f2f782f3f713d61c362 => http://x/?q=a%C3b path=2f q=713d6125433362 f=\n",
            "687474703a2f2f782f61c362 => http://x/a%C3%62 path=2f61254333253632 q= f=\n",
            "75726e3a61c362 => urn:a%C3b path=6125433362 q= f=\n",
            "rfc path: The specified path is malformed\n",
            "whatwg path: https://x/%FF\n",
            "whatwg path seq: https://x/%C3%62\n",
            "whatwg query: https://x/?q=%C3\n",
        )
    );
}

/// Verifies `property_exists()` also hides the URI prelude's opaque slot while a user class
/// with the same property name keeps it visible.
#[test]
fn test_uri_handle_slot_is_invisible_to_property_exists() {
    let out = compile_and_run(
        r#"<?php
class UserPropertyExists {
    public string $__elephc_uri_handle = "mine";
}
$uri = new Uri\Rfc3986\Uri("https://example.com/");
var_dump(property_exists($uri, "__elephc_uri_handle"));
var_dump(property_exists(new UserPropertyExists(), "__elephc_uri_handle"));
"#,
    );
    assert_eq!(out, "bool(false)\nbool(true)\n");
}

/// Verifies the internal readonly URI classes compile an undeclared write but raise php-src's
/// exact `Cannot create dynamic property` error at runtime, and that a later read warns and
/// answers null instead of creating storage.
#[test]
fn test_uri_dynamic_property_writes_raise_the_engine_error() {
    let out = compile_and_run_capture(
        r#"<?php
$rfc = new Uri\Rfc3986\Uri("https://example.com/");
try {
    $rfc->foo = 1;
    echo "stored\n";
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
$whatwg = new Uri\WhatWg\Url("https://example.com/");
try {
    $whatwg->foo = 1;
    echo "stored\n";
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
var_dump($rfc->foo);
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "Cannot create dynamic property Uri\\Rfc3986\\Uri::$foo\n\
Cannot create dynamic property Uri\\WhatWg\\Url::$foo\nNULL\n",
        "{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("Warning: Undefined property: Uri\\Rfc3986\\Uri::$foo"),
        "{}",
        out.stderr
    );
}

/// Verifies Reflection reports the engine shape of both URI classes: internal, readonly,
/// final, and the php-src modifiers bitmask.
#[test]
fn test_uri_reflection_reports_internal_readonly_shape() {
    let out = compile_and_run(
        r#"<?php
$rfc = new ReflectionClass(Uri\Rfc3986\Uri::class);
var_dump($rfc->isInternal(), $rfc->isReadOnly(), $rfc->isFinal(), $rfc->getModifiers());
$whatwg = new ReflectionClass(Uri\WhatWg\Url::class);
var_dump($whatwg->isInternal(), $whatwg->isReadOnly(), $whatwg->isFinal(), $whatwg->getModifiers());
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nbool(true)\nbool(true)\nint(65568)\nbool(true)\nbool(true)\nbool(true)\nint(65568)\n"
    );
}

/// Verifies lexbor's non-decimal IPv4 warnings: leading-zero decimal parts name the
/// part-to-host suffix, hexadecimal parts name the path or the part text, and values above
/// 255 add `Ipv4OutOfRangePart`.
#[test]
fn test_uri_whatwg_ipv4_non_decimal_warnings() {
    let out = compile_and_run(
        r#"<?php
foreach (["http://0x7f.1/", "http://0xffffffff/", "http://0x7f.0x1/", "http://0x7f.0x100/", "http://01.02.03.04/x", "http://0x100/"] as $case) {
    $errors = [];
    $url = new Uri\WhatWg\Url($case, null, $errors);
    echo json_encode($case), " => ", $url->toAsciiString(), "\n";
    foreach ($errors as $error) {
        echo "  ", $error->type->name, " ", json_encode($error->context), " ", var_export($error->failure, true), "\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "\"http:\\/\\/0x7f.1\\/\" => http://127.0.0.1/\n",
            "  Ipv4NonDecimalPart \"\\/\" false\n",
            "\"http:\\/\\/0xffffffff\\/\" => http://255.255.255.255/\n",
            "  Ipv4OutOfRangePart \"\\/\" true\n",
            "  Ipv4NonDecimalPart \"\\/\" false\n",
            "\"http:\\/\\/0x7f.0x1\\/\" => http://127.0.0.1/\n",
            "  Ipv4NonDecimalPart \"0x1\" false\n",
            "  Ipv4NonDecimalPart \"0x7f.0x1\" false\n",
            "\"http:\\/\\/0x7f.0x100\\/\" => http://127.0.1.0/\n",
            "  Ipv4OutOfRangePart \"0x100\" true\n",
            "  Ipv4NonDecimalPart \"0x100\" false\n",
            "  Ipv4NonDecimalPart \"\\/\" false\n",
            "\"http:\\/\\/01.02.03.04\\/x\" => http://1.2.3.4/x\n",
            "  Ipv4NonDecimalPart \"04\" false\n",
            "  Ipv4NonDecimalPart \"03.04\" false\n",
            "  Ipv4NonDecimalPart \"02.03.04\" false\n",
            "  Ipv4NonDecimalPart \"01.02.03.04\" false\n",
            "\"http:\\/\\/0x100\\/\" => http://0.0.1.0/\n",
            "  Ipv4OutOfRangePart \"\\/\" true\n",
            "  Ipv4NonDecimalPart \"\\/\" false\n",
        )
    );
}

/// Verifies each of the host, query, and fragment native strings is charged against the PHP
/// memory budget, not only the path-segment footprint. Each case keeps the input string alive
/// before the measurement, so the delta is the URI object's own charge.
#[test]
fn test_uri_bridge_charges_host_query_fragment_storage() {
    let out = compile_and_run(
        r#"<?php
function measure_rfc(string $uri): int {
    $before = memory_get_usage();
    $object = new Uri\Rfc3986\Uri($uri);
    $after = memory_get_usage();
    unset($object);
    return $after - $before;
}
function measure_whatwg(string $uri): int {
    $before = memory_get_usage();
    $object = Uri\WhatWg\Url::parse($uri);
    $after = memory_get_usage();
    unset($object);
    return $after - $before;
}
$host = str_repeat("a.", 50000) . "com";
$query = str_repeat("q", 100000);
$fragment = str_repeat("f", 100000);
foreach ([
    "host" => "https://$host/",
    "query" => "https://example.com/?$query",
    "fragment" => "https://example.com/#$fragment",
] as $name => $uri) {
    $rfc = measure_rfc($uri);
    $whatwg = measure_whatwg($uri);
    echo $name, ": ", $rfc >= 100000 ? "rfc" : "rfc-missing", " ",
        $whatwg >= 100000 ? "whatwg" : "whatwg-missing", "\n";
}
"#,
    );
    assert_eq!(
        out,
        "host: rfc whatwg\nquery: rfc whatwg\nfragment: rfc whatwg\n"
    );
}

/// Verifies the native charge follows the URI object's whole lifecycle: clone, component
/// writes, host writes, and `unserialize()` all publish a fresh charge for the new handle.
#[test]
fn test_uri_bridge_charge_lifecycle_covers_clone_writes_and_unserialize() {
    let out = compile_and_run(
        r#"<?php
function measure_clone(Uri\WhatWg\Url $base): int {
    $before = memory_get_usage();
    $copy = clone $base;
    $after = memory_get_usage();
    unset($copy);
    return $after - $before;
}
function measure_with(Uri\WhatWg\Url $base, int $component, string $value): int {
    $before = memory_get_usage();
    $changed = $component === 0 ? $base->withQuery($value) : $base->withFragment($value);
    $after = memory_get_usage();
    unset($changed);
    return $after - $before;
}
function measure_host(Uri\WhatWg\Url $base, string $host): int {
    $before = memory_get_usage();
    $changed = $base->withHost($host);
    $after = memory_get_usage();
    unset($changed);
    return $after - $before;
}
function measure_unserialize(string $serialized): int {
    $before = memory_get_usage();
    $restored = unserialize($serialized);
    $after = memory_get_usage();
    unset($restored);
    return $after - $before;
}
$query = str_repeat("q", 100000);
$host = str_repeat("a.", 50000) . "com";
$base = new Uri\WhatWg\Url("https://example.com/x?$query");
$serialized = serialize($base);
foreach ([
    "clone" => measure_clone($base),
    "query" => measure_with($base, 0, $query),
    "fragment" => measure_with($base, 1, $query),
    "host" => measure_host($base, $host),
    "unserialize" => measure_unserialize($serialized),
] as $name => $delta) {
    echo $name, "=", $delta >= 100000 ? "ok" : "missing", "\n";
}
"#,
    );
    assert_eq!(
        out,
        "clone=ok\nquery=ok\nfragment=ok\nhost=ok\nunserialize=ok\n"
    );
}

/// A scheme write does not turn a schemed colon path into a `./`-prefixed one; removing the
/// scheme is the case that needs the disambiguating prefix.
#[test]
fn test_uri_rfc3986_scheme_and_path_writes_preserve_schemed_colon_paths() {
    let out = compile_and_run(
        r#"<?php
$uri = new Uri\Rfc3986\Uri("mailto:a:b");
echo $uri->toString(), "|", $uri->getRawPath(), "\n";
$sameScheme = $uri->withScheme("mailto");
echo $sameScheme->toString(), "|", $sameScheme->getRawPath(), "\n";
$samePath = $uri->withPath("a:b");
echo $samePath->toString(), "|", $samePath->getRawPath(), "\n";
$noScheme = $uri->withScheme(null);
echo $noScheme->toString(), "|", $noScheme->getRawPath(), "\n";
"#,
    );
    assert_eq!(
        out,
        "mailto:a:b|a:b\nmailto:a:b|a:b\nmailto:a:b|a:b\n./a:b|./a:b\n"
    );
}

/// A direct destructor call is rejected with a catchable `Error` and leaves the native handle
/// intact; automatic cleanup still releases it.
#[test]
fn test_uri_direct_destructor_call_is_rejected_without_freeing_the_handle() {
    let out = compile_and_run(
        r#"<?php
$rfc = new Uri\Rfc3986\Uri("https://example.com/a");
try {
    $rfc->__destruct();
    echo "freed\n";
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
echo $rfc->toString(), "\n";
$whatwg = new Uri\WhatWg\Url("https://example.com/a");
try {
    $whatwg->__destruct();
    echo "freed\n";
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
echo $whatwg->toAsciiString(), "\n";
unset($rfc, $whatwg);
echo "ok\n";
"#,
    );
    assert_eq!(
        out,
        "Call to undefined method Uri\\Rfc3986\\Uri::__destruct()\nhttps://example.com/a\nCall to undefined method Uri\\WhatWg\\Url::__destruct()\nhttps://example.com/a\nok\n"
    );
}

/// Verifies lexbor-style Punycode recovery for `xn--` labels the strict `idna` crate rejects.
#[test]
fn test_uri_whatwg_relaxed_alabel_normalization() {
    let out = compile_and_run(
        r#"<?php
$issueErrors = [];
$issue = Uri\WhatWg\Url::parse("https://xn--a-.com", null, $issueErrors);
echo $issue === null ? "null" : $issue->toAsciiString(), "|", count($issueErrors), "\n";

foreach ([
    "https://xn--a-.com",
    "https://xn--a-.com.",
    "https://xn--a-..com/",
    "https://xn--./",
    "https://xn--9ca-.com",
    "https://xn--bcher-kva-.com",
    "https://xn--a",
    "https://xn--a.com",
    "https://xn--0x7f-.1/",
    "https://xn--0x100-.1/",
] as $case) {
    $url = Uri\WhatWg\Url::parse($case);
    echo $url === null ? "null" : $url->toAsciiString(), "\n";
}

$base = new Uri\WhatWg\Url("https://example.com/");
foreach (["https://xn--a-.com/", "https://xn--9ca-.com/"] as $case) {
    $parsed = Uri\WhatWg\Url::parse($case, $base);
    $resolved = $base->resolve($case);
    echo $parsed->toAsciiString(), "|", $resolved->toAsciiString(), "\n";
}

$errors = [];
$repaired = Uri\WhatWg\Url::parse("https://xn--a-.com/a^b", $base, $errors);
echo $repaired->toAsciiString(), "|", $repaired->getPath(), "\n";
foreach ($errors as $error) {
    echo "  ", $error->type->name, " ", json_encode($error->context), "\n";
}

$network = $base->resolve("//xn--a-.com/a^b");
echo $network->toAsciiString(), "|", $network->getPath(), "\n";

$hosted = (new Uri\WhatWg\Url("https://example.com/a?q#f"))->withHost("xn--a-.com");
echo $hosted->toAsciiString(), "|", $hosted->getAsciiHost(), "\n";

$invalid = new Uri\WhatWg\Url("https://example.com/");
try {
    $invalid->withHost("EXAMPLE bücher.example");
    echo "accepted\n";
} catch (Uri\WhatWg\InvalidUrlException $error) {
    echo $error->getMessage(), "|", count($error->errors), "|", $error->errors[0]->type->name, "\n";
}

$backslashed = $base->resolve("\\\\xn--a-.com/p?q\\z#f\\g");
echo $backslashed->toAsciiString(), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "https://a.com/|0\n",
            "https://a.com/\n",
            "https://a.com./\n",
            "https://a..com/\n",
            "https://./\n",
            "https://9ca.com/\n",
            "https://bcher-kva.com/\n",
            "null\n",
            "null\n",
            "https://127.0.0.1/\n",
            "null\n",
            "https://a.com/|https://a.com/\n",
            "https://9ca.com/|https://9ca.com/\n",
            "https://a.com/a%5Eb|/a%5Eb\n",
            "  InvalidUrlUnit \"^b\"\n",
            "https://a.com/a%5Eb|/a%5Eb\n",
            "https://a.com/a?q#f|a.com\n",
            "The specified host is malformed (DomainInvalidCodePoint)|1|DomainInvalidCodePoint\n",
            "https://a.com/p?q\\z#f\\g\n",
        )
    );
}

/// Verifies php-src's reflected URI parameter types and the exact runtime `TypeError` wording.
#[test]
fn test_uri_signatures_and_runtime_type_errors_match_php() {
    let out = compile_and_run(
        r#"<?php
function badUri(): mixed { return "not a uri"; }
$parse = new ReflectionMethod(Uri\Rfc3986\Uri::class, "parse");
foreach ($parse->getParameters() as $parameter) {
    echo $parameter->getName(), ":", $parameter->getType(), "\n";
}
$equals = new ReflectionMethod(Uri\WhatWg\Url::class, "equals");
foreach ($equals->getParameters() as $parameter) {
    echo $parameter->getName(), ":", $parameter->getType(), "\n";
}
$whatwgParse = new ReflectionMethod(Uri\WhatWg\Url::class, "parse");
foreach ($whatwgParse->getParameters() as $parameter) {
    echo $parameter->getName(), ":", $parameter->hasType() ? $parameter->getType() : "none", "\n";
}
$rfcEquals = new ReflectionMethod(Uri\Rfc3986\Uri::class, "equals");
foreach ($rfcEquals->getParameters() as $parameter) {
    echo $parameter->getName(), ":", $parameter->getType(), "\n";
}
$bad = badUri();
$base = Uri\Rfc3986\Uri::parse("https://example.com/base");
$whatwgBase = new Uri\WhatWg\Url("https://example.com");
echo Uri\Rfc3986\Uri::parse("child", $base)->toString(), "\n";
try {
    $base->equals($bad);
} catch (Throwable $error) {
    echo $error::class, ": ", $error->getMessage(), "\n";
}
try {
    new Uri\WhatWg\Url("https://example.com", $bad);
} catch (Throwable $error) {
    echo $error::class, ": ", $error->getMessage(), "\n";
}
try {
    Uri\WhatWg\Url::parse("child", $bad);
} catch (Throwable $error) {
    echo $error::class, ": ", $error->getMessage(), "\n";
}
try {
    $whatwgBase->equals($bad);
} catch (Throwable $error) {
    echo $error::class, ": ", $error->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "uri:string\n",
            "baseUrl:?Uri\\Rfc3986\\Uri\n",
            "url:Uri\\WhatWg\\Url\n",
            "comparisonMode:Uri\\UriComparisonMode\n",
            "uri:string\n",
            "baseUrl:?Uri\\WhatWg\\Url\n",
            "errors:none\n",
            "uri:Uri\\Rfc3986\\Uri\n",
            "comparisonMode:Uri\\UriComparisonMode\n",
            "https://example.com/child\n",
            "TypeError: Uri\\Rfc3986\\Uri::equals(): Argument #1 ($uri) must be of type Uri\\Rfc3986\\Uri, string given\n",
            "TypeError: Uri\\WhatWg\\Url::__construct(): Argument #2 ($baseUrl) must be of type ?Uri\\WhatWg\\Url, string given\n",
            "TypeError: Uri\\WhatWg\\Url::parse(): Argument #2 ($baseUrl) must be of type ?Uri\\WhatWg\\Url, string given\n",
            "TypeError: Uri\\WhatWg\\Url::equals(): Argument #1 ($url) must be of type Uri\\WhatWg\\Url, string given\n",
        )
    );
}

/// Verifies WHATWG fatal validation entries carry php-src's exact error kind and context
/// suffix, plus the soft warnings lexbor emits for extra solidi and an empty IPv4 final part.
#[test]
fn test_uri_whatwg_failure_contexts_match_php() {
    let out = compile_and_run(
        r#"<?php
$cases = [
    "https://[::1:2:3:4:5:6:7:8]/",
    "https://[::1.2.3.256]/",
    "https://[1:2:3]/",
    "https://1.2.3.256/",
    "https://1.2.3.4.5/",
    "https://example.com:99999/",
    "https://example.com:99a/",
    "https://:80/",
    "https://user@:80/",
    "https://exa mple.com/",
    "https://exa%23mple.org/",
    "https://exa<mple.com/",
    "foo://exa[mple.org/",
    "https:///path",
    "https:////path",
    "http:///",
    "file://c:/x",
    "https://xn--",
    "https://1.2.3./",
];
foreach ($cases as $case) {
    $errors = [];
    $url = Uri\WhatWg\Url::parse($case, null, $errors);
    echo json_encode($case), " => ", $url === null ? "null" : $url->toAsciiString(), "\n";
    foreach ($errors as $error) {
        echo "  ", $error->type->name, " ", var_export($error->failure, true), " ", json_encode($error->context), "\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        r##""https:\/\/[::1:2:3:4:5:6:7:8]\/" => null
  Ipv6TooManyPieces true "8]\/"
"https:\/\/[::1.2.3.256]\/" => null
  Ipv4InIpv6OutOfRangePart true "6]\/"
"https:\/\/[1:2:3]\/" => null
  Ipv6TooFewPieces true "]\/"
"https:\/\/1.2.3.256\/" => null
  Ipv4OutOfRangePart true "256"
"https:\/\/1.2.3.4.5\/" => null
  Ipv4TooManyParts true "5"
"https:\/\/example.com:99999\/" => null
  PortOutOfRange true "\/"
"https:\/\/example.com:99a\/" => null
  PortInvalid true "a\/"
"https:\/\/:80\/" => null
  HostMissing true ":80\/"
"https:\/\/user@:80\/" => null
  HostMissing true ":80\/"
  InvalidCredentials false "@:80\/"
"https:\/\/exa mple.com\/" => null
  DomainInvalidCodePoint true " mple.com"
"https:\/\/exa%23mple.org\/" => null
  DomainInvalidCodePoint true "#mple.org"
"https:\/\/exa<mple.com\/" => null
  DomainInvalidCodePoint true "<mple.com"
"foo:\/\/exa[mple.org\/" => null
  HostInvalidCodePoint true "[mple.org\/"
"https:\/\/\/path" => https://path/
  SpecialSchemeMissingFollowingSolidus false "\/path"
"https:\/\/\/\/path" => https://path/
  SpecialSchemeMissingFollowingSolidus false "\/path"
  SpecialSchemeMissingFollowingSolidus false "\/\/path"
"http:\/\/\/" => null
  HostMissing true ""
  SpecialSchemeMissingFollowingSolidus false "\/"
"file:\/\/c:\/x" => file:///c:/x
  FileInvalidWindowsDriveLetterHost false "c:\/x"
"https:\/\/xn--" => null
"https:\/\/1.2.3.\/" => https://1.2.0.3/
  Ipv4EmptyPart false ""
"##
    );
}

/// Verifies lexbor's special-scheme solidus diagnostics for repeated backslash/slash runs,
/// including the duplicated full-run entry and the file-scheme reverse-solidus pair.
#[test]
fn test_uri_whatwg_special_scheme_solidus_warnings() {
    let out = compile_and_run(
        r#"<?php
$cases = ["https:\\\\x", "https://\\x", "https:x", "file:\\/x", "file:\\/\\/x"];
foreach ($cases as $case) {
    $errors = [];
    $url = new Uri\WhatWg\Url($case, null, $errors);
    echo json_encode($case), " => ", $url->toAsciiString(), "\n";
    foreach ($errors as $error) {
        echo "  ", $error->type->name, " ", json_encode($error->context), "\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "\"https:\\\\\\\\x\" => https://x/\n",
            "  SpecialSchemeMissingFollowingSolidus \"\\\\x\"\n",
            "  SpecialSchemeMissingFollowingSolidus \"\\\\\\\\x\"\n",
            "  SpecialSchemeMissingFollowingSolidus \"\\\\\\\\x\"\n",
            "\"https:\\/\\/\\\\x\" => https://x/\n",
            "  SpecialSchemeMissingFollowingSolidus \"\\\\x\"\n",
            "\"https:x\" => https://x/\n",
            "  SpecialSchemeMissingFollowingSolidus \"x\"\n",
            "\"file:\\\\\\/x\" => file://x/\n",
            "  InvalidReverseSoldius \"\\\\\\/x\"\n",
            "  SpecialSchemeMissingFollowingSolidus \"\\\\\\/x\"\n",
            "\"file:\\\\\\/\\\\\\/x\" => file:////x\n",
            "  InvalidReverseSoldius \"\\\\\\/x\"\n",
            "  InvalidReverseSoldius \"\\\\\\/\\\\\\/x\"\n",
            "  SpecialSchemeMissingFollowingSolidus \"\\\\\\/\\\\\\/x\"\n",
        )
    );
}

/// Verifies the heap-exhaustion fatal uses php-src's message shape, bytes, and stderr stream.
#[test]
fn test_uri_memory_limit_fatal_matches_php() {
    let output = compile_and_run_capture_with_heap_size(
        r#"<?php
Uri\Rfc3986\Uri::parse(str_repeat("/", 1024 * 1024));
echo "not reached\n";
"#,
        8 * 1024 * 1024,
    );
    assert!(!output.success, "the program should abort on heap exhaustion");
    assert!(output.stdout.is_empty(), "stdout: {}", output.stdout);
    assert!(
        output
            .stderr
            .starts_with("Fatal error: Allowed memory size of 8388608 bytes exhausted "),
        "stderr: {}",
        output.stderr
    );
    assert!(
        output.stderr.contains("(tried to allocate "),
        "stderr: {}",
        output.stderr
    );
}

/// `clone` and `with*()` results own independent native handles: the original and clone stay
/// unchanged across many mutations, and releasing all three leaves a clean heap. This also
/// covers the rebind path that copies the PHP object without cloning its bridge handle.
#[test]
fn test_uri_clone_and_with_keep_handles_independent() {
    let dir = make_cli_test_dir("uri_clone_with_independence");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
$u = new Uri\Rfc3986\Uri("https://example.com/a/b");
$c = clone $u;
for ($i = 0; $i < 200; $i++) {
    $m = $u->withPath("/x/" . $i);
}
echo $u->toString(), "|", $c->toString(), "|", $m->toString(), "\n";
unset($u, $c, $m);
echo "ok\n";
"#,
    )
    .unwrap();
    let compile = elephc_cli_command(&dir)
        .arg("--heap-debug")
        .arg(&php_path)
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = run_binary(&dir.join("main"), &dir);
    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    fs::remove_dir_all(&dir).unwrap();
    assert_eq!(
        stdout,
        "https://example.com/a/b|https://example.com/a/b|https://example.com/x/199\nok\n",
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("HEAP DEBUG: leak summary: clean"),
        "URI handle or charge leaked: {stderr}"
    );
}

/// Verifies a resolved WHATWG reference inherits the base's saved host and file-path forms.
#[test]
fn test_uri_whatwg_resolution_preserves_base_overrides() {
    let out = compile_and_run(
        r##"<?php
$base = new Uri\WhatWg\Url("https://xn--a-.com/");
$resolved = $base->resolve("child");
echo $resolved->toAsciiString(), "|", $resolved->getAsciiHost(), "\n";

$file = new Uri\WhatWg\Url("file:////x/");
foreach (["child", "../y", "/z", "?q", "#f", ""] as $reference) {
    $url = $file->resolve($reference);
    echo "[", $reference, "] => ", $url->toAsciiString(), "|", $url->getPath(), "\n";
}

$hosted = (new Uri\WhatWg\Url("file:////x/"))->withHost("example.com")->withScheme("https");
foreach (["child", "../y", "/z", "?q", "#f", ""] as $reference) {
    $url = $hosted->resolve($reference);
    echo "hosted[", $reference, "] => ", $url->toAsciiString(), "|", $url->getPath(), "\n";
}
"##,
    );
    assert_eq!(
        out,
        concat!(
            "https://a.com/child|a.com\n",
            "[child] => file:////x/child|//x/child\n",
            "[../y] => file:////y|//y\n",
            "[/z] => file:///z|/z\n",
            "[?q] => file:////x/?q|//x/\n",
            "[#f] => file:////x/#f|//x/\n",
            "[] => file:////x/|//x/\n",
            "hosted[child] => https://example.com//x/child|//x/child\n",
            "hosted[../y] => https://example.com//y|//y\n",
            "hosted[/z] => https://example.com/z|/z\n",
            "hosted[?q] => https://example.com//x/?q|//x/\n",
            "hosted[#f] => https://example.com//x/#f|//x/\n",
            "hosted[] => https://example.com//x/|//x/\n",
        )
    );
}

/// Verifies unrelated WHATWG component writes keep the saved file path, and host/scheme
/// changes preserve php-src's path component.
#[test]
fn test_uri_whatwg_unrelated_writes_preserve_file_path() {
    let out = compile_and_run(
        r#"<?php
$url = new Uri\WhatWg\Url("file:////x");
echo $url->withScheme("file")->toAsciiString(), "|", $url->getPath(), "\n";
echo $url->withHost("")->toAsciiString(), "|", $url->getPath(), "\n";
$hosted = $url->withHost("example.com");
echo $hosted->toAsciiString(), "|", $hosted->getPath(), "\n";
echo $hosted->withScheme("https")->toAsciiString(), "|", $hosted->getPath(), "\n";
echo $url->withPath("/y")->toAsciiString(), "|", $url->withPath("/y")->getPath(), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "file:////x|//x\n",
            "file:////x|//x\n",
            "file://example.com//x|//x\n",
            "https://example.com//x|//x\n",
            "file:///y|/y\n",
        )
    );
}

/// Verifies a long single path segment is charged by its retained byte length, so the
/// native path cannot hide behind the per-segment node estimate.
#[test]
fn test_uri_bridge_charges_path_bytes() {
    let out = compile_and_run(
        r#"<?php
$before = memory_get_usage();
$rfc = new Uri\Rfc3986\Uri("https://example.com/" . str_repeat("a", 100000));
$afterRfc = memory_get_usage();
$whatwg = Uri\WhatWg\Url::parse("https://example.com/" . str_repeat("a", 100000));
$afterWhatwg = memory_get_usage();
echo ($afterRfc - $before) >= 100000 ? "rfc\n" : "rfc-missing\n";
echo ($afterWhatwg - $afterRfc) >= 100000 ? "whatwg\n" : "whatwg-missing\n";
"#,
    );
    assert_eq!(out, "rfc\nwhatwg\n");
}

/// Verifies a dynamic/null-safe `ReflectionClass::newInstanceWithoutConstructor()` still
/// refuses the internal-final URI classes with php-src's exact `ReflectionException`.
#[test]
fn test_uri_reflection_constructorless_refusal_survives_dynamic_class_name() {
    let out = compile_and_run(
        r#"<?php
$reflection = new ReflectionClass(Uri\Rfc3986\Uri::class);
try {
    $reflection?->newInstanceWithoutConstructor();
    echo "created\n";
} catch (ReflectionException $error) {
    echo $error->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "Class Uri\\Rfc3986\\Uri is an internal class marked as final that cannot be instantiated without invoking its constructor\n"
    );
}

/// Verifies a direct `__clone()` call cannot replace and leak the original native handle,
/// while the `clone` operator still produces an independent native value.
#[test]
fn test_uri_direct_clone_call_does_not_leak_the_native_handle() {
    let out = compile_and_run(
        r#"<?php
$rfc = new Uri\Rfc3986\Uri("https://example.com/a");
try {
    $rfc->__clone();
    echo "bad\n";
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
$rfcCopy = clone $rfc;
echo $rfc->toString(), "|", $rfcCopy->toString(), "\n";

$url = new Uri\WhatWg\Url("https://example.com/a");
try {
    $url->__clone();
    echo "bad\n";
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
$urlCopy = clone $url;
echo $url->toAsciiString(), "|", $urlCopy->toAsciiString(), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "Call to undefined method Uri\\Rfc3986\\Uri::__clone()\n",
            "https://example.com/a|https://example.com/a\n",
            "Call to undefined method Uri\\WhatWg\\Url::__clone()\n",
            "https://example.com/a|https://example.com/a\n",
        )
    );
}

/// Verifies resolving a relative reference still reports lexbor's component warnings, using
/// the base scheme to distinguish path reverse solidi from query/fragment invalid units.
#[test]
fn test_uri_whatwg_relative_reference_warnings() {
    let out = compile_and_run(
        r##"<?php
$base = new Uri\WhatWg\Url("https://example.com/base");
foreach (["/a b", "/a%", "a b", "a%", "?a b", "#a%", "a\b"] as $reference) {
    $errors = [];
    $url = $base->resolve($reference, $errors);
    echo "[", $reference, "] => ", $url->toAsciiString(), "\n";
    foreach ($errors as $error) {
        echo "  ", $error->type->name, " ", json_encode($error->context), "\n";
    }
}
"##,
    );
    assert_eq!(
        out,
        concat!(
            "[/a b] => https://example.com/a%20b\n",
            "  InvalidUrlUnit \" b\"\n",
            "[/a%] => https://example.com/a%\n",
            "  InvalidUrlUnit \"%\"\n",
            "[a b] => https://example.com/a%20b\n",
            "  InvalidUrlUnit \" b\"\n",
            "[a%] => https://example.com/a%\n",
            "  InvalidUrlUnit \"%\"\n",
            "[?a b] => https://example.com/base?a%20b\n",
            "  InvalidUrlUnit \" b\"\n",
            "[#a%] => https://example.com/base#a%\n",
            "  InvalidUrlUnit \"%\"\n",
            "[a\\b] => https://example.com/a/b\n",
            "  InvalidReverseSoldius \"\\\\b\"\n",
        )
    );
}

/// Verifies `--with-uri` is rejected on a PHP 8.4 profile instead of exposing a misleading
/// `extension_loaded('uri')` while every URI class stays undeclared.
#[test]
fn test_uri_with_flag_rejects_php_84_profile() {
    let stderr = compile_cli_file_with_flags_expect_failure(
        "<?php echo 1;",
        &["--php-version", "8.4", "--with-uri"],
    );
    assert!(
        stderr.contains("--with-uri requires --php-version 8.5 or later"),
        "unexpected diagnostic: {stderr}"
    );
}

/// Verifies a hostile WHATWG input with thousands of forbidden units parses without
/// materializing a quadratic soft-error list when the caller passes no by-reference slot.
#[test]
fn test_uri_whatwg_hostile_input_stays_linear_without_soft_errors() {
    let out = compile_and_run(
        r#"<?php
$url = new Uri\WhatWg\Url("http://h/" . str_repeat("%zz", 8192));
echo $url->getAsciiHost(), ":", strlen($url->getPath()), "\n";
"#,
    );
    assert_eq!(out, "h:24577\n");
}

/// Verifies IDNA recovery rejects hosts containing forbidden domain code points (`%`, DEL)
/// instead of encoding them into an `xn--` label.
#[test]
fn test_uri_whatwg_idna_rejects_forbidden_domain_code_points() {
    let out = compile_and_run(
        r#"<?php
foreach (["http://é%40x.com/", "http://é%2f.com/", "http://é\x7f.com/"] as $case) {
    $url = Uri\WhatWg\Url::parse($case);
    echo $url === null ? "null" : $url->toAsciiString(), "\n";
}
"#,
    );
    assert_eq!(out, "null\nnull\nnull\n");
}

/// Verifies RFC 3986 `resolve()` removes dot segments from an absolute reference's own path,
/// matching RFC 3986 5.2.2 rather than only for relative references.
#[test]
fn test_uri_rfc3986_resolve_removes_dot_segments_from_absolute_references() {
    let out = compile_and_run(
        r#"<?php
$base = new Uri\Rfc3986\Uri("http://example.com/base");
foreach (["x:a/../b", "x:/a/../b", "x:a/./b", "x:a/../../b"] as $reference) {
    echo $reference, " => ", $base->resolve($reference)->toString(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "x:a/../b => x:b\nx:/a/../b => x:/b\nx:a/./b => x:a/b\nx:a/../../b => x:b\n"
    );
}

/// Verifies WHATWG mutators and base resolution match lexbor: an opaque path ignores
/// `withPath`, `withHost` truncates at a URL delimiter, extra leading slashes form a
/// network-path reference, and IPv4-mapped IPv6 uses the normalized spelling.
#[test]
fn test_uri_whatwg_mutators_and_resolution_values() {
    let out = compile_and_run(
        r#"<?php
$opaque = new Uri\WhatWg\Url("mailto:a@b");
foreach (["x y", "/x", ""] as $path) {
    echo json_encode($path), " => ", $opaque->withPath($path)->toAsciiString(), "\n";
}
$host = new Uri\WhatWg\Url("https://example.com/a");
foreach (["h/x", "h\\x", "h?x", "h#x", "h/x?y#z"] as $value) {
    echo json_encode($value), " => ", $host->withHost($value)->toAsciiString(), "\n";
}
$base = new Uri\WhatWg\Url("http://example.com/base");
foreach (["////x", "/////x", "\\\\x"] as $reference) {
    echo json_encode($reference), " => ", (new Uri\WhatWg\Url($reference, $base))->toAsciiString(), "\n";
}
foreach (["http://[::ffff:1.2.3.4]/", "http://[::1.2.3.4]/"] as $spelling) {
    $url = new Uri\WhatWg\Url($spelling);
    echo $url->getUnicodeHost(), "|", $url->toUnicodeString(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "\"x y\" => mailto:a@b\n",
            "\"\\/x\" => mailto:a@b\n",
            "\"\" => mailto:a@b\n",
            "\"h\\/x\" => https://h/a\n",
            "\"h\\\\x\" => https://h/a\n",
            "\"h?x\" => https://h/a\n",
            "\"h#x\" => https://h/a\n",
            "\"h\\/x?y#z\" => https://h/a\n",
            "\"\\/\\/\\/\\/x\" => http://x/\n",
            "\"\\/\\/\\/\\/\\/x\" => http://x/\n",
            "\"\\\\\\\\x\" => http://x/\n",
            "[::ffff:102:304]|http://[::ffff:102:304]/\n",
            "[::102:304]|http://[::102:304]/\n",
        )
    );
}

/// Verifies a colon inside a relative path or authority is not mistaken for a scheme
/// separator, so the reference inherits the base scheme's warnings (`InvalidReverseSoldius`
/// for a backslash path, `InvalidCredentials` for userinfo).
#[test]
fn test_uri_whatwg_relative_reference_colons_are_not_schemes() {
    let out = compile_and_run(
        r#"<?php
$errors = [];
$url = Uri\WhatWg\Url::parse("/x:y\\z", new Uri\WhatWg\Url("https://example.com/"), $errors);
echo $url->toAsciiString(), "|", count($errors), "|", $errors[0]->type->name, "\n";
$errors = [];
$url = Uri\WhatWg\Url::parse("//user@example.com:443/", new Uri\WhatWg\Url("https://base/"), $errors);
echo $url->toAsciiString(), "|", count($errors), "|", $errors[0]->type->name, "\n";
"#,
    );
    assert_eq!(
        out,
        "https://example.com/x:y/z|1|InvalidReverseSoldius\nhttps://user@example.com/|1|InvalidCredentials\n"
    );
}

/// Verifies an IPvFuture address rejects percent escapes: RFC 3986 only allows unreserved,
/// sub-delims and `:` in its tail, both from the constructor and from `withHost()`.
#[test]
fn test_uri_rfc3986_ipvfuture_rejects_percent_escapes() {
    let out = compile_and_run(
        r#"<?php
foreach (["https://[v1.%41]/", "https://[v1.]/"] as $spelling) {
    try { new Uri\Rfc3986\Uri($spelling); echo "accepted|"; }
    catch (Uri\InvalidUriException $error) { echo "malformed|"; }
}
echo new Uri\Rfc3986\Uri("https://[v1.abc]/")->toString(), "|";
echo new Uri\Rfc3986\Uri("https://[vF.a:b]/")->toString(), "\n";
$base = new Uri\Rfc3986\Uri("https://example.com/");
try { $base->withHost("[v1.%41]"); echo "accepted\n"; }
catch (Uri\InvalidUriException $error) { echo "malformed\n"; }
"#,
    );
    assert_eq!(
        out,
        "malformed|malformed|https://[v1.abc]/|https://[vf.a:b]/\nmalformed\n"
    );
}

/// Verifies reflection hides the prelude's native handle storage and compiler helpers, and
/// reports the same method/property counts and membership tests as php-src's internal classes.
#[test]
fn test_uri_reflection_hides_internal_members() {
    let out = compile_and_run(
        r#"<?php
$rfc = new ReflectionClass(Uri\Rfc3986\Uri::class);
echo count($rfc->getMethods()), "|", count($rfc->getProperties()), "|",
    (int) $rfc->hasMethod("__destruct"), "|", (int) $rfc->hasMethod("__clone"), "|",
    (int) method_exists(Uri\Rfc3986\Uri::class, "__destruct"), "|",
    (int) method_exists(Uri\Rfc3986\Uri::class, "__elephc_with"), "|",
    (int) property_exists(Uri\Rfc3986\Uri::class, "__elephc_uri_handle"), "\n";
$whatwg = new ReflectionClass(Uri\WhatWg\Url::class);
echo count($whatwg->getMethods()), "|", count($whatwg->getProperties()), "|",
    (int) $whatwg->hasMethod("__destruct"), "|", (int) $whatwg->hasMethod("__clone"), "|",
    (int) method_exists(Uri\WhatWg\Url::class, "__destruct"), "|",
    (int) method_exists(Uri\WhatWg\Url::class, "__elephc_with"), "|",
    (int) property_exists(Uri\WhatWg\Url::class, "__elephc_uri_handle"), "\n";
"#,
    );
    assert_eq!(
        out,
        "35|0|0|0|0|0|0\n28|0|0|0|0|0|0\n"
    );
}

/// Verifies a supplied by-reference soft-error variable receives the parse warnings even when
/// it starts as null, while an omitted argument leaves no caller storage to update.
#[test]
fn test_uri_whatwg_supplied_null_soft_errors_receive_warnings() {
    let out = compile_and_run(
        r#"<?php
$errors = null;
new Uri\WhatWg\Url("https://example.com/a%", null, $errors);
echo count($errors), "|", $errors[0]->type->name, "\n";
$errors = null;
Uri\WhatWg\Url::parse("https://example.com/a%", errors: $errors);
echo count($errors), "|", $errors[0]->type->name, "\n";
$errors = null;
new Uri\WhatWg\Url("https://example.com/", null, $errors);
var_dump($errors);
"#,
    );
    assert_eq!(
        out,
        "1|InvalidUrlUnit\n1|InvalidUrlUnit\narray(0) {\n}\n"
    );
}

/// Verifies extra leading separators keep their PATH meaning for `file:` and non-special
/// bases, while a special non-file base still collapses them into an authority.
#[test]
fn test_uri_whatwg_extra_solidus_keeps_file_and_non_special_paths() {
    let out = compile_and_run(
        r#"<?php
$file = new Uri\WhatWg\Url("file:///base/");
foreach (["///tmp/x", "////x", "/////x"] as $reference) {
    echo json_encode($reference), " => ", (new Uri\WhatWg\Url($reference, $file))->toAsciiString(), "\n";
}
$other = new Uri\WhatWg\Url("foo://h/base");
foreach (["//x", "///x", "////x"] as $reference) {
    echo "foo ", json_encode($reference), " => ", (new Uri\WhatWg\Url($reference, $other))->toAsciiString(), "\n";
}
$https = new Uri\WhatWg\Url("https://example.com/base");
echo (new Uri\WhatWg\Url("///tmp/x", $https))->toAsciiString(), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "\"\\/\\/\\/tmp\\/x\" => file:///tmp/x\n",
            "\"\\/\\/\\/\\/x\" => file:////x\n",
            "\"\\/\\/\\/\\/\\/x\" => file://///x\n",
            "foo \"\\/\\/x\" => foo://x\n",
            "foo \"\\/\\/\\/x\" => foo:///x\n",
            "foo \"\\/\\/\\/\\/x\" => foo:////x\n",
            "https://tmp/x\n",
        )
    );
}

/// Verifies a backslash in a reference against a non-special base stays a literal path byte
/// instead of becoming an authority separator.
#[test]
fn test_uri_whatwg_non_special_backslashes_stay_path_bytes() {
    let out = compile_and_run(
        r#"<?php
$base = new Uri\WhatWg\Url("foo://h/base");
foreach (["\\x", "\\\\x", "/__ELEPHC_BACKSLASH_ESC__/a\\b"] as $reference) {
    echo json_encode($reference), " => ", (new Uri\WhatWg\Url($reference, $base))->toAsciiString(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "\"\\\\x\" => foo://h/\\x\n\"\\\\\\\\x\" => foo://h/\\\\x\n\"\\/__ELEPHC_BACKSLASH_ESC__\\/a\\\\b\" => foo://h/__ELEPHC_BACKSLASH_ESC__/a\\b\n"
    );
}

/// Verifies the internal-class reflection filter matches only the two URI classes, so a user
/// class in the `Uri\` namespace keeps its destructor, clone handler, and compiler-prefixed
/// methods visible.
#[test]
fn test_uri_user_namespace_members_stay_visible() {
    let out = compile_and_run(
        r#"<?php
namespace Uri;
class Custom {
    public function __destruct() { echo "d|"; }
    public function __clone() { echo "c|"; }
    public function __elephc_helper() { echo "h|"; }
}
$object = new Custom;
$object->__destruct();
$object->__clone();
$object->__elephc_helper();
$reflection = new \ReflectionClass(Custom::class);
echo count($reflection->getMethods()), "|",
    (int) method_exists(Custom::class, "__destruct"), "|",
    (int) method_exists(Custom::class, "__elephc_helper"), "\n";
"#,
    );
    assert_eq!(out, "d|c|h|3|1|1\nd|");
}

/// Verifies the extra forbidden-domain-code-point repair only applies to special schemes:
/// a non-special host keeps its `%XX` and UTF-8 spellings, while a literal space stays invalid.
#[test]
fn test_uri_whatwg_opaque_hosts_keep_percent_and_utf8() {
    let out = compile_and_run(
        r#"<?php
foreach (["foo://exa%20mple.org/", "foo://éxample.org/"] as $spelling) {
    $url = new Uri\WhatWg\Url($spelling);
    echo $url->toAsciiString(), "|", json_encode($url->getAsciiHost()), "\n";
}
try { new Uri\WhatWg\Url("foo://exa mple.org/"); echo "accepted\n"; }
catch (Uri\WhatWg\InvalidUrlException $error) { echo "rejected\n"; }
"#,
    );
    assert_eq!(
        out,
        "foo://exa%20mple.org/|\"exa%20mple.org\"\nfoo://%C3%A9xample.org/|\"%C3%A9xample.org\"\nrejected\n"
    );
}

/// Verifies overlong and malformed UTF-8 in a percent-encoded host is rejected rather than
/// IDNA-mapped to a different ASCII host.
#[test]
fn test_uri_whatwg_rejects_overlong_host_utf8() {
    let out = compile_and_run(
        r#"<?php
// Overlong encodings of "evil.com" and "127.0.0.1", plus an invalid continuation byte.
foreach ([
    "http://%C1%A5%C1%B6%C1%A9%C1%AC.com/",
    "http://%C0%B1%C0%B2%C0%B7%C0%AE%C0%B0%C0%AE%C0%B0%C0%AE%C0%B1/",
    "http://%C0%65%C0%76%C0%69%C0%6C.com/",
] as $input) {
    echo Uri\WhatWg\Url::parse($input) === null ? "null|" : "accepted|";
    try { new Uri\WhatWg\Url($input); echo "constructed\n"; }
    catch (Uri\WhatWg\InvalidUrlException $error) { echo "rejected\n"; }
}
"#,
    );
    assert_eq!(out, "null|rejected\nnull|rejected\nnull|rejected\n");
}

/// Verifies `withHost()` keeps percent escapes on a non-special host while still rejecting a
/// literal forbidden byte, and still rejects a decoded slash on a special host.
#[test]
fn test_uri_whatwg_with_host_keeps_non_special_escapes() {
    let out = compile_and_run(
        r#"<?php
$base = new Uri\WhatWg\Url("foo://h/");
echo $base->withHost("exa%2Fmple.org")->toAsciiString(), "|",
    $base->withHost("exa%40mple.org")->toAsciiString(), "\n";
try { $base->withHost("exa@mple.org"); echo "accepted\n"; }
catch (Uri\WhatWg\InvalidUrlException $error) { echo "rejected\n"; }
$special = new Uri\WhatWg\Url("https://h/");
try { $special->withHost("exa%2Fmple.org"); echo "accepted\n"; }
catch (Uri\WhatWg\InvalidUrlException $error) { echo "rejected\n"; }
"#,
    );
    assert_eq!(
        out,
        "foo://exa%2Fmple.org/|foo://exa%40mple.org/\nrejected\nrejected\n"
    );
}

/// Verifies an absolute reference keeps its own scheme's separator rules when resolved against
/// a non-special base: a special-scheme backslash becomes the path separator, not a marker
/// that corrupts the host.
#[test]
fn test_uri_whatwg_absolute_reference_keeps_its_host() {
    let out = compile_and_run(
        r#"<?php
$base = new Uri\WhatWg\Url("foo://h/base");
$url = new Uri\WhatWg\Url("https://example.com\\foo", $base);
echo $url->toAsciiString(), "|", $url->getAsciiHost(), "\n";
try { new Uri\WhatWg\Url("foo://exa\\mple.org/x", $base); echo "accepted\n"; }
catch (Uri\WhatWg\InvalidUrlException $error) { echo "rejected\n"; }
"#,
    );
    assert_eq!(out, "https://example.com/foo|example.com\nrejected\n");
}

/// Verifies the reversible backslash encoding leaves path escapes inherited from the base
/// untouched, while still preserving a literal backslash in the relative reference.
#[test]
fn test_uri_whatwg_backslash_resolution_preserves_base_escapes() {
    let out = compile_and_run(
        r#"<?php
foreach ([
    ["foo://h/dir%25name/base", "child\\name"],
    ["foo://h/dir%5Cname/base", "child\\name"],
    ["foo://h/dir%25name/base", "/a%5Cb"],
] as [$base, $reference]) {
    $url = new Uri\WhatWg\Url($reference, new Uri\WhatWg\Url($base));
    echo $url->toAsciiString(), "|", json_encode($url->getPath()), "\n";
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "foo://h/dir%25name/child\\name|\"\\/dir%25name\\/child\\\\name\"\n",
            "foo://h/dir%5Cname/child\\name|\"\\/dir%5Cname\\/child\\\\name\"\n",
            "foo://h/a%5Cb|\"\\/a%5Cb\"\n",
        )
    );
}

/// Verifies a network-path reference's authority is not re-encoded by the non-special
/// backslash path rewrite when the backslash appears in its query or path.
#[test]
fn test_uri_whatwg_network_path_keeps_its_escaped_host() {
    let out = compile_and_run(
        r#"<?php
$base = new Uri\WhatWg\Url("foo://h/base");
$url = new Uri\WhatWg\Url("//exa%20mple.org/a\\b?q\\z#f\\g", $base);
echo $url->toAsciiString(), "|", json_encode($url->getAsciiHost()), "|",
    json_encode($url->getPath()), "|", json_encode($url->getQuery()), "\n";
"#,
    );
    assert_eq!(
        out,
        "foo://exa%20mple.org/a\\b?q\\z#f\\g|\"exa%20mple.org\"|\"\\/a\\\\b\"|\"q\\\\z\"\n"
    );
}

/// Verifies the non-special backslash path never rewrites percent escapes, so an encoded dot
/// segment still participates in dot-segment removal (`%2e` + `..` removes its parent).
#[test]
fn test_uri_whatwg_backslash_path_preserves_encoded_dot_segments() {
    let out = compile_and_run(
        r#"<?php
$base = new Uri\WhatWg\Url("foo://h/base");
$url = new Uri\WhatWg\Url("/a/%2e/../b?q\\z", $base);
echo $url->toAsciiString(), "|", json_encode($url->getPath()), "|", json_encode($url->getQuery()), "\n";
"#,
    );
    assert_eq!(
        out,
        "foo://h/b?q\\z|\"\\/b\"|\"q\\\\z\"\n"
    );
}

/// Verifies absolute special URLs classify backslashes by component: the path reports
/// `InvalidReverseSoldius`, while query/fragment bytes report `InvalidUrlUnit`.
#[test]
fn test_uri_whatwg_absolute_warning_components() {
    let out = compile_and_run(
        r#"<?php
foreach (["https://h/p?q\\z#f\\g", "https://h/p\\x?q"] as $spelling) {
    $errors = [];
    new Uri\WhatWg\Url($spelling, null, $errors);
    echo json_encode($spelling), " => ";
    foreach ($errors as $error) {
        echo $error->type->name, "(", json_encode($error->context), ") ";
    }
    echo "\n";
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "\"https:\\/\\/h\\/p?q\\\\z#f\\\\g\" => InvalidUrlUnit(\"\\\\g\") InvalidUrlUnit(\"\\\\z#f\\\\g\") \n",
            "\"https:\\/\\/h\\/p\\\\x?q\" => InvalidReverseSoldius(\"\\\\x?q\") \n",
        )
    );
}

/// Verifies a forward-slash network reference reports each path backslash once: the
/// authority scan owns path bytes, and query/fragment bytes stay `InvalidUrlUnit`.
#[test]
fn test_uri_whatwg_network_relative_warnings_are_not_duplicated() {
    let out = compile_and_run(
        r#"<?php
$base = new Uri\WhatWg\Url("https://base/");
foreach (["//h/p\\x", "//h/p\\x?q\\z#f\\g"] as $reference) {
    $errors = [];
    $url = new Uri\WhatWg\Url($reference, $base, $errors);
    echo json_encode($reference), " => ", $url->toAsciiString(), " errors=";
    foreach ($errors as $error) {
        echo $error->type->name, ",", json_encode($error->context), " ";
    }
    echo "\n";
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "\"\\/\\/h\\/p\\\\x\" => https://h/p/x errors=InvalidReverseSoldius,\"\\\\x\" \n",
            "\"\\/\\/h\\/p\\\\x?q\\\\z#f\\\\g\" => https://h/p/x?q\\z#f\\g errors=",
            "InvalidUrlUnit,\"\\\\g\" InvalidUrlUnit,\"\\\\z#f\\\\g\" ",
            "InvalidReverseSoldius,\"\\\\x?q\\\\z#f\\\\g\" \n",
        )
    );
}

/// Verifies a network-relative reference locates its authority for failure details even though
/// it has no written scheme: port, domain, IPv6, IPv4, and missing-host errors use the same
/// context and failure kinds php-src reports.
#[test]
fn test_uri_whatwg_network_relative_failures_use_the_reference_authority() {
    let out = compile_and_run(
        r#"<?php
$base = new Uri\WhatWg\Url("https://base/");
$cases = ["//example.com:99999/path", "//user@example.com:99999/p?q", "//exa mple.com/p", "//[bad/p", "//1.2.3.999/p", "//:80/p"];
foreach ($cases as $reference) {
    echo json_encode($reference), " => ";
    try {
        new Uri\WhatWg\Url($reference, $base);
        echo "OK\n";
    } catch (Uri\WhatWg\InvalidUrlException $error) {
        echo $error->getMessage(), " [";
        foreach ($error->errors as $entry) {
            echo $entry->type->name, ",", json_encode($entry->context), " ";
        }
        echo "]\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        r#""\/\/example.com:99999\/path" => The specified URI is malformed (PortOutOfRange) [PortOutOfRange,"\/path" ]
"\/\/user@example.com:99999\/p?q" => The specified URI is malformed (PortOutOfRange) [PortOutOfRange,"\/p?q" InvalidCredentials,"@example.com:99999\/p?q" ]
"\/\/exa mple.com\/p" => The specified URI is malformed (DomainInvalidCodePoint) [DomainInvalidCodePoint," mple.com" ]
"\/\/[bad\/p" => The specified URI is malformed (Ipv6Unclosed) [Ipv6Unclosed,"d\/p" ]
"\/\/1.2.3.999\/p" => The specified URI is malformed (Ipv4OutOfRangePart) [Ipv4OutOfRangePart,"999" ]
"\/\/:80\/p" => The specified URI is malformed (HostMissing) [HostMissing,":80\/p" ]
"#
    );
}

/// Verifies a special-scheme authority ends at a backslash, so path text containing `@` is
/// not reported as userinfo. A non-special scheme keeps the backslash inside its host and
/// still reports the real credentials warning.
#[test]
fn test_uri_whatwg_special_backslash_does_not_create_credentials() {
    let out = compile_and_run(
        r#"<?php
foreach (["https://h\\user@example.com/", "https://h\\user@example.com:80/p", "foo://h\\user@example.com/", "https://h\\\\user@example.com/", "file://h\\user@example.com/"] as $reference) {
    $errors = [];
    $url = new Uri\WhatWg\Url($reference, null, $errors);
    $parts = [];
    foreach ($errors as $error) {
        $parts[] = $error->type->name . "," . json_encode($error->context);
    }
    echo json_encode($reference), " => ", $url->toAsciiString(), " host=", json_encode($url->getAsciiHost()), " path=", json_encode($url->getPath()), " errors=", implode(" ", $parts), "\n";
}
"#,
    );
    assert_eq!(
        out,
        r#""https:\/\/h\\user@example.com\/" => https://h/user@example.com/ host="h" path="\/user@example.com\/" errors=InvalidReverseSoldius,"\\user@example.com\/"
"https:\/\/h\\user@example.com:80\/p" => https://h/user@example.com:80/p host="h" path="\/user@example.com:80\/p" errors=InvalidReverseSoldius,"\\user@example.com:80\/p"
"foo:\/\/h\\user@example.com\/" => foo://h%5Cuser@example.com/ host="example.com" path="\/" errors=InvalidCredentials,"@example.com\/"
"https:\/\/h\\\\user@example.com\/" => https://h//user@example.com/ host="h" path="\/\/user@example.com\/" errors=InvalidReverseSoldius,"\\user@example.com\/" InvalidReverseSoldius,"\\\\user@example.com\/"
"file:\/\/h\\user@example.com\/" => file://h/user@example.com/ host="h" path="\/user@example.com\/" errors=InvalidReverseSoldius,"\\user@example.com\/"
"#
    );
}

/// Verifies a special-scheme backslash path still reaches the component scanner, so invalid
/// or truncated percent escapes after it report `InvalidUrlUnit` in addition to the path's
/// `InvalidReverseSoldius`.
#[test]
fn test_uri_whatwg_special_backslash_path_reports_percent_escapes() {
    let out = compile_and_run(
        r#"<?php
foreach (["https://h\\a%", "https://h\\a%zz", "https://h\\a%20b", "https://h\\a%?q%#f%"] as $reference) {
    $errors = [];
    $url = new Uri\WhatWg\Url($reference, null, $errors);
    $parts = [];
    foreach ($errors as $error) {
        $parts[] = $error->type->name . "," . json_encode($error->context);
    }
    echo json_encode($reference), " => ", $url->toAsciiString(), " errors=", implode(" ", $parts), "\n";
}
"#,
    );
    assert_eq!(
        out,
        r#""https:\/\/h\\a%" => https://h/a% errors=InvalidUrlUnit,"%" InvalidReverseSoldius,"\\a%"
"https:\/\/h\\a%zz" => https://h/a%zz errors=InvalidUrlUnit,"%zz" InvalidReverseSoldius,"\\a%zz"
"https:\/\/h\\a%20b" => https://h/a%20b errors=InvalidReverseSoldius,"\\a%20b"
"https:\/\/h\\a%?q%#f%" => https://h/a%?q%#f% errors=InvalidUrlUnit,"%" InvalidUrlUnit,"%#f%" InvalidUrlUnit,"%?q%#f%" InvalidReverseSoldius,"\\a%?q%#f%"
"#
    );
}

/// Verifies the absolute-file slash override requires a real scheme prefix: a colon inside a
/// query, fragment, or relative path must not add leading separators to the resolved path.
#[test]
fn test_uri_whatwg_file_override_ignores_colons_outside_the_scheme() {
    let out = compile_and_run(
        r##"<?php
$base = new Uri\WhatWg\Url("file:///base");
foreach (["?q:////x", "#f:////x", "?q:////", "/x:////y", "file:////x"] as $reference) {
    $url = new Uri\WhatWg\Url($reference, $base);
    echo json_encode($reference), " => ", $url->toAsciiString(), " path=", json_encode($url->getPath()), "\n";
}
"##,
    );
    assert_eq!(
        out,
        r##""?q:\/\/\/\/x" => file:///base?q:////x path="\/base"
"#f:\/\/\/\/x" => file:///base#f:////x path="\/base"
"?q:\/\/\/\/" => file:///base?q://// path="\/base"
"\/x:\/\/\/\/y" => file:///x:////y path="\/x:\/\/\/\/y"
"file:\/\/\/\/x" => file:////x path="\/\/x"
"##
    );
}

/// Verifies extra special-scheme separators still locate the authority for port, credential,
/// and missing-solidus diagnostics, and that failure paths keep the separator warnings.
#[test]
fn test_uri_whatwg_extra_solidus_authority_diagnostics() {
    let out = compile_and_run(
        r#"<?php
$cases = [
    "https:///example.com:99999/path",
    "https:///user@example.com/",
    "https:////user@example.com/",
    "https://\\user@example.com/",
    "file:////x",
];
foreach ($cases as $reference) {
    echo json_encode($reference), " => ";
    try {
        $url = new Uri\WhatWg\Url($reference, null, $errors);
        echo "OK ", $url->toAsciiString(), " host=", json_encode($url->getAsciiHost()), " path=", json_encode($url->getPath());
    } catch (Uri\WhatWg\InvalidUrlException $error) {
        echo $error->getMessage();
        $errors = $error->errors;
    }
    $parts = [];
    foreach ($errors as $entry) {
        $parts[] = $entry->type->name . "," . json_encode($entry->context);
    }
    echo " errors=", implode(" ", $parts), "\n";
}
"#,
    );
    assert_eq!(
        out,
        r#""https:\/\/\/example.com:99999\/path" => The specified URI is malformed (PortOutOfRange) errors=PortOutOfRange,"\/path" SpecialSchemeMissingFollowingSolidus,"\/example.com:99999\/path"
"https:\/\/\/user@example.com\/" => OK https://user@example.com/ host="example.com" path="\/" errors=InvalidCredentials,"@example.com\/" SpecialSchemeMissingFollowingSolidus,"\/user@example.com\/"
"https:\/\/\/\/user@example.com\/" => OK https://user@example.com/ host="example.com" path="\/" errors=InvalidCredentials,"@example.com\/" SpecialSchemeMissingFollowingSolidus,"\/user@example.com\/" SpecialSchemeMissingFollowingSolidus,"\/\/user@example.com\/"
"https:\/\/\\user@example.com\/" => OK https://user@example.com/ host="example.com" path="\/" errors=InvalidCredentials,"@example.com\/" SpecialSchemeMissingFollowingSolidus,"\\user@example.com\/"
"file:\/\/\/\/x" => OK file:////x host="" path="\/\/x" errors=
"#
    );
}

/// Verifies the trimmed reference, not the raw input, reaches `Url::join()`: a leading space
/// must not hide the leading-backslash repair that keeps non-special path bytes literal.
#[test]
fn test_uri_whatwg_trimmed_reference_preserves_leading_backslash() {
    let out = compile_and_run(
        r#"<?php
$base = new Uri\WhatWg\Url("foo://h/base");
foreach ([" \\x", "\\\\x", " \\\\x", " \\a/b", "  child\\name"] as $reference) {
    $url = new Uri\WhatWg\Url($reference, $base);
    echo json_encode($reference), " => ", $url->toAsciiString(), " path=", json_encode($url->getPath()), "\n";
}
"#,
    );
    assert_eq!(
        out,
        r#"" \\x" => foo://h/\x path="\/\\x"
"\\\\x" => foo://h/\\x path="\/\\\\x"
" \\\\x" => foo://h/\\x path="\/\\\\x"
" \\a\/b" => foo://h/\a/b path="\/\\a\/b"
"  child\\name" => foo://h/child\name path="\/child\\name"
"#
    );
}

/// Verifies relaxed IDNA recovery percent-decodes the special-scheme host first. Escaped
/// labels recover through parsing, resolution, and `withHost()`; an escape inside the `xn--`
/// prefix keeps lexbor's ASCII spelling while the Unicode host still decodes.
#[test]
fn test_uri_whatwg_escaped_labels_recover_through_all_entry_points() {
    let out = compile_and_run(
        r#"<?php
$cases = [
    "https://xn--a-.c%6Fm/",
    "https://%78n--a-.com/",
    "https://x%6E--a-.com/",
    "https://%78%6E--a-.com/",
];
foreach ($cases as $reference) {
    $url = new Uri\WhatWg\Url($reference);
    echo json_encode($reference), " => ", $url->toAsciiString(), " host=", json_encode($url->getAsciiHost()), " uni=", json_encode($url->getUnicodeHost()), "\n";
}
$base = new Uri\WhatWg\Url("https://base/");
foreach (["//xn--a-.c%6Fm/p", "//%78n--a-.com/"] as $reference) {
    $url = new Uri\WhatWg\Url($reference, $base);
    echo "resolve ", json_encode($reference), " => ", $url->toAsciiString(), " host=", json_encode($url->getAsciiHost()), " uni=", json_encode($url->getUnicodeHost()), "\n";
}
$url = new Uri\WhatWg\Url("https://base/");
foreach (["xn--a-.c%6Fm", "%78n--a-.com"] as $host) {
    $written = $url->withHost($host);
    echo "withHost ", json_encode($host), " => ", $written->toAsciiString(), " host=", json_encode($written->getAsciiHost()), " uni=", json_encode($written->getUnicodeHost()), "\n";
}
"#,
    );
    assert_eq!(
        out,
        r#""https:\/\/xn--a-.c%6Fm\/" => https://a.com/ host="a.com" uni="a.com"
"https:\/\/%78n--a-.com\/" => https://xn--a-.com/ host="xn--a-.com" uni="a.com"
"https:\/\/x%6E--a-.com\/" => https://xn--a-.com/ host="xn--a-.com" uni="a.com"
"https:\/\/%78%6E--a-.com\/" => https://xn--a-.com/ host="xn--a-.com" uni="a.com"
resolve "\/\/xn--a-.c%6Fm\/p" => https://a.com/p host="a.com" uni="a.com"
resolve "\/\/%78n--a-.com\/" => https://xn--a-.com/ host="xn--a-.com" uni="a.com"
withHost "xn--a-.c%6Fm" => https://a.com/ host="a.com" uni="a.com"
withHost "%78n--a-.com" => https://xn--a-.com/ host="xn--a-.com" uni="a.com"
"#
    );
}

/// A runtime `null` bound to a URI method's `string` parameter is coerced as for any internal
/// method (PHP only deprecates it), not refused by the user-function scalar parameter guard.
#[test]
fn test_uri_methods_coerce_a_runtime_null_string_argument() {
    let out = compile_and_run(
        r#"<?php
error_reporting(E_ALL & ~E_DEPRECATED);
function box(mixed $v): mixed { return $v; }
$n = box(null);
$u = Uri\Rfc3986\Uri::parse($n);
echo $u === null ? "null" : "[" . $u->toString() . "]", "\n";
$v = Uri\Rfc3986\Uri::parse(uri: $n);
echo $v === null ? "null" : "[" . $v->toString() . "]", "\n";
$w = Uri\Rfc3986\Uri::parse("https://example.com/x");
echo $w->withPath($n)->toString(), "|", $w->withPath(box("/p"))->toString(), "\n";
echo (new Uri\WhatWg\Url("https://example.com/"))->withQuery($n)->toAsciiString(), "\n";
"#,
    );
    assert_eq!(out, "[]\n[]\nhttps://example.com|https://example.com/p\nhttps://example.com/\n");
}

/// Verifies `is_a()` and `is_subclass_of()` answer for the injected URI classes and exception
/// hierarchy when the target class name is only known at run time, for object and class-name
/// subjects alike.
#[test]
fn test_uri_classes_with_runtime_is_a_target() {
    let out = compile_and_run(
        r#"<?php
$u = new Uri\Rfc3986\Uri("https://example.com/a");
$w = Uri\WhatWg\Url::parse("https://example.com/");
$names = [$argv[1] ?? "Uri\\Rfc3986\\Uri", "uri\\whatwg\\url", "Uri\\UriException",
          "Uri\\WhatWg\\InvalidUrlException", "Uri\\InvalidUriException", "Uri\\UriError", "Throwable", "Nope"];
foreach ($names as $n) {
    echo json_encode([is_a($u, $n), is_a($w, $n), is_subclass_of("Uri\\WhatWg\\InvalidUrlException", $n),
        is_a("Uri\\WhatWg\\InvalidUrlException", $n, true), is_subclass_of($w, $n), is_a($n, "Uri\\UriException", true)]), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "[true,false,false,false,false,false]\n\
         [false,true,false,false,false,false]\n\
         [false,false,true,true,false,true]\n\
         [false,false,false,true,false,true]\n\
         [false,false,true,true,false,true]\n\
         [false,false,false,false,false,false]\n\
         [false,false,true,true,false,false]\n\
         [false,false,false,false,false,false]\n"
    );
}

/// The URI methods keep coercing a runtime `null` when they are called through a callable
/// value (the descriptor invoker), as their direct calls do; a user method with the same
/// parameter is still refused.
#[test]
fn test_uri_methods_called_through_a_callable_coerce_a_runtime_null() {
    let out = compile_and_run(
        r#"<?php
error_reporting(E_ALL & ~E_DEPRECATED);
function box(mixed $v): mixed { return $v; }
class P { public function withPath(string $p): string { return "[" . $p . "]"; } }
$n = box(null);
$u = new Uri\Rfc3986\Uri("https://example.com/x");
echo call_user_func_array($u->withPath(...), [$n])->toString(), "\n";
echo call_user_func([$u, "withPath"], $n)->toString(), "\n";
$f = $u->withPath(...);
echo $f($n)->toString(), "\n";
echo call_user_func([new Uri\WhatWg\Url("https://example.com/"), "withQuery"], $n)->toAsciiString(), "\n";
try { echo call_user_func([new P, "withPath"], $n), "\n"; } catch (TypeError $e) { echo get_class($e), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "https://example.com\nhttps://example.com\nhttps://example.com\nhttps://example.com/\nTypeError\n"
    );
}

/// Under `strict_types=1` the URI methods refuse a runtime `null` or `int` bound to their
/// `string` parameters, called directly or through a callable value, with PHP's internal-method
/// message (no "called in" suffix); a string still binds.
#[test]
fn test_uri_methods_check_runtime_scalar_arguments_under_strict_types() {
    let out = compile_and_run(
        r#"<?php
declare(strict_types=1);
function box(mixed $v): mixed { return $v; }
function show(TypeError $e): void { echo $e->getMessage(), "\n"; }
$u = new Uri\Rfc3986\Uri("https://example.com/x");
foreach ([null, 5, "/p"] as $raw) {
    $v = box($raw);
    try { echo $u->withPath($v)->toString(), "\n"; } catch (TypeError $e) { show($e); }
    try { echo call_user_func_array($u->withPath(...), [$v])->toString(), "\n"; } catch (TypeError $e) { show($e); }
    try { echo call_user_func([$u, "withPath"], $v)->toString(), "\n"; } catch (TypeError $e) { show($e); }
    try { $f = $u->withPath(...); echo $f($v)->toString(), "\n"; } catch (TypeError $e) { show($e); }
    try { echo call_user_func_array([$u, "withPath"], [$v])->toString(), "\n"; } catch (TypeError $e) { show($e); }
    try { $p = Uri\Rfc3986\Uri::parse($v); echo $p === null ? "null" : $p->toString(), "\n"; } catch (TypeError $e) { show($e); }
}
"#,
    );
    assert_eq!(
        out,
        "Uri\\Rfc3986\\Uri::withPath(): Argument #1 ($path) must be of type string, null given\nUri\\Rfc3986\\Uri::withPath(): Argument #1 ($path) must be of type string, null given\nUri\\Rfc3986\\Uri::withPath(): Argument #1 ($path) must be of type string, null given\nUri\\Rfc3986\\Uri::withPath(): Argument #1 ($path) must be of type string, null given\nUri\\Rfc3986\\Uri::withPath(): Argument #1 ($path) must be of type string, null given\nUri\\Rfc3986\\Uri::parse(): Argument #1 ($uri) must be of type string, null given\nUri\\Rfc3986\\Uri::withPath(): Argument #1 ($path) must be of type string, int given\nUri\\Rfc3986\\Uri::withPath(): Argument #1 ($path) must be of type string, int given\nUri\\Rfc3986\\Uri::withPath(): Argument #1 ($path) must be of type string, int given\nUri\\Rfc3986\\Uri::withPath(): Argument #1 ($path) must be of type string, int given\nUri\\Rfc3986\\Uri::withPath(): Argument #1 ($path) must be of type string, int given\nUri\\Rfc3986\\Uri::parse(): Argument #1 ($uri) must be of type string, int given\nhttps://example.com/p\nhttps://example.com/p\nhttps://example.com/p\nhttps://example.com/p\nhttps://example.com/p\n/p\n"
    );
}

/// Verifies a `mixed` value reaches URI class parameters (positional and named) and a nullable
/// URI class return through PHP's run-time class check, with PHP's `TypeError` for the wrong
/// URI class, null, or a string.
#[test]
fn test_uri_classes_at_mixed_class_typed_boundaries() {
    let out = compile_and_run(
        r#"<?php
use Uri\Rfc3986\Uri;
use Uri\WhatWg\Url;
function take(?Uri $u): string { return $u?->toString() ?? "null"; }
function takeUrl(Url $u): string { return $u->toAsciiString(); }
function pick(array $a, int $i): ?Uri { return $a[$i] ?? null; }
function getMixed(mixed $v): mixed { return $v; }
$vals = [new Uri("https://example.com/a"), Url::parse("https://example.com/"), null, "x"];
foreach ([0, 1, 2, 3] as $i) {
    try { echo take(getMixed($vals[$i])), "\n"; } catch (TypeError $e) { echo explode(", called", $e->getMessage())[0], "\n"; }
    try { echo takeUrl(u: getMixed($vals[$i])), "\n"; } catch (TypeError $e) { echo explode(", called", $e->getMessage())[0], "\n"; }
    try { $r = pick($vals, $i); echo $r?->toString() ?? "null", "\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
"#,
    );
    assert_eq!(
        out,
        "https://example.com/a\n\
         takeUrl(): Argument #1 ($u) must be of type Uri\\WhatWg\\Url, Uri\\Rfc3986\\Uri given\n\
         https://example.com/a\n\
         take(): Argument #1 ($u) must be of type ?Uri\\Rfc3986\\Uri, Uri\\WhatWg\\Url given\n\
         https://example.com/\n\
         pick(): Return value must be of type ?Uri\\Rfc3986\\Uri, Uri\\WhatWg\\Url returned\n\
         null\n\
         takeUrl(): Argument #1 ($u) must be of type Uri\\WhatWg\\Url, null given\n\
         null\n\
         take(): Argument #1 ($u) must be of type ?Uri\\Rfc3986\\Uri, string given\n\
         takeUrl(): Argument #1 ($u) must be of type Uri\\WhatWg\\Url, string given\n\
         pick(): Return value must be of type ?Uri\\Rfc3986\\Uri, string returned\n"
    );
}
