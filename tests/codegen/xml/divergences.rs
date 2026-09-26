//! Purpose:
//! Regression fixtures for the "Differences from PHP" list of `docs/php/xml.md`: every
//! documented divergence that a program can observe is pinned here, each with PHP 8.5.10's
//! output recorded in a comment and elephc's actual output asserted, so a change that
//! silently closes or widens a divergence fails a test and forces the docs to move with it.
//!
//! Called from:
//! - `cargo test --test codegen_tests xml::divergences` through Rust's test harness.
//!
//! Key details:
//! - Each PHP transcript was taken with `php` 8.5.10 (libxml2 2.15.3) on the same source;
//!   the two outputs differ ONLY in the documented way. PHP's `Deprecated:` / `Warning:`
//!   lines go to STDOUT on the CLI, which is why they show up in the recorded transcripts
//!   but never in elephc's, whose diagnostics use stderr or do not exist.
//! - Divergences already pinned elsewhere are NOT repeated: the compile-time rejection of
//!   non-variable `xml_parse_into_struct()` outputs and of closure literals with surplus
//!   required parameters lives in `tests/error_tests/xml.rs`; the eval-declared handler
//!   `Error` and the eval-side `function_exists()` split live in `super::eval`.
//! - Every fixture that links the bridge starts with `skip_without_xml_native(...)`; the
//!   one fixture that deliberately never links it (`function_exists()` without the bridge)
//!   is not gated, exactly like `super::eval::test_xml_absent_bridge_inside_eval`.

use crate::support::*;

/// The PHP 8.4 deprecation of `xml_set_object()` and of non-callable string handlers, and
/// the PHP 8.5 deprecation of `xml_parser_free()`, are not emitted: the calls behave as
/// PHP's do and nothing reaches either stream.
#[test]
fn test_xml_php84_and_85_deprecations_are_not_emitted() {
    if skip_without_xml_native("test_xml_php84_and_85_deprecations_are_not_emitted") {
        return;
    }
    let out = compile_and_run_capture(
        r#"<?php
class H { public function s($p, $n, $a) { echo "S $n\n"; } public function e($p, $n) { echo "E $n\n"; } }
$p = xml_parser_create();
var_dump(xml_set_object($p, new H));
var_dump(xml_set_element_handler($p, "s", "e"));
var_dump(xml_parse($p, "<a/>", true));
var_dump(xml_parser_free($p));
$q = xml_parser_create();
var_dump(xml_set_element_handler($q, null, null));
var_dump(xml_parse($q, "<b/>", true));
"#,
    );
    assert!(out.success, "program failed: stdout={:?} stderr={}", out.stdout, out.stderr);
    // PHP 8.5.10 prints the same values, each deprecated call preceded by its notice:
    //   "\nDeprecated: Function xml_set_object() is deprecated since 8.4, provide a proper
    //    method callable to xml_set_*_handler() functions in <file> on line 4\n"
    //   before the first bool(true),
    //   "\nDeprecated: xml_set_element_handler(): Passing non-callable strings is deprecated
    //    since 8.4 in <file> on line 5\n" before the second, and
    //   "\nDeprecated: Function xml_parser_free() is deprecated since 8.5, as it has no
    //    effect since PHP 8.0 in <file> on line 7\n" before the fourth.
    assert_eq!(
        out.stdout,
        "bool(true)\nbool(true)\nS A\nE A\nint(1)\nbool(true)\nbool(true)\nint(1)\n"
    );
    assert_eq!(out.stderr, "", "no deprecation may be emitted on stderr either");
}

/// `xml_parse_into_struct()` called from inside a handler throws the same recursion
/// `Error` as `xml_parse()`; PHP warns and returns `false` instead.
#[test]
fn test_xml_parse_into_struct_inside_a_handler_throws_error() {
    if skip_without_xml_native("test_xml_parse_into_struct_inside_a_handler_throws_error") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$p = xml_parser_create();
xml_set_element_handler($p, function ($p, $n, $a) {
    echo "S $n\n";
    try {
        $r = xml_parse_into_struct($p, "<x/>", $vals);
        var_dump($r);
    } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
}, null);
var_dump(xml_parse($p, "<a/>", true));
"#,
    );
    // PHP 8.5.10:
    //   "S A\n\nWarning: xml_parse_into_struct(): Parser must not be called recursively in
    //    <file> on line 6\nbool(false)\nint(1)\n"
    assert_eq!(out, "S A\nError: Parser must not be called recursively\nint(1)\n");
}

/// Past 255 nested levels the struct is truncated exactly like PHP's (the opens beyond
/// the limit are dropped, the innermost close turns the level-255 entry into `complete`,
/// and the remaining closes keep their real levels: 255 + 44 + 255 = 554 entries for a
/// 300-deep document), but PHP's "Maximum depth exceeded" warning is not printed; a
/// 255-deep document is untouched on both sides.
#[test]
fn test_xml_parse_into_struct_max_depth_truncates_without_warning() {
    if skip_without_xml_native("test_xml_parse_into_struct_max_depth_truncates_without_warning") {
        return;
    }
    let out = compile_and_run_capture(
        r#"<?php
$deep = 300;
$xml = str_repeat("<d>", $deep) . "x" . str_repeat("</d>", $deep);
$p = xml_parser_create();
$r = xml_parse_into_struct($p, $xml, $vals, $idx);
var_dump($r, xml_get_error_code($p), count($vals), count($idx['D']));
$levels = [];
foreach ($vals as $v) { $levels[] = $v['level']; }
echo max($levels), " ", min($levels), " ", $vals[0]['type'], " ", $vals[count($vals)-1]['type'], " ", $vals[254]['type'], " ", $vals[255]['type'], "\n";
$shallow = 255;
$xml = str_repeat("<d>", $shallow) . "x" . str_repeat("</d>", $shallow);
$p = xml_parser_create();
$r = xml_parse_into_struct($p, $xml, $vals2, $idx2);
var_dump($r, count($vals2), count($idx2['D']));
"#,
    );
    assert!(out.success, "program failed: stdout={:?} stderr={}", out.stdout, out.stderr);
    // PHP 8.5.10 prints the same values after
    //   "\nWarning: xml_parse_into_struct(): Maximum depth exceeded - Results truncated in
    //    <file> on line 5\n"
    assert_eq!(
        out.stdout,
        "int(1)\nint(0)\nint(554)\nint(554)\n299 1 open close complete close\nint(1)\nint(509)\nint(509)\n"
    );
    assert_eq!(out.stderr, "", "the depth warning must not surface on stderr either");
}

/// Only public methods can be bound as handlers: a private or protected `[$this, 'm']`
/// pair is rejected with a `TypeError` and a private or protected name looked up through
/// `xml_set_object()` with the "does not exist" `ValueError`, while the public method on
/// the same object binds and fires. PHP accepts all of them and calls them.
#[test]
fn test_xml_non_public_method_handlers_are_rejected() {
    if skip_without_xml_native("test_xml_non_public_method_handlers_are_rejected") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
class Vis {
    private function ps($p, $n, $a) { echo "private S $n\n"; }
    protected function pe($p, $n) { echo "protected E $n\n"; }
    private function pc($p, $d) { echo "private C $d\n"; }
    protected function pd($p, $d) { echo "protected D $d\n"; }
    public function pub($p, $n, $a) { echo "public S $n\n"; }
    public function run() {
        $p = xml_parser_create();
        try { var_dump(xml_set_element_handler($p, [$this, 'ps'], null)); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
        try { var_dump(xml_set_element_handler($p, null, [$this, 'pe'])); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
        try { var_dump(xml_set_character_data_handler($p, [$this, 'pc'])); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
        try { var_dump(xml_set_default_handler($p, [$this, 'pd'])); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
        var_dump(xml_set_element_handler($p, [$this, 'pub'], null));
        var_dump(xml_parse($p, "<a>x</a>", true));
        $q = xml_parser_create();
        xml_set_object($q, $this);
        try { var_dump(xml_set_element_handler($q, 'ps', null)); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
        try { var_dump(xml_set_element_handler($q, null, 'pe')); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
        try { var_dump(xml_set_character_data_handler($q, 'pc')); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
        var_dump(xml_set_element_handler($q, 'pub', null));
        var_dump(xml_parse($q, "<b>y</b>", true));
    }
}
(new Vis)->run();
"#,
    );
    // PHP 8.5.10 binds every one of them (bool(true) x5, then "public S A", "private C x",
    // int(1); after the xml_set_object() / non-callable-string deprecations bool(true) x4,
    // then "public S B", "private C y", int(1)).
    assert_eq!(
        out,
        "TypeError: xml_set_element_handler(): Argument #2 ($start_handler) must be of type callable|string|null\nTypeError: xml_set_element_handler(): Argument #3 ($end_handler) must be of type callable|string|null\nTypeError: xml_set_character_data_handler(): Argument #2 ($handler) must be of type callable|string|null\nTypeError: xml_set_default_handler(): Argument #2 ($handler) must be of type callable|string|null\nbool(true)\npublic S A\nint(1)\nValueError: xml_set_element_handler(): Argument #2 ($start_handler) method Vis::ps() does not exist\nValueError: xml_set_element_handler(): Argument #3 ($end_handler) method Vis::pe() does not exist\nValueError: xml_set_character_data_handler(): Argument #2 ($handler) method Vis::pc() does not exist\nbool(true)\npublic S B\nint(1)\n"
    );
}

/// The `xml_set_object()` swap error names the bound method as the caller spelled it
/// (`"CDATA"`), where PHP prints the method's declared spelling (`"cdata"`); the handler
/// itself is resolved case-insensitively on both sides.
#[test]
fn test_xml_set_object_swap_error_names_the_caller_spelling() {
    if skip_without_xml_native("test_xml_set_object_swap_error_names_the_caller_spelling") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
final class Sink { public function cdata($p, $d) { echo "C:$d\n"; } }
final class Other { public function nothing() {} }
$s = xml_parser_create();
xml_set_object($s, new Sink());
xml_set_character_data_handler($s, "CDATA");
xml_parse($s, "<a>x</a>", true);
try { xml_set_object($s, new Other()); } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
"#,
    );
    // PHP 8.5.10 (after the xml_set_object() / non-callable-string deprecations):
    //   "C:x\nValueError: xml_set_object(): Argument #2 ($object) cannot safely swap to
    //    object of class Other as method \"cdata\" does not exist, which was set via
    //    xml_set_character_data_handler()\n"
    assert_eq!(
        out,
        "C:x\nValueError: xml_set_object(): Argument #2 ($object) cannot safely swap to object of class Other as method \"CDATA\" does not exist, which was set via xml_set_character_data_handler()\n"
    );
}

/// `xml_parser_free()` called from inside a handler answers `false` like PHP, but without
/// PHP's "Parser cannot be freed while it is parsing" warning; the parse continues and the
/// parser is freed normally afterwards.
#[test]
fn test_xml_parser_free_inside_a_handler_returns_false_without_warning() {
    if skip_without_xml_native("test_xml_parser_free_inside_a_handler_returns_false_without_warning") {
        return;
    }
    let out = compile_and_run_capture(
        r#"<?php
$q = xml_parser_create();
xml_set_element_handler($q, function ($q, $n, $a) { echo "S $n\n"; var_dump(xml_parser_free($q)); }, function ($q, $n) { echo "E $n\n"; });
var_dump(xml_parse($q, "<a><b/></a>", true));
var_dump(xml_parser_free($q));
"#,
    );
    assert!(out.success, "program failed: stdout={:?} stderr={}", out.stdout, out.stderr);
    // PHP 8.5.10 prints the same values; each in-handler bool(false) is preceded by the
    // xml_parser_free() deprecation and
    //   "\nWarning: xml_parser_free(): Parser cannot be freed while it is parsing in <file>
    //    on line 3\n"
    // and the final bool(true) by the deprecation alone.
    assert_eq!(
        out.stdout,
        "S A\nbool(false)\nS B\nbool(false)\nE B\nE A\nint(1)\nbool(true)\n"
    );
    assert_eq!(out.stderr, "", "no warning may be emitted on stderr either");
}

/// An unresolvable `openUri()` target answers `false` with the stream layer's own
/// `fopen()` warning on stderr instead of PHP's "Unable to resolve file path" text on
/// stdout, and `xmlwriter_open_uri()` throws `ValueError` where PHP warns and returns
/// `false`; `XMLWriter::toUri()` throws the same `ValueError` on both sides.
#[test]
fn test_xmlwriter_open_uri_failure_is_reported_by_the_stream_layer() {
    if skip_without_xml_native("test_xmlwriter_open_uri_failure_is_reported_by_the_stream_layer") {
        return;
    }
    let out = compile_and_run_capture(
        r#"<?php
$w = new XMLWriter();
var_dump($w->openUri("/nonexistent-elephc-dir/x.xml"));
try { var_dump(xmlwriter_open_uri("/nonexistent-elephc-dir/x.xml")); } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { var_dump(XMLWriter::toUri("/nonexistent-elephc-dir/x.xml")); } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
"#,
    );
    assert!(out.success, "program failed: stdout={:?} stderr={}", out.stdout, out.stderr);
    // PHP 8.5.10:
    //   "\nWarning: XMLWriter::openUri(): Unable to resolve file path in <file> on line 3\n
    //    bool(false)\n\nWarning: xmlwriter_open_uri(): Unable to resolve file path in <file>
    //    on line 4\nbool(false)\nValueError: XMLWriter::toUri(): Argument #1 ($uri) must
    //    resolve to a valid file path\n"
    assert_eq!(
        out.stdout,
        "bool(false)\nValueError: xmlwriter_open_uri(): Argument #1 ($uri) must resolve to a valid file path\nValueError: XMLWriter::toUri(): Argument #1 ($uri) must resolve to a valid file path\n"
    );
    assert!(
        !out.stdout.contains("Unable to resolve file path"),
        "PHP's openUri() warning text must not be printed: {}",
        out.stdout
    );
    assert_eq!(
        out.stderr.matches("Warning: fopen(): Failed to open stream").count(),
        3,
        "each failed open reports through the stream layer's fopen() warning: {}",
        out.stderr
    );
}

/// `openUri()` supports plain paths plus `php://output` / `php://stdout` / `php://stderr`
/// only: a `file://` URI and `php://memory` answer `false` (and `toUri()` throws the
/// `ValueError` for them), where PHP opens both; `php://stdout` behaves like PHP's.
#[test]
fn test_xmlwriter_open_uri_rejects_file_and_memory_schemes() {
    if skip_without_xml_native("test_xmlwriter_open_uri_rejects_file_and_memory_schemes") {
        return;
    }
    let out = compile_and_run_capture(
        r#"<?php
$path = getcwd() . "/elephc_xw_file_uri.xml";
$w = new XMLWriter();
$ok = $w->openUri("file://" . $path);
var_dump($ok);
if ($ok) { $w->writeElement("f"); $w->flush(); }
unset($w);
var_dump(@file_get_contents($path));
@unlink($path);
$w = new XMLWriter();
$ok = $w->openUri("php://memory");
var_dump($ok);
if ($ok) { var_dump($w->writeElement("m"), $w->flush()); }
try { var_dump(XMLWriter::toUri("php://memory") instanceof XMLWriter); } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { var_dump(XMLWriter::toUri("file://" . $path) instanceof XMLWriter); } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
@unlink($path);
$w = new XMLWriter();
var_dump($w->openUri("php://stdout"));
$w->writeElement("out"); var_dump($w->flush());
"#,
    );
    assert!(out.success, "program failed: stdout={:?} stderr={}", out.stdout, out.stderr);
    // PHP 8.5.10:
    //   "bool(true)\nstring(4) \"<f/>\"\nbool(true)\nbool(true)\nint(4)\nbool(true)\n
    //    bool(true)\nbool(true)\n<out/>int(6)\n"
    assert_eq!(
        out.stdout,
        "bool(false)\nbool(false)\nbool(false)\nValueError: XMLWriter::toUri(): Argument #1 ($uri) must resolve to a valid file path\nValueError: XMLWriter::toUri(): Argument #1 ($uri) must resolve to a valid file path\nbool(true)\n<out/>int(6)\n"
    );
    assert_eq!(
        out.stderr.matches("Warning: fopen(): Failed to open stream").count(),
        4,
        "the two schemes fail through the stream layer, once per open: {}",
        out.stderr
    );
}

/// `XMLWriter::toStream()` does not detect a closed resource: it hands back a writer that
/// accepts content and flushes without an error (the byte count reported for a closed
/// handle is platform-dependent: 0 on Linux, the buffered length on macOS), where PHP
/// throws `TypeError`; an open stream receives PHP's bytes.
#[test]
fn test_xmlwriter_to_stream_does_not_detect_a_closed_resource() {
    if skip_without_xml_native("test_xmlwriter_to_stream_does_not_detect_a_closed_resource") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$h = fopen("php://memory", "w+");
fclose($h);
try { $w = XMLWriter::toStream($h); var_dump($w instanceof XMLWriter); var_dump($w->writeElement("s")); var_dump(is_int($w->flush())); } catch (TypeError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
$g = fopen("php://memory", "w+");
$w = XMLWriter::toStream($g); $w->writeElement("ok"); var_dump($w->flush()); rewind($g); var_dump(stream_get_contents($g));
"#,
    );
    // PHP 8.5.10:
    //   "TypeError: XMLWriter::toStream(): supplied resource is not a valid stream
    //    resource\nint(5)\nstring(5) \"<ok/>\"\n"
    assert_eq!(out, "bool(true)\nbool(true)\nbool(true)\nint(5)\nstring(5) \"<ok/>\"\n");
}

/// `xml_parse_into_struct()`'s `$values` / `$index` are typed `mixed` — they still report
/// `array`, count, index and iterate like arrays — and are left untouched when a handler
/// throws, where PHP hands back the partial arrays gathered before the throw.
#[test]
fn test_xml_parse_into_struct_outputs_are_mixed_and_untouched_when_a_handler_throws() {
    if skip_without_xml_native(
        "test_xml_parse_into_struct_outputs_are_mixed_and_untouched_when_a_handler_throws",
    ) {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$p = xml_parser_create();
xml_set_element_handler($p, function ($p, $n, $a) { echo "S $n\n"; if ($n === 'C') throw new RuntimeException("boom"); }, null);
$vals = "untouched"; $idx = "untouched-too";
try { $r = xml_parse_into_struct($p, "<a><b/><c/><d/></a>", $vals, $idx); var_dump($r); } catch (RuntimeException $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
var_dump($vals, $idx);
$p = xml_parser_create();
var_dump(xml_parse_into_struct($p, "<r><i>1</i></r>", $ok, $okIdx));
var_dump(gettype($ok), gettype($okIdx), count($ok), $ok[1]['value'], $okIdx['I'][0]);
foreach ($ok as $k => $e) { echo $k, "=", $e['tag'], " "; } echo "\n";
foreach ($okIdx as $tag => $positions) { echo $tag, ":", implode(",", $positions), " "; } echo "\n";
"#,
    );
    // PHP 8.5.10 prints, in place of the two "untouched" strings, the partial arrays:
    //   $vals = [["tag" => "A", "type" => "open", "level" => 1],
    //            ["tag" => "B", "type" => "complete", "level" => 2]]
    //   $idx  = ["A" => [0], "B" => [1]]
    // (as var_dump output), and the rest identically.
    assert_eq!(
        out,
        "S A\nS B\nS C\nRuntimeException: boom\nstring(9) \"untouched\"\nstring(13) \"untouched-too\"\nint(1)\nstring(5) \"array\"\nstring(5) \"array\"\nint(3)\nstring(1) \"1\"\nint(1)\n0=R 1=I 2=R \nR:0,2 I:1 \n"
    );
}

/// `xml_parser_set_option()` returns and stores what PHP does for an out-of-range
/// `XML_OPTION_SKIP_TAGSTART` and a value of the wrong type, but without PHP's
/// `E_WARNING`s; the unsupported-encoding `ValueError` is thrown on both sides.
#[test]
fn test_xml_parser_set_option_does_not_warn_on_bad_values() {
    if skip_without_xml_native("test_xml_parser_set_option_does_not_warn_on_bad_values") {
        return;
    }
    let out = compile_and_run_capture(
        r#"<?php
$p = xml_parser_create();
var_dump(xml_parser_set_option($p, XML_OPTION_SKIP_TAGSTART, -1), xml_parser_get_option($p, XML_OPTION_SKIP_TAGSTART));
var_dump(xml_parser_set_option($p, XML_OPTION_SKIP_TAGSTART, "abc"), xml_parser_get_option($p, XML_OPTION_SKIP_TAGSTART));
var_dump(xml_parser_set_option($p, XML_OPTION_CASE_FOLDING, "yes"), xml_parser_get_option($p, XML_OPTION_CASE_FOLDING));
var_dump(xml_parser_set_option($p, XML_OPTION_SKIP_WHITE, [1]), xml_parser_get_option($p, XML_OPTION_SKIP_WHITE));
try { xml_parser_set_option($p, XML_OPTION_TARGET_ENCODING, 5); } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
"#,
    );
    assert!(out.success, "program failed: stdout={:?} stderr={}", out.stdout, out.stderr);
    // PHP 8.5.10 prints the same values, with
    //   "\nWarning: xml_parser_set_option(): Argument #3 ($value) must be between 0 and
    //    2147483647 for option XML_OPTION_SKIP_TAGSTART in <file> on line 3\n"
    // before the first bool(false) and
    //   "\nWarning: xml_parser_set_option(): Argument #3 ($value) must be of type
    //    string|int|bool, array given in <file> on line 6\n"
    // before the array-valued call's bool(true).
    assert_eq!(
        out.stdout,
        "bool(false)\nint(0)\nbool(true)\nint(0)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\nValueError: xml_parser_set_option(): Argument #3 ($value) is not a supported target encoding\n"
    );
    assert_eq!(out.stderr, "", "no warning may be emitted on stderr either");
}

/// A dynamically named handler (its name held in a variable, so the checker cannot see
/// the callee) that declares more required parameters than its event supplies throws a
/// catchable `ArgumentCountError` when the event fires. The callback body must not run,
/// and execution continues after the catch. The closure-literal form is a compile error,
/// pinned in `tests/error_tests/xml.rs`.
#[test]
fn test_xml_dynamic_handler_with_surplus_required_parameters_is_catchable() {
    if skip_without_xml_native(
        "test_xml_dynamic_handler_with_surplus_required_parameters_is_catchable",
    ) {
        return;
    }
    let out = compile_and_run_capture(
        r#"<?php
function too_many($p, $n, $a, $extra) { echo "never\n"; }
$name = "too_many";
$p = xml_parser_create();
var_dump(xml_set_element_handler($p, $name, null));
try { var_dump(xml_parse($p, "<a/>", true)); } catch (ArgumentCountError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
echo "after\n";
"#,
    );
    // PHP 8.5.10 (exit 0):
    //   "bool(true)\nArgumentCountError: Too few arguments to function too_many(), 3 passed
    //    and exactly 4 expected\nafter\n"
    assert!(out.success, "the ArgumentCountError must be catchable: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(true)\nArgumentCountError: call_user_func_array(): missing required argument\nafter\n"
    );
    assert!(
        !out.stdout.contains("never"),
        "the rejected callback body must not run: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stderr, "");
}

/// In a program that never links the bridge, `function_exists()` answers `true` for the
/// ten registry builtins — `xml_parse_into_struct()` and the nine `xml_set_*_handler()`
/// setters — and `false` for the prelude functions, while PHP with `ext/xml` loaded
/// answers `true` for all of them.
///
/// Deliberately NOT gated on the managed libxml2 artifact: nothing here links
/// `elephc_xml`, so the fixture runs everywhere (like
/// `super::eval::test_xml_absent_bridge_inside_eval`).
#[test]
fn test_xml_function_exists_without_the_bridge() {
    let out = compile_and_run(
        r#"<?php
foreach (["xml_set_element_handler","xml_set_character_data_handler","xml_set_default_handler","xml_set_processing_instruction_handler","xml_set_notation_decl_handler","xml_set_unparsed_entity_decl_handler","xml_set_external_entity_ref_handler","xml_set_start_namespace_decl_handler","xml_set_end_namespace_decl_handler","xml_parse_into_struct","xml_parse","xml_parser_create","xml_set_object","xmlwriter_open_memory"] as $f) {
    echo $f, ": ", var_export(function_exists($f), true), "\n";
}
var_dump(extension_loaded("xml"));
"#,
    );
    // PHP 8.5.10 answers "true" for all fourteen names and bool(true).
    assert_eq!(
        out,
        "xml_set_element_handler: true\nxml_set_character_data_handler: true\nxml_set_default_handler: true\nxml_set_processing_instruction_handler: true\nxml_set_notation_decl_handler: true\nxml_set_unparsed_entity_decl_handler: true\nxml_set_external_entity_ref_handler: true\nxml_set_start_namespace_decl_handler: true\nxml_set_end_namespace_decl_handler: true\nxml_parse_into_struct: true\nxml_parse: false\nxml_parser_create: false\nxml_set_object: false\nxmlwriter_open_memory: false\nbool(false)\n"
    );
}
