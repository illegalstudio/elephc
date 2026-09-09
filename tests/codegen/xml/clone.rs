//! Purpose:
//! End-to-end fixtures pinning that `XMLParser` and `XMLWriter` are uncloneable: `clone`
//! throws PHP's `Error` (naming the runtime class for subclasses) and, because the
//! prelude's `__clone()` hook detaches the bridge handle from the shallow copy before
//! throwing, the original keeps working after the failed clone.
//!
//! Called from:
//! - `cargo test --test codegen_tests xml::clone` through Rust's test harness.
//!
//! Key details:
//! - Expected output is what PHP 8.5.10 prints for the same program.
//! - The compiler clones shallowly first and then runs `__clone()` on the copy (AOT and
//!   Magician's eval both do), so the copy briefly owns the original's integer handle; the
//!   fixtures use the original after the failed clone, after `unset()` and after leaving a
//!   function scope, which is where a copy that still owned the handle would have freed it.

use crate::support::*;

/// `clone $writer` throws PHP's `Error`; the original still writes and its buffer is intact,
/// both when the failed clone happens inline and when it happens inside a function whose
/// temporaries are released on return.
#[test]
fn test_xmlwriter_clone_throws_and_original_survives() {
    if skip_without_xml_native("test_xmlwriter_clone_throws_and_original_survives") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
function try_clone(XMLWriter $w): void {
    try {
        $c = clone $w;
        echo "cloned\n";
    } catch (Error $e) {
        echo get_class($e), ": ", $e->getMessage(), "\n";
    }
}
$w = new XMLWriter();
var_dump($w->openMemory());
$w->startElement('root');
try {
    $copy = clone $w;
    echo "cloned\n";
} catch (Error $e) {
    echo get_class($e), ": ", $e->getMessage(), "\n";
}
unset($copy);
var_dump($w->writeElement('item', 'v'));
try_clone($w);
var_dump($w->writeElement('again', 'w'));
var_dump($w->endElement());
echo $w->outputMemory(), "|END\n";
unset($w);
echo "done\n";
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nError: Trying to clone an uncloneable object of class XMLWriter\nbool(true)\nError: Trying to clone an uncloneable object of class XMLWriter\nbool(true)\nbool(true)\n<root><item>v</item><again>w</again></root>|END\ndone\n"
    );
}

/// `clone $parser` throws PHP's `Error`; the original still parses incrementally across the
/// failed clones and `xml_parser_free()` frees it exactly once.
#[test]
fn test_xml_parser_clone_throws_and_original_parses() {
    if skip_without_xml_native("test_xml_parser_clone_throws_and_original_parses") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
function try_clone(XMLParser $p): void {
    try {
        $c = clone $p;
        echo "cloned\n";
    } catch (Error $e) {
        echo get_class($e), ": ", $e->getMessage(), "\n";
    }
}
$p = xml_parser_create();
xml_set_element_handler($p, function ($p, $n, $a) { echo "S $n\n"; }, function ($p, $n) { echo "E $n\n"; });
try {
    $copy = clone $p;
    echo "cloned\n";
} catch (Error $e) {
    echo get_class($e), ": ", $e->getMessage(), "\n";
}
unset($copy);
var_dump(xml_parse($p, "<a>", false));
try_clone($p);
var_dump(xml_parse($p, "<b/></a>", true), xml_get_error_code($p), xml_get_current_line_number($p));
var_dump(xml_parser_free($p));
unset($p);
echo "done\n";
"#,
    );
    assert_eq!(
        out,
        "Error: Trying to clone an uncloneable object of class XMLParser\nint(1)\nError: Trying to clone an uncloneable object of class XMLParser\nS A\nS B\nE B\nE A\nint(1)\nint(0)\nint(1)\nbool(true)\ndone\n"
    );
}

/// A user subclass of `XMLWriter` inherits the guard and the message names the runtime
/// class; the subclass's own properties and the writer buffer survive the failed clone.
#[test]
fn test_xmlwriter_subclass_clone_names_runtime_class() {
    if skip_without_xml_native("test_xmlwriter_subclass_clone_names_runtime_class") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
class MyWriter extends XMLWriter {
    public string $tag = "sub";
}
function try_clone(MyWriter $w): void {
    try {
        $c = clone $w;
        echo "cloned\n";
    } catch (Error $e) {
        echo get_class($e), ": ", $e->getMessage(), "\n";
    }
}
$s = new MyWriter();
$s->openMemory();
$s->startElement('s');
try {
    $copy = clone $s;
    echo "cloned\n";
} catch (Error $e) {
    echo get_class($e), ": ", $e->getMessage(), "\n";
}
unset($copy);
try_clone($s);
var_dump($s->writeElement('k', 'x'), $s->tag, get_class($s));
$s->endElement();
echo $s->outputMemory(), "|END\n";
unset($s);
echo "done\n";
"#,
    );
    assert_eq!(
        out,
        "Error: Trying to clone an uncloneable object of class MyWriter\nError: Trying to clone an uncloneable object of class MyWriter\nbool(true)\nstring(3) \"sub\"\nstring(8) \"MyWriter\"\n<s><k>x</k></s>|END\ndone\n"
    );
}

/// Inside `eval()` the same catchable `Error` is raised for a compiled writer and parser
/// created outside eval and for ones created inside it; every original keeps working
/// afterwards, the host writer once eval has returned (the host parser is driven inside
/// eval, because locals eval touched are `mixed` afterwards and typed builtin parameters
/// reject them).
#[test]
fn test_xml_clone_inside_eval_throws_and_originals_survive() {
    if skip_without_xml_native("test_xml_clone_inside_eval_throws_and_originals_survive") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$outer = new XMLWriter();
$outer->openMemory();
$outer->startElement('outer');
$op = xml_parser_create();
eval('
try { $x = clone $outer; echo "cloned\n"; } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
unset($x);
try { $xp = clone $op; echo "cloned\n"; } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
unset($xp);
$inner = new XMLWriter();
$inner->openMemory();
$inner->startElement("inner");
try { $y = clone $inner; echo "cloned\n"; } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
unset($y);
var_dump($inner->writeElement("i", "1"));
$inner->endElement();
echo $inner->outputMemory(), "|END\n";
$ip = xml_parser_create();
try { $iq = clone $ip; echo "cloned\n"; } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
unset($iq);
var_dump(xml_parse($ip, "<z/>", true), xml_parser_free($ip));
var_dump(xml_parse($op, "<q/>", true), xml_parser_free($op));
unset($inner, $ip, $op);
');
var_dump($outer->writeElement('o', '2'));
$outer->endElement();
echo $outer->outputMemory(), "|END\n";
unset($outer);
echo "done\n";
"#,
    );
    assert_eq!(
        out,
        "Error: Trying to clone an uncloneable object of class XMLWriter\nError: Trying to clone an uncloneable object of class XMLParser\nError: Trying to clone an uncloneable object of class XMLWriter\nbool(true)\n<inner><i>1</i></inner>|END\nError: Trying to clone an uncloneable object of class XMLParser\nint(1)\nbool(true)\nint(1)\nbool(true)\nbool(true)\n<outer><o>2</o></outer>|END\ndone\n"
    );
}
