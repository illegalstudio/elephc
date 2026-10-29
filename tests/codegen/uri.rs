//! Purpose:
//! Focused end-to-end coverage for PHP 8.5's `ext/uri` surface: RFC 3986 reads,
//! normalization and mutation, WHATWG parsing and validation, opaque handle introspection,
//! serialization, and the `extension_loaded("uri")` surface.
//!
//! Called from:
//! - `cargo test --test codegen_tests uri_` through Rust's test harness.
//!
//! Key details:
//! - Expected outputs were measured on PHP 8.5.10 with the bundled `uri` extension.
//! - The exhaustive php-src PHPT matrix lives in `tests/codegen/uri_php_src.rs`; these
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
            "/zz\n",
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
/// a query is the lexbor `%20` case pinned by the vendored fixture.
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
            "\"foo:a b ?c\" => foo:a b%20?c path=\"a b%20\"\n",
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
