//! Purpose:
//! End-to-end regressions for PHP-visible SimpleXML `var_dump()` and `print_r()` output.
//!
//! Called from:
//! - `cargo test --test codegen_tests codegen::simplexml_debug_output` through Rust's test harness.
//!
//! Key details:
//! - Both renderers must call the concrete runtime `__debugInfo()` exactly once per object walk.
//! - Nested native wrappers remain real subclass-aware objects and projected values are released.

use crate::support::{
    compile_and_run, compile_and_run_capture, compile_and_run_with_heap_debug,
};

/// Verifies native SimpleXML projections preserve PHP's recursive dump and print shapes.
#[test]
fn simplexml_native_var_dump_and_print_r_match_php_recursive_shape() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$xml = simplexml_load_string('<r id="7"><a><b>B</b></a><a>A2</a><c>C</c></r>');
if ($xml === false) { exit(2); }
var_dump($xml);
print_r($xml);
"#,
    );
    assert!(
        out.success,
        "program failed after stdout {:?}: {}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        concat!(
            "object(SimpleXMLElement)#1 (3) {\n",
            "  [\"@attributes\"]=>\n",
            "  array(1) {\n",
            "    [\"id\"]=>\n",
            "    string(1) \"7\"\n",
            "  }\n",
            "  [\"a\"]=>\n",
            "  array(2) {\n",
            "    [0]=>\n",
            "    object(SimpleXMLElement)#2 (1) {\n",
            "      [\"b\"]=>\n",
            "      string(1) \"B\"\n",
            "    }\n",
            "    [1]=>\n",
            "    string(2) \"A2\"\n",
            "  }\n",
            "  [\"c\"]=>\n",
            "  string(1) \"C\"\n",
            "}\n",
            "SimpleXMLElement Object\n",
            "(\n",
            "    [@attributes] => Array\n",
            "        (\n",
            "            [id] => 7\n",
            "        )\n",
            "\n",
            "    [a] => Array\n",
            "        (\n",
            "            [0] => SimpleXMLElement Object\n",
            "                (\n",
            "                    [b] => B\n",
            "                )\n",
            "\n",
            "            [1] => A2\n",
            "        )\n",
            "\n",
            "    [c] => C\n",
            ")\n",
        )
    );
}

/// Verifies php-src PHPT 022's named-view and foreach attribute projections exactly.
#[test]
fn simplexml_phpt_022_attributes_inside_foreach_matches_php() {
    let out = compile_and_run(
        r#"<?php
$xml = "<pres><content><file glob=\"slide_*.xml\"/></content></pres>";
$sxe = simplexml_load_string($xml);

echo "===CONTENT===\n";
var_dump($sxe->content);

echo "===FILE===\n";
var_dump($sxe->content->file);

echo "===FOREACH===\n";
foreach ($sxe->content->file as $file) {
    var_dump($file);
    var_dump($file['glob']);
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "===CONTENT===\n",
            "object(SimpleXMLElement)#2 (1) {\n",
            "  [\"file\"]=>\n",
            "  object(SimpleXMLElement)#3 (1) {\n",
            "    [\"@attributes\"]=>\n",
            "    array(1) {\n",
            "      [\"glob\"]=>\n",
            "      string(11) \"slide_*.xml\"\n",
            "    }\n",
            "  }\n",
            "}\n",
            "===FILE===\n",
            "object(SimpleXMLElement)#3 (1) {\n",
            "  [\"@attributes\"]=>\n",
            "  array(1) {\n",
            "    [\"glob\"]=>\n",
            "    string(11) \"slide_*.xml\"\n",
            "  }\n",
            "}\n",
            "===FOREACH===\n",
            "object(SimpleXMLElement)#3 (1) {\n",
            "  [\"@attributes\"]=>\n",
            "  array(1) {\n",
            "    [\"glob\"]=>\n",
            "    string(11) \"slide_*.xml\"\n",
            "  }\n",
            "}\n",
            "object(SimpleXMLElement)#4 (1) {\n",
            "  [0]=>\n",
            "  string(11) \"slide_*.xml\"\n",
            "}\n",
        )
    );
}

/// Verifies a user subclass override is dispatched dynamically once per renderer.
#[test]
fn simplexml_subclass_debug_override_is_dynamic_and_single_shot() {
    let out = compile_and_run(
        r#"<?php
class DebugOverrideXml extends SimpleXMLElement {
    public static int $calls = 0;

    public function __debugInfo(): array {
        self::$calls++;
        return ['call' => self::$calls];
    }
}

$xml = simplexml_load_string('<r><a>A</a></r>', DebugOverrideXml::class);
if ($xml === false) { exit(2); }
var_dump($xml);
print_r($xml);
echo 'calls=' . DebugOverrideXml::$calls . "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "object(DebugOverrideXml)#1 (1) {\n",
            "  [\"call\"]=>\n",
            "  int(1)\n",
            "}\n",
            "DebugOverrideXml Object\n",
            "(\n",
            "    [call] => 2\n",
            ")\n",
            "calls=2\n",
        )
    );
}

/// Verifies recursive user projections terminate with each renderer's exact PHP marker.
#[test]
fn simplexml_subclass_recursive_debug_projection_is_guarded() {
    let out = compile_and_run(
        r#"<?php
class RecursiveDebugXml extends SimpleXMLElement {
    public function __debugInfo(): array {
        return ['self' => $this];
    }
}

$xml = simplexml_load_string('<r/>', RecursiveDebugXml::class);
if ($xml === false) { exit(2); }
var_dump($xml);
print_r($xml);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "object(RecursiveDebugXml)#1 (1) {\n",
            "  [\"self\"]=>\n",
            "  *RECURSION*\n",
            "}\n",
            "RecursiveDebugXml Object\n",
            "(\n",
            "    [self] => RecursiveDebugXml Object\n",
            " *RECURSION*\n",
            ")\n",
        )
    );
}

/// Verifies nullable overrides emit PHP 8.5's deprecation once per debug walk.
#[test]
fn simplexml_subclass_null_debug_projection_emits_exact_deprecation() {
    let out = compile_and_run_capture(
        r#"<?php
class NullDebugXml extends SimpleXMLElement {
    public function __debugInfo(): ?array {
        return null;
    }
}

$xml = simplexml_load_string('<r/>', NullDebugXml::class);
if ($xml === false) { exit(2); }
var_dump($xml);
print_r($xml);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "object(NullDebugXml)#1 (0) {\n",
            "}\n",
            "NullDebugXml Object\n",
            "(\n",
            ")\n",
        )
    );
    assert_eq!(
        out.stderr,
        concat!(
            "Deprecated: Returning null from NullDebugXml::__debugInfo() is deprecated, return an empty array instead\n",
            "Deprecated: Returning null from NullDebugXml::__debugInfo() is deprecated, return an empty array instead\n",
        )
    );
}
