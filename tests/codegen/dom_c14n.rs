//! Purpose:
//! End-to-end regressions for legacy and modern DOM canonicalization.
//!
//! Called from:
//! - `cargo test --test codegen_tests codegen::dom_c14n` through Rust's test harness.
//!
//! Key details:
//! - Tests exercise runtime array marshalling for XPath maps and namespace prefix lists.
//! - Memory, local-file, and registered-stream results are compared with PHP 8.5 behavior.

use crate::support::{compile_and_run, compile_and_run_capture};

/// Verifies subtree, comments, XPath namespaces, exclusive prefixes, and modern output.
#[test]
fn c14n_canonicalizes_legacy_and_modern_nodes_with_php_options() {
    let out = compile_and_run(
        r#"<?php
$document = new DOMDocument();
$document->loadXML(
    '<root xmlns:p="urn:x">'
    . '<p:item b="1" a="2"><!--c--></p:item><other/>'
    . '</root>'
);
$item = $document->documentElement->firstChild;
echo $item->C14N(), "\n";
echo $item->C14N(false, true), "\n";
$xpath = [
    "query" => "//p:item",
    "namespaces" => ["p" => "urn:x"],
];
echo $document->C14N(false, false, $xpath), "\n";
echo $document->C14N(true, false, null, ["p"]), "\n";

$modern = Dom\XMLDocument::createFromString(
    '<root xmlns:p="urn:x"><p:item/></root>'
);
echo $modern->documentElement->firstChild->C14N();
"#,
    );
    assert_eq!(
        out,
        concat!(
            "<p:item xmlns:p=\"urn:x\" a=\"2\" b=\"1\"></p:item>\n",
            "<p:item xmlns:p=\"urn:x\" a=\"2\" b=\"1\"><!--c--></p:item>\n",
            "<p:item></p:item>\n",
            "<root xmlns:p=\"urn:x\"><p:item a=\"2\" b=\"1\"></p:item>",
            "<other></other></root>\n",
            "<p:item xmlns:p=\"urn:x\"></p:item>",
        )
    );
}

/// Verifies missing/wrong XPath query options and the non-exclusive prefix notice.
#[test]
fn c14n_reports_exact_xpath_errors_and_prefix_notice() {
    let out = compile_and_run_capture(
        r#"<?php
$document = new DOMDocument();
$document->loadXML("<root><item/></root>");
foreach ([[], ["query" => 1], ["query" => "count(//*)"]] as $xpath) {
    try {
        $document->C14N(false, false, $xpath);
    } catch (Throwable $error) {
        echo get_class($error), "|", $error->getMessage(), "\n";
    }
}
echo $document->C14N(false, false, null, []);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "ValueError|DOMNode::C14N(): Argument #3 ($xpath) ",
            "must have a \"query\" key\n",
            "TypeError|DOMNode::C14N(): Argument #3 ($xpath) ",
            "\"query\" option must be a string, int given\n",
            "Error|XPath query did not return a nodeset\n",
            "<root><item></item></root>",
            "Notice: DOMNode::C14N(): Inclusive namespace prefixes ",
            "only allowed in exclusive mode.\n",
        )
    );
    assert_eq!(
        out.stderr,
        concat!(
            "Notice: DOMNode::C14N(): Inclusive namespace prefixes ",
            "only allowed in exclusive mode.\n",
        )
    );
}

/// Verifies local-file byte counts and registered stream writes use the same canonical bytes.
#[test]
fn c14n_file_writes_local_and_registered_stream_targets() {
    let out = compile_and_run(
        r#"<?php
class C14nStream {
    public $context;

    public function stream_open($path, $mode, $options, &$openedPath) {
        echo "O", $mode, "|";
        return true;
    }

    public function stream_write($data) {
        echo $data, "|";
        return strlen($data);
    }

    public function stream_flush() {
        echo "F|";
        return true;
    }

    public function stream_close() {
        echo "C|";
    }
}

$document = new DOMDocument();
$document->loadXML("<root><item/></root>");
$path = "/tmp/elephc-dom-c14n-file.xml";
echo $document->C14NFile($path), ":", file_get_contents($path), "|";
unlink($path);
stream_wrapper_register("c14nout", C14nStream::class);
echo $document->C14NFile("c14nout://result");
"#,
    );
    assert_eq!(
        out,
        concat!(
            "26:<root><item></item></root>|",
            "Owb|<root><item></item></root>|F|C|26",
        )
    );
}

/// Verifies wrapper-visible notices precede open and retained write strings outlive the bridge call.
#[test]
fn c14n_file_orders_notices_and_owns_registered_stream_write_arguments() {
    let out = compile_and_run_capture(
        r#"<?php
class RetainingC14nStream {
    public $context;
    public static string $written = "";

    public function stream_open($path, $mode, $options, &$openedPath) {
        echo "O|";
        return true;
    }

    public function stream_write($data) {
        self::$written .= $data;
        echo "W|";
        return strlen($data);
    }

    public function stream_flush() {
        echo "F|";
        return true;
    }

    public function stream_close() {
        echo "C|";
    }
}

stream_wrapper_register("c14nretain", RetainingC14nStream::class);
$document = new DOMDocument();
$document->loadXML("<root><item/></root>");
$count = $document->C14NFile(
    "c14nretain://result",
    false,
    false,
    null,
    []
);
echo $count, ":", RetainingC14nStream::$written;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "O|W|F|C|26:<root><item></item></root>",
        "stderr: {}",
        out.stderr,
    );
    assert_eq!(
        out.stderr,
        concat!(
            "Notice: DOMNode::C14NFile(): Inclusive namespace prefixes ",
            "only allowed in exclusive mode.\n",
        )
    );
}

/// Verifies legacy detached nodes and modern detached roots use distinct exception channels.
#[test]
fn c14n_rejects_nodes_without_php_visible_documents() {
    let out = compile_and_run(
        r#"<?php
$legacy = new DOMElement("legacy");
try {
    $legacy->C14N();
} catch (Throwable $error) {
    echo get_class($error), "|", $error->getMessage(), "\n";
}

$modern = Dom\XMLDocument::createEmpty();
$root = $modern->createElement("root");
try {
    $root->C14N();
} catch (Throwable $error) {
    echo get_class($error), "|", $error->getCode(), "|", $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "Error|Node must be associated with a document\n",
            "DOMException|3|Canonicalization can only happen on nodes ",
            "attached to a document.",
        )
    );
}

/// Verifies modern namespace relinking, DOM-built namespaces, and empty HTML safety.
#[test]
fn c14n_preserves_modern_namespace_declarations_and_empty_html() {
    let out = compile_and_run(
        r#"<?php
$parsed = Dom\XMLDocument::createFromString(
    '<root xmlns="urn:a" attr="val"/>'
);
$parsed->documentElement->setAttributeNS(
    "http://www.w3.org/2000/xmlns/",
    "xmlns:ns1",
    "urn:a"
);
echo $parsed->C14N(), "\n";

$built = Dom\XMLDocument::createEmpty();
$root = $built->createElementNS("urn:envelope", "env:Root");
$built->appendChild($root);
$child = $built->createElementNS("urn:child", "x:Child");
$root->appendChild($child);
echo $built->C14N(), "\n";

$emptyHtml = Dom\HTMLDocument::createEmpty();
var_dump($emptyHtml->C14N());
"#,
    );
    assert_eq!(
        out,
        concat!(
            "<root xmlns=\"urn:a\" xmlns:ns1=\"urn:a\" attr=\"val\"></root>\n",
            "<env:Root xmlns:env=\"urn:envelope\"><x:Child ",
            "xmlns:x=\"urn:child\"></x:Child></env:Root>\n",
            "string(0) \"\"\n",
        )
    );
}

/// Verifies special XML nodes and reference-backed option arrays remain canonical.
#[test]
fn c14n_handles_special_nodes_and_reference_backed_options() {
    let out = compile_and_run(
        r#"<?php
$document = new DOMDocument();
$document->loadXML(
    '<?xml version="1.0"?>'
    . '<!DOCTYPE doc []>'
    . '<doc xmlns=""><![CDATA[bar]]><!-- x --><temp xmlns=""/>'
    . '<?pi-no-data          ?></doc>'
);
echo $document->documentElement->C14N(withComments: true), "\n";

$namespaces = ["a" => ""];
$xpath = ["query" => "(//doc | //temp)", "namespaces" => $namespaces];
$prefixes = ["unused"];
foreach ($xpath["namespaces"] as $key => &$value) {
}
unset($value);
foreach ($xpath as $key => &$value) {
}
unset($value);
foreach ($prefixes as $key => &$value) {
}
unset($value);
echo $document->C14N(true, false, $xpath, $prefixes);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "<doc>bar<!-- x --><temp></temp><?pi-no-data?></doc>\n",
            "<doc><temp></temp></doc>",
        )
    );
}

/// Verifies exact object type names and internal libxml XPath error collection.
#[test]
fn c14n_reports_object_query_types_and_internal_xpath_errors() {
    let out = compile_and_run(
        r#"<?php
class C14nQueryObject {}
$document = new DOMDocument();
$document->loadXML("<root/>");
try {
    $document->C14N(false, false, ["query" => new C14nQueryObject()]);
} catch (TypeError $typeError) {
    echo $typeError->getMessage(), "\n";
}
libxml_use_internal_errors(true);
try {
    $document->C14N(false, false, ["query" => "["]);
} catch (Error $xpathError) {
    echo $xpathError->getMessage(), "\n";
}
$libxmlError = libxml_get_last_error();
echo $libxmlError->level, "|", $libxmlError->code, "|";
echo trim($libxmlError->message);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "DOMNode::C14N(): Argument #3 ($xpath) \"query\" option ",
            "must be a string, C14nQueryObject given\n",
            "XPath query did not return a nodeset\n",
            "2|1207|Invalid expression",
        )
    );
}
