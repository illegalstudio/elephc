//! Purpose:
//! End-to-end fixtures for `xml_parse_into_struct()`: the values/index layout, skip-white and
//! tag-start options, coexistence with user handlers, error truncation, undefined output
//! variables, and outputs living in every variable storage kind (`static`, `global`,
//! `use (&$v)` capture).
//!
//! Called from:
//! - `cargo test --test codegen_tests xml::into_struct` through Rust's test harness.
//!
//! Key details:
//! - `$values` / `$index` are never predeclared, exactly like real PHP code.
//! - The storage-kind fixtures read the outputs where the checker can see the write (inside
//!   the writing function or closure, or through a second `global` view): the lowering
//!   routes the store by storage kind, which is what these pin.

use crate::support::*;

/// The struct arrays carry PHP's entry layout and key order.
#[test]
fn test_xml_parse_into_struct_layout() {
    if skip_without_xml_native("test_xml_parse_into_struct_layout") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$xml = "<?xml version='1.0'?>\n<root a='1'>\n <item id='x'>hello</item>\n <item>  </item>\n <empty/>\n <?pi data?>\n <!-- c -->\n <d>a<e>b</e>c</d>\n tail\n</root>";
$p = xml_parser_create();
$r = xml_parse_into_struct($p, $xml, $vals, $idx);
var_dump($r);
echo json_encode($vals), "\n";
echo json_encode($idx), "\n";
$p = xml_parser_create();
xml_parser_set_option($p, XML_OPTION_SKIP_WHITE, 1);
xml_parser_set_option($p, XML_OPTION_CASE_FOLDING, 0);
xml_parser_set_option($p, XML_OPTION_SKIP_TAGSTART, 1);
$r = xml_parse_into_struct($p, $xml, $vals2);
var_dump($r);
echo json_encode($vals2), "\n";
"#,
    );
    assert_eq!(
        out,
        "int(1)\n[{\"tag\":\"ROOT\",\"type\":\"open\",\"level\":1,\"attributes\":{\"A\":\"1\"},\"value\":\"\\n \"},{\"tag\":\"ITEM\",\"type\":\"complete\",\"level\":2,\"attributes\":{\"ID\":\"x\"},\"value\":\"hello\"},{\"tag\":\"ROOT\",\"value\":\"\\n \",\"type\":\"cdata\",\"level\":1},{\"tag\":\"ITEM\",\"type\":\"complete\",\"level\":2,\"value\":\"  \"},{\"tag\":\"ROOT\",\"value\":\"\\n \",\"type\":\"cdata\",\"level\":1},{\"tag\":\"EMPTY\",\"type\":\"complete\",\"level\":2},{\"tag\":\"ROOT\",\"value\":\"\\n \\n \\n \",\"type\":\"cdata\",\"level\":1},{\"tag\":\"D\",\"type\":\"open\",\"level\":2,\"value\":\"a\"},{\"tag\":\"E\",\"type\":\"complete\",\"level\":3,\"value\":\"b\"},{\"tag\":\"D\",\"value\":\"c\",\"type\":\"cdata\",\"level\":2},{\"tag\":\"D\",\"type\":\"close\",\"level\":2},{\"tag\":\"ROOT\",\"value\":\"\\n tail\\n\",\"type\":\"cdata\",\"level\":1},{\"tag\":\"ROOT\",\"type\":\"close\",\"level\":1}]\n{\"ROOT\":[0,2,4,6,11,12],\"ITEM\":[1,3],\"EMPTY\":[5],\"D\":[7,9,10],\"E\":[8]}\nint(1)\n[{\"tag\":\"oot\",\"type\":\"open\",\"level\":1,\"attributes\":{\"a\":\"1\"}},{\"tag\":\"tem\",\"type\":\"complete\",\"level\":2,\"attributes\":{\"id\":\"x\"},\"value\":\"hello\"},{\"tag\":\"tem\",\"type\":\"complete\",\"level\":2},{\"tag\":\"mpty\",\"type\":\"complete\",\"level\":2},{\"tag\":\"\",\"type\":\"open\",\"level\":2,\"value\":\"a\"},{\"tag\":\"\",\"type\":\"complete\",\"level\":3,\"value\":\"b\"},{\"tag\":\"\",\"value\":\"c\",\"type\":\"cdata\",\"level\":2},{\"tag\":\"\",\"type\":\"close\",\"level\":2},{\"tag\":\"oot\",\"value\":\"\\n tail\\n\",\"type\":\"cdata\",\"level\":1},{\"tag\":\"oot\",\"type\":\"close\",\"level\":1}]\n"
    );
}

/// User handlers keep firing while the struct is built, a malformed document returns 0
/// with the entries gathered so far, and the arrays iterate like ordinary PHP arrays.
#[test]
fn test_xml_parse_into_struct_with_handlers_and_errors() {
    if skip_without_xml_native("test_xml_parse_into_struct_with_handlers_and_errors") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$p = xml_parser_create();
xml_set_element_handler($p, function($p,$n,$a){ echo "S $n\n"; }, function($p,$n){ echo "E $n\n"; });
xml_set_character_data_handler($p, function($p,$d){ echo "C ", var_export($d,true), "\n"; });
$r = xml_parse_into_struct($p, "<a>x<b>y</b></a>", $vals, $idx);
var_dump($r, count($vals), count($idx));
foreach ($vals as $entry) { echo $entry['tag'], ":", $entry['type'], ":", $entry['level'], "\n"; }
$p = xml_parser_create();
$r = xml_parse_into_struct($p, "<a>x<b>y</a>", $bad, $badIdx);
var_dump($r, xml_get_error_code($p));
echo json_encode($bad), " ", json_encode($badIdx), "\n";
$p = xml_parser_create();
var_dump(xml_parse_into_struct($p, "<r/>", values: $named, index: $namedIdx));
echo json_encode($named), " ", json_encode($namedIdx), "\n";
"#,
    );
    assert_eq!(
        out,
        "S A\nC 'x'\nS B\nC 'y'\nE B\nE A\nint(1)\nint(3)\nint(2)\nA:open:1\nB:complete:2\nA:close:1\nint(0)\nint(76)\n[{\"tag\":\"A\",\"type\":\"open\",\"level\":1,\"value\":\"x\"},{\"tag\":\"B\",\"type\":\"open\",\"level\":2,\"value\":\"y\"}] {\"A\":[0],\"B\":[1]}\nint(1)\n[{\"tag\":\"R\",\"type\":\"complete\",\"level\":1}] {\"R\":[0]}\n"
    );
}

/// A `static` output keeps the array across calls, and an `$index` omitted from a named
/// call (the planner materializes its `null` default) leaves the values intact.
#[test]
fn test_xml_parse_into_struct_static_outputs() {
    if skip_without_xml_native("test_xml_parse_into_struct_static_outputs") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
function withStatic(string $xml) { static $sv = null; $p = xml_parser_create(); xml_parse_into_struct($p, $xml, $sv, $si); echo count($sv), " ", count($si), "\n"; }
withStatic("<a><b/></a>");
withStatic("<a><b/><c/></a>");
$r = xml_parse_into_struct(xml_parser_create(), "<a><b/></a>", values: $v2);
echo $r, " ", count($v2), "\n";
"#,
    );
    assert_eq!(out, "3 2\n4 3\n1 3\n");
}

/// `global` outputs written inside a function are visible through another function's
/// `global` view of the same names.
#[test]
fn test_xml_parse_into_struct_global_outputs() {
    if skip_without_xml_native("test_xml_parse_into_struct_global_outputs") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
function withGlobal(string $xml) { global $gv, $gi; $p = xml_parser_create(); xml_parse_into_struct($p, $xml, $gv, $gi); echo count($gv), " ", count($gi), "\n"; }
withGlobal("<a><b/></a>");
function readGlobal() { global $gv, $gi; echo $gv[0]['tag'], " ", json_encode($gi), "\n"; }
readGlobal();
"#,
    );
    assert_eq!(out, "3 2\nA {\"A\":[0,2],\"B\":[1]}\n");
}

/// Outputs captured by reference (`use (&$v)`) are written through the shared cell, so
/// the enclosing scope observes the arrays after the closure ran.
#[test]
fn test_xml_parse_into_struct_by_ref_captured_outputs() {
    if skip_without_xml_native("test_xml_parse_into_struct_by_ref_captured_outputs") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$captured = null; $ci = null;
$f = function (string $xml) use (&$captured, &$ci) { $p = xml_parser_create(); xml_parse_into_struct($p, $xml, $captured, $ci); echo count($captured), " ", count($ci), "\n"; };
$f("<a><b/></a>");
$f("<a><b/><c/></a>");
echo gettype($captured), " ", gettype($ci), "\n";
"#,
    );
    assert_eq!(out, "3 2\n4 3\narray array\n");
}
