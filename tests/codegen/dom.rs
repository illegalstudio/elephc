//! Purpose:
//! End-to-end regressions for PHP DOM calls lowered through the native extension ABI.
//!
//! Called from:
//! - `cargo test --test codegen_tests codegen::dom` through Rust's test harness.
//!
//! Key details:
//! - Fixtures exercise real bridge linking, wrapper lifetime, argument encoding, and serialization.
//! - Expected output follows the PHP 8.5/libxml2 behavior frozen by the DOM specification.

use crate::support::{
    compile_and_run, compile_and_run_capture, compile_and_run_with_heap_debug,
};

/// Verifies native namespace wrappers expose php-src's recursion-safe virtual debug properties.
#[test]
fn dom_namespace_node_var_dump_projects_virtual_properties_heap_cleanly() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function namespaceDebugMethodVisible(DOMNameSpaceNode $node): bool {
    return method_exists($node, '__debugInfo');
}
$document = new DOMDocument();
$document->loadXML('<root/>');
$xpath = new DOMXPath($document);
$node = $xpath->query('//namespace::*')->item(0);
var_dump($node);
if ($node instanceof DOMNameSpaceNode) {
    echo namespaceDebugMethodVisible($node) ? "visible\n" : "hidden\n";
}
unset($node, $xpath, $document);
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(
        out.stdout,
        concat!(
            "object(DOMNameSpaceNode)#4 (10) {\n",
            "  [\"nodeName\"]=>\n  string(9) \"xmlns:xml\"\n",
            "  [\"nodeValue\"]=>\n  string(36) \"http://www.w3.org/XML/1998/namespace\"\n",
            "  [\"nodeType\"]=>\n  int(18)\n",
            "  [\"prefix\"]=>\n  string(3) \"xml\"\n",
            "  [\"localName\"]=>\n  string(3) \"xml\"\n",
            "  [\"namespaceURI\"]=>\n  string(36) \"http://www.w3.org/XML/1998/namespace\"\n",
            "  [\"isConnected\"]=>\n  bool(true)\n",
            "  [\"ownerDocument\"]=>\n  string(22) \"(object value omitted)\"\n",
            "  [\"parentNode\"]=>\n  string(22) \"(object value omitted)\"\n",
            "  [\"parentElement\"]=>\n  string(22) \"(object value omitted)\"\n",
            "}\n",
            "hidden\n",
        )
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Verifies native DOM collections expose live virtual debug properties without a PHP method.
#[test]
fn dom_collection_var_dump_projects_virtual_properties_heap_cleanly() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function nodeListDebugVisible(DOMNodeList $nodes): bool {
    return method_exists($nodes, '__debugInfo');
}
$document = new DOMDocument();
$document->loadXML('<root id="r"><child/></root>');
$nodes = $document->getElementsByTagName('*');
$attributes = $document->documentElement->attributes;
var_dump($nodes, $attributes);
var_dump(nodeListDebugVisible($nodes));
unset($attributes, $nodes, $document);
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(
        out.stdout,
        concat!(
            "object(DOMNodeList)#2 (1) {\n",
            "  [\"length\"]=>\n  int(2)\n",
            "}\n",
            "object(DOMNamedNodeMap)#4 (1) {\n",
            "  [\"length\"]=>\n  int(1)\n",
            "}\n",
            "bool(false)\n",
        )
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Verifies attached legacy elements project php-src's ordered virtual property surface.
#[test]
fn dom_element_var_dump_projects_virtual_properties_heap_cleanly() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$document = new DOMDocument();
$document->loadXML('<root><child/></root>');
$element = $document->documentElement;
var_dump($element);
unset($element, $document);
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert!(
        out.stdout.contains("object(DOMElement)#") && out.stdout.contains(" (27) {"),
        "missing DOMElement virtual projection: {}",
        out.stdout
    );
    for property in ["tagName", "firstElementChild", "nodeName", "ownerDocument", "textContent"] {
        assert!(
            out.stdout.contains(&format!("[\"{property}\"]=>")),
            "DOMElement projection omitted {property}: {}",
            out.stdout
        );
    }
    assert!(
        out.stdout.contains("string(22) \"(object value omitted)\"")
            && !out.stdout.contains("uninitialized")
            && out.stdout.ends_with("}\n"),
        "DOMElement projection diverged from php-src: {}",
        out.stdout
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Verifies `var_dump()` releases DOM debug-projection wrapper temporaries before reuse.
///
/// php-src recycles the first document's detached wrappers before the second document is
/// materialized.  This preserves the observable `DOMElement` handle order from bug80602_3.
#[test]
fn dom_debug_projection_releases_wrapper_temporaries_before_next_document() {
    let out = compile_and_run_capture(
        r#"<?php
$document = new DOMDocument();
$document->loadXML('<a>foo<last/></a>');
$target = $document->documentElement->lastChild;
$target->before('bar', $document->documentElement->firstChild, 'baz');
var_dump($target);

$document = new DOMDocument();
$document->loadXML('<a>foo<last/></a>');
$target = $document->documentElement->lastChild;
$target->after('bar', $document->documentElement->firstChild, 'baz');
var_dump($target);
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    let headers = out
        .stdout
        .lines()
        .filter(|line| line.starts_with("object(DOMElement)#"))
        .collect::<Vec<_>>();
    assert_eq!(
        headers,
        vec!["object(DOMElement)#3 (27) {", "object(DOMElement)#2 (27) {"],
        "DOM debug projection retained wrappers beyond var_dump: {}",
        out.stdout
    );
}

/// Asserts a heap-debug run releases every allocation it reports before process exit.
fn assert_dom_debug_projection_heap_clean(case: &str, stderr: &str) {
    let summary = stderr
        .lines()
        .find(|line| line.starts_with("HEAP DEBUG: allocs="))
        .unwrap_or_else(|| panic!("{case}: missing heap-debug summary: {stderr}"));
    let parse_stat = |name: &str| {
        summary
            .split_whitespace()
            .find_map(|field| field.strip_prefix(&format!("{name}=")))
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or_else(|| {
                panic!("{case}: missing {name} in heap-debug summary: {summary}")
            })
    };
    let allocs = parse_stat("allocs");
    let frees = parse_stat("frees");
    let live_blocks = parse_stat("live_blocks");
    let live_bytes = parse_stat("live_bytes");

    assert_eq!(allocs, frees, "{case}: allocation delta leaked: {stderr}");
    assert_eq!(live_blocks, 0, "{case}: live blocks leaked: {stderr}");
    assert_eq!(live_bytes, 0, "{case}: live bytes leaked: {stderr}");
    assert!(
        stderr
            .lines()
            .any(|line| line == "HEAP DEBUG: leak summary: clean"),
        "{case}: missing clean heap-debug conclusion: {stderr}"
    );
}

/// Distinguishes debug-projection hash ownership from ordinary DOM wrapper-cache ownership.
///
/// The shared mutation matches the first half of upstream `bug80602_3`.  The probes then add
/// no debug projection, one projection, two projections, or a direct virtual-property read.
/// All four must reclaim their allocations; the dump count intentionally avoids object-ID ties.
#[test]
fn dom_debug_projection_heap_debug_matrix_reclaims_each_probe() {
    for (case, probe, expected_dumps, expected_tail) in [
        ("baseline", "echo \"baseline\\n\";", 0, "baseline\n"),
        (
            "one var_dump",
            "var_dump($target); echo \"one\\n\";",
            1,
            "one\n",
        ),
        (
            "two var_dumps",
            "var_dump($target); var_dump($target); echo \"two\\n\";",
            2,
            "two\n",
        ),
        (
            "direct virtual property",
            "echo $target->nodeName, \"\\n\";",
            0,
            "last\n",
        ),
    ] {
        let source = format!(
            r#"<?php
$document = new DOMDocument();
$document->loadXML('<a>foo<last/></a>');
$target = $document->documentElement->lastChild;
$target->before('bar', $document->documentElement->firstChild, 'baz');
{probe}
unset($target, $document);
"#,
        );
        let out = compile_and_run_with_heap_debug(&source);

        assert!(
            out.success,
            "{case}: program failed: stdout={:?} stderr={}",
            out.stdout,
            out.stderr
        );
        assert_eq!(
            out.stdout.matches("object(DOMElement)#").count(),
            expected_dumps,
            "{case}: unexpected debug-projection count: {}",
            out.stdout
        );
        assert!(
            out.stdout.ends_with(expected_tail),
            "{case}: unexpected probe output: {}",
            out.stdout
        );
        assert_dom_debug_projection_heap_clean(case, &out.stderr);
    }
}

/// Verifies inherited DOM node lifecycle hooks reject both operations with concrete classes.
#[test]
fn dom_node_serialization_hooks_match_php_concrete_class_errors() {
    let out = compile_and_run(
        r#"<?php
function showLegacyLifecycleErrors(DOMNode $node): void {
    try {
        $node->__sleep();
    } catch (Exception $exception) {
        echo get_class($node) . "|" . get_class($exception) . "|";
        echo $exception->getMessage() . "\n";
    }
    try {
        $node->__wakeup();
    } catch (Exception $exception) {
        echo get_class($node) . "|" . get_class($exception) . "|";
        echo $exception->getMessage() . "\n";
    }
}

function showModernLifecycleErrors(Dom\Node $node): void {
    try {
        $node->__sleep();
    } catch (Exception $exception) {
        echo get_class($node) . "|" . get_class($exception) . "|";
        echo $exception->getMessage() . "\n";
    }
    try {
        $node->__wakeup();
    } catch (Exception $exception) {
        echo get_class($node) . "|" . get_class($exception) . "|";
        echo $exception->getMessage() . "\n";
    }
}

$legacy = new DOMDocument();
$legacy->loadXML("<root/>");
showLegacyLifecycleErrors($legacy);
showLegacyLifecycleErrors(new DOMElement("root"));

$modern = Dom\XMLDocument::createEmpty();
showModernLifecycleErrors($modern);
showModernLifecycleErrors($modern->createElement("root"));

$html = Dom\HTMLDocument::createEmpty();
showModernLifecycleErrors($html);
showModernLifecycleErrors($html->createElement("p"));
"#,
    );
    assert_eq!(
        out,
        concat!(
            "DOMDocument|Exception|Serialization of 'DOMDocument' is not allowed, unless serialization methods are implemented in a subclass\n",
            "DOMDocument|Exception|Unserialization of 'DOMDocument' is not allowed, unless unserialization methods are implemented in a subclass\n",
            "DOMElement|Exception|Serialization of 'DOMElement' is not allowed, unless serialization methods are implemented in a subclass\n",
            "DOMElement|Exception|Unserialization of 'DOMElement' is not allowed, unless unserialization methods are implemented in a subclass\n",
            "Dom\\XMLDocument|Exception|Serialization of 'Dom\\XMLDocument' is not allowed, unless serialization methods are implemented in a subclass\n",
            "Dom\\XMLDocument|Exception|Unserialization of 'Dom\\XMLDocument' is not allowed, unless unserialization methods are implemented in a subclass\n",
            "Dom\\Element|Exception|Serialization of 'Dom\\Element' is not allowed, unless serialization methods are implemented in a subclass\n",
            "Dom\\Element|Exception|Unserialization of 'Dom\\Element' is not allowed, unless unserialization methods are implemented in a subclass\n",
            "Dom\\HTMLDocument|Exception|Serialization of 'Dom\\HTMLDocument' is not allowed, unless serialization methods are implemented in a subclass\n",
            "Dom\\HTMLDocument|Exception|Unserialization of 'Dom\\HTMLDocument' is not allowed, unless unserialization methods are implemented in a subclass\n",
            "Dom\\HTMLElement|Exception|Serialization of 'Dom\\HTMLElement' is not allowed, unless serialization methods are implemented in a subclass\n",
            "Dom\\HTMLElement|Exception|Unserialization of 'Dom\\HTMLElement' is not allowed, unless unserialization methods are implemented in a subclass\n",
        )
    );
}

/// Verifies PHP's legacy schema/config placeholders, including the config deprecation.
#[test]
fn legacy_schema_type_info_and_document_config_match_php() {
    let out = compile_and_run_capture(
        r#"<?php
$element = new DOMElement("root");
$attribute = new DOMAttr("name", "value");
$document = new DOMDocument();
var_dump($element->schemaTypeInfo);
var_dump($attribute->schemaTypeInfo);
var_dump($document->config);
$suppressed = @$document->config;
var_dump($suppressed);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "NULL\nNULL\nNULL\nNULL\n");
    assert_eq!(
        out.stderr,
        "Deprecated: Property DOMDocument::$config is deprecated\n"
    );
}

/// Verifies `DOMException::$code` remains an ordinary mutable inherited property.
#[test]
fn dom_exception_code_property_is_compiler_resident_and_mutable() {
    let out = compile_and_run(
        r#"<?php
try {
    new DOMElement("bad name");
} catch (DOMException $exception) {
    echo get_class($exception) . "|" . $exception->code . "|";
    $exception->code = 9;
    echo $exception->code . "|" . $exception->getCode();
}
"#,
    );
    assert_eq!(out, "DOMException|5|9|9");
}

/// Verifies the internal adjacent-position enum uses ordinary backed-enum semantics.
#[test]
fn adjacent_position_enum_is_compiler_resident_and_matches_php() {
    let out = compile_and_run(
        r#"<?php
foreach (Dom\AdjacentPosition::cases() as $case) {
    echo $case->name . "=" . $case->value . ";";
}
echo "\n";
echo Dom\AdjacentPosition::from("beforebegin")->name . "|";
var_dump(Dom\AdjacentPosition::tryFrom("bad"));
try {
    Dom\AdjacentPosition::from("bad");
} catch (ValueError $error) {
    echo get_class($error) . "|" . $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "BeforeBegin=beforebegin;AfterBegin=afterbegin;",
            "BeforeEnd=beforeend;AfterEnd=afterend;\n",
            "BeforeBegin|NULL\n",
            "ValueError|\"bad\" is not a valid backing value for enum ",
            "Dom\\AdjacentPosition",
        )
    );
}

/// Verifies an ordinary legacy document parses and serializes through its native wrapper.
#[test]
fn legacy_document_loadxml_and_savexml_round_trip() {
    let out = compile_and_run(
        r#"<?php
$document = new DOMDocument();
if (!$document->loadXML("<root><message>Hello DOM</message></root>")) {
    exit(1);
}
echo $document->saveXML();
"#,
    );
    assert_eq!(
        out,
        "<?xml version=\"1.0\"?>\n<root><message>Hello DOM</message></root>\n"
    );
}

/// Verifies distinct non-empty constructor strings retain their own ABI byte ranges.
#[test]
fn legacy_document_constructor_preserves_multiple_string_arguments() {
    let out = compile_and_run(
        r#"<?php
$document = new DOMDocument("1.1", "UTF-8");
echo $document->version . "|" . $document->encoding . "\n";
echo $document->saveXML();
"#,
    );
    assert_eq!(out, "1.1|UTF-8\n<?xml version=\"1.1\" encoding=\"UTF-8\"?>\n");
}

/// Verifies modern static construction returns a live wrapper with PHP's default metadata.
#[test]
fn modern_xml_document_factory_materializes_wrapper() {
    let out = compile_and_run(
        r#"<?php
$document = Dom\XMLDocument::createEmpty();
echo $document->xmlVersion . "|" . $document->xmlEncoding . "|";
echo $document->saveXml();
"#,
    );
    assert_eq!(
        out,
        "1.0|UTF-8|<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"
    );
}

/// Verifies legacy tree construction preserves wrapper identity and core node properties.
#[test]
fn legacy_tree_construction_preserves_php_wrapper_identity() {
    let out = compile_and_run(
        r#"<?php
$document = new DOMDocument();
$element = $document->createElement("root", "value");
if ($element === false) {
    exit(2);
}
$text = $document->createTextNode("tail");
echo get_class($element) . "|";
echo $element->nodeName . "|" . $element->nodeType . "|" . $element->textContent . "|";
echo $element->ownerDocument === $document ? "O" : "X";
echo $document->appendChild($element) === $element ? "A" : "X";
echo $document->documentElement === $element ? "R" : "X";
echo $element->appendChild($text) === $text ? "T" : "X";
echo $text->parentNode === $element ? "P" : "X";
echo "|" . $element->textContent . "|";
echo $document->saveXML();
"#,
    );
    assert_eq!(
        out,
        "DOMElement|root|1|value|OARTP|valuetail|<?xml version=\"1.0\"?>\n<root>valuetail</root>\n"
    );
}

/// Verifies modern XML tree construction uses modern wrapper classes with shared identity.
#[test]
fn modern_xml_tree_construction_preserves_php_wrapper_identity() {
    let out = compile_and_run(
        r#"<?php
$document = Dom\XMLDocument::createEmpty();
$element = $document->createElement("root");
$text = $document->createTextNode("value");
echo get_class($element) . "|";
echo $document->appendChild($element) === $element ? "A" : "X";
echo $document->documentElement === $element ? "R" : "X";
echo $element->appendChild($text) === $text ? "T" : "X";
echo $text->ownerDocument === $document ? "O" : "X";
echo $text->parentNode === $element ? "P" : "X";
echo "|" . $element->textContent . "|";
echo $document->saveXml();
"#,
    );
    assert_eq!(
        out,
        "Dom\\Element|ARTOP|value|<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<root>value</root>"
    );
}

/// Verifies legacy document factories expose the exact PHP node classes and kinds.
#[test]
fn legacy_document_core_node_factories_match_php() {
    let out = compile_and_run(
        r#"<?php
$document = new DOMDocument();
$cdata = $document->createCDATASection("a");
if ($cdata === false) { exit(2); }
$comment = $document->createComment("b");
$fragment = $document->createDocumentFragment();
$instruction = $document->createProcessingInstruction("pi", "c");
if ($instruction === false) { exit(3); }
$entity = $document->createEntityReference("ent");
if ($entity === false) { exit(4); }
echo get_class($cdata) . "|" . $cdata->nodeName . "|" . $cdata->nodeType . "|" . $cdata->textContent . "\n";
echo get_class($comment) . "|" . $comment->nodeName . "|" . $comment->nodeType . "|" . $comment->textContent . "\n";
echo get_class($fragment) . "|" . $fragment->nodeName . "|" . $fragment->nodeType . "|" . $fragment->textContent . "\n";
echo get_class($instruction) . "|" . $instruction->nodeName . "|" . $instruction->nodeType . "|" . $instruction->textContent . "\n";
echo get_class($entity) . "|" . $entity->nodeName . "|" . $entity->nodeType . "|" . $entity->textContent . "\n";
"#,
    );
    assert_eq!(
        out,
        "DOMCdataSection|#cdata-section|4|a\nDOMComment|#comment|8|b\nDOMDocumentFragment|#document-fragment|11|\nDOMProcessingInstruction|pi|7|c\nDOMEntityReference|ent|5|\n"
    );
}

/// Verifies modern XML factories preserve modern classes and nullable entity text content.
#[test]
fn modern_xml_core_node_factories_match_php() {
    let out = compile_and_run(
        r#"<?php
$document = Dom\XMLDocument::createEmpty();
$cdata = $document->createCDATASection("a");
$comment = $document->createComment("b");
$fragment = $document->createDocumentFragment();
$instruction = $document->createProcessingInstruction("pi", "c");
$entity = $document->createEntityReference("ent");
echo get_class($cdata) . "|" . $cdata->nodeName . "|" . $cdata->nodeType . "|" . $cdata->textContent . "\n";
echo get_class($comment) . "|" . $comment->nodeName . "|" . $comment->nodeType . "|" . $comment->textContent . "\n";
echo get_class($fragment) . "|" . $fragment->nodeName . "|" . $fragment->nodeType . "|" . $fragment->textContent . "\n";
echo get_class($instruction) . "|" . $instruction->nodeName . "|" . $instruction->nodeType . "|" . $instruction->textContent . "\n";
echo get_class($entity) . "|" . $entity->nodeName . "|" . $entity->nodeType . "|" . ($entity->textContent === null ? "N" : "X") . "\n";
"#,
    );
    assert_eq!(
        out,
        "Dom\\CDATASection|#cdata-section|4|a\nDom\\Comment|#comment|8|b\nDom\\DocumentFragment|#document-fragment|11|\nDom\\ProcessingInstruction|pi|7|c\nDom\\EntityReference|ent|5|N\n"
    );
}

/// Verifies legacy navigation, inclusive ancestry, connectivity, and detachment match PHP.
#[test]
fn legacy_node_navigation_and_detachment_match_php() {
    let out = compile_and_run(
        r#"<?php
$document = new DOMDocument();
$root = $document->createElement("root");
if ($root === false) { exit(2); }
$comment = $document->createComment("c");
$a = $document->createElement("a");
if ($a === false) { exit(3); }
$text = $document->createTextNode("t");
$b = $document->createElement("b");
if ($b === false) { exit(4); }
$document->appendChild($root);
$root->appendChild($comment);
$root->appendChild($a);
$root->appendChild($text);
$root->appendChild($b);
echo $root->firstChild === $comment ? "F" : "x";
echo $root->lastChild === $b ? "L" : "x";
echo $a->previousSibling === $comment ? "P" : "x";
echo $a->nextSibling === $text ? "N" : "x";
echo $a->parentElement === $root ? "E" : "x";
echo $a->parentNode === $root ? "p" : "x";
echo $a->isConnected ? "C" : "x";
echo $root->hasChildNodes() ? "H" : "x";
echo $a->hasChildNodes() ? "x" : "0";
echo $a->isSameNode($a) ? "S" : "x";
echo $root->contains($b) ? "T" : "x";
echo $b->contains($root) ? "x" : "f";
echo $a->getRootNode() === $document ? "R" : "x";
$root->removeChild($a);
echo $a->isConnected ? "x" : "D";
echo $a->parentNode === null ? "U" : "x";
echo $a->getRootNode() === $a ? "A" : "x";
"#,
    );
    assert_eq!(out, "FLPNEpCH0STfRDUA");
}

/// Verifies modern navigation shares PHP's identity and connectivity semantics.
#[test]
fn modern_node_navigation_and_detachment_match_php() {
    let out = compile_and_run(
        r#"<?php
$document = Dom\XMLDocument::createEmpty();
$root = $document->createElement("root");
$comment = $document->createComment("c");
$a = $document->createElement("a");
$text = $document->createTextNode("t");
$b = $document->createElement("b");
$document->appendChild($root);
$root->appendChild($comment);
$root->appendChild($a);
$root->appendChild($text);
$root->appendChild($b);
echo $root->firstChild === $comment ? "F" : "x";
echo $root->lastChild === $b ? "L" : "x";
echo $a->previousSibling === $comment ? "P" : "x";
echo $a->nextSibling === $text ? "N" : "x";
echo $a->parentElement === $root ? "E" : "x";
echo $a->parentNode === $root ? "p" : "x";
echo $a->isConnected ? "C" : "x";
echo $root->hasChildNodes() ? "H" : "x";
echo $a->hasChildNodes() ? "x" : "0";
echo $a->isSameNode($a) ? "S" : "x";
echo $root->contains($b) ? "T" : "x";
echo $b->contains($root) ? "x" : "f";
echo $a->getRootNode() === $document ? "R" : "x";
$root->removeChild($a);
echo $a->isConnected ? "x" : "D";
echo $a->parentNode === null ? "U" : "x";
echo $a->getRootNode() === $a ? "A" : "x";
"#,
    );
    assert_eq!(out, "FLPNEpCH0STfRDUA");
}

/// Verifies legacy insert/replace mutations and structured DOM exceptions match PHP.
#[test]
fn legacy_tree_mutations_throw_catchable_dom_exceptions() {
    let out = compile_and_run(
        r#"<?php
$document = new DOMDocument();
$root = $document->createElement("root");
$a = $document->createElement("a");
$b = $document->createElement("b");
$c = $document->createElement("c");
if ($root === false) { exit(2); }
if ($a === false) { exit(3); }
if ($b === false) { exit(4); }
if ($c === false) { exit(5); }
$document->appendChild($root);
$root->appendChild($a);
$root->appendChild($b);
$root->appendChild($c);
echo $root->insertBefore($c, $b) === $c ? "I" : "x";
echo $root->firstChild === $a ? "a" : "x";
echo $a->nextSibling === $c ? "c" : "x";
echo $c->nextSibling === $b ? "b" : "x";
echo $root->replaceChild($a, $c) === $c ? "R" : "x";
echo $root->firstChild === $a && $a->nextSibling === $b ? "O" : "x";
echo $c->parentNode === null ? "D" : "x";
try {
    $root->removeChild($c);
} catch (DOMException $error) {
    echo "|" . $error->getCode() . ":" . $error->getMessage();
}
try {
    $root->appendChild($root);
} catch (DOMException $error) {
    echo "|" . $error->getCode() . ":" . $error->getMessage();
}
$other = new DOMDocument();
$foreign = $other->createElement("foreign");
if ($foreign === false) { exit(3); }
try {
    $root->appendChild($foreign);
} catch (DOMException $error) {
    echo "|" . $error->getCode() . ":" . $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "IacbROD|8:Not Found Error|3:Hierarchy Request Error|4:Wrong Document Error"
    );
}

/// Verifies modern document hierarchy validation uses PHP's operation-specific message.
#[test]
fn modern_tree_mutations_throw_catchable_dom_exceptions() {
    let out = compile_and_run(
        r#"<?php
$document = Dom\XMLDocument::createEmpty();
$root = $document->createElement("root");
$a = $document->createElement("a");
$b = $document->createElement("b");
$c = $document->createElement("c");
$document->appendChild($root);
$root->appendChild($a);
$root->appendChild($b);
$root->appendChild($c);
echo $root->insertBefore($c, $b) === $c ? "I" : "x";
echo $root->firstChild === $a ? "a" : "x";
echo $a->nextSibling === $c ? "c" : "x";
echo $c->nextSibling === $b ? "b" : "x";
echo $root->replaceChild($a, $c) === $c ? "R" : "x";
echo $root->firstChild === $a && $a->nextSibling === $b ? "O" : "x";
echo $c->parentNode === null ? "D" : "x";
try {
    $root->removeChild($c);
} catch (DOMException $error) {
    echo "|" . $error->getCode() . ":" . $error->getMessage();
}
try {
    $root->appendChild($root);
} catch (DOMException $error) {
    echo "|" . $error->getCode() . ":" . $error->getMessage();
}
$second = $document->createElement("second");
try {
    $document->appendChild($second);
} catch (DOMException $error) {
    echo "|" . $error->getCode() . ":" . $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "IacbROD|8:Not Found Error|3:Hierarchy Request Error|3:Cannot have more than one element child in a document"
    );
}

/// Verifies legacy namespace metadata, paths, cloning, and node-value writes match PHP.
#[test]
fn legacy_node_metadata_clone_and_value_semantics_match_php() {
    let out = compile_and_run(
        r#"<?php
$document = new DOMDocument();
if (!$document->loadXML('<root xmlns="urn:d" xmlns:p="urn:p" p:a="v"><p:child>txt</p:child></root>')) {
    exit(2);
}
$root = $document->documentElement;
if ($root === null) { exit(3); }
$child = $root->firstChild;
if ($child === null) { exit(4); }
echo $root->nodeValue . "|";
echo $child->namespaceURI . "|" . $child->prefix . "|" . $child->localName . "|";
echo $child->lookupNamespaceURI("p") . "|" . $child->lookupPrefix("urn:p") . "|";
echo $child->isDefaultNamespace("urn:d") ? "D" : "x";
echo $root->hasAttributes() ? "|A" : "|x";
echo "|" . $child->getLineNo() . "|" . $child->getNodePath() . "|";
$shallow = $root->cloneNode(false);
$deep = $root->cloneNode(true);
if ($shallow === false) { exit(5); }
if ($deep === false) { exit(6); }
echo get_class($shallow) . ":" . ($shallow->firstChild === null ? "0" : "x") . "|";
$deepChild = $deep->firstChild;
if ($deepChild === null) { exit(7); }
echo get_class($deep) . ":" . $deepChild->nodeName . "|";
$child->nodeValue = "new";
echo $child->textContent;
"#,
    );
    assert_eq!(
        out,
        "txt|urn:p|p|child|urn:p|p|D|A|1|/*/p:child|DOMElement:0|DOMElement:p:child|new"
    );
}

/// Verifies modern node-value nullability and concrete clone wrappers match PHP.
#[test]
fn modern_node_metadata_and_clone_semantics_match_php() {
    let out = compile_and_run(
        r#"<?php
$document = Dom\XMLDocument::createFromString(
    '<root xmlns="urn:d" xmlns:p="urn:p" p:a="v"><p:child>txt</p:child></root>'
);
$root = $document->documentElement;
if ($root === null) { exit(3); }
$child = $root->firstChild;
if ($child === null) { exit(4); }
if (!$child instanceof Dom\Element) { exit(5); }
echo $root->nodeValue === null ? "N|" : "x|";
echo $child->namespaceURI . "|" . $child->prefix . "|" . $child->localName . "|";
echo $child->lookupNamespaceURI("p") . "|" . $child->lookupPrefix("urn:p") . "|";
echo $child->isDefaultNamespace("urn:d") ? "D" : "x";
echo $root->hasAttributes() ? "|A" : "|x";
echo "|" . $child->getLineNo() . "|" . $child->getNodePath() . "|";
$shallow = $root->cloneNode(false);
$deep = $root->cloneNode(true);
echo get_class($shallow) . ":" . ($shallow->firstChild === null ? "0" : "x") . "|";
$deepChild = $deep->firstChild;
if ($deepChild === null) { exit(6); }
echo get_class($deep) . ":" . $deepChild->nodeName;
"#,
    );
    assert_eq!(
        out,
        "N|urn:p|p|child|urn:p|p|D|A|1|/*/p:child|Dom\\Element:0|Dom\\Element:p:child"
    );
}

/// Verifies substituted element content and template-aware clone/import/adoption match PHP 8.5.
#[test]
fn modern_substituted_content_and_template_graph_copies_match_php() {
    let out = compile_and_run(
        r#"<?php
$xml = Dom\XMLDocument::createFromString('<root><old/></root>');
$root = $xml->documentElement;
$root->substitutedNodeValue = '&#x31;';
echo "S1|{$root->substitutedNodeValue}|";
var_export($root->nodeValue);
echo "|" . $xml->saveXML($root) . "\n";
$root->substitutedNodeValue = '&lt;&gt;';
echo "S2|{$root->substitutedNodeValue}|" . $xml->saveXML($root) . "\n";
$root->substitutedNodeValue = '';
echo "S3|" . strlen($root->substitutedNodeValue) . "|" . $xml->saveXML($root) . "\n";

$html = Dom\HTMLDocument::createFromString(
    '<!doctype html><template><b>x</b><template><i>n</i></template></template>',
    LIBXML_NOERROR,
);
$template = $html->head->firstChild;
if (!$template instanceof Dom\Element) { exit(2); }
$template->substitutedNodeValue = '&lt;y&gt;';
echo "T0|{$template->substitutedNodeValue}|{$template->textContent}|";
echo $template->innerHTML . "|" . $template->outerHTML . "\n";
$shallow = $template->cloneNode(false);
$deep = $template->cloneNode(true);
$objectClone = clone $template;
echo "T1|" . $html->saveHTML($shallow) . "|";
echo $html->saveHTML($deep) . "|" . $html->saveHTML($objectClone) . "\n";

$documentClone = clone $html;
$shallowDocument = $html->cloneNode(false);
echo "D1|" . get_class($documentClone) . "|";
echo $documentClone->saveHTML($documentClone->head->firstChild) . "|";
echo get_class($shallowDocument) . ":";
echo $shallowDocument->childNodes->length . "\n";

$other = Dom\HTMLDocument::createEmpty();
$imported = $other->importNode($template, true);
echo "I1|" . $other->saveHTML($imported) . "|";
echo ($imported->ownerDocument === $other ? "O" : "x") . "|";
$same = $html->importNode($template, true);
echo ($same === $template ? "S" : "x") . "\n";

$adopted = $other->adoptNode($template);
echo "A1|" . $other->saveHTML($adopted) . "|";
echo ($adopted->ownerDocument === $other ? "O" : "x") . "|";
echo $html->saveHTML() . "\n";
"#,
    );
    assert_eq!(
        out,
        "S1|1|NULL|<root>1</root>\nS2|<>|<root>&lt;&gt;</root>\nS3|0|<root/>\nT0|<y>|<y>|<b>x</b><template><i>n</i></template>|<template><b>x</b><template><i>n</i></template></template>\nT1|<template></template>|<template></template>|<template></template>\nD1|Dom\\HTMLDocument|<template></template>|Dom\\HTMLDocument:0\nI1|<template></template>|O|S\nA1|<template></template>|O|<!DOCTYPE html><html><head></head><body></body></html>\n"
    );
}

/// Verifies legacy scalar attributes, stable attribute wrappers, and element traversal.
#[test]
fn legacy_element_attributes_and_navigation_match_php() {
    let out = compile_and_run(
        r#"<?php
$document = new DOMDocument();
$root = $document->createElement("root");
$a = $document->createElement("a");
$b = $document->createElement("b");
if ($root === false) { exit(2); }
if ($a === false) { exit(3); }
if ($b === false) { exit(4); }
$document->appendChild($root);
$root->appendChild($document->createComment("c"));
$root->appendChild($a);
$root->appendChild($document->createTextNode("t"));
$root->appendChild($b);
echo $root->getAttribute("missing") === "" ? "E" : "x";
echo $root->removeAttribute("missing") ? "x" : "0";
$attribute = $root->setAttribute("id", "main");
if (!$attribute instanceof DOMAttr) { exit(5); }
echo get_class($attribute) . "|";
echo $root->getAttribute("id") . "|" . ($root->hasAttribute("id") ? "H" : "x");
echo $root->getAttributeNode("id") === $attribute ? "I" : "x";
$root->setAttribute("class", "one two");
echo "|" . $root->id . "|" . $root->className . "|" . $root->tagName;
echo "|" . ($root->firstElementChild === $a ? "F" : "x");
echo $root->lastElementChild === $b ? "L" : "x";
echo $a->nextElementSibling === $b ? "N" : "x";
echo $b->previousElementSibling === $a ? "P" : "x";
echo "|" . $root->childElementCount;
echo "|" . ($root->removeAttribute("id") ? "R" : "x");
echo $attribute->ownerElement === null ? "D" : "x";
echo "|" . $attribute->value;
"#,
    );
    assert_eq!(
        out,
        "E0DOMAttr|main|HI|main|one two|root|FLNP|2|RD|main"
    );
}

/// Verifies modern nullable attributes, void mutations, and concrete attribute wrappers.
#[test]
fn modern_element_attributes_and_navigation_match_php() {
    let out = compile_and_run(
        r#"<?php
$document = Dom\XMLDocument::createEmpty();
$root = $document->createElement("root");
$a = $document->createElement("a");
$b = $document->createElement("b");
$document->appendChild($root);
$root->appendChild($document->createComment("c"));
$root->appendChild($a);
$root->appendChild($document->createTextNode("t"));
$root->appendChild($b);
echo $root->getAttribute("missing") === null ? "N" : "x";
echo $root->removeAttribute("missing") === null ? "V" : "x";
echo $root->setAttribute("id", "main") === null ? "S" : "x";
$attribute = $root->getAttributeNode("id");
if ($attribute === null) { exit(2); }
echo "|" . get_class($attribute);
echo "|" . $root->getAttribute("id") . "|" . ($root->hasAttribute("id") ? "H" : "x");
$root->setAttribute("class", "one two");
echo "|" . $root->id . "|" . $root->className . "|" . $root->tagName;
echo "|" . ($root->firstElementChild === $a ? "F" : "x");
echo $root->lastElementChild === $b ? "L" : "x";
echo $a->nextElementSibling === $b ? "N" : "x";
echo $b->previousElementSibling === $a ? "P" : "x";
echo "|" . $root->childElementCount;
$root->removeAttribute("id");
echo "|" . ($attribute->ownerElement === null ? "D" : "x");
echo "|" . $attribute->value;
"#,
    );
    assert_eq!(
        out,
        "NVS|Dom\\Attr|main|H|main|one two|root|FLNP|2|D|main"
    );
}

/// Verifies legacy strings and modern enum positions preserve PHP adjacent-mutation semantics.
#[test]
fn element_adjacent_mutations_match_php() {
    let out = compile_and_run(
        r#"<?php
$document = new DOMDocument();
$document->loadXML("<root><a/><b/></root>");
$root = $document->documentElement;
$a = $root->firstElementChild;
$other = new DOMDocument();
$x = $other->createElement("x");
if ($x === false) {
    exit(2);
}
echo $a->insertAdjacentElement("BeFoReBeGiN", $x) === $x ? "I" : "x";
echo $x->ownerDocument === $document ? "O" : "x";
$a->insertAdjacentText("afterbegin", "T&");
echo "|" . $document->saveXML($root);
$detached = $document->createElement("det");
$z = $document->createElement("z");
if ($detached === false) {
    exit(3);
}
if ($z === false) {
    exit(3);
}
echo "|" . ($detached->insertAdjacentElement(
    "beforebegin",
    $z,
) === null ? "N" : "x");
$q = $document->createElement("q");
if ($q === false) {
    exit(4);
}
try {
    $a->insertAdjacentElement("wat", $q);
} catch (DOMException $error) {
    echo "|" . $error->getCode() . ":" . $error->getMessage();
}
$document->strictErrorChecking = false;
echo "|" . (@$a->insertAdjacentElement(
    "wat",
    $q,
) === null ? "S" : "x");

$modern = Dom\XMLDocument::createFromString("<root><a/><b/></root>");
$modernRoot = $modern->documentElement;
$modernA = $modernRoot->firstElementChild;
$modernB = $modernRoot->lastElementChild;
$modernX = $modern->createElement("x");
echo "|" . ($modernA->insertAdjacentElement(
    Dom\AdjacentPosition::BeforeBegin,
    $modernX,
) === $modernX ? "I" : "x");
$modernA->insertAdjacentText(Dom\AdjacentPosition::AfterBegin, "T&");
$foreignDocument = Dom\XMLDocument::createEmpty();
$foreign = $foreignDocument->createElement("foreign");
echo $modernB->insertAdjacentElement(
    Dom\AdjacentPosition::BeforeBegin,
    $foreign,
) === $foreign ? "F" : "x";
echo $foreign->ownerDocument === $modern ? "O" : "x";
echo "|" . $modern->saveXml($modernRoot);
$modernDetached = $modern->createElement("det");
echo "|" . ($modernDetached->insertAdjacentElement(
    Dom\AdjacentPosition::AfterEnd,
    $modern->createElement("z"),
) === null ? "N" : "x");
"#,
    );
    assert_eq!(
        out,
        "IO|<root><x/><a>T&amp;</a><b/></root>|N|12:Syntax Error|S|IFO|<root><x/><a>T&amp;</a><foreign/><b/></root>|N"
    );
}

/// Verifies family-specific document constraints and legacy loose-mode warnings.
#[test]
fn element_adjacent_document_constraints_match_php() {
    let out = compile_and_run_capture(
        r#"<?php
$legacy = new DOMDocument();
$legacy->loadXML("<root/>");
$legacy->documentElement->insertAdjacentText("beforebegin", "text");
echo $legacy->saveXML();
$legacy->strictErrorChecking = false;
$loose = $legacy->createElement("loose");
if ($loose === false) {
    exit(2);
}
echo $legacy->documentElement->insertAdjacentElement("wat", $loose) === null
    ? "|N"
    : "|x";

$modern = Dom\XMLDocument::createFromString("<root/>");
try {
    $modern->documentElement->insertAdjacentText(
        Dom\AdjacentPosition::BeforeBegin,
        "text",
    );
} catch (DOMException $error) {
    echo "|" . $error->getCode() . ":" . $error->getMessage();
}
$second = $modern->createElement("second");
try {
    $modern->documentElement->insertAdjacentElement(
        Dom\AdjacentPosition::AfterEnd,
        $second,
    );
} catch (DOMException $error) {
    echo "|" . $error->getCode() . ":" . $error->getMessage();
}
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "<?xml version=\"1.0\"?>\ntext\n<root/>\n|N|3:Cannot insert text as a child of a document|3:Cannot have more than one element child in a document"
    );
    assert_eq!(
        out.stderr,
        "Warning: DOMElement::insertAdjacentElement(): Syntax Error\n"
    );
}

/// Verifies legacy prefix writes preserve php-src namespace rebinding and loose errors.
#[test]
fn legacy_node_prefix_writes_match_php() {
    let out = compile_and_run_capture(
        r#"<?php
$document = new DOMDocument();
$document->loadXML(
    '<p:r xmlns:p="urn:x" xmlns:a="urn:y" p:z="v"><p:c/></p:r>',
);
$root = $document->documentElement;
if ($root === null) {
    exit(2);
}
$root->prefix = "q";
echo $root->nodeName . "|" . $root->prefix . "|" . $root->namespaceURI;
echo "|" . $root->lookupNamespaceURI("q") . "\n";
$root->prefix = "";
echo $root->nodeName . "|" . $root->prefix;
echo "|" . $root->lookupNamespaceURI(null) . "\n";
try {
    $root->prefix = "xml";
} catch (DOMException $exception) {
    echo $exception->getCode() . ":" . $exception->getMessage() . "\n";
}
$document->strictErrorChecking = false;
$root->prefix = "xml";
echo "L:" . var_export($root->prefix, true) . "\n";
try {
    $root->prefix = "a";
} catch (DOMException $exception) {
    echo "C:" . $exception->getCode() . ":" . $exception->getMessage() . "\n";
}
$root->prefix = "z\0tail";
echo "N:" . $root->nodeName . "|" . $root->prefix;
echo "|" . $root->lookupNamespaceURI("z") . "\n";
$attribute = $root->getAttributeNodeNS("urn:x", "z");
if (!$attribute instanceof DOMAttr) {
    exit(3);
}
$attribute->prefix = "q";
echo "A:" . $attribute->nodeName . "|" . $attribute->prefix;
echo "|" . $attribute->namespaceURI . "\n";
$attribute->prefix = "";
echo "E:" . $attribute->nodeName;
echo "|" . var_export($attribute->prefix, true);
echo "|" . $attribute->namespaceURI . "\n";
$attribute->prefix = "xmlns";
$plain = $document->createElement("plain");
if ($plain === false) {
    exit(4);
}
$plain->prefix = "u";
echo "P:" . var_export($plain->prefix, true) . "\n";
$text = $document->createTextNode("t");
$text->prefix = "u";
echo "T:" . var_export($text->prefix, true) . "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "q:r|q|urn:x|urn:x\nr||urn:x\n14:Namespace Error\nL:''\nC:14:Namespace Error\nN:z:r|z|urn:x\nA:q:z|q|urn:x\nE:z|''|urn:x\nP:''\nT:''\n"
    );
    assert_eq!(
        out.stderr,
        "Warning: Unknown: Namespace Error\nWarning: Unknown: Namespace Error\n"
    );
}

/// Verifies modern element and attribute renaming matches PHP namespace and class rules.
#[test]
fn modern_node_renaming_matches_php() {
    let out = compile_and_run(
        r#"<?php
$document = Dom\XMLDocument::createFromString(
    '<root xmlns:a="urn:a"><a:child attrib="value"/></root>',
);
$root = $document->documentElement;
$root->rename("urn:x", "x:foo");
echo "E1|" . $root->nodeName . "|" . $root->namespaceURI;
echo "|" . $root->prefix . "|" . $document->saveXml($root) . "\n";
$root->rename("urn:x", "a:foo");
echo "E2|" . $root->nodeName . "|" . $root->namespaceURI;
echo "|" . $root->prefix . "|" . $document->saveXml($root) . "\n";
$root->rename("", "foo");
echo "E3|" . $root->nodeName;
echo "|" . ($root->namespaceURI === null ? "NULL" : "x");
echo "|" . ($root->prefix === null ? "NULL" : "x");
echo "|" . $document->saveXml($root) . "\n";
$root->rename(null, "bar");

$child = $root->firstElementChild;
if ($child === null) {
    exit(2);
}
$attribute = $child->getAttributeNode("attrib");
if ($attribute === null) {
    exit(2);
}
$attribute->rename("urn:x", "x:foo");
echo "A1|" . $attribute->nodeName . "|" . $attribute->namespaceURI;
echo "|" . $attribute->prefix . "|" . $document->saveXml($root) . "\n";
$attribute->rename("urn:x", "foo");
echo "A2|" . $attribute->nodeName . "|" . $attribute->namespaceURI;
echo "|" . ($attribute->prefix === null ? "NULL" : "x");
echo "|" . $document->saveXml($root) . "\n";
$root->setAttribute("a", "b");
$root->setAttribute("c", "d");
$conflicting = $root->getAttributeNode("a");
if ($conflicting === null) {
    exit(2);
}
try {
    $conflicting->rename(null, "c");
} catch (DOMException $error) {
    echo "C|" . $error->getCode() . "|" . $error->getMessage() . "\n";
}
try {
    $root->rename("", "a:b");
} catch (DOMException $error) {
    echo "V1|" . $error->getCode() . "|" . $error->getMessage() . "\n";
}
try {
    $root->rename("urn:a", "a:b:c");
} catch (DOMException $error) {
    echo "V2|" . $error->getCode() . "|" . $error->getMessage() . "\n";
}

$xhtml = "http://www.w3.org/1999/xhtml";
$htmlElement = $document->createElementNS($xhtml, "foo:bar");
$htmlElement->rename($xhtml, "foo:baz");
echo "H1|" . $htmlElement->nodeName . "|" . $htmlElement->namespaceURI;
echo "|" . $htmlElement->prefix . "\n";
try {
    $htmlElement->rename("urn:a", "foo:baz");
} catch (DOMException $error) {
    echo "H2|" . $error->getCode() . "|" . $error->getMessage() . "\n";
}
$xmlElement = $document->createElementNS("urn:a", "foo:bar");
try {
    $xmlElement->rename($xhtml, "foo:baz");
} catch (DOMException $error) {
    echo "H3|" . $error->getCode() . "|" . $error->getMessage() . "\n";
}

$html = Dom\HTMLDocument::createFromString(
    "<!doctype html><html><body><template>a<div>foo</div>b</template></body></html>",
);
$body = $html->body;
if ($body === null) {
    exit(3);
}
$template = $body->firstElementChild;
if ($template === null) {
    exit(4);
}
try {
    $template->rename("http://www.w3.org/1999/xhtml", "screwthis");
} catch (DOMException $error) {
    echo "T|" . $error->getCode() . "|" . $error->getMessage();
    echo "|" . $template->nodeName . "|" . $html->saveHtml($template) . "\n";
}
"#,
    );
    assert_eq!(
        out,
        "E1|x:foo|urn:x|x|<x:foo xmlns:x=\"urn:x\" xmlns:a=\"urn:a\"><a:child attrib=\"value\"/></x:foo>\nE2|a:foo|urn:x|a|<ns1:foo xmlns:ns1=\"urn:x\" xmlns:a=\"urn:a\"><a:child attrib=\"value\"/></ns1:foo>\nE3|foo|NULL|NULL|<foo xmlns:a=\"urn:a\"><a:child attrib=\"value\"/></foo>\nA1|x:foo|urn:x|x|<bar xmlns:a=\"urn:a\"><a:child xmlns:x=\"urn:x\" x:foo=\"value\"/></bar>\nA2|foo|urn:x|NULL|<bar xmlns:a=\"urn:a\"><a:child xmlns:ns1=\"urn:x\" ns1:foo=\"value\"/></bar>\nC|13|An attribute with the given name in the given namespace already exists\nV1|14|Namespace Error\nV2|5|Invalid Character Error\nH1|foo:baz|http://www.w3.org/1999/xhtml|foo\nH2|13|It is not possible to move an element out of the HTML namespace because the HTML namespace is tied to the HTMLElement class\nH3|13|It is not possible to move an element into the HTML namespace because the HTML namespace is tied to the HTMLElement class\nT|13|It is not possible to rename the template element because it hosts a document fragment|TEMPLATE|<template>a<div>foo</div>b</template>\n"
    );
}

/// Verifies modern XML and HTML element markup parsing and replacement match PHP.
#[test]
fn modern_element_markup_mutations_match_php() {
    let out = compile_and_run(
        r#"<?php
$xml = Dom\XMLDocument::createFromString(
    '<root xmlns:x="urn:x"><div>old</div></root>',
);
$root = $xml->documentElement;
$div = $root->firstElementChild;
echo "X0|" . $div->innerHTML . "|" . $div->outerHTML . "\n";
$div->innerHTML = '<x:item/>text&amp;';
echo "X1|" . $xml->saveXML($root) . "\n";
$before = $xml->saveXML($root);
try {
    $div->innerHTML = '<broken>';
} catch (DOMException $error) {
    echo "XE|" . $error->getCode() . "|" . $error->getMessage();
    echo "|" . ($before === $xml->saveXML($root) ? "same" : "changed") . "\n";
}
$item = $div->firstElementChild;
$item->outerHTML = '<a/><b>z</b>';
echo "X2|" . $xml->saveXML($root) . "|" . $item->tagName . "\n";
$div->insertAdjacentHTML(Dom\AdjacentPosition::BeforeBegin, '<before/>');
$div->insertAdjacentHTML(Dom\AdjacentPosition::AfterBegin, '<start/>');
$div->insertAdjacentHTML(Dom\AdjacentPosition::BeforeEnd, '<end/>');
$div->insertAdjacentHTML(Dom\AdjacentPosition::AfterEnd, '<after/>');
echo "X3|" . $xml->saveXML($root) . "\n";
try {
    $root->outerHTML = '<replacement/>';
} catch (DOMException $error) {
    echo "XO|" . $error->getCode() . "|" . $error->getMessage() . "\n";
}
$detached = $xml->createElement('detached');
try {
    $detached->insertAdjacentHTML(
        Dom\AdjacentPosition::BeforeBegin,
        'text',
    );
} catch (DOMException $error) {
    echo "XD|" . $error->getCode() . "|" . $error->getMessage() . "\n";
}

$html = Dom\HTMLDocument::createFromString(
    '<div><b>x</b></div>',
    LIBXML_NOERROR,
);
$body = $html->body;
$htmlDiv = $body->firstElementChild;
echo "H0|" . $htmlDiv->innerHTML . "|" . $htmlDiv->outerHTML . "\n";
$htmlDiv->innerHTML =
    '<p>foo</p><template><i>inside</i></template>';
$template = $htmlDiv->lastElementChild;
echo "H1|" . $htmlDiv->innerHTML . "|" . $template->innerHTML;
echo "|" . $template->outerHTML . "\n";
$template->insertAdjacentHTML(
    Dom\AdjacentPosition::AfterBegin,
    '<strong>s</strong>',
);
echo "HT|" . $template->innerHTML . "\n";
$htmlDiv->insertAdjacentHTML(
    Dom\AdjacentPosition::BeforeBegin,
    'text<span>a</span>',
);
echo "H2|" . $html->saveHTML($body) . "\n";
$htmlDiv->outerHTML = '<section>q</section>';
echo "H3|" . $html->saveHTML($body) . "|" . $htmlDiv->tagName . "\n";
$style = $html->createElement('style');
$body->appendChild($style);
$style->innerHTML = '<p>raw</p>';
echo "H4|" . $style->innerHTML . "|" . $html->saveHTML($style);
echo "|" . $html->saveXML($style) . "\n";
"#,
    );
    assert_eq!(
        out,
        "X0|old|<div>old</div>\nX1|<root xmlns:x=\"urn:x\"><div><x:item/>text&amp;</div></root>\nXE|12|XML fragment is not well-formed|same\nX2|<root xmlns:x=\"urn:x\"><div><a/><b>z</b>text&amp;</div></root>|x:item\nX3|<root xmlns:x=\"urn:x\"><before/><div><start/><a/><b>z</b>text&amp;<end/></div><after/></root>\nXO|13|Invalid Modification Error\nXD|7|No Modification Allowed Error\nH0|<b>x</b>|<div><b>x</b></div>\nH1|<p>foo</p><template><i>inside</i></template>|<i>inside</i>|<template><i>inside</i></template>\nHT|<i>inside</i>\nH2|<body>text<span>a</span><div><p>foo</p><template><i>inside</i></template></div></body>\nH3|<body>text<span>a</span><section>q</section></body>|DIV\nH4|<p>raw</p>|<style><p>raw</p></style>|<style xmlns=\"http://www.w3.org/1999/xhtml\">&lt;p&gt;raw&lt;/p&gt;</style>\n"
    );
}

/// Verifies markup well-formedness, namespace edge cases, and HTML UTF-8 repair match PHP.
#[test]
fn modern_element_markup_edge_cases_match_php() {
    let out = compile_and_run(
        r#"<?php
$document = Dom\XMLDocument::createEmpty();
$container = $document->createElement("container");
$container->append("Hello, \x01 world!");
try {
    echo $container->innerHTML;
} catch (DOMException $error) {
    echo "WF1|" . $error->getCode() . "|" . $error->getMessage() . "\n";
}
try {
    echo $container->outerHTML;
} catch (DOMException $error) {
    echo "WF2|" . $error->getCode() . "|" . $error->getMessage() . "\n";
}
$container = $document->createElement("container");
$container->append($document->createComment("Hello -- world"));
try {
    echo $container->innerHTML;
} catch (DOMException $error) {
    echo "WF3|" . $error->getCode() . "|" . $error->getMessage() . "\n";
}

$document = Dom\XMLDocument::createFromString(
    '<root><x xmlns=""/></root>',
);
echo "NS1|" . $document->documentElement->innerHTML . "\n";
$x = $document->getElementsByTagName("x")->item(0);
$x->setAttributeNS(
    "http://www.w3.org/2000/xmlns/",
    "xmlns:a",
    "",
);
try {
    echo $document->documentElement->innerHTML;
} catch (DOMException $error) {
    echo "NS2|" . $error->getCode() . "|" . $error->getMessage() . "\n";
}

$document = Dom\XMLDocument::createFromString('<root/>');
$child = $document->createElementNS('urn:a', 'child');
$document->documentElement->appendChild($child);
$child->setAttributeNS(
    "http://www.w3.org/2000/xmlns/",
    "xmlns",
    "urn:b",
);
$child->innerHTML = '<default/>';
echo "NS3|" . $document->saveXML($document->documentElement);
echo "|" . $child->namespaceURI;
echo "|" . $child->firstElementChild->namespaceURI . "\n";

$html = Dom\HTMLDocument::createEmpty();
$element = $html->createElement("div");
$html->appendChild($element);
$element->innerHTML = "invalid\xffutf-8𐍈";
echo "U|" . $element->innerHTML . "\n";
$element->innerHTML =
    '<svg xml:space="default" xlink:href="about:blank" xmlns:foo="barspace"></svg>';
$svg = $element->firstElementChild;
echo "SVG|" . $element->innerHTML;
echo "|" . $svg->attributes->item(0)->localName;
echo "|" . $svg->attributes->item(0)->namespaceURI . "\n";
"#,
    );
    assert_eq!(
        out,
        "WF1|12|The resulting XML serialization is not well-formed\nWF2|12|The resulting XML serialization is not well-formed\nWF3|12|The resulting XML serialization is not well-formed\nNS1|<x xmlns=\"\"/>\nNS2|12|The resulting XML serialization is not well-formed\nNS3|<root><child xmlns=\"urn:a\"><default/></child></root>|urn:a|urn:a\nU|invalid�utf-8𐍈\nSVG|<svg xml:space=\"default\" xlink:href=\"about:blank\" xmlns:foo=\"barspace\"></svg>|space|http://www.w3.org/XML/1998/namespace\n"
    );
}

/// Verifies legacy namespace-aware factories, attribute lookup, removal, and toggling.
#[test]
fn legacy_namespaced_attributes_and_factories_match_php() {
    let out = compile_and_run(
        r#"<?php
$document = new DOMDocument();
$root = $document->createElementNS("urn:r", "r:root", "v");
if ($root === false) { exit(2); }
$document->appendChild($root);
$plain = $document->createAttribute("empty");
$namespaced = $document->createAttributeNS("urn:z", "z:q");
if (!$plain instanceof DOMAttr) { exit(3); }
if (!$namespaced instanceof DOMAttr) { exit(4); }
echo get_class($plain) . ":" . $plain->value;
echo "|" . get_class($namespaced) . ":" . $namespaced->nodeName;
echo "|" . ($root->setAttributeNS("urn:a", "a:x", "1") === null ? "S" : "x");
$root->setAttributeNS(null, "plain", "2");
$names = $root->getAttributeNames();
echo "|" . count($names) . ":" . $names[0] . ":" . $names[1] . ":" . $names[2] . ":" . $names[3];
echo "|" . $root->getAttributeNS("urn:a", "x");
echo "|" . ($root->getAttributeNS("urn:none", "x") === "" ? "E" : "x");
$attribute = $root->getAttributeNodeNS("urn:a", "x");
if ($attribute === null) { exit(5); }
if (!$attribute instanceof DOMAttr) { exit(6); }
echo "|" . get_class($attribute) . ":" . $attribute->name . ":" . $attribute->nodeName;
$root->removeAttributeNS("urn:a", "x");
echo "|" . ($attribute->ownerElement === null ? "D" : "x");
echo "|" . ($root->toggleAttribute("on") ? "T" : "x");
echo $root->hasAttribute("on") ? "1" : "x";
echo $root->toggleAttribute("on") ? "x" : "F";
echo $root->hasAttribute("on") ? "x" : "0";
echo $root->toggleAttribute("off", false) ? "x" : "F";
echo $root->hasAttribute("off") ? "x" : "0";
"#,
    );
    assert_eq!(
        out,
        "DOMAttr:|DOMAttr:z:q|S|4:xmlns:r:xmlns:a:a:x:plain|1|E|DOMAttr:x:a:x|D|T1F0F0"
    );
}

/// Verifies modern namespace-aware factories, nullable lookup, and attribute-name order.
#[test]
fn modern_namespaced_attributes_and_factories_match_php() {
    let out = compile_and_run(
        r#"<?php
$document = Dom\XMLDocument::createEmpty();
$root = $document->createElementNS("urn:r", "r:root");
$document->appendChild($root);
$plain = $document->createAttribute("empty");
$namespaced = $document->createAttributeNS("urn:z", "z:q");
echo get_class($plain) . ":" . $plain->value;
echo "|" . get_class($namespaced) . ":" . $namespaced->nodeName;
echo "|" . ($root->setAttributeNS("urn:a", "a:x", "1") === null ? "S" : "x");
$root->setAttributeNS(null, "plain", "2");
$names = $root->getAttributeNames();
echo "|" . count($names) . ":" . $names[0] . ":" . $names[1];
echo "|" . $root->getAttributeNS("urn:a", "x");
echo "|" . ($root->getAttributeNS("urn:none", "x") === null ? "N" : "x");
$attribute = $root->getAttributeNodeNS("urn:a", "x");
if ($attribute === null) { exit(2); }
echo "|" . get_class($attribute) . ":" . $attribute->name . ":" . $attribute->nodeName;
$root->removeAttributeNS("urn:a", "x");
echo "|" . ($attribute->ownerElement === null ? "D" : "x");
echo "|" . ($root->toggleAttribute("on") ? "T" : "x");
echo $root->hasAttribute("on") ? "1" : "x";
echo $root->toggleAttribute("on") ? "x" : "F";
echo $root->hasAttribute("on") ? "x" : "0";
"#,
    );
    assert_eq!(
        out,
        "Dom\\Attr:|Dom\\Attr:z:q|S|2:a:x:plain|1|N|Dom\\Attr:a:x:a:x|D|T1F0"
    );
}

/// Verifies legacy attribute-node replacement, identity, and structured failures.
#[test]
fn legacy_attribute_node_mutations_match_php() {
    let out = compile_and_run(
        r#"<?php
$document = new DOMDocument();
$root = $document->createElement("root");
if ($root === false) { exit(2); }
$document->appendChild($root);
$first = $document->createAttribute("x");
$second = $document->createAttribute("x");
if ($first instanceof DOMAttr) {
    if ($second instanceof DOMAttr) {
        $first->value = "1";
        $second->value = "2";
        echo $root->setAttributeNode($first) === null ? "N" : "x";
        echo $root->setAttributeNode($first) === null ? "S" : "x";
        $old = $root->setAttributeNode($second);
        if (!$old instanceof DOMAttr) { exit(4); }
        echo "|" . ($old === $first ? "I" : "x") . ":" . $old->value;
        echo ":" . ($old->ownerElement === null ? "D" : "x");
        $removed = $root->removeAttributeNode($second);
        if (!$removed instanceof DOMAttr) { exit(5); }
        echo "|" . ($removed === $second ? "R" : "x");
        echo ":" . ($removed->ownerElement === null ? "D" : "x");
        try {
            $root->removeAttributeNode($second);
        } catch (DOMException $exception) {
            echo "|" . $exception->getCode() . ":" . $exception->getMessage();
        }
    } else {
        exit(3);
    }
} else {
    exit(3);
}
$other = new DOMDocument();
$foreign = $other->createAttribute("foreign");
if ($foreign instanceof DOMAttr) {
    try {
        $root->setAttributeNode($foreign);
    } catch (DOMException $exception) {
        echo "|" . $exception->getCode() . ":" . $exception->getMessage();
    }
} else {
    exit(6);
}
"#,
    );
    assert_eq!(
        out,
        "NS|I:1:D|R:D|8:Not Found Error|4:Wrong Document Error"
    );
}

/// Verifies modern attribute adoption and in-use validation preserve wrapper identity.
#[test]
fn modern_attribute_node_mutations_match_php() {
    let out = compile_and_run(
        r#"<?php
$document = Dom\XMLDocument::createEmpty();
$root = $document->createElement("root");
$otherElement = $document->createElement("other");
$document->appendChild($root);
$first = $document->createAttribute("x");
$second = $document->createAttribute("x");
$first->value = "1";
$second->value = "2";
echo $root->setAttributeNode($first) === null ? "N" : "x";
echo $root->setAttributeNode($first) === null ? "S" : "x";
$old = $root->setAttributeNode($second);
if ($old === null) { exit(2); }
echo "|" . ($old === $first ? "I" : "x") . ":" . $old->value;
echo ":" . ($old->ownerElement === null ? "D" : "x");
echo "|" . ($root->removeAttributeNode($second) === $second ? "R" : "x");
try {
    $root->removeAttributeNode($second);
} catch (DOMException $exception) {
    echo "|" . $exception->getCode() . ":" . $exception->getMessage();
}
$foreignDocument = Dom\XMLDocument::createEmpty();
$foreign = $foreignDocument->createAttributeNS("urn:f", "f:y");
$foreign->value = "z";
echo "|" . ($root->setAttributeNodeNS($foreign) === null ? "A" : "x");
echo ":" . ($foreign->ownerDocument === $document ? "O" : "x");
echo ":" . $root->getAttributeNS("urn:f", "y");
try {
    $otherElement->setAttributeNode($foreign);
} catch (DOMException $exception) {
    echo "|" . $exception->getCode() . ":" . $exception->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "NS|I:1:D|R|8:Not Found Error|A:O:z|10:Inuse Attribute Error"
    );
}

/// Verifies XML ID marking, lookup updates, and missing-attribute error policy.
#[test]
fn element_id_attribute_marking_matches_php() {
    let legacy = compile_and_run(
        r#"<?php
$document = new DOMDocument();
$document->loadXML("<r xmlns:n=\"u\" id=\"a\" n:x=\"b\" y=\"c\"/>");
$root = $document->documentElement;
if ($root === null) { exit(2); }
$plain = $root->getAttributeNode("id");
$namespaced = $root->getAttributeNodeNS("u", "x");
$byNode = $root->getAttributeNode("y");
if (!$plain instanceof DOMAttr) { exit(3); }
if (!$namespaced instanceof DOMAttr) { exit(4); }
if (!$byNode instanceof DOMAttr) { exit(5); }
echo $plain->isId() ? "x" : "0";
echo $document->getElementById("a") === null ? "N" : "x";
$root->setIdAttribute("id", true);
echo $plain->isId() ? "I" : "x";
echo $document->getElementById("a") === $root ? "F" : "x";
$root->setIdAttribute("id", false);
echo $plain->isId() ? "x" : "O";
echo $document->getElementById("a") === null ? "N" : "x";
$root->setIdAttributeNS("u", "x", true);
echo $namespaced->isId() ? "S" : "x";
echo $document->getElementById("b") === $root ? "B" : "x";
$root->setIdAttributeNode($byNode, true);
echo $byNode->isId() ? "T" : "x";
echo $document->getElementById("c") === $root ? "C" : "x";
try {
    $root->setIdAttribute("missing", true);
} catch (DOMException $error) {
    echo "|" . $error->getCode() . ":" . $error->getMessage();
}
$document->strictErrorChecking = false;
$root->setIdAttribute("missing", true);
echo "|L";
"#,
    );
    assert_eq!(legacy, "0NIFONSBTC|8:Not Found Error|L");

    let modern = compile_and_run(
        r#"<?php
$document = Dom\XMLDocument::createFromString("<r xmlns:n=\"u\" id=\"a\" n:x=\"b\" y=\"c\"/>");
$root = $document->documentElement;
if ($root === null) { exit(2); }
$plain = $root->getAttributeNode("id");
$namespaced = $root->getAttributeNodeNS("u", "x");
$byNode = $root->getAttributeNode("y");
if ($plain === null) { exit(3); }
if ($namespaced === null) { exit(4); }
if ($byNode === null) { exit(5); }
echo $plain->isId() ? "x" : "0";
echo $document->getElementById("a") === null ? "N" : "x";
$root->setIdAttribute("id", true);
echo $plain->isId() ? "I" : "x";
echo $document->getElementById("a") === $root ? "F" : "x";
$root->setIdAttribute("id", false);
echo $plain->isId() ? "x" : "O";
echo $document->getElementById("a") === null ? "N" : "x";
$root->setIdAttributeNS("u", "x", true);
echo $namespaced->isId() ? "S" : "x";
echo $document->getElementById("b") === $root ? "B" : "x";
$root->setIdAttributeNode($byNode, true);
echo $byNode->isId() ? "T" : "x";
echo $document->getElementById("c") === $root ? "C" : "x";
try {
    $root->setIdAttribute("missing", true);
} catch (DOMException $error) {
    echo "|" . $error->getCode() . ":" . $error->getMessage();
}
"#,
    );
    assert_eq!(modern, "0NIFONSBTC|8:Not Found Error");
}

/// Verifies legacy and modern QName validation preserve PHP's exception codes/messages.
#[test]
fn namespace_qname_failures_match_php() {
    let out = compile_and_run(
        r#"<?php
$legacy = new DOMDocument();
try {
    $legacy->createElementNS(null, "a:b");
} catch (DOMException $exception) {
    echo $exception->getCode() . ":" . $exception->getMessage();
}
try {
    $legacy->createElementNS("urn:x", "a b");
} catch (DOMException $exception) {
    echo "|" . $exception->getCode() . ":" . $exception->getMessage();
}
$modern = Dom\XMLDocument::createEmpty();
try {
    $modern->createElementNS("urn:x", "a b");
} catch (DOMException $exception) {
    echo "|" . $exception->getCode() . ":" . $exception->getMessage();
}
try {
    $modern->createAttributeNS("urn:x", "xml:a");
} catch (DOMException $exception) {
    echo "|" . $exception->getCode() . ":" . $exception->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "14:Namespace Error|14:Namespace Error|5:Invalid Character Error|14:Namespace Error"
    );
}

/// Verifies legacy node lists and named-node maps are fresh wrappers over live queries.
#[test]
fn legacy_live_dom_collections_match_php() {
    let output = compile_and_run_capture(
        r#"<?php
$document = new DOMDocument();
$document->loadXML("<root><a id=\"1\"/><b/><a id=\"2\"/></root>");
$root = $document->documentElement;
if ($root === null) { exit(2); }
$children = $root->childNodes;
echo get_class($children) . ":" . $children->length . ":" . $children->count();
echo ":" . get_class($children->item(0));
echo ":" . ($children->item(9) === null ? "N" : "x");
echo ":" . ($root->childNodes === $root->childNodes ? "x" : "F");
$matches = $document->getElementsByTagName("a");
echo "|" . get_class($matches) . ":" . $matches->length;
$added = $document->createElement("a");
if ($added === false) { exit(3); }
$root->appendChild($added);
echo ":" . $matches->length . ":" . $children->length;
$root->setAttribute("plain", "v");
$root->setAttributeNS("urn:n", "n:q", "w");
$attributes = $root->attributes;
if ($attributes === null) { exit(4); }
echo "|" . get_class($attributes) . ":" . $attributes->length . ":" . $attributes->count();
$plain = $attributes->getNamedItem("plain");
$namespaced = $attributes->getNamedItemNS("urn:n", "q");
echo ":" . ($plain === $root->getAttributeNode("plain") ? "I" : "x");
echo ":" . ($namespaced === $root->getAttributeNodeNS("urn:n", "q") ? "J" : "x");
echo ":" . ($attributes->item(9) === null ? "N" : "x");
echo ":" . ($root->attributes === $root->attributes ? "x" : "F");
$root->removeAttribute("plain");
echo ":" . $attributes->length;
"#,
    );
    assert!(
        output.success,
        "stdout: {}\nstderr: {}",
        output.stdout,
        output.stderr
    );
    assert_eq!(
        output.stdout,
        "DOMNodeList:3:3:DOMElement:N:F|DOMNodeList:2:3:4|DOMNamedNodeMap:2:2:I:J:N:F:1"
    );
}

/// Verifies modern live collection lookup and concrete member materialization.
#[test]
fn modern_live_dom_collections_match_php() {
    let output = compile_and_run_capture(
        r#"<?php
$document = Dom\XMLDocument::createFromString("<root xmlns:h=\"http://www.w3.org/1999/xhtml\"><a id=\"1\"/><b/><a id=\"2\"/><h:a name=\"html-name\"/></root>");
$root = $document->documentElement;
if ($root === null) { exit(2); }
$children = $root->childNodes;
echo get_class($children) . ":" . $children->length . ":" . $children->count();
echo ":" . get_class($children->item(0));
echo ":" . ($children->item(9) === null ? "N" : "x");
echo ":" . ($root->childNodes === $root->childNodes ? "x" : "F");
$matches = $document->getElementsByTagName("a");
echo "|" . get_class($matches) . ":" . $matches->length;
$added = $document->createElement("a");
$root->appendChild($added);
echo ":" . $matches->length . ":" . $children->length;
$added->setAttribute("id", "live");
echo ":" . ($matches->namedItem("live") === $added ? "M" : "x");
echo ":" . ($matches->namedItem("") === null ? "N" : "x");
$added->removeAttribute("id");
$added->setAttribute("name", "live");
echo ":" . ($matches->namedItem("live") === null ? "N" : "x");
$all = $document->getElementsByTagName("*");
$htmlNamed = $all->namedItem("html-name");
echo ":" . ($htmlNamed !== null && $htmlNamed->nodeName === "h:a" ? "H" : "x");
$root->setAttribute("plain", "v");
$root->setAttributeNS("urn:n", "n:q", "w");
$attributes = $root->attributes;
echo "|" . get_class($attributes) . ":" . $attributes->length . ":" . $attributes->count();
$plain = $attributes->getNamedItem("plain");
$namespaced = $attributes->getNamedItemNS("urn:n", "q");
echo ":" . ($plain === $root->getAttributeNode("plain") ? "I" : "x");
echo ":" . ($namespaced === $root->getAttributeNodeNS("urn:n", "q") ? "J" : "x");
echo ":" . ($attributes->item(9) === null ? "N" : "x");
echo ":" . ($root->attributes === $root->attributes ? "x" : "F");
$root->removeAttribute("plain");
echo ":" . $attributes->length;
"#,
    );
    assert!(
        output.success,
        "stdout: {}\nstderr: {}",
        output.stdout,
        output.stderr
    );
    assert_eq!(
        output.stdout,
        "Dom\\NodeList:4:4:Dom\\Element:N:F|Dom\\HTMLCollection:2:3:5:M:N:N:H|Dom\\NamedNodeMap:3:3:I:J:N:F:2"
    );
}

/// Verifies PHP 8.5 dimension reads on legacy and modern DOM collections.
#[test]
fn dom_collection_dimension_reads_match_php_and_are_heap_clean() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
$legacy = new DOMDocument();
$legacy->loadXML('<root id="r"><child id="named"/></root>');
$legacyList = $legacy->getElementsByTagName('*');
$legacyMap = $legacy->documentElement->attributes;
echo get_class($legacyList[0]), ':', $legacyList[0]->nodeName, ':';
echo $legacyMap[0]->nodeName, ':', $legacyMap['id']->nodeValue, '|';

$modern = Dom\XMLDocument::createFromString('<root id="r"><child id="named"/></root>');
$modernList = $modern->childNodes;
$modernHtml = $modern->getElementsByTagName('*');
$modernMap = $modern->documentElement->attributes;
$named = 'named';
$attributeName = 'id';
echo get_class($modernList[0]), ':', $modernList[0]->nodeName, ':';
echo $modernHtml[1]->nodeName, ':', $modernHtml[$named]->nodeName, ':';
echo $modernMap[0]->name, ':', $modernMap[$attributeName]->value, '|';
echo $legacy->getElementsByTagName('*')[1]->nodeName, ':';
echo $modern->getElementsByTagName('*')[1]->nodeName, ':';
echo $modern->querySelectorAll('*')[0]->nodeName, '|';
echo $legacyList['0']->nodeName, ':', $legacyList[99] === null ? 'N' : 'V', '|';
echo $modernList['0']->nodeName, ':', $modernList[99] === null ? 'N' : 'V', '|';
echo $modernHtml['1']->nodeName, ':', $modernHtml[99] === null ? 'N' : 'V', '|';
echo $legacyMap['0']->nodeName, ':', $legacyMap['missing'] === null ? 'N' : 'V', '|';
echo $modernMap['0']->name, ':', $modernMap['missing'] === null ? 'N' : 'V';
"#,
    );
    assert!(
        output.success,
        "program failed: stdout={} stderr={}",
        output.stdout,
        output.stderr
    );
    assert_eq!(
        output.stdout,
        concat!(
            "DOMElement:root:id:r|Dom\\Element:root:child:child:id:r|child:child:root|",
            "root:N|root:N|child:N|id:N|id:N",
        )
    );
    assert!(
        output.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        output.stderr
    );
}

/// Verifies modern CSS selectors, snapshots, quirks, and exact failures match PHP.
#[test]
fn modern_css_selector_operations_match_php() {
    let output = compile_and_run_capture(
        r#"<?php
$xml = Dom\XMLDocument::createFromString(
    '<root><section id="s" class="box hot"><p class="hot">one</p><p class="cold"><b id="leaf">two</b></p></section><p class="hot">three</p></root>'
);
$root = $xml->documentElement;
$section = $xml->querySelector('section.box > p.hot');
$all = $xml->querySelectorAll('p.hot, #leaf');
echo get_class($section) . ':' . $section->textContent;
echo '|' . get_class($all) . ':' . $all->length . ':' . $all->count();
echo ':' . $all->item(0)->textContent . ':' . $all->item(2)->nodeName;
echo ':' . ($all->item(0) === $section ? 'I' : 'X');
$root->append($xml->createElement('p'));
echo ':' . $all->length;
echo '|' . ($section->matches('section#s.box') ? 'T' : 'F');
$leaf = $xml->querySelector('#leaf');
echo ':' . $leaf->closest('section')->getAttribute('id');
echo ':' . ($root->querySelector('root') === null ? 'N' : 'X');
echo ':' . ($xml->querySelector('missing') === null ? 'N' : 'X');
try {
    $xml->querySelector('@invalid');
} catch (DOMException $exception) {
    echo '|D' . $exception->getCode() . ':' . $exception->getMessage();
}
try {
    $section->matches(':blank');
} catch (DOMException $exception) {
    echo '|B' . $exception->getCode() . ':' . $exception->getMessage();
}

$standards = Dom\HTMLDocument::createFromString(
    '<!doctype html><div id="Case" class="Token"></div><table><colgroup><col class="selected"></colgroup><tbody><tr><td>x</td></tr></tbody></table>',
    LIBXML_NOERROR
);
$quirks = Dom\HTMLDocument::createFromString(
    '<div id="Case" class="Token"></div>',
    LIBXML_NOERROR
);
echo '|Q'
    . $standards->querySelectorAll('#case.token')->length
    . ':'
    . $quirks->querySelectorAll('#case.token')->length;
try {
    $standards->querySelectorAll('col.selected||td');
} catch (ValueError $exception) {
    echo '|V:' . $exception->getMessage();
}
"#,
    );
    assert!(
        output.success,
        "stdout: {}\nstderr: {}",
        output.stdout,
        output.stderr
    );
    assert_eq!(
        output.stdout,
        "Dom\\Element:one|Dom\\NodeList:3:3:one:p:I:3|F:s:N:N|D12:Invalid selector (Selectors. Unexpected token: @invalid)|B9::blank selector is not implemented because CSSWG has not yet decided its semantics (https://github.com/w3c/csswg-drafts/issues/1967)|Q0:1|V:Dom\\Document::querySelectorAll(): Argument #1 ($selectors) contains an unsupported selector"
    );
    assert_eq!(output.stderr, "");
}

/// Verifies live class-name collections, quirks modes, and validation match PHP.
#[test]
fn modern_class_name_collections_match_php() {
    let output = compile_and_run_capture(
        r#"<?php
$standards = Dom\HTMLDocument::createFromString(
    '<!doctype html><main class="Foo Bar" id="root"><p class="foo bar" id="one"></p><p class="Foo Bar" name="two"></p><p class="foo bars"></p></main>',
    LIBXML_NOERROR
);
$all = $standards->getElementsByClassName("foo \n bar foo");
$nested = $standards->documentElement->getElementsByClassName("foo bar");
echo get_class($all) . ':' . $all->length . ':' . $all->count();
echo ':' . $nested->length . ':' . $nested->item(0)->id;
echo ':' . ($nested->namedItem("one") === $nested->item(0) ? 'I' : 'X');
$new = $standards->createElement('p');
$new->className = 'foo bar';
$new->id = 'new';
$standards->documentElement->append($new);
echo ':' . $all->length . ':' . $nested->length . ':' . $all->namedItem('new')->id;
$new->className = 'other';
$new->append('invalidate');
echo ':' . $all->length;
echo '|E'
    . $standards->getElementsByClassName('')->length
    . ':'
    . $standards->getElementsByClassName("\t\n\f\r ")->length
    . ':'
    . $standards->getElementsByClassName("\v")->length;
try {
    $standards->getElementsByClassName("Foo\0ignored");
} catch (ValueError $exception) {
    echo '|N:' . $exception->getMessage();
}

$quirks = Dom\HTMLDocument::createFromString(
    '<main class="Foo Bar"><p class="fOo bAr" id="q"></p></main>',
    LIBXML_NOERROR
);
echo '|Q' . $quirks->getElementsByClassName('foo bar')->length;

$limited = Dom\HTMLDocument::createFromString(
    '<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.0 Transitional//EN" "http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd"><main class="Bar"></main>',
    LIBXML_NOERROR
);
echo '|L'
    . $limited->getElementsByClassName('bar')->length
    . ':'
    . $limited->querySelectorAll('.bar')->length;

$xml = Dom\XMLDocument::createFromString(
    '<root xmlns:x="urn:x"><item x:class="hit"/><item class="hit"/></root>'
);
echo '|X' . $xml->getElementsByClassName('hit')->length;
"#,
    );
    assert!(
        output.success,
        "stdout: {}\nstderr: {}",
        output.stdout,
        output.stderr
    );
    assert_eq!(
        output.stdout,
        "Dom\\HTMLCollection:1:1:1:one:I:2:2:new:1|E0:0:0|N:Dom\\Document::getElementsByClassName(): Argument #1 ($classNames) must not contain any null bytes|Q2|L0:1|X1"
    );
    assert_eq!(output.stderr, "");
}

/// Verifies modern class token lists, exact errors, identity, and graph rehoming.
#[test]
fn modern_class_token_lists_match_php() {
    let output = compile_and_run_capture(
        r#"<?php
$document = Dom\XMLDocument::createFromString(
    '<root class=" alpha  beta alpha "><child/></root>'
);
$root = $document->documentElement;
if ($root === null) { exit(2); }
$tokens = $root->classList;
echo get_class($tokens)
    . ':' . ($tokens === $root->classList ? 'I' : 'X')
    . ':' . $tokens->length
    . ':' . $tokens->count()
    . ':[' . $tokens->value . ']'
    . ':' . ($tokens->item(-1) === null ? 'N' : 'X')
    . ':' . $tokens->item(1)
    . ':' . ($tokens->contains('') ? 'T' : 'F')
    . ':' . ($tokens->contains(' ') ? 'T' : 'F');

$root->setAttribute('class', " one\t two one ");
echo '|S:[' . $tokens->value . ']:' . $tokens->length;
$tokens->add('three', 'two');
echo ':[' . $tokens->value . ']';
$tokens->remove('one');
echo ':[' . $tokens->value . ']';
echo ':' . ($tokens->toggle('two') ? 'T' : 'F')
    . ':[' . $tokens->value . ']';
echo ':' . ($tokens->toggle('two', false) ? 'T' : 'F')
    . ':[' . $tokens->value . ']';
echo ':' . ($tokens->toggle('two', null) ? 'T' : 'F')
    . ':[' . $tokens->value . ']';
echo ':' . ($tokens->toggle('two', true) ? 'T' : 'F')
    . ':[' . $tokens->value . ']';
echo ':' . ($tokens->replace('three', 'four') ? 'T' : 'F')
    . ':[' . $tokens->value . ']';
echo ':' . ($tokens->replace('missing', 'x') ? 'T' : 'F');
echo ':' . ($tokens->replace('four', 'two') ? 'T' : 'F')
    . ':[' . $tokens->value . ']';
echo ':' . ($tokens->replace('two', 'two') ? 'T' : 'F')
    . ':[' . $tokens->value . ']';

try {
    $tokens->add('safe', '');
} catch (DOMException $exception) {
    echo '|A' . $exception->getCode() . ':' . $exception->getMessage()
        . ':' . ($tokens->contains('safe') ? 'X' : 'N');
}
try {
    $tokens->remove('two', 'bad token');
} catch (DOMException $exception) {
    echo '|R' . $exception->getCode() . ':' . $exception->getMessage()
        . ':' . ($tokens->contains('two') ? 'I' : 'X');
}
try {
    $tokens->add("x\0y");
} catch (ValueError $exception) {
    echo '|V:' . $exception->getMessage();
}
try {
    $tokens->contains("x\0y");
} catch (ValueError $exception) {
    echo '|C:' . $exception->getMessage();
}
try {
    $tokens->supports('two');
} catch (TypeError $exception) {
    echo '|T:' . $exception->getMessage();
}
try {
    $tokens->value = "x\0y";
} catch (ValueError $exception) {
    echo '|W:' . $exception->getMessage();
}

$tokens->value = ' loose  loose z ';
echo '|P:[' . $tokens->value . ']:' . $tokens->length;
$tokens->add();
echo ':[' . $tokens->value . ']';
$root->remove();
$tokens->add('detached');
echo ':[' . $tokens->value . ']';

$other = Dom\XMLDocument::createFromString('<host/>');
$adopted = $other->adoptNode($root);
$host = $other->documentElement;
if ($host === null) { exit(3); }
$host->append($root);
$tokens->add('adopted');
echo '|D:' . ($adopted === $root ? 'I' : 'X')
    . ':' . ($root->classList === $tokens ? 'I' : 'X')
    . ':[' . $tokens->value . ']';
"#,
    );
    assert!(
        output.success,
        "stdout: {}\nstderr: {}",
        output.stdout,
        output.stderr
    );
    assert_eq!(
        output.stdout,
        "Dom\\TokenList:I:2:2:[ alpha  beta alpha ]:N:beta:F:F|S:[ one\t two one ]:2:[one two three]:[two three]:F:[three]:F:[three]:T:[three two]:T:[three two]:T:[four two]:F:T:[two]:T:[two]|A12:The empty string is not a valid token:N|R5:The token must not contain any ASCII whitespace:I|V:Dom\\TokenList::add(): Argument #1 must not contain any null bytes|C:Dom\\TokenList::contains(): Argument #1 ($token) must not contain any null bytes|T:Attribute \"class\" does not define any supported tokens|W:Value must not contain any null bytes|P:[ loose  loose z ]:2:[loose z]:[loose z detached]|D:I:I:[loose z detached adopted]"
    );
    assert_eq!(output.stderr, "");
}

/// Verifies namespace-aware descendant collections, wildcards, and concrete members.
#[test]
fn namespace_aware_descendant_collections_match_php() {
    let legacy = compile_and_run(
        r#"<?php
$document = new DOMDocument();
$document->loadXML("<r xmlns:x=\"u\"><x:a/><a/><x:b/><a/></r>");
$namespaced = $document->getElementsByTagNameNS("u", "a");
$wildcardNamespace = $document->getElementsByTagNameNS("*", "a");
$wildcardLocal = $document->getElementsByTagNameNS("u", "*");
$noNamespace = $document->getElementsByTagNameNS(null, "a");
$legacyLocal = $document->getElementsByTagName("a");
$legacyQualified = $document->getElementsByTagName("x:a");
echo get_class($namespaced) . ":" . $namespaced->length;
echo ":" . $namespaced->item(0)->nodeName;
echo "|" . $wildcardNamespace->length . ":" . $wildcardLocal->length;
echo ":" . $noNamespace->length;
echo "|" . $legacyLocal->length . ":" . $legacyQualified->length;
"#,
    );
    assert_eq!(legacy, "DOMNodeList:1:x:a|3:2:2|3:0");

    let modern = compile_and_run(
        r#"<?php
$document = Dom\XMLDocument::createFromString("<r xmlns:x=\"u\"><x:a/><a/><x:b/><a/></r>");
$namespaced = $document->getElementsByTagNameNS("u", "a");
$wildcardNamespace = $document->getElementsByTagNameNS("*", "a");
$wildcardLocal = $document->getElementsByTagNameNS("u", "*");
$noNamespace = $document->getElementsByTagNameNS(null, "a");
$modernLocal = $document->getElementsByTagName("a");
$modernQualified = $document->getElementsByTagName("x:a");
echo get_class($namespaced) . ":" . $namespaced->length;
echo ":" . $namespaced->item(0)->nodeName;
echo "|" . $wildcardNamespace->length . ":" . $wildcardLocal->length;
echo ":" . $noNamespace->length;
echo "|" . $modernLocal->length . ":" . $modernQualified->length;
"#,
    );
    assert_eq!(modern, "Dom\\HTMLCollection:1:x:a|3:2:2|2:1");
}

/// Verifies ParentNode element navigation and modern live child collections.
#[test]
fn parent_node_element_views_match_php() {
    let legacy = compile_and_run(
        r#"<?php
$document = new DOMDocument();
$root = $document->createElement("r");
if ($root === false) { exit(2); }
$document->appendChild($document->createComment("before"));
$document->appendChild($root);
echo ($document->firstElementChild === $root ? "F" : "x");
echo ":" . ($document->lastElementChild === $root ? "L" : "x");
echo ":" . $document->childElementCount;
$fragment = $document->createDocumentFragment();
$fragment->appendChild($document->createComment("inside"));
$child = $document->createElement("child");
if ($child === false) { exit(3); }
$fragment->appendChild($child);
echo "|" . ($fragment->firstElementChild === $child ? "F" : "x");
echo ":" . ($fragment->lastElementChild === $child ? "L" : "x");
echo ":" . $fragment->childElementCount;
"#,
    );
    assert_eq!(legacy, "F:L:1|F:L:1");

    let modern = compile_and_run(
        r#"<?php
$document = Dom\XMLDocument::createFromString("<r>t<a id=\"a\"/><!--c--><b id=\"b\"/></r>");
$root = $document->documentElement;
if ($root === null) { exit(2); }
$documentChildren = $document->children;
$children = $root->children;
$documentFirst = $document->firstElementChild;
$first = $root->firstElementChild;
$last = $root->lastElementChild;
if ($documentFirst === null) { exit(3); }
if ($first === null) { exit(4); }
if ($last === null) { exit(5); }
echo get_class($documentChildren) . ":" . $documentChildren->length;
echo ":" . $document->childElementCount . ":" . $documentFirst->nodeName;
echo "|" . get_class($children) . ":" . $children->length;
echo ":" . $root->childElementCount . ":" . $first->nodeName;
echo ":" . $last->nodeName;
echo ":" . $children->item(0)->nodeName . ":" . $children->item(1)->nodeName;
$root->removeChild($last);
echo "|" . $children->length . ":" . $children->item(0)->nodeName;
"#,
    );
    assert_eq!(
        modern,
        "Dom\\HTMLCollection:1:1:r|Dom\\HTMLCollection:2:2:a:b:a:b|1:a"
    );
}

/// Verifies document, doctype, attribute-ID, and processing-instruction metadata.
#[test]
fn document_and_doctype_metadata_match_php() {
    let legacy = compile_and_run(
        r#"<?php
$document = new DOMDocument();
echo $document->documentURI === null ? "N|" : "x|";
$document->loadXML("<!DOCTYPE r PUBLIC \"pub\" \"sys\" [<!ELEMENT r ANY><!ENTITY e \"v\">]><r xml:id=\"id\"/>");
$doctype = $document->doctype;
$root = $document->documentElement;
if ($doctype === null) { exit(2); }
if ($root === null) { exit(3); }
$attribute = $root->getAttributeNodeNS("http://www.w3.org/XML/1998/namespace", "id");
if (!$attribute instanceof DOMAttr) { exit(4); }
$instruction = $document->createProcessingInstruction("pi", "data");
if ($instruction === false) { exit(5); }
$root->appendChild($instruction);
echo get_class($doctype) . ":" . $doctype->nodeType;
echo ":" . $doctype->name . ":" . $doctype->publicId . ":" . $doctype->systemId;
echo ":" . str_replace("\n", "/", $doctype->internalSubset ?? "N");
echo "|" . ($attribute->isId() ? "I" : "x");
echo "|" . $instruction->target . ":" . $instruction->data;
$instruction->data = "new";
$document->documentURI = "urn:test";
echo ":" . $instruction->data . "|" . $document->documentURI . "\n";
"#,
    );
    assert_eq!(
        legacy,
        "N|DOMDocumentType:10:r:pub:sys:<!ELEMENT r ANY>/<!ENTITY e \"v\">/|I|pi:data:new|urn:test\n"
    );

    let modern = compile_and_run(
        r#"<?php
$empty = Dom\XMLDocument::createEmpty();
echo $empty->URL . ":" . $empty->documentURI . "|";
$document = Dom\XMLDocument::createFromString("<!DOCTYPE r [<!ELEMENT r EMPTY>]><r/>");
$doctype = $document->doctype;
if ($doctype === null) { exit(2); }
echo $document->URL === $document->documentURI ? "U" : "x";
echo $document->URL === "about:blank" ? "x|" : "B|";
echo get_class($doctype) . ":" . $doctype->nodeType . ":" . $doctype->name;
echo ":" . str_replace("\n", "/", $doctype->internalSubset ?? "N");
$document->URL = "urn:modern";
echo "|" . $document->URL . ":" . $document->documentURI . "\n";
"#,
    );
    assert_eq!(
        modern,
        "about:blank:about:blank|UB|Dom\\DocumentType:10:r:<!ELEMENT r EMPTY>/|urn:modern:urn:modern\n"
    );
}

/// Verifies mutable legacy parser flags retain php-src defaults and survive loads.
#[test]
fn legacy_document_parser_configuration_matches_php() {
    let out = compile_and_run(
        r#"<?php
libxml_use_internal_errors(true);
$document = new DOMDocument();
echo $document->formatOutput ? "1" : "0";
echo $document->preserveWhiteSpace ? "1" : "0";
echo $document->recover ? "1" : "0";
echo $document->resolveExternals ? "1" : "0";
echo $document->strictErrorChecking ? "1" : "0";
echo $document->substituteEntities ? "1" : "0";
echo $document->validateOnParse ? "1" : "0";

$document->formatOutput = true;
$document->preserveWhiteSpace = false;
$document->recover = true;
$document->resolveExternals = true;
$document->strictErrorChecking = false;
$document->substituteEntities = true;
$document->validateOnParse = false;
$document->loadXML("<r>\n  <a/>\n</r>");
$root = $document->documentElement;
if ($root === null) { exit(2); }
echo "|" . $root->childNodes->length . "|";
echo $document->formatOutput ? "1" : "0";
echo $document->preserveWhiteSpace ? "1" : "0";
echo $document->recover ? "1" : "0";
echo $document->resolveExternals ? "1" : "0";
echo $document->strictErrorChecking ? "1" : "0";
echo $document->substituteEntities ? "1" : "0";
echo $document->validateOnParse ? "1" : "0";

$document->loadXML("<!DOCTYPE r [<!ELEMENT r (#PCDATA)><!ENTITY e \"v\">]><r>&e;</r>");
$entityRoot = $document->documentElement;
if ($entityRoot === null) { exit(3); }
echo "|" . $document->saveXML($entityRoot);
echo "|" . ($document->loadXML("<r><a></r>") ? "R" : "x");
$recoveredRoot = $document->documentElement;
if ($recoveredRoot === null) { exit(4); }
echo "|" . str_replace("\n", "/", $document->saveXML($recoveredRoot));
"#,
    );
    assert_eq!(
        out,
        "0100100|1|1011010|<r>v</r>|R|<r>/  <a/>/</r>"
    );
}

/// Verifies writable XML declaration properties and their catchable value errors.
#[test]
fn document_xml_declaration_writes_match_php() {
    let out = compile_and_run(
        r#"<?php
$legacy = new DOMDocument();
$legacy->version = "1.1";
$legacy->encoding = "ISO-8859-1";
$legacy->standalone = true;
echo $legacy->version . ":" . $legacy->xmlVersion;
echo ":" . $legacy->encoding;
echo ":" . ($legacy->standalone ? "1" : "0");
echo ":" . ($legacy->xmlStandalone ? "1" : "0") . "|";
try {
    $legacy->encoding = "not-an-encoding";
} catch (ValueError $error) {
    echo get_class($error) . ":" . $error->getMessage();
}

$modern = Dom\XMLDocument::createEmpty();
$modern->xmlVersion = "1.1";
$modern->xmlStandalone = true;
echo "|" . $modern->xmlVersion;
echo ":" . ($modern->xmlStandalone ? "1" : "0") . "|";
try {
    $modern->xmlVersion = "2.0";
} catch (ValueError $error) {
    echo get_class($error) . ":" . $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "1.1:1.1:ISO-8859-1:1:1|ValueError:Invalid document encoding|1.1:1|ValueError:Invalid XML version"
    );
}

/// Verifies parsed namespace declarations become live modern attributes without breaking lookup.
#[test]
fn modern_namespace_declarations_match_php_attribute_semantics() {
    let output = compile_and_run(
        r#"<?php
$document = Dom\XMLDocument::createFromString("<r xmlns=\"urn:d\" xmlns:p=\"urn:p\" a=\"1\"><p:x/></r>");
$element = $document->documentElement;
$attributes = $element->attributes;
$default = $attributes->item(0);
$prefixed = $attributes->item(1);
$names = $element->getAttributeNames();
echo get_class($attributes) . ":" . $attributes->length . ":" . $attributes->count();
echo "|" . $default->name . ":" . $default->localName . ":" . ($default->prefix === null ? "N" : $default->prefix) . ":" . $default->namespaceURI . ":" . $default->value;
echo "|" . $prefixed->name . ":" . $prefixed->localName . ":" . $prefixed->prefix . ":" . $prefixed->namespaceURI . ":" . $prefixed->value;
echo "|" . ($element->getAttributeNode("xmlns") === $default ? "I" : "x");
echo ":" . ($element->getAttributeNodeNS("http://www.w3.org/2000/xmlns/", "p") === $prefixed ? "J" : "x");
echo ":" . $element->getAttribute("xmlns:p") . ":" . ($element->hasAttribute("xmlns:p") ? "Y" : "x");
echo "|" . count($names) . ":" . $names[0] . ":" . $names[1] . ":" . $names[2];
echo "|" . $element->lookupNamespaceURI("p") . ":" . $element->lookupPrefix("urn:p");
echo ":" . $element->lookupNamespaceURI("xml") . ":" . $element->lookupNamespaceURI("xmlns");
$child = $element->firstElementChild;
$element->removeAttribute("xmlns");
$element->removeAttribute("xmlns:p");
echo "|" . $attributes->length . ":" . $element->lookupNamespaceURI(null);
echo ":" . ($element->lookupNamespaceURI("p") === null ? "N" : "x");
echo ":" . $child->lookupNamespaceURI("p");
echo "|" . $document->saveXML();
"#,
    );
    assert_eq!(
        output,
        "Dom\\NamedNodeMap:3:3|xmlns:xmlns:N:http://www.w3.org/2000/xmlns/:urn:d|xmlns:p:p:xmlns:http://www.w3.org/2000/xmlns/:urn:p|I:J:urn:p:Y|3:xmlns:xmlns:p:a|urn:p:p:http://www.w3.org/XML/1998/namespace:http://www.w3.org/2000/xmlns/|1:urn:d:N:urn:p|<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<r xmlns=\"urn:d\" a=\"1\"><p:x xmlns:p=\"urn:p\"/></r>"
    );
}

/// Verifies character-data methods use PHP's UTF-8 offsets, returns, writes, and exceptions.
#[test]
fn character_data_operations_match_php() {
    let legacy = compile_and_run(
        r#"<?php
$document = new DOMDocument();
$node = $document->createTextNode("Aé😀Z");
echo $node->data . ":" . $node->length . ":" . $node->substringData(1, 2);
echo ":" . ($node->appendData("!") === true ? "T" : "x") . ":" . $node->data;
echo ":" . ($node->insertData(1, "X") === true ? "T" : "x") . ":" . $node->data;
echo ":" . ($node->deleteData(2, 2) === true ? "T" : "x") . ":" . $node->data;
echo ":" . ($node->replaceData(1, 2, "é😀") === true ? "T" : "x") . ":" . $node->data . ":" . $node->length;
$node->data = "é😀";
echo ":" . $node->data . ":" . $node->length;
try { $node->substringData(-1, 1); echo ":x"; } catch (DOMException $e) { echo ":" . $e->getCode() . ":" . $e->getMessage(); }
try { $node->substringData(99, 1); echo ":x"; } catch (DOMException $e) { echo ":" . $e->getCode() . ":" . $e->getMessage(); }
try { $node->substringData(0, -1); echo ":x"; } catch (DOMException $e) { echo ":" . $e->getCode() . ":" . $e->getMessage(); }
$node->data = "abcd";
try { $node->deleteData(1, -1); echo ":x"; } catch (DOMException $e) { echo ":" . $e->getCode() . ":" . $e->getMessage(); }
"#,
    );
    assert_eq!(
        legacy,
        "Aé😀Z:4:é😀:T:Aé😀Z!:T:AXé😀Z!:T:AXZ!:T:Aé😀!:4:é😀:2:1:Index Size Error:1:Index Size Error:1:Index Size Error:1:Index Size Error"
    );

    let modern = compile_and_run(
        r#"<?php
$document = Dom\XMLDocument::createEmpty();
$node = $document->createTextNode("Aé😀Z");
echo $node->data . ":" . $node->length . ":" . $node->substringData(1, 2);
echo ":" . ($node->appendData("!") === null ? "N" : "x") . ":" . $node->data;
echo ":" . ($node->insertData(1, "X") === null ? "N" : "x") . ":" . $node->data;
echo ":" . ($node->deleteData(2, 2) === null ? "N" : "x") . ":" . $node->data;
echo ":" . ($node->replaceData(1, 2, "é😀") === null ? "N" : "x") . ":" . $node->data . ":" . $node->length;
$node->data = "é😀";
echo ":" . $node->data . ":" . $node->length;
try { $node->substringData(-1, 1); echo ":x"; } catch (DOMException $e) { echo ":" . $e->getCode() . ":" . $e->getMessage(); }
try { $node->substringData(99, 1); echo ":x"; } catch (DOMException $e) { echo ":" . $e->getCode() . ":" . $e->getMessage(); }
echo ":" . $node->substringData(0, -1);
$node->data = "abcd";
$node->deleteData(1, -1);
echo ":" . $node->data;
$node->data = "abcd";
$node->replaceData(1, -1, "X");
echo ":" . $node->data;
"#,
    );
    assert_eq!(
        modern,
        "Aé😀Z:4:é😀:N:Aé😀Z!:N:AXé😀Z!:N:AXZ!:N:Aé😀!:4:é😀:2:1:Index Size Error:1:Index Size Error:é😀:a:aX"
    );
}

/// Verifies text splitting, adjacent-run data, whitespace aliases, and error families.
#[test]
fn text_operations_match_php() {
    let legacy = compile_and_run(
        r#"<?php
$document = new DOMDocument();
$root = $document->createElement("r");
if ($root === false) { exit(2); }
$document->appendChild($root);
$text = $document->createTextNode("aé🙂z");
$root->appendChild($text);
try {
    $text->splitText(-1);
    echo "x";
} catch (ValueError $error) {
    echo get_class($error) . ":" . $error->getCode() . ":" . $error->getMessage();
}
echo "|" . ($text->splitText(99) === false ? "F" : "x");
$split = $text->splitText(2);
if ($split === false) { exit(3); }
$cdata = $document->createCDATASection("C");
if ($cdata === false) { exit(4); }
$root->appendChild($cdata);
echo "|" . $text->data . ":" . $split->data;
echo ":" . ($text->nextSibling === $split ? "N" : "x");
echo ":" . ($split->previousSibling === $text ? "P" : "x");
echo ":" . $split->wholeText . ":" . $cdata->wholeText;
$blank = $document->createTextNode(" \n\t");
$nonblank = $document->createTextNode(" x ");
echo "|" . ($blank->isWhitespaceInElementContent() ? "W" : "x");
echo ":" . ($blank->isElementContentWhitespace() ? "A" : "x");
echo ":" . ($nonblank->isWhitespaceInElementContent() ? "x" : "0");
"#,
    );
    assert_eq!(
        legacy,
        "ValueError:0:DOMText::splitText(): Argument #1 ($offset) must be greater than or equal to 0|F|aé:🙂z:N:P:aé🙂zC:aé🙂zC|W:A:0"
    );

    let modern = compile_and_run(
        r#"<?php
$document = Dom\XMLDocument::createEmpty();
$root = $document->createElement("r");
$document->appendChild($root);
$text = $document->createTextNode("aé🙂z");
$root->appendChild($text);
$cdata = $document->createCDATASection("C");
$root->appendChild($cdata);
try {
    $text->splitText(-1);
    echo "x";
} catch (ValueError $error) {
    echo get_class($error) . ":" . $error->getCode() . ":" . $error->getMessage();
}
try {
    $text->splitText(99);
    echo "|x";
} catch (DOMException $error) {
    echo "|" . $error->getCode() . ":" . $error->getMessage();
}
$split = $text->splitText(2);
echo "|" . get_class($split) . ":" . $text->data . ":" . $split->data;
echo ":" . ($text->nextSibling === $split ? "N" : "x");
echo ":" . ($split->previousSibling === $text ? "P" : "x");
echo ":" . $split->wholeText;
"#,
    );
    assert_eq!(
        modern,
        "ValueError:0:Dom\\Text::splitText(): Argument #1 ($offset) must be greater than or equal to 0|1:Index Size Error|Dom\\Text:aé:🙂z:N:P:aé🙂zC"
    );
}

/// Verifies structural equality and document-position bitmasks for both DOM families.
#[test]
fn node_comparison_operations_match_php() {
    let legacy = compile_and_run(
        r#"<?php
$first = new DOMDocument();
$first->loadXML("<r b=\"2\" a=\"1\"><x/>t</r>");
$second = new DOMDocument();
$second->loadXML("<r a=\"1\" b=\"2\"><x/>t</r>");
$different = new DOMDocument();
$different->loadXML("<r a=\"1\" b=\"3\"><x/>t</r>");
$root = $first->documentElement;
$same = $second->documentElement;
$changed = $different->documentElement;
if ($root === null) { exit(2); }
if ($same === null) { exit(3); }
if ($changed === null) { exit(4); }
$child = $root->firstChild;
$text = $root->lastChild;
if ($child === null) { exit(5); }
if ($text === null) { exit(6); }
echo ($root->isEqualNode(null) ? "1" : "0");
echo ($root->isEqualNode($same) ? "1" : "0");
echo ($root->isEqualNode($changed) ? "1" : "0");
echo "|" . $root->compareDocumentPosition($child);
echo ":" . $child->compareDocumentPosition($root);
echo ":" . $child->compareDocumentPosition($text);
echo ":" . $text->compareDocumentPosition($child);
$detached = $different->createElement("q");
if ($detached === false) { exit(7); }
$position = $root->compareDocumentPosition($detached);
echo "|" . (($position & 1) ? "D" : "x");
echo (($position & 32) ? "I" : "x");
echo (($position & 6) ? "O" : "x");
"#,
    );
    assert_eq!(legacy, "010|20:10:4:2|DIO");

    let modern = compile_and_run(
        r#"<?php
$first = Dom\XMLDocument::createFromString("<r b=\"2\" a=\"1\"><x/>t</r>");
$second = Dom\XMLDocument::createFromString("<r a=\"1\" b=\"2\"><x/>t</r>");
$different = Dom\XMLDocument::createFromString("<r a=\"1\" b=\"3\"><x/>t</r>");
$root = $first->documentElement;
$same = $second->documentElement;
$changed = $different->documentElement;
if ($root === null) { exit(2); }
if ($same === null) { exit(3); }
if ($changed === null) { exit(4); }
$child = $root->firstChild;
$text = $root->lastChild;
if ($child === null) { exit(5); }
if ($text === null) { exit(6); }
echo ($root->isEqualNode(null) ? "1" : "0");
echo ($root->isEqualNode($same) ? "1" : "0");
echo ($root->isEqualNode($changed) ? "1" : "0");
echo "|" . $root->compareDocumentPosition($child);
echo ":" . $child->compareDocumentPosition($root);
echo ":" . $child->compareDocumentPosition($text);
echo ":" . $text->compareDocumentPosition($child);
$detached = $different->createElement("q");
$position = $root->compareDocumentPosition($detached);
echo "|" . (($position & 1) ? "D" : "x");
echo (($position & 32) ? "I" : "x");
echo (($position & 6) ? "O" : "x");
"#,
    );
    assert_eq!(modern, "010|20:10:4:2|DIO");
}

/// Verifies normalization merges text recursively while preserving detached wrappers.
#[test]
fn node_normalization_matches_php() {
    let legacy = compile_and_run(
        r#"<?php
$document = new DOMDocument();
$root = $document->createElement("r");
$nested = $document->createElement("n");
$cdata = $document->createCDATASection("x");
if ($root === false) { exit(2); }
if ($nested === false) { exit(3); }
if ($cdata === false) { exit(4); }
$document->appendChild($root);
$first = $document->createTextNode("a");
$merged = $document->createTextNode("b");
$empty = $document->createTextNode("");
$left = $document->createTextNode("c");
$right = $document->createTextNode("d");
$nestedFirst = $document->createTextNode("e");
$nestedMerged = $document->createTextNode("f");
$root->appendChild($first);
$root->appendChild($merged);
$root->appendChild($empty);
$root->appendChild($cdata);
$root->appendChild($left);
$root->appendChild($right);
$root->appendChild($nested);
$nested->appendChild($nestedFirst);
$nested->appendChild($nestedMerged);
$document->normalizeDocument();
echo $root->childNodes->length . ":" . $root->textContent;
echo "|" . $first->data . ":" . ($first->parentNode === $root ? "P" : "x");
echo "|" . $merged->data . ":" . ($merged->parentNode === null ? "D" : "x");
echo "|" . ($empty->parentNode === null ? "E" : "x");
echo "|" . $left->data . ":" . $right->data;
echo ":" . ($right->parentNode === null ? "V" : "x");
echo "|" . $nestedFirst->data . ":" . $nestedMerged->data;
echo ":" . ($nestedMerged->parentNode === null ? "Q" : "x");
"#,
    );
    assert_eq!(
        legacy,
        "4:abxcdef|ab:P|b:D|E|cd:d:V|ef:f:Q"
    );

    let modern = compile_and_run(
        r#"<?php
$document = Dom\XMLDocument::createEmpty();
$root = $document->createElement("r");
$nested = $document->createElement("n");
$cdata = $document->createCDATASection("x");
$document->appendChild($root);
$first = $document->createTextNode("a");
$merged = $document->createTextNode("b");
$empty = $document->createTextNode("");
$left = $document->createTextNode("c");
$right = $document->createTextNode("d");
$nestedFirst = $document->createTextNode("e");
$nestedMerged = $document->createTextNode("f");
$root->appendChild($first);
$root->appendChild($merged);
$root->appendChild($empty);
$root->appendChild($cdata);
$root->appendChild($left);
$root->appendChild($right);
$root->appendChild($nested);
$nested->appendChild($nestedFirst);
$nested->appendChild($nestedMerged);
$root->normalize();
echo $root->childNodes->length . ":" . $root->textContent;
echo "|" . $first->data . ":" . ($first->parentNode === $root ? "P" : "x");
echo "|" . $merged->data . ":" . ($merged->parentNode === null ? "D" : "x");
echo "|" . ($empty->parentNode === null ? "E" : "x");
echo "|" . $left->data . ":" . $right->data;
echo ":" . ($right->parentNode === null ? "V" : "x");
echo "|" . $nestedFirst->data . ":" . $nestedMerged->data;
echo ":" . ($nestedMerged->parentNode === null ? "Q" : "x");
"#,
    );
    assert_eq!(
        modern,
        "4:abxcdef|ab:P|b:D|E|cd:d:V|ef:f:Q"
    );
}

/// Verifies variadic ParentNode and ChildNode mutation order, flattening, and identity.
#[test]
fn variadic_tree_mutations_match_php() {
    let legacy = compile_and_run_capture(
        r#"<?php
$document = new DOMDocument();
$root = $document->createElement("r");
$a = $document->createElement("a");
$b = $document->createElement("b");
$c = $document->createElement("c");
if ($root === false) { exit(2); }
if ($a === false) { exit(3); }
if ($b === false) { exit(4); }
if ($c === false) { exit(5); }
$document->appendChild($root);
$root->append($a, $b, $c);
$root->prepend("0", $c);
echo $document->saveXML($root);
$a->before("x", $a);
$a->after($a, "y");
$a->replaceWith("A", $a);
$b->replaceWith("B");
echo "|" . $document->saveXML($root);
$fragment = $document->createDocumentFragment();
$f1 = $document->createElement("f1");
$f2 = $document->createElement("f2");
if ($f1 === false) { exit(6); }
if ($f2 === false) { exit(7); }
$fragment->append($f1, $f2);
$root->append($fragment, "z");
echo "|" . $document->saveXML($root);
echo ":" . $fragment->childNodes->length;
$detached = $root->firstChild;
if ($detached === null) { exit(8); }
$root->replaceChildren($a, "tail");
echo "|" . $document->saveXML($root);
echo ":" . ($detached->parentNode === null ? "D" : "x");
$a->remove();
echo "|" . $document->saveXML($root);
try {
    $a->remove();
} catch (DOMException $error) {
    echo "|" . $error->getCode() . ":" . $error->getMessage();
}
"#,
    );
    assert!(
        legacy.success,
        "stdout: {}\nstderr: {}",
        legacy.stdout,
        legacy.stderr
    );
    assert_eq!(
        legacy.stdout,
        "<r>0<c/><a/><b/></r>|<r>0<c/>xA<a/>yB</r>|<r>0<c/>xA<a/>yB<f1/><f2/>z</r>:0|<r><a/>tail</r>:D|<r>tail</r>|8:Not Found Error"
    );

    let modern = compile_and_run_capture(
        r#"<?php
$document = Dom\XMLDocument::createEmpty();
$root = $document->createElement("r");
$a = $document->createElement("a");
$b = $document->createElement("b");
$c = $document->createElement("c");
$document->appendChild($root);
$root->append($a, $b, $c);
$root->prepend("0", $c);
echo $document->saveXML($root);
$a->before("x", $a);
$a->after($a, "y");
$a->replaceWith("A", $a);
$b->replaceWith("B");
echo "|" . $document->saveXML($root);
$fragment = $document->createDocumentFragment();
$f1 = $document->createElement("f1");
$f2 = $document->createElement("f2");
$fragment->append($f1, $f2);
$root->append($fragment, "z");
echo "|" . $document->saveXML($root);
echo ":" . $fragment->childNodes->length;
$detached = $root->firstChild;
if ($detached === null) { exit(2); }
$root->replaceChildren($a, "tail");
echo "|" . $document->saveXML($root);
echo ":" . ($detached->parentNode === null ? "D" : "x");
$a->remove();
echo "|" . $document->saveXML($root);
try {
    $a->remove();
} catch (DOMException $error) {
    echo "|" . $error->getCode() . ":" . $error->getMessage();
}
"#,
    );
    assert!(
        modern.success,
        "stdout: {}\nstderr: {}",
        modern.stdout,
        modern.stderr
    );
    assert_eq!(
        modern.stdout,
        "<r>0<c/><a/><b/></r>|<r>0<c/>xA<a/>yB</r>|<r>0<c/>xA<a/>yB<f1/><f2/>z</r>:0|<r><a/>tail</r>:D|<r>tail</r>|8:Not Found Error"
    );
}

/// Verifies dynamic variadic node-or-string arguments and exact PHP diagnostics.
#[test]
fn variadic_tree_mutation_mixed_contracts_match_php() {
    let out = compile_and_run_capture(
        r#"<?php
function opaque_dom_variadic(mixed $value): mixed {
    return $value;
}

class RejectedDomStringable {
    public function __toString(): string {
        echo "BAD";
        return "bad";
    }
}

$legacy = new DOMDocument();
$legacyRoot = $legacy->createElement("r");
$legacyChild = $legacy->createElement("c");
if ($legacyRoot === false || $legacyChild === false) { exit(2); }
$legacy->appendChild($legacyRoot);
$legacyRoot->append(opaque_dom_variadic("a"), opaque_dom_variadic($legacyChild));
echo $legacy->saveXML($legacyRoot), "\n";
try {
    $legacyRoot->append("x", opaque_dom_variadic(42));
} catch (TypeError $error) {
    echo "|", $error->getMessage(), "\n";
}
try {
    $legacyRoot->append(opaque_dom_variadic(true));
} catch (TypeError $error) {
    echo "|", $error->getMessage(), "\n";
}
try {
    $legacyRoot->append(opaque_dom_variadic(new RejectedDomStringable()));
} catch (TypeError $error) {
    echo "|", $error->getMessage(), "\n";
}
try {
    $legacyRoot->append("x", $legacyChild, opaque_dom_variadic(new stdClass()));
} catch (TypeError $error) {
    echo "|", $error->getMessage(), "\n";
}

$modern = Dom\XMLDocument::createEmpty();
$modernRoot = $modern->createElement("r");
$modernChild = $modern->createElement("c");
$modern->appendChild($modernRoot);
$modernRoot->append(opaque_dom_variadic("a"), opaque_dom_variadic($modernChild));
echo $modern->saveXML($modernRoot), "\n";
try {
    $modernRoot->append("x", opaque_dom_variadic(42));
} catch (TypeError $error) {
    echo "|", $error->getMessage(), "\n";
}
try {
    $modernRoot->append(opaque_dom_variadic(false));
} catch (TypeError $error) {
    echo "|", $error->getMessage(), "\n";
}
try {
    $modernRoot->append(opaque_dom_variadic(new RejectedDomStringable()));
} catch (TypeError $error) {
    echo "|", $error->getMessage(), "\n";
}
try {
    $modernRoot->append("x", $modernChild, opaque_dom_variadic(new stdClass()));
} catch (TypeError $error) {
    echo "|", $error->getMessage(), "\n";
}
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(
        out.stdout,
        concat!(
            "<r>a<c/></r>\n",
            "|DOMElement::append(): Argument #2 must be of type DOMNode|string, int given\n",
            "|DOMElement::append(): Argument #1 must be of type DOMNode|string, bool given\n",
            "|DOMElement::append(): Argument #1 must be of type DOMNode|string, RejectedDomStringable given\n",
            "|DOMElement::append(): Argument #3 must be of type DOMNode|string, stdClass given\n",
            "<r>a<c/></r>\n",
            "|Dom\\Element::append(): Argument #2 must be of type Dom\\Node|string, int given\n",
            "|Dom\\Element::append(): Argument #1 must be of type Dom\\Node|string, bool given\n",
            "|Dom\\Element::append(): Argument #1 must be of type Dom\\Node|string, RejectedDomStringable given\n",
            "|Dom\\Element::append(): Argument #3 must be of type Dom\\Node|string, stdClass given\n",
        )
    );
    assert_eq!(out.stderr, "");
}

/// Verifies dynamic writes to the modern body property use typed-property errors.
#[test]
fn modern_document_body_mixed_property_type_errors_match_php() {
    let out = compile_and_run_capture(
        r#"<?php
function opaque_dom_body(mixed $value): mixed {
    return $value;
}

$document = Dom\HTMLDocument::createFromString("<!doctype html><html><body></body></html>");
try {
    $document->body = opaque_dom_body(new stdClass());
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
try {
    $document->body = opaque_dom_body(42);
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
try {
    $document->body = opaque_dom_body(true);
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(
        out.stdout,
        concat!(
            "Cannot assign stdClass to property Dom\\Document::$body of type ?Dom\\HTMLElement\n",
            "Cannot assign int to property Dom\\Document::$body of type ?Dom\\HTMLElement\n",
            "Cannot assign true to property Dom\\Document::$body of type ?Dom\\HTMLElement\n",
        )
    );
    assert_eq!(out.stderr, "");
}

/// Verifies an exception from a later string coercion unwinds earlier DOM arguments cleanly.
#[test]
fn dom_later_stringable_throw_releases_earlier_coercions() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class FirstDomString {
    public function __toString(): string {
        return str_repeat("n", 16);
    }
}

class ThrowingDomString {
    public function __toString(): string {
        throw new Exception("stop");
    }
}

$document = new DOMDocument();
for ($index = 0; $index < 12; $index++) {
    try {
        $document->createElement(new FirstDomString(), new ThrowingDomString());
    } catch (Exception $error) {
        echo ".";
    }
}
echo "|done";
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stdout, "............|done");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected throwing DOM string coercions to remain heap-clean, got: {}",
        out.stderr
    );
}

/// Verifies non-DOM Mixed dispatch still links latent DOM property and method branches.
#[test]
fn mixed_dispatch_with_dom_candidates_links_required_runtime() {
    let out = compile_and_run(
        r#"<?php
function opaque_dom_link_candidate(mixed $value): mixed {
    return $value;
}

class DomLinkRow {
    public string $name = "Ada";
}

class DomLinkSink {
    public function append(string $value): string {
        return $value;
    }
}

$row = opaque_dom_link_candidate(new DomLinkRow());
$sink = opaque_dom_link_candidate(new DomLinkSink());
echo $row->name, ":", $sink->append("ok");
"#,
    );
    assert_eq!(out, "Ada:ok");
}

/// Verifies stub-only virtual readonly handlers throw without blocking writable peers.
#[test]
fn virtual_readonly_property_writes_match_php() {
    let output = compile_and_run_capture(
        r#"<?php
$legacy = new DOMDocument();
$legacy->loadXML("<root/>");
$legacyRoot = $legacy->documentElement;
if ($legacyRoot === null) { exit(2); }
try {
    $legacyRoot->nodeName = "changed";
} catch (Error $error) {
    echo get_class($error) . ":" . $error->getMessage();
}
$legacyRoot->textContent = "content";
echo "|" . $legacy->saveXML($legacyRoot);

$modern = Dom\XMLDocument::createFromString("<root/>");
$modernRoot = $modern->documentElement;
if ($modernRoot === null) { exit(3); }
try {
    $modernRoot->nodeName = "changed";
} catch (Error $error) {
    echo "|" . get_class($error) . ":" . $error->getMessage();
}
try {
    $modernRoot->nodeValue = "changed";
} catch (Error $error) {
    echo "|" . get_class($error) . ":" . $error->getMessage();
}
$modernRoot->textContent = "content";
echo "|" . $modern->saveXML($modernRoot);
try {
    $modern->textContent = "changed";
} catch (Error $error) {
    echo "|" . get_class($error) . ":" . $error->getMessage();
}

"#,
    );
    assert!(
        output.success,
        "stdout: {}\nstderr: {}",
        output.stdout,
        output.stderr
    );
    assert_eq!(
        output.stdout,
        "Error:Cannot modify readonly property DOMElement::$nodeName|<root>content</root>|Error:Cannot modify readonly property Dom\\Element::$nodeName|Error:Cannot modify readonly property Dom\\Element::$nodeValue|<root>content</root>|Error:Cannot modify readonly property Dom\\XMLDocument::$textContent"
    );
}

/// Verifies element attribute properties, namespaced presence, and element siblings.
#[test]
fn element_attribute_properties_and_ns_presence_match_php() {
    let output = compile_and_run_capture(
        r#"<?php
$legacy = new DOMDocument();
$legacy->loadXML('<root xmlns:p="urn:x" p:v="1"><a/>text<b/></root>');
$legacyRoot = $legacy->documentElement;
if ($legacyRoot === null) { exit(2); }
$legacyLast = $legacyRoot->lastElementChild;
if ($legacyLast === null) { exit(3); }
$legacyText = $legacy->createTextNode("text");
$legacyRoot->insertBefore($legacyText, $legacyLast);
$legacyAttribute = $legacy->createAttribute("probe");
if ($legacyAttribute === false) { exit(4); }
echo ($legacyRoot->hasAttributeNS("urn:x", "v") ? "1" : "0");
echo ($legacyRoot->hasAttributeNS("urn:no", "v") ? "1" : "0");
echo ":" . $legacyText->previousElementSibling?->nodeName;
echo ":" . $legacyText->nextElementSibling?->nodeName;
echo ":" . ($legacyAttribute->specified ? "1" : "0");
$legacyRoot->id = "i";
$legacyRoot->className = "c";
echo ":" . $legacy->saveXML($legacyRoot);

$modern = Dom\XMLDocument::createFromString(
    '<root xmlns:p="urn:x" p:v="1"><a/>text<b/></root>'
);
$modernRoot = $modern->documentElement;
if ($modernRoot === null) { exit(5); }
$modernLast = $modernRoot->lastElementChild;
if ($modernLast === null) { exit(6); }
$modernText = $modern->createTextNode("text");
$modernRoot->insertBefore($modernText, $modernLast);
$modernAttribute = $modern->createAttribute("probe");
echo "|" . ($modernRoot->hasAttributeNS("urn:x", "v") ? "1" : "0");
echo ($modernRoot->hasAttributeNS("urn:no", "v") ? "1" : "0");
echo ":" . $modernText->previousElementSibling?->nodeName;
echo ":" . $modernText->nextElementSibling?->nodeName;
echo ":" . ($modernAttribute->specified ? "1" : "0");
$modernRoot->id = "i";
$modernRoot->className = "c";
echo ":" . $modern->saveXML($modernRoot);
"#,
    );
    assert!(
        output.success,
        "stdout: {}\nstderr: {}",
        output.stdout,
        output.stderr
    );
    assert_eq!(
        output.stdout,
        "10:a:b:1:<root xmlns:p=\"urn:x\" p:v=\"1\" id=\"i\" class=\"c\"><a/>texttext<b/></root>|10:a:b:1:<root xmlns:p=\"urn:x\" p:v=\"1\" id=\"i\" class=\"c\"><a/>texttext<b/></root>"
    );
}

/// Verifies every publicly constructible legacy node keeps its private owner hidden.
#[test]
fn legacy_direct_node_constructors_match_php() {
    let output = compile_and_run(
        r#"<?php
$element = new DOMElement("p:e", "v", "urn:x");
echo get_class($element) . ":" . $element->nodeName;
echo ":" . var_export($element->nodeValue, true);
echo ":" . var_export($element->textContent, true);
echo ":" . ($element->ownerDocument === null ? "N" : "x") . "|";
$attribute = new DOMAttr("a", "v");
echo get_class($attribute) . ":" . $attribute->nodeName;
echo ":" . var_export($attribute->nodeValue, true);
echo ":" . var_export($attribute->textContent, true);
echo ":" . ($attribute->ownerDocument === null ? "N" : "x") . "|";
$text = new DOMText("v");
echo get_class($text) . ":" . $text->nodeName;
echo ":" . var_export($text->nodeValue, true);
echo ":" . var_export($text->textContent, true);
echo ":" . ($text->ownerDocument === null ? "N" : "x") . "|";
$comment = new DOMComment("v");
echo get_class($comment) . ":" . $comment->nodeName;
echo ":" . var_export($comment->nodeValue, true);
echo ":" . var_export($comment->textContent, true);
echo ":" . ($comment->ownerDocument === null ? "N" : "x") . "|";
$cdata = new DOMCdataSection("v");
echo get_class($cdata) . ":" . $cdata->nodeName;
echo ":" . var_export($cdata->nodeValue, true);
echo ":" . var_export($cdata->textContent, true);
echo ":" . ($cdata->ownerDocument === null ? "N" : "x") . "|";
$instruction = new DOMProcessingInstruction("pi", "v");
echo get_class($instruction) . ":" . $instruction->nodeName;
echo ":" . var_export($instruction->nodeValue, true);
echo ":" . var_export($instruction->textContent, true);
echo ":" . ($instruction->ownerDocument === null ? "N" : "x") . "|";
$reference = new DOMEntityReference("amp");
echo get_class($reference) . ":" . $reference->nodeName;
echo ":" . var_export($reference->nodeValue, true);
echo ":" . var_export($reference->textContent, true);
echo ":" . ($reference->ownerDocument === null ? "N" : "x") . "|";
$fragment = new DOMDocumentFragment();
echo get_class($fragment) . ":" . $fragment->nodeName;
echo ":" . var_export($fragment->nodeValue, true);
echo ":" . var_export($fragment->textContent, true);
echo ":" . ($fragment->ownerDocument === null ? "N" : "x") . "|";
"#,
    );
    assert_eq!(
        output,
        "DOMElement:p:e:'v':'v':N|DOMAttr:a:'v':'v':N|DOMText:#text:'v':'v':N|DOMComment:#comment:'v':'v':N|DOMCdataSection:#cdata-section:'v':'v':N|DOMProcessingInstruction:pi:'v':'v':N|DOMEntityReference:amp:NULL:'&':N|DOMDocumentFragment:#document-fragment:NULL:'':N|"
    );
}

/// Verifies first attachment adopts directly constructed nodes into a legacy document.
#[test]
fn legacy_direct_nodes_adopt_on_first_attachment_like_php() {
    let output = compile_and_run(
        r#"<?php
$document = new DOMDocument();
$element = new DOMElement("root");
$attribute = new DOMAttr("id", "x");
$text = new DOMText("value");
$document->appendChild($element);
$element->setAttributeNode($attribute);
$element->appendChild($text);
echo ($element->ownerDocument === $document ? "E" : "x");
echo ($attribute->ownerDocument === $document ? "A" : "x");
echo ($text->ownerDocument === $document ? "T" : "x");
echo ":" . $document->saveXML($element);
"#,
    );
    assert_eq!(output, "EAT:<root id=\"x\">value</root>");
}

/// Verifies direct constructor name failures preserve PHP's DOMException class and code.
#[test]
fn legacy_direct_node_constructor_errors_match_php() {
    let output = compile_and_run(
        r#"<?php
try {
    new DOMElement("1bad");
} catch (DOMException $error) {
    echo "element:" . get_class($error) . ":" . $error->getCode() . ":" . $error->getMessage();
}
try {
    new DOMElement("p:a", null, "");
} catch (DOMException $error) {
    echo "|namespace:" . get_class($error) . ":" . $error->getCode() . ":" . $error->getMessage();
}
try {
    new DOMAttr("1bad");
} catch (DOMException $error) {
    echo "|attribute:" . get_class($error) . ":" . $error->getCode() . ":" . $error->getMessage();
}
try {
    new DOMProcessingInstruction("1bad", "x");
} catch (DOMException $error) {
    echo "|instruction:" . get_class($error) . ":" . $error->getCode() . ":" . $error->getMessage();
}
try {
    new DOMEntityReference("a b");
} catch (DOMException $error) {
    echo "|reference:" . get_class($error) . ":" . $error->getCode() . ":" . $error->getMessage();
}
"#,
    );
    assert_eq!(
        output,
        "element:DOMException:5:Invalid Character Error|namespace:DOMException:14:Namespace Error|attribute:DOMException:5:Invalid Character Error|instruction:DOMException:5:Invalid Character Error|reference:DOMException:5:Invalid Character Error"
    );
}

/// Verifies manual legacy constructors preserve wrapper identity and prior attached graphs.
#[test]
fn legacy_manual_constructors_replace_native_resources_and_are_heap_clean() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
$document = new DOMDocument();
$document->loadXML('<container/>');
$root = $document->documentElement;
$element = new DOMElement('old', 'one');
$root->appendChild($element);
$element->__construct('new', 'two');
echo get_class($element), ':', $element->nodeName, ':';
echo $element->ownerDocument === null ? 'N' : 'x', ':';
echo $document->saveXML($root), '|';
$root->appendChild($element);
echo $document->saveXML($root), '|';

$oldRoot = $root;
$document->__construct('1.1', 'UTF-8');
echo $document->saveXML(), ':';
echo $oldRoot->nodeName, ':';
echo $oldRoot->ownerDocument === $document ? 'x' : 'O';
"#,
    );
    assert!(
        output.success,
        "program failed: stdout={} stderr={}",
        output.stdout,
        output.stderr
    );
    assert_eq!(
        output.stdout,
        concat!(
            "DOMElement:new:N:<container><old>one</old></container>|",
            "<container><old>one</old><new>two</new></container>|",
            "<?xml version=\"1.1\" encoding=\"UTF-8\"?>\n:container:O",
        )
    );
    assert!(
        output.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected manual constructor replacement to remain heap-clean, got: {}",
        output.stderr
    );
}

/// Verifies empty legacy fragments warn once, return false, and stay detached.
#[test]
fn legacy_empty_fragment_append_warning_matches_php_and_is_heap_clean() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
$document = new DOMDocument();
$document->loadXML('<root/>');
$root = $document->documentElement;
$fragment = new DOMDocumentFragment();
var_dump($root->appendChild($fragment));
var_dump(@$root->appendChild($fragment));
echo $fragment->ownerDocument === null ? 'N' : 'x';
$modern = Dom\XMLDocument::createFromString('<root/>');
$modernRoot = $modern->documentElement;
$modernFragment = $modern->createDocumentFragment();
var_dump($modernRoot->appendChild($modernFragment) === $modernFragment);
unset($modernFragment, $modernRoot, $modern, $fragment, $root, $document);
"#,
    );
    assert!(
        output.success,
        "program failed: stdout={} stderr={}",
        output.stdout,
        output.stderr
    );
    assert_eq!(output.stdout, "bool(false)\nbool(false)\nNbool(true)\n");
    let warning = "Warning: DOMNode::appendChild(): Document Fragment is empty";
    assert_eq!(
        output.stderr.matches(warning).count(),
        1,
        "expected one unsuppressed empty-fragment warning, got: {}",
        output.stderr
    );
    assert!(
        output.stderr.contains(" on line 6\n"),
        "expected PHP call-site location, got: {}",
        output.stderr
    );
    assert!(
        output.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected empty-fragment warning path to remain heap-clean, got: {}",
        output.stderr
    );
}

/// Verifies JSON ignores virtual native fields but retains user descendant storage.
#[test]
fn dom_native_wrapper_json_encoding_is_safe_and_heap_clean() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
class VisibleDocument extends DOMDocument {
    public string $label = 'x';
}

$native = new DOMDocument();
$visible = new VisibleDocument();
$visible->label = 'x';
echo json_encode($native), '|', json_encode($visible);
unset($visible, $native);
"#,
    );
    assert!(
        output.success,
        "program failed: stdout={} stderr={}",
        output.stdout,
        output.stderr
    );
    assert_eq!(output.stdout, "{}|{\"label\":\"x\"}");
    assert!(
        output.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected native-wrapper JSON encoding to remain heap-clean, got: {}",
        output.stderr
    );
}

/// Verifies stateless implementation wrappers, identity, features, and detached doctypes.
#[test]
fn dom_implementation_basics_match_php() {
    let output = compile_and_run(
        r#"<?php
$implementation = new DOMImplementation();
echo $implementation->hasFeature("XML", "2.0") ? "X" : "x";
echo $implementation->hasFeature("Core", "1.0") ? "C" : "x";
echo $implementation->hasFeature("Core", "2.0") ? "x" : "N";
$legacy = new DOMDocument();
echo ":" . get_class($legacy->implementation);
echo ":" . ($legacy->implementation === $legacy->implementation ? "same" : "new");
$legacyType = $implementation->createDocumentType("root", "pub", "sys");
echo ":" . get_class($legacyType) . ":" . $legacyType->name;
echo ":" . $legacyType->publicId . ":" . $legacyType->systemId;
echo ":" . ($legacyType->ownerDocument === null ? "N" : "x");
try {
    $legacyType->entities;
} catch (Throwable $error) {
    echo ":" . get_class($error) . ":" . $error->getCode() . ":" . $error->getMessage();
}
try {
    $legacyType->notations;
} catch (Throwable $error) {
    echo ":" . get_class($error) . ":" . $error->getCode() . ":" . $error->getMessage();
}

$modernImplementation = new Dom\Implementation();
$modern = Dom\XMLDocument::createEmpty();
echo "|" . get_class($modern->implementation);
echo ":" . ($modern->implementation === $modern->implementation ? "same" : "new");
$modernType = $modernImplementation->createDocumentType("root", "pub", "sys");
echo ":" . get_class($modernType) . ":" . $modernType->name;
echo ":" . $modernType->publicId . ":" . $modernType->systemId;
echo ":" . ($modernType->ownerDocument === null ? "N" : "x");
try {
    $modernType->entities;
} catch (Throwable $error) {
    echo ":" . get_class($error) . ":" . $error->getCode() . ":" . $error->getMessage();
}
try {
    $modernType->notations;
} catch (Throwable $error) {
    echo ":" . get_class($error) . ":" . $error->getCode() . ":" . $error->getMessage();
}
"#,
    );
    assert_eq!(
        output,
        "XCN:DOMImplementation:new:DOMDocumentType:root:pub:sys:N:DOMException:11:Invalid State Error:DOMException:11:Invalid State Error|Dom\\Implementation:same:Dom\\DocumentType:root:pub:sys:N:DOMException:11:Invalid State Error:DOMException:11:Invalid State Error"
    );
}

/// Verifies a directly allocated stateless implementation can enter the native bridge.
#[test]
fn direct_dom_implementation_feature_probe_matches_php() {
    assert_eq!(
        compile_and_run(
            r#"<?php
$implementation = new DOMImplementation();
echo $implementation->hasFeature("XML", "2.0") ? "X" : "x";
"#,
        ),
        "X"
    );
}

/// Verifies dynamic wrapper allocation preserves hidden metadata across repeated native calls.
#[test]
fn dynamic_dom_implementation_allocation_matches_php() {
    assert_eq!(
        compile_and_run(
            r#"<?php
$class = DOMImplementation::class;
$implementation = new $class();
echo get_class($implementation);
"#,
        ),
        "DOMImplementation"
    );
}

/// Verifies legacy implementation properties expose fresh stateless wrappers.
#[test]
fn legacy_dom_implementation_property_identity_matches_php() {
    assert_eq!(
        compile_and_run(
            r#"<?php
$document = new DOMDocument();
echo get_class($document->implementation);
echo ":" . ($document->implementation === $document->implementation ? "same" : "new");
"#,
        ),
        "DOMImplementation:new"
    );
}

/// Verifies direct implementation factories materialize detached document types.
#[test]
fn direct_dom_implementation_doctype_factory_matches_php() {
    assert_eq!(
        compile_and_run(
            r#"<?php
$legacy = (new DOMImplementation())->createDocumentType("root", "pub", "sys");
echo get_class($legacy) . ":" . $legacy->name . ":" . $legacy->publicId;
echo ":" . ($legacy->ownerDocument === null ? "N" : "x");
"#,
        ),
        "DOMDocumentType:root:pub:N"
    );
}

/// Verifies the modern implementation factory materializes a detached document type.
#[test]
fn direct_modern_dom_implementation_doctype_factory_matches_php() {
    assert_eq!(
        compile_and_run(
            r#"<?php
$modern = (new Dom\Implementation())->createDocumentType("root", "pub", "sys");
echo get_class($modern) . ":" . $modern->name . ":" . $modern->systemId;
echo ":" . ($modern->ownerDocument === null ? "N" : "x");
"#,
        ),
        "Dom\\DocumentType:root:sys:N"
    );
}

/// Verifies legacy and modern doctype factories preserve their distinct QName errors.
#[test]
fn dom_implementation_doctype_factory_errors_match_php() {
    assert_eq!(
        compile_and_run(
            r#"<?php
try {
    (new DOMImplementation())->createDocumentType("");
} catch (Throwable $error) {
    echo get_class($error) . ":" . $error->getCode() . ":" . $error->getMessage();
}
$accepted = (new DOMImplementation())->createDocumentType("a b");
echo "|legacy:" . $accepted->name;
foreach (["", "a b"] as $name) {
    try {
        (new Dom\Implementation())->createDocumentType($name, "", "");
    } catch (Throwable $error) {
        echo "|" . get_class($error) . ":" . $error->getCode() . ":" . $error->getMessage();
    }
}
"#,
        ),
        "ValueError:0:DOMImplementation::createDocumentType(): Argument #1 ($qualifiedName) must not be empty|legacy:a b|DOMException:14:Namespace Error|DOMException:14:Namespace Error"
    );
}

/// Verifies modern document implementation properties preserve wrapper identity.
#[test]
fn modern_dom_implementation_property_identity_matches_php() {
    assert_eq!(
        compile_and_run(
            r#"<?php
$document = Dom\XMLDocument::createEmpty();
echo get_class($document->implementation);
echo ":" . ($document->implementation === $document->implementation ? "same" : "new");
"#,
        ),
        "Dom\\Implementation:same"
    );
}

/// Verifies legacy implementation document creation preserves doctype identity and errors.
#[test]
fn legacy_dom_implementation_document_factory_matches_php() {
    let output = compile_and_run(
        r#"<?php
$implementation = new DOMImplementation();
$doctype = $implementation->createDocumentType("root", "pub", "sys");
if ($doctype === false) { exit(2); }
$document = $implementation->createDocument("urn:test", "p:root", $doctype);
echo get_class($document) . ":" . $document->documentElement->nodeName;
echo ":" . ($document->doctype === $doctype ? "same" : "new");
echo ":" . ($doctype->ownerDocument === $document ? "owner" : "lost");
echo ":" . ($doctype->parentNode === $document ? "parent" : "lost");
echo "|" . $document->saveXML();
$unbound = $implementation->createDocument(null, "p:root");
echo "|" . $unbound->documentElement->nodeName . ":";
echo $unbound->documentElement->namespaceURI === null ? "N" : "x";
try {
    $implementation->createDocument(null, "again", $doctype);
} catch (Throwable $error) {
    echo "|" . get_class($error) . ":" . $error->getCode() . ":" . $error->getMessage();
}
"#,
    );
    assert_eq!(
        output,
        "DOMDocument:p:root:same:owner:parent|<?xml version=\"1.0\"?>\n<!DOCTYPE root PUBLIC \"pub\" \"sys\">\n<p:root xmlns:p=\"urn:test\"/>\n|root:N|DOMException:4:Wrong Document Error"
    );
}

/// Verifies modern implementation document creation auto-adopts a doctype between documents.
#[test]
fn modern_dom_implementation_document_factory_matches_php() {
    let output = compile_and_run(
        r#"<?php
$implementation = new Dom\Implementation();
$doctype = $implementation->createDocumentType("root", "", "");
$first = $implementation->createDocument(null, "first", $doctype);
$second = $implementation->createDocument("urn:test", "p:second", $doctype);
echo get_class($second) . ":";
echo $first->doctype === null ? "moved" : "stale";
echo ":" . ($second->doctype === $doctype ? "same" : "new");
echo ":" . ($doctype->ownerDocument === $second ? "owner" : "lost");
echo "|" . $first->saveXml();
echo "|" . $second->saveXml();
try {
    $implementation->createDocument(null, "bad name");
} catch (DOMException $error) {
    echo "|" . $error->getCode() . ":" . $error->getMessage();
}
try {
    $implementation->createDocument(null, "p:root");
} catch (DOMException $error) {
    echo "|" . $error->getCode() . ":" . $error->getMessage();
}
"#,
    );
    assert_eq!(
        output,
        "Dom\\XMLDocument:moved:same:owner|<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<first/>|<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE root>\n<p:second xmlns:p=\"urn:test\"/>|5:Invalid Character Error|14:Namespace Error"
    );
}

/// Verifies modern HTML implementation creation reproduces PHP's exact initial XML tree.
#[test]
fn modern_dom_implementation_html_factory_matches_php() {
    let output = compile_and_run(
        r#"<?php
$implementation = new Dom\Implementation();
$none = $implementation->createHTMLDocument();
$empty = $implementation->createHTMLDocument("");
$title = $implementation->createHTMLDocument("A < B & C");
echo get_class($none) . "|" . $none->saveXml();
echo "|" . $empty->saveXml();
echo "|" . $title->saveXml();
$associated = Dom\HTMLDocument::createEmpty()->implementation;
$associatedType = $associated->createDocumentType("root", "", "");
$associatedXml = $associated->createDocument(null, "root", $associatedType);
echo "|" . get_class($associatedXml) . ":" . get_class($associated->createHTMLDocument());
"#,
    );
    assert_eq!(
        output,
        "Dom\\HTMLDocument|<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\"><head></head><body></body></html>|<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title></title></head><body></body></html>|<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>A &lt; B &amp; C</title></head><body></body></html>|Dom\\XMLDocument:Dom\\HTMLDocument"
    );
}

/// Verifies modern document encoding, head, and body metadata match PHP.
#[test]
fn modern_document_encoding_head_and_body_match_php() {
    let output = compile_and_run(
        r#"<?php
$document = (new Dom\Implementation())->createHTMLDocument();
$document->characterSet = "latin1";
echo $document->characterSet . ":" . $document->charset . ":" . $document->inputEncoding;
echo "|" . get_class($document->head) . ":" . $document->head->nodeName;
echo ":" . get_class($document->body) . ":" . $document->body->nodeName;
$same = $document->body;
$document->body = $same;
echo ":" . ($document->body === $same ? "same" : "lost");
$replacement = $document->createElement("FRAMESET");
echo "|" . get_class($replacement) . ":" . $replacement->nodeName . ":" . $replacement->localName;
$document->body = $replacement;
echo ":" . ($document->body === $replacement ? "same" : "lost");
echo ":" . ($same->parentNode === null ? "detached" : "attached");
echo ":" . ($same->ownerDocument === $document ? "owner" : "lost");
echo "|" . $document->saveXml();
try {
    $document->body = null;
} catch (DOMException $error) {
    echo "|" . $error->getCode() . ":" . $error->getMessage();
}
$empty = Dom\HTMLDocument::createEmpty();
try {
    $empty->body = $document->createElement("body");
} catch (DOMException $error) {
    echo "|" . $error->getCode() . ":" . $error->getMessage();
}
try {
    $document->charset = "x-invalid";
} catch (ValueError $error) {
    echo "|" . get_class($error) . ":" . $error->getMessage();
}
"#,
    );
    assert_eq!(
        output,
        "windows-1252:windows-1252:windows-1252|Dom\\HTMLElement:HEAD:Dom\\HTMLElement:BODY:same|Dom\\HTMLElement:FRAMESET:frameset:same:detached:owner|<?xml version=\"1.0\" encoding=\"windows-1252\" standalone=\"yes\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\"><head></head><frameset></frameset></html>|3:The new body must either be a body or a frameset tag|3:A body can only be set if there is a document element|ValueError:Invalid document encoding"
    );
}

/// Verifies body assignment distinguishes HTML namespace class from local name.
#[test]
fn modern_document_body_namespace_rules_match_php() {
    let output = compile_and_run(
        r#"<?php
$document = (new Dom\Implementation())->createHTMLDocument();
$prefixed = $document->createElementNS(
    "http://www.w3.org/1999/xhtml",
    "prefix:body",
);
$document->body = $prefixed;
echo $document->body->nodeName;

try {
    $document->body = $document->createElementNS("urn:a", "body");
} catch (Throwable $error) {
    echo "|" . $error->getMessage();
}

$svg = (new Dom\Implementation())->createDocument(
    "http://www.w3.org/2000/svg",
    "svg",
);
$svg->body = $svg->createElementNS(
    "http://www.w3.org/1999/xhtml",
    "body",
);
echo "|" . ($svg->body === null ? "null" : "body");
"#,
    );
    assert_eq!(
        output,
        "PREFIX:BODY|Cannot assign Dom\\Element to property Dom\\Document::$body of type ?Dom\\HTMLElement|null"
    );
}

/// Verifies HTML and SVG title algorithms preserve PHP's direct-text rules.
#[test]
fn modern_document_title_metadata_matches_php() {
    let output = compile_and_run(
        r#"<?php
$document = (new Dom\Implementation())->createHTMLDocument();
echo "[" . $document->title . "]";
$document->title = "  A \t B\n C  ";
echo "|[" . $document->title . "]";
$title = $document->head->firstElementChild;
$title->appendChild($document->createElement("span"));
$title->appendChild($document->createTextNode(" D "));
echo "|[" . $document->title . "]";
$document->title = "";
echo "|[" . $document->title . "]:" . $title->childNodes->length;
echo "|" . $document->saveXml();

$implementation = new Dom\Implementation();
$svg = $implementation->createDocument("http://www.w3.org/2000/svg", "svg");
$svg->title = " S  V\tG ";
echo "|[" . $svg->title . "]:" . $svg->saveXml();
"#,
    );
    assert_eq!(
        output,
        "[]|[A B C]|[A B C D]|[]:1|<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title></title></head><body></body></html>|[S V G]:<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\"><title> S  V\tG </title></svg>"
    );
}

/// Verifies title replacement retains old wrappers and handles prefixed SVG roots.
#[test]
fn modern_document_title_wrapper_and_prefix_rules_match_php() {
    let output = compile_and_run(
        r#"<?php
$html = (new Dom\Implementation())->createHTMLDocument();
$html->title = "old";
$title = $html->head->firstElementChild;
$old = $title->firstChild;
$html->title = "new";
echo ($old->parentNode === null ? "detached" : "attached");
echo ":" . $old->nodeValue;
echo ":" . ($old->ownerDocument === $html ? "owner" : "lost");

$svg = (new Dom\Implementation())->createDocument(
    "http://www.w3.org/2000/svg",
    "svg:svg",
);
$svg->title = "test";
echo "|" . $svg->saveXml();
$created = $svg->documentElement->firstElementChild;
echo "|" . ($created->prefix === null ? "null" : $created->prefix);
echo ":" . $created->namespaceURI;
"#,
    );
    assert_eq!(
        output,
        "detached:old:owner|<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg:svg xmlns:svg=\"http://www.w3.org/2000/svg\"><svg:title>test</svg:title></svg:svg>|null:http://www.w3.org/2000/svg"
    );
}

/// Verifies concrete modern element classes derive from namespace rather than document class.
#[test]
fn modern_element_wrapper_class_follows_namespace() {
    let output = compile_and_run(
        r#"<?php
$implementation = new Dom\Implementation();
$xml = $implementation->createDocument(
    "http://www.w3.org/1999/xhtml",
    "html",
);
$head = $xml->createElementNS(
    "http://www.w3.org/1999/xhtml",
    "head",
);
$body = $xml->createElementNS(
    "http://www.w3.org/1999/xhtml",
    "body",
);
$xml->documentElement->append($head, $body);
echo get_class($xml->head) . ":" . get_class($xml->body);

$html = $implementation->createHTMLDocument();
$svg = $html->createElementNS("http://www.w3.org/2000/svg", "svg");
$html->body->append($svg);
echo "|" . get_class($html->body->lastElementChild);
"#,
    );
    assert_eq!(
        output,
        "Dom\\HTMLElement:Dom\\HTMLElement|Dom\\Element"
    );
}

/// Verifies document import, adoption, ID lookup, and descendant wrapper rehoming.
#[test]
fn document_import_adopt_and_id_lookup_match_php() {
    let legacy = compile_and_run(
        r#"<?php
$source = new DOMDocument();
$source->loadXML("<r><x xml:id=\"id\"><y/></x></r>");
$target = new DOMDocument();
$target->loadXML("<q/>");
$sourceRoot = $source->documentElement;
$targetRoot = $target->documentElement;
if ($sourceRoot === null) { exit(2); }
if ($targetRoot === null) { exit(3); }
$node = $sourceRoot->firstChild;
if ($node === null) { exit(4); }
$descendant = $node->firstChild;
if ($descendant === null) { exit(5); }
$found = $source->getElementById("id");
echo "id:" . ($found === $node ? "I" : "x");
$shallow = $target->importNode($node);
$deep = $target->importNode($node, true);
if ($shallow === false) { exit(6); }
if ($deep === false) { exit(7); }
echo "|import:" . get_class($shallow) . ":" . $shallow->childNodes->length;
echo ":" . $deep->childNodes->length;
echo ":" . ($shallow->ownerDocument === $target ? "O" : "x");
echo ":" . ($target->importNode($targetRoot) === $targetRoot ? "I" : "x");
$adopted = $target->adoptNode($node);
if ($adopted === false) { exit(8); }
echo "|adopt:" . ($adopted === $node ? "I" : "x");
echo ":" . ($descendant->ownerDocument === $target ? "Y" : "x");
echo ":" . ($node->parentNode === null ? "D" : "x");
echo ":" . $source->saveXML($sourceRoot);
$targetRoot->appendChild($node);
echo "|" . $target->saveXML($targetRoot);
"#,
    );
    assert_eq!(
        legacy,
        "id:I|import:DOMElement:0:1:O:I|adopt:I:Y:D:<r/>|<q><x xml:id=\"id\"><y/></x></q>"
    );

    let modern = compile_and_run(
        r#"<?php
$source = Dom\XMLDocument::createFromString("<r><x xml:id=\"id\"><y/></x></r>");
$target = Dom\XMLDocument::createFromString("<q/>");
$sourceRoot = $source->documentElement;
$targetRoot = $target->documentElement;
if ($sourceRoot === null) { exit(2); }
if ($targetRoot === null) { exit(3); }
$node = $sourceRoot->firstChild;
if ($node === null) { exit(4); }
$descendant = $node->firstChild;
if ($descendant === null) { exit(5); }
$found = $source->getElementById("id");
echo "id:" . ($found === $node ? "I" : "x");
$shallow = $target->importNode($node);
$deep = $target->importNode($node, true);
echo "|import:" . get_class($shallow) . ":" . $shallow->childNodes->length;
echo ":" . $deep->childNodes->length;
echo ":" . ($shallow->ownerDocument === $target ? "O" : "x");
echo ":" . ($target->importNode($targetRoot) === $targetRoot ? "I" : "x");
$adopted = $target->adoptNode($node);
echo "|adopt:" . ($adopted === $node ? "I" : "x");
echo ":" . ($descendant->ownerDocument === $target ? "Y" : "x");
echo ":" . ($node->parentNode === null ? "D" : "x");
echo ":" . $source->saveXML($sourceRoot);
$targetRoot->appendChild($node);
echo "|" . $target->saveXML($targetRoot);
try {
    $target->importNode($source);
    echo "|x";
} catch (DOMException $error) {
    echo "|" . $error->getCode() . ":" . $error->getMessage();
}
"#,
    );
    assert_eq!(
        modern,
        "id:I|import:Dom\\Element:0:1:O:I|adopt:I:Y:D:<r/>|<q><x xml:id=\"id\"><y/></x></q>|9:Not Supported Error"
    );
}

/// Verifies navigation materializes parsed legacy nodes with their concrete PHP classes.
#[test]
fn legacy_parsed_navigation_materializes_concrete_node_classes() {
    let out = compile_and_run(
        r#"<?php
$document = new DOMDocument();
if (!$document->loadXML("<root><!--c--><child/>text</root>")) { exit(2); }
$root = $document->documentElement;
if ($root === null) { exit(3); }
$comment = $root->firstChild;
if ($comment === null) { exit(4); }
$child = $comment->nextSibling;
if ($child === null) { exit(5); }
$text = $child->nextSibling;
if ($text === null) { exit(6); }
echo get_class($comment) . "|" . get_class($child) . "|" . get_class($text);
"#,
    );
    assert_eq!(out, "DOMComment|DOMElement|DOMText");
}

/// Verifies navigation materializes parsed modern nodes with their concrete PHP classes.
#[test]
fn modern_parsed_navigation_materializes_concrete_node_classes() {
    let out = compile_and_run(
        r#"<?php
$document = Dom\XMLDocument::createFromString("<root><!--c--><child/>text</root>");
$root = $document->documentElement;
if ($root === null) { exit(3); }
$comment = $root->firstChild;
if ($comment === null) { exit(4); }
$child = $comment->nextSibling;
if ($child === null) { exit(5); }
$text = $child->nextSibling;
if ($text === null) { exit(6); }
echo get_class($comment) . "|" . get_class($child) . "|" . get_class($text);
"#,
    );
    assert_eq!(out, "Dom\\Comment|Dom\\Element|Dom\\Text");
}

/// Verifies legacy HTML4 string loading and document/subtree serialization match PHP.
#[test]
fn legacy_html_string_parsing_and_serialization_match_php() {
    let out = compile_and_run(
        r#"<?php
$document = new DOMDocument();
echo $document->loadHTML("<!doctype html><title>T</title><p id=x>A&amp;B<br><svg><path/></svg>") ? "T|" : "F|";
echo $document->saveHTML();
$root = $document->documentElement;
if (!$root instanceof DOMElement) { exit(2); }
echo "--|" . $document->saveHTML($root);
try {
    $document->loadHTML("");
} catch (ValueError $error) {
    echo "--|" . get_class($error) . ":" . $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "T|<!DOCTYPE html>\n<html><head><title>T</title></head><body><p id=\"x\">A&amp;B<br><svg><path></path></svg></p></body></html>\n--|<html><head><title>T</title></head><body><p id=\"x\">A&amp;B<br><svg><path></path></svg></p></body></html>--|ValueError:DOMDocument::loadHTML(): Argument #1 ($source) must not be empty"
    );
}

/// Verifies legacy and modern DOM file routes plus base `Exception` translation.
#[test]
fn document_file_round_trips_and_exceptions_match_php() {
    let out = compile_and_run(
        r#"<?php
$xmlInput = "dom-file-input.xml";
$htmlInput = "dom-file-input.html";
$modernXmlOutput = "dom-file-modern.xml";
$modernHtmlOutput = "dom-file-modern.html";
$modernHtmlXmlOutput = "dom-file-modern-html.xml";
$legacyXmlOutput = "dom-file-legacy.xml";
$legacyHtmlOutput = "dom-file-legacy.html";
file_put_contents($xmlInput, "<root><empty/></root>");
file_put_contents($htmlInput, "<!doctype html><title>T</title><p>A<br>");

$xml = Dom\XMLDocument::createFromFile($xmlInput);
echo $xml->saveXmlFile($modernXmlOutput);
echo ":" . file_get_contents($modernXmlOutput);

$html = Dom\HTMLDocument::createFromFile($htmlInput);
echo "|" . $html->saveHtmlFile($modernHtmlOutput);
echo ":" . file_get_contents($modernHtmlOutput);
echo "|" . $html->saveXmlFile($modernHtmlXmlOutput);
echo ":" . file_get_contents($modernHtmlXmlOutput);

$legacy = new DOMDocument();
echo "|" . ($legacy->load($xmlInput) ? "X" : "x");
echo ":" . $legacy->save($legacyXmlOutput);
echo ":" . file_get_contents($legacyXmlOutput);
echo "|" . ($legacy->loadHTMLFile($htmlInput) ? "H" : "x");
echo ":" . $legacy->saveHTMLFile($legacyHtmlOutput);
echo ":" . file_get_contents($legacyHtmlOutput);

try {
    Dom\XMLDocument::createFromFile("dom-file-missing.xml");
} catch (Exception $error) {
    echo "|" . get_class($error) . ":" . $error->getMessage();
}

unlink($xmlInput);
unlink($htmlInput);
unlink($modernXmlOutput);
unlink($modernHtmlOutput);
unlink($modernHtmlXmlOutput);
unlink($legacyXmlOutput);
unlink($legacyHtmlOutput);
"#,
    );
    assert_eq!(
        out,
        "60:<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<root><empty/></root>|82:<!DOCTYPE html><html><head><title>T</title></head><body><p>A<br></p></body></html>|178:<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>T</title></head><body><p>A<br /></p></body></html>|X:44:<?xml version=\"1.0\"?>\n<root><empty/></root>\n|H:84:<!DOCTYPE html>\n<html><head><title>T</title></head><body><p>A<br></p></body></html>\n|Exception:Cannot open file 'dom-file-missing.xml'"
    );
}

/// Verifies modern XML source validation and default UTF-8 metadata match PHP.
#[test]
fn modern_xml_string_options_and_encoding_match_php() {
    let out = compile_and_run(
        r#"<?php
try {
    Dom\XMLDocument::createFromString("");
} catch (ValueError $error) {
    echo get_class($error) . ":" . $error->getMessage() . "|";
}
try {
    Dom\XMLDocument::createFromString("<r/>", Dom\HTML_NO_DEFAULT_NS);
} catch (ValueError $error) {
    echo get_class($error) . ":" . $error->getMessage() . "|";
}
try {
    Dom\XMLDocument::createFromString("<r/>", 0, "bad");
} catch (ValueError $error) {
    echo get_class($error) . ":" . $error->getMessage() . "|";
}
$document = Dom\XMLDocument::createFromString("<r/>");
echo $document->characterSet . ":" . $document->saveXml();
"#,
    );
    assert_eq!(
        out,
        "ValueError:Dom\\XMLDocument::createFromString(): Argument #1 ($source) must not be empty|ValueError:Dom\\XMLDocument::createFromString(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_RECOVER, LIBXML_NOENT, LIBXML_NO_XXE, LIBXML_DTDLOAD, LIBXML_DTDATTR, LIBXML_DTDVALID, LIBXML_NOERROR, LIBXML_NOWARNING, LIBXML_NOBLANKS, LIBXML_XINCLUDE, LIBXML_NSCLEAN, LIBXML_NOCDATA, LIBXML_NONET, LIBXML_PEDANTIC, LIBXML_COMPACT, LIBXML_PARSEHUGE, LIBXML_BIGLINES)|ValueError:Dom\\XMLDocument::createFromString(): Argument #3 ($overrideEncoding) must be a valid document encoding|UTF-8:<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<r/>"
    );
}

/// Verifies modern HTML5 parsing, namespace wrappers, and serialization match PHP.
#[test]
fn modern_html_string_parsing_and_serialization_match_php() {
    let out = compile_and_run(
        r#"<?php
$document = Dom\HTMLDocument::createFromString(
    "<!doctype html><title>T</title><p id=x>A&nbsp;<br><svg><title>S</title></svg>",
    LIBXML_NOERROR
);
echo $document->saveHtml();
echo "|" . get_class($document->documentElement);
echo ":" . get_class($document->body);
echo ":" . get_class($document->body->firstElementChild->lastElementChild);
echo "|" . $document->title;
echo "|" . $document->characterSet;
echo "|" . $document->saveHtml($document->body);
"#,
    );
    assert_eq!(
        out,
        "<!DOCTYPE html><html><head><title>T</title></head><body><p id=\"x\">A&nbsp;<br><svg><title>S</title></svg></p></body></html>|Dom\\HTMLElement:Dom\\HTMLElement:Dom\\Element|T|UTF-8|<body><p id=\"x\">A&nbsp;<br><svg><title>S</title></svg></p></body>"
    );
}

/// Verifies modern HTML parser options and override-encoding validation match PHP.
#[test]
fn modern_html_parser_options_match_php() {
    let out = compile_and_run(
        r#"<?php
$withoutNamespace = Dom\HTMLDocument::createFromString(
    "<p>x",
    LIBXML_NOERROR | Dom\HTML_NO_DEFAULT_NS
);
echo $withoutNamespace->saveHtml();
echo ":" . ($withoutNamespace->documentElement->namespaceURI === null ? "N" : "x");
$withoutImplied = Dom\HTMLDocument::createFromString(
    "<p>x",
    LIBXML_NOERROR | LIBXML_HTML_NOIMPLIED
);
echo "|" . $withoutImplied->saveHtml();
$encoded = Dom\HTMLDocument::createFromString(
    "<p>x",
    LIBXML_NOERROR,
    "latin1"
);
echo "|" . $encoded->characterSet;
try {
    Dom\HTMLDocument::createFromString("<p>x", 1);
    echo "|x";
} catch (ValueError $error) {
    echo "|" . $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "<html><head></head><body><p>x</p></body></html>:N|<p>x</p>|windows-1252|Dom\\HTMLDocument::createFromString(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\\HTML_NO_DEFAULT_NS)"
    );
}

/// Verifies modern HTML string and file serialization honor the document encoding.
#[test]
fn modern_html_serialization_uses_document_encoding() {
    let out = compile_and_run(
        r#"<?php
$document = Dom\HTMLDocument::createFromString("<!doctype html><p title=\"é € 😀\">é € 😀</p>");
$document->characterSet = "windows-1252";
$serialized = $document->saveHtml();
echo strlen($serialized) . ":" . bin2hex($serialized);
$path = "dom-encoded-output.html";
echo "|" . $document->saveHtmlFile($path);
echo ":" . bin2hex(file_get_contents($path));
unlink($path);
"#,
    );
    assert_eq!(
        out,
        "80:3c21444f43545950452068746d6c3e3c68746d6c3e3c686561643e3c2f686561643e3c626f64793e3c70207469746c653d22e92080203f223ee92080203f3c2f703e3c2f626f64793e3c2f68746d6c3e|80:3c21444f43545950452068746d6c3e3c68746d6c3e3c686561643e3c2f686561643e3c626f64793e3c70207469746c653d22e92080203f223ee92080203f3c2f703e3c2f626f64793e3c2f68746d6c3e"
    );
}

/// Verifies parsed HTML template contents stay hidden from child APIs but serialize.
#[test]
fn modern_html_template_content_matches_php() {
    let out = compile_and_run(
        r#"<?php
$document = Dom\HTMLDocument::createFromString(
    "<!doctype html><template><p>A</p><template><b>B</b></template></template>"
);
$template = $document->head->firstElementChild;
if (!$template instanceof Dom\HTMLElement) { exit(2); }
echo $template->childNodes->length;
echo ":" . strlen($template->textContent);
echo ":" . ($template->firstElementChild === null ? "N" : "x");
echo "|" . $document->saveHtml($template);
echo "|" . $document->saveXml($template);
echo "|" . $document->saveHtml();
"#,
    );
    assert_eq!(
        out,
        "0:0:N|<template><p>A</p><template><b>B</b></template></template>|<template xmlns=\"http://www.w3.org/1999/xhtml\"><p>A</p><template><b>B</b></template></template>|<!DOCTYPE html><html><head><template><p>A</p><template><b>B</b></template></template></head><body></body></html>"
    );
}

/// Verifies XML save flags match PHP for documents, subtrees, formats, and files.
#[test]
fn xml_serialization_options_match_php() {
    let out = compile_and_run(
        r#"<?php
$legacy = new DOMDocument();
$legacy->loadXML("<root><a/><br xmlns=\"http://www.w3.org/1999/xhtml\"/></root>");
echo "L0[" . str_replace("\n", "/", $legacy->saveXML()) . "]";
echo "L2[" . str_replace("\n", "/", $legacy->saveXML(options: LIBXML_NOXMLDECL)) . "]";
echo "L4[" . str_replace("\n", "/", $legacy->saveXML(options: LIBXML_NOEMPTYTAG)) . "]";
echo "LN4[" . $legacy->saveXML($legacy->documentElement, LIBXML_NOEMPTYTAG) . "]";

$modern = Dom\XMLDocument::createFromString(
    "<root><a/><br xmlns=\"http://www.w3.org/1999/xhtml\"/></root>"
);
echo "M0[" . str_replace("\n", "/", $modern->saveXml()) . "]";
echo "M2[" . str_replace("\n", "/", $modern->saveXml(options: LIBXML_NOXMLDECL)) . "]";
echo "M4[" . str_replace("\n", "/", $modern->saveXml(options: LIBXML_NOEMPTYTAG)) . "]";
echo "MN4[" . $modern->saveXml($modern->documentElement, LIBXML_NOEMPTYTAG) . "]";
$modern->formatOutput = true;
echo "MF4[" . str_replace("\n", "/", $modern->saveXml($modern, LIBXML_NOEMPTYTAG)) . "]";

$html = Dom\HTMLDocument::createFromString(
    "<!doctype html><html><body><div></div><br></body></html>"
);
echo "H2[" . str_replace("\n", "/", $html->saveXml(options: LIBXML_NOXMLDECL)) . "]";
echo "H4[" . str_replace("\n", "/", $html->saveXml(options: LIBXML_NOEMPTYTAG)) . "]";
echo "HN4[" . $html->saveXml($html->body, LIBXML_NOEMPTYTAG) . "]";

$path = "dom-save-options.xml";
$count = $modern->saveXmlFile($path, LIBXML_NOXMLDECL);
echo "FILE2[" . $count . ":" . str_replace("\n", "/", file_get_contents($path)) . "]";
$count = $modern->saveXmlFile($path, LIBXML_NOEMPTYTAG);
echo "FILE4[" . $count . ":" . str_replace("\n", "/", file_get_contents($path)) . "]";
unlink($path);
"#,
    );
    assert_eq!(
        out,
        "L0[<?xml version=\"1.0\"?>/<root><a/><br xmlns=\"http://www.w3.org/1999/xhtml\"/></root>/]L2[<root><a/><br xmlns=\"http://www.w3.org/1999/xhtml\"/></root>/]L4[<?xml version=\"1.0\"?>/<root><a></a><br xmlns=\"http://www.w3.org/1999/xhtml\"></br></root>/]LN4[<root><a></a><br xmlns=\"http://www.w3.org/1999/xhtml\"></br></root>]M0[<?xml version=\"1.0\" encoding=\"UTF-8\"?>/<root><a/><br xmlns=\"http://www.w3.org/1999/xhtml\" /></root>]M2[<root><a/><br xmlns=\"http://www.w3.org/1999/xhtml\" /></root>]M4[<?xml version=\"1.0\" encoding=\"UTF-8\"?>/<root><a></a><br xmlns=\"http://www.w3.org/1999/xhtml\"></br></root>]MN4[<root><a></a><br xmlns=\"http://www.w3.org/1999/xhtml\"></br></root>]MF4[<?xml version=\"1.0\" encoding=\"UTF-8\"?>/<root>/  <a></a>/  <br xmlns=\"http://www.w3.org/1999/xhtml\"></br>/</root>]H2[<!DOCTYPE html>/<html xmlns=\"http://www.w3.org/1999/xhtml\"><head></head><body><div></div><br /></body></html>]H4[<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>/<!DOCTYPE html>/<html xmlns=\"http://www.w3.org/1999/xhtml\"><head></head><body><div></div><br></br></body></html>]HN4[<body xmlns=\"http://www.w3.org/1999/xhtml\"><div></div><br></br></body>]FILE2[106:<?xml version=\"1.0\" encoding=\"UTF-8\"?>/<root>/  <a/>/  <br xmlns=\"http://www.w3.org/1999/xhtml\" />/</root>]FILE4[112:<?xml version=\"1.0\" encoding=\"UTF-8\"?>/<root>/  <a></a>/  <br xmlns=\"http://www.w3.org/1999/xhtml\"></br>/</root>]"
    );
}

/// Verifies the context-local libxml error controls and empty retrieval surface.
#[test]
fn libxml_error_mode_and_empty_state_match_php() {
    let out = compile_and_run(
        r#"<?php
echo libxml_use_internal_errors() ? "1" : "0";
echo libxml_use_internal_errors(true) ? "1" : "0";
echo libxml_use_internal_errors() ? "1" : "0";
echo count(libxml_get_errors());
echo libxml_get_last_error() === false ? "F" : "E";
echo libxml_clear_errors() === null ? "N" : "X";
echo libxml_use_internal_errors(false) ? "1" : "0";
"#,
    );
    assert_eq!(out, "0010FN1");
}

/// Verifies modern HTML parser diagnostics preserve PHP order, fields, and suppression.
#[test]
fn modern_html_parser_diagnostics_match_php() {
    let out = compile_and_run_capture(
        r#"<?php
libxml_use_internal_errors(true);
Dom\HTMLDocument::createFromString("<>x</> <!doctype html>");
$errors = libxml_get_errors();
echo count($errors);
foreach ($errors as $error) {
    echo "|" . $error->level;
    echo ":" . $error->code;
    echo ":" . $error->column;
    echo ":" . $error->message;
    echo ":" . $error->file;
    echo ":" . $error->line;
}
libxml_clear_errors();
Dom\HTMLDocument::createFromString("<>x</> <!doctype html>", LIBXML_NOERROR);
echo "|Q" . count(libxml_get_errors());
echo libxml_get_last_error() === false ? "F" : "E";
Dom\HTMLDocument::createFromString("<!doctype html>\n<p>é<>x");
$unicodeError = libxml_get_last_error();
echo "|U" . $unicodeError->line . ":" . $unicodeError->column;
libxml_clear_errors();
file_put_contents("dom-parser-warning.html", "<>x</> <!doctype html>");
Dom\HTMLDocument::createFromFile("dom-parser-warning.html");
$fileError = libxml_get_last_error();
echo "|F" . $fileError->column;
echo ":" . $fileError->message;
echo ":" . $fileError->file;
unlink("dom-parser-warning.html");
libxml_use_internal_errors(false);
Dom\HTMLDocument::createFromString("<>x</> <!doctype html>");
@Dom\HTMLDocument::createFromString("<>x</> <!doctype html>");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "4|2:1:2:tokenizer error invalid-first-character-of-tag-name in Entity, line: 1, column: 2:Entity:1|2:1:6:tokenizer error missing-end-tag-name in Entity, line: 1, column: 6:Entity:1|2:1:1:tree error unexpected-token-in-initial-mode in Entity, line: 1, column: 1-7:Entity:1|2:1:10:tree error doctype-token-in-body-mode in Entity, line: 1, column: 10-16:Entity:1|Q0F|U2:6|F10:tree error doctype-token-in-body-mode in dom-parser-warning.html, line: 1, column: 10-16:dom-parser-warning.html"
    );
    assert_eq!(
        out.stderr,
        "Warning: Dom\\HTMLDocument::createFromString(): tokenizer error invalid-first-character-of-tag-name in Entity, line: 1, column: 2\nWarning: Dom\\HTMLDocument::createFromString(): tokenizer error missing-end-tag-name in Entity, line: 1, column: 6\nWarning: Dom\\HTMLDocument::createFromString(): tree error unexpected-token-in-initial-mode in Entity, line: 1, column: 1-7\nWarning: Dom\\HTMLDocument::createFromString(): tree error doctype-token-in-body-mode in Entity, line: 1, column: 10-16\n"
    );
}

/// Verifies XML parser warnings, suppression flags, and internal errors match PHP.
#[test]
fn xml_parser_diagnostics_match_php() {
    let out = compile_and_run_capture(
        r#"<?php
$legacy = new DOMDocument();
echo $legacy->loadXML("<root>", LIBXML_RECOVER) ? "L" : "l";
$modern = Dom\XMLDocument::createFromString("<root>", LIBXML_RECOVER);
echo $modern->documentElement !== null ? "|M" : "|m";
try {
    Dom\XMLDocument::createFromString("<root>");
} catch (DOMException $error) {
    echo "|D";
}
echo $legacy->loadXML("<root>", LIBXML_NOERROR) ? "|q" : "|Q";

libxml_clear_errors();
libxml_use_internal_errors(true);
echo $legacy->loadXML("<root>", LIBXML_NOERROR) ? "|i" : "|I";
try {
    Dom\XMLDocument::createFromString("<root>", LIBXML_NOERROR);
} catch (DOMException $error) {
    echo "|" . get_class($error);
}
$errors = libxml_get_errors();
echo "|" . count($errors);
echo "|" . libxml_get_last_error()->code;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "L|M|D|Q|I|DOMException|2|77");
    assert_eq!(
        out.stderr,
        "Warning: DOMDocument::loadXML(): Premature end of data in tag root line 1 in Entity, line: 1\nWarning: Dom\\XMLDocument::createFromString(): Premature end of data in tag root line 1 in Entity, line: 1\nWarning: Dom\\XMLDocument::createFromString(): Premature end of data in tag root line 1 in Entity, line: 1\n"
    );
}

/// Verifies malformed XML produces PHP-visible `LibXMLError` values and ordered state.
#[test]
fn malformed_xml_materializes_libxml_error_objects() {
    let out = compile_and_run(
        r#"<?php
libxml_use_internal_errors(true);
$document = new DOMDocument();
echo $document->loadXML("<root>") ? "T" : "F";
$errors = libxml_get_errors();
echo "|" . count($errors);
$error = libxml_get_last_error();
if ($error === false) {
    exit(2);
}
echo "|" . $error->level;
echo "|" . $error->code;
echo "|" . $error->column;
echo "|" . $error->message;
echo "|" . $error->file;
echo "|" . $error->line;
"#,
    );
    assert_eq!(
        out,
        "F|1|3|77|7|Premature end of data in tag root line 1\n||1"
    );
}

/// Verifies `LibXMLError` remains an ordinary mutable and cloneable typed PHP object.
#[test]
fn libxml_error_value_objects_support_construction_mutation_and_clone() {
    let out = compile_and_run_capture(
        r#"<?php
$fresh = new LibXMLError();
echo isset($fresh->level) ? "I" : "U";
$fresh->level = 4;
$fresh->message = "fresh";
$freshCopy = clone $fresh;
$freshCopy->level = 5;
echo "|" . $fresh->level . ":" . $fresh->message;
echo "|" . $freshCopy->level . ":" . $freshCopy->message;

libxml_use_internal_errors(true);
$document = new DOMDocument();
$document->loadXML("<root>");
$errors = libxml_get_errors();
$first = $errors[0];
$copy = clone $first;
$copy->level = 9;
$copy->message = "copy";
echo "|" . $first->level . ":" . $first->code;
echo "|" . $copy->level . ":" . $copy->message;
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "U|4:fresh|5:fresh|3:77|9:copy");
}

/// Verifies libxml retains, returns, and clears a PHP callable through the host vtable.
#[test]
fn libxml_external_entity_loader_round_trips_callable_ownership() {
    let out = compile_and_run_capture(
        r#"<?php
echo libxml_get_external_entity_loader() === null ? "N" : "X";
$token = "owned";
$loader = function (?string $public, string $system, array $context) use ($token) {
    return null;
};
echo libxml_set_external_entity_loader($loader) ? "T" : "F";
$current = libxml_get_external_entity_loader();
if ($current === null) {
    echo "X";
} else {
    echo $current(null, "system-id", []) === null ? "C" : "X";
}
echo libxml_set_external_entity_loader(null) ? "T" : "F";
echo libxml_get_external_entity_loader() === null ? "N" : "X";
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "NTCTN");
}

/// Verifies every PHP callable representation crosses the libxml bridge as a descriptor.
#[test]
fn libxml_external_entity_loader_accepts_all_callable_shapes() {
    let out = compile_and_run_capture(
        r#"<?php
function entity_loader_function(?string $public, string $system, array $context) {
    return "f";
}

class EntityLoaderShapes {
    public function load(?string $public, string $system, array $context) {
        return "i";
    }

    public static function loadStatic(?string $public, string $system, array $context) {
        return "s";
    }

    public function __invoke(?string $public, string $system, array $context) {
        return "o";
    }
}

$object = new EntityLoaderShapes();
$zero = 0;
$one = 1;

echo libxml_set_external_entity_loader("entity_loader_function") ? "T" : "F";
$current = libxml_get_external_entity_loader();
echo is_callable($current) ? "C" : "X";
if ($current === null) echo "X"; else echo $current(null, "system", []);
echo libxml_set_external_entity_loader(null) ? "N" : "F";

echo libxml_set_external_entity_loader([$object, "load"]) ? "T" : "F";
$current = libxml_get_external_entity_loader();
echo is_callable($current) ? "C" : "X";
if ($current === null) echo "X"; else echo $current(null, "system", []);
echo libxml_set_external_entity_loader(null) ? "N" : "F";

echo libxml_set_external_entity_loader(["EntityLoaderShapes", "loadStatic"]) ? "T" : "F";
$current = libxml_get_external_entity_loader();
echo is_callable($current) ? "C" : "X";
if ($current === null) echo "X"; else echo $current(null, "system", []);
echo libxml_set_external_entity_loader(null) ? "N" : "F";

echo libxml_set_external_entity_loader([$zero => $object, $one => "load"]) ? "T" : "F";
$current = libxml_get_external_entity_loader();
echo is_callable($current) ? "C" : "X";
if ($current === null) echo "X"; else echo $current(null, "system", []);
echo libxml_set_external_entity_loader(null) ? "N" : "F";

echo libxml_set_external_entity_loader([$zero => "EntityLoaderShapes", $one => "loadStatic"]) ? "T" : "F";
$current = libxml_get_external_entity_loader();
echo is_callable($current) ? "C" : "X";
if ($current === null) echo "X"; else echo $current(null, "system", []);
echo libxml_set_external_entity_loader(null) ? "N" : "F";

echo libxml_set_external_entity_loader($object) ? "T" : "F";
$current = libxml_get_external_entity_loader();
echo is_callable($current) ? "C" : "X";
if ($current === null) echo "X"; else echo $current(null, "system", []);
echo libxml_set_external_entity_loader(null) ? "N" : "F";
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "TCfNTCiNTCsNTCiNTCsNTCoN");
}

/// Verifies receiver-bound bridge descriptors balance their temporary and retained owners.
#[test]
fn libxml_external_entity_loader_callable_array_heap_is_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class HeapEntityLoader {
    public function load(?string $public, string $system, array $context) {
        return null;
    }
}

$loader = new HeapEntityLoader();
libxml_set_external_entity_loader([$loader, "load"]);
unset($loader);
libxml_set_external_entity_loader(null);
echo "clean";
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "clean");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Verifies a destructor Throwable raised by host callable release crosses Rust as structured state.
#[test]
fn libxml_external_entity_loader_release_rethrows_original_object() {
    let out = compile_and_run_capture(
        r#"<?php
class ThrowingEntityLoader {
    public function load(?string $public, string $system, array $context) {
        return null;
    }

    public function __destruct() {
        echo "D";
        $nested = new DOMDocument();
        $root = $nested->createElement("r");
        if ($root === false) {
            echo "x";
        } else {
            $nested->appendChild($root);
            echo $root->nodeName;
        }
        throw new Exception("release");
    }
}

$loaderObject = new ThrowingEntityLoader();
libxml_set_external_entity_loader([$loaderObject, "load"]);
unset($loaderObject);
try {
    echo "B";
    libxml_set_external_entity_loader(null);
    echo "X";
} catch (Throwable $exception) {
    echo "C:" . $exception->getMessage();
}
echo ":" . (libxml_get_external_entity_loader() === null ? "N" : "Y");
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "BDrC:release:N");
}

/// Verifies parser entity callbacks receive php-src arguments and may re-enter DOM safely.
#[test]
fn libxml_external_entity_loader_invocation_is_php_compatible_and_reentrant() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
libxml_use_internal_errors(true);
libxml_set_external_entity_loader(
    function (?string $public, string $system, array $context) {
        echo "C:" . ($public ?? "N") . ":" . count($context) . ":";
        echo ($context["intSubName"] ?? "N") . ":";
        echo ($context["extSubSystem"] ?? "N") . ":";
        $nested = new DOMDocument();
        $nested->loadXML("<nested/>");
        echo $nested->documentElement->nodeName . ":";
        return null;
    }
);
$document = new DOMDocument();
echo "B:";
var_dump(
    $document->loadXML(
        '<!DOCTYPE root PUBLIC "PUB" "virtual.dtd"><root/>',
        LIBXML_DTDLOAD
    )
);
$errors = libxml_get_errors();
echo ":A:" . count($errors) . "\n";
unset($errors);
libxml_set_external_entity_loader(null);
libxml_clear_errors();
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(
        out.stdout,
        "B:C:PUB:4:root:PUB:nested:bool(true)\n:A:1\n"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Verifies a parser callback rethrows the exact PHP Throwable after nested DOM work.
#[test]
fn libxml_external_entity_loader_rethrows_exact_callback_throwable() {
    let out = compile_and_run_capture(
        r#"<?php
$token = new Exception("loader");
libxml_set_external_entity_loader(
    function (?string $public, string $system, array $context) use ($token) {
        $nested = new DOMDocument();
        $nested->loadXML("<n/>");
        echo "C:" . $nested->documentElement->nodeName . ":";
        throw $token;
    }
);
$document = new DOMDocument();
try {
    echo "B:";
    $document->loadXML(
        '<!DOCTYPE root SYSTEM "virtual.dtd"><root/>',
        LIBXML_DTDLOAD
    );
    echo "X";
} catch (Throwable $caught) {
    echo ($caught === $token ? "S" : "D") . ":" . $caught->getMessage();
}
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stdout, "B:C:n:S:loader");
    assert_eq!(out.stderr, "");
}

/// Verifies resolver stream resources feed libxml incrementally and release every lease.
#[test]
fn libxml_external_entity_loader_stream_resource_is_read_and_released() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
libxml_use_internal_errors(true);
libxml_set_external_entity_loader(
    function (?string $public, string $system, array $context) {
        echo "C:" . ($public ?? "N") . ":";
        echo ($context["intSubName"] ?? "N") . "|";
        $stream = fopen("php://temp", "r+");
        fwrite(
            $stream,
            "<!ELEMENT root (#PCDATA)>\n<!ENTITY answer \"42\">"
        );
        rewind($stream);
        return $stream;
    }
);
$document = new DOMDocument();
echo $document->loadXML(
    '<!DOCTYPE root SYSTEM "virtual.dtd"><root>&answer;</root>',
    LIBXML_DTDLOAD | LIBXML_NOENT
) ? "T" : "F";
echo ":" . $document->documentElement->textContent;
echo ":" . count(libxml_get_errors());
libxml_set_external_entity_loader(null);
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stdout, "C:N:root|T:42:0");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Verifies a resolver may return a registered wrapper stream with PHP-sized buffered reads.
#[test]
fn libxml_external_entity_loader_reads_registered_stream_wrapper_like_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class EntityStream {
    public $context;
    private string $data =
        "<!ELEMENT root (#PCDATA)>\n<!ENTITY answer \"42\">";
    private int $offset = 0;

    public function stream_open(
        string $path,
        string $mode,
        int $options,
        ?string &$openedPath
    ): bool {
        echo "O|";
        return true;
    }

    public function stream_read(int $count): string {
        echo "R" . $count . "|";
        $chunk = substr($this->data, $this->offset, 7);
        $this->offset += strlen($chunk);
        return $chunk;
    }

    public function stream_eof(): bool {
        return $this->offset >= strlen($this->data);
    }

    public function stream_close(): void {
        echo "X|";
    }

    public function stream_stat(): array {
        return [];
    }
}

stream_wrapper_register("entity", "EntityStream");
libxml_use_internal_errors(true);
libxml_set_external_entity_loader(
    function (?string $public, string $system, array $context) {
        echo "C:";
        return fopen("entity://virtual.dtd", "r");
    }
);
$document = new DOMDocument();
echo $document->loadXML(
    '<!DOCTYPE root SYSTEM "virtual.dtd"><root>&answer;</root>',
    LIBXML_DTDLOAD | LIBXML_NOENT
) ? "T" : "F";
echo ":" . $document->documentElement->textContent;
echo ":" . count(libxml_get_errors());
libxml_set_external_entity_loader(null);
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(
        out.stdout,
        "C:O|R8192|R8192|R8192|R8192|R8192|R8192|R8192|R8192|X|T:42:0"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Verifies the active libxml stream context accepts Elephc's stream resource representation.
#[test]
fn libxml_streams_context_accepts_stream_context_resource() {
    let out = compile_and_run(
        r#"<?php
$context = stream_context_create(["http" => ["method" => "POST"]]);
echo libxml_set_streams_context($context) === null ? "N" : "X";
"#,
    );
    assert_eq!(out, "N");
}

/// Verifies direct DOM wrapper reads inherit the selected libxml context exactly like PHP.
#[test]
fn libxml_streams_context_propagates_to_direct_dom_file_reads() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class ContextReadWrapper {
    public $context;
    private string $data = "<root>ok</root>";
    private int $offset = 0;

    public function stream_open(
        string $path,
        string $mode,
        int $options,
        ?string &$openedPath
    ): bool {
        $context = $this->context
            ? stream_context_get_options($this->context)
            : [];
        echo "O:$path:$mode:$options:"
            . ($context["ctx"]["name"] ?? "none") . "|";
        return true;
    }

    public function stream_read(int $count): string {
        echo "R$count|";
        $chunk = substr($this->data, $this->offset, 5);
        $this->offset += strlen($chunk);
        return $chunk;
    }

    public function stream_eof(): bool {
        return $this->offset >= strlen($this->data);
    }

    public function stream_close(): void {
        echo "C|";
    }

    public function url_stat(string $path, int $flags): array {
        echo "S:$path:$flags|";
        return [];
    }
}

stream_wrapper_register("ctxread", ContextReadWrapper::class);
$one = stream_context_create(["ctx" => ["name" => "one"]]);
$two = stream_context_create(["ctx" => ["name" => "two"]]);

libxml_set_streams_context($two);
$legacy = new DOMDocument();
var_dump($legacy->load("ctxread://legacy"));
echo $legacy->documentElement->textContent . "|";

libxml_set_streams_context($one);
$modern = Dom\XMLDocument::createFromFile("ctxread://modern");
echo $modern->documentElement->textContent . "|";
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(
        out.stdout,
        "S:ctxread://legacy:2|O:ctxread://legacy:rb:0:two|\
R8192|R8192|R8192|R8192|C|bool(true)\n\
ok|S:ctxread://modern:2|O:ctxread://modern:rb:0:one|\
R8192|R8192|R8192|R8192|C|ok|"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Verifies all DOM file serializers use selected contexts, partial writes, flush, and one close.
#[test]
fn dom_file_saves_use_registered_stream_wrappers_like_php() {
    let out = compile_and_run(
        r#"<?php
class ContextWriteWrapper {
    public $context;
    public static string $written = "";
    public static string $mode = "";
    public static string $contextName = "";
    public static int $options = -1;
    public static int $writes = 0;
    public static int $flushes = 0;
    public static int $closes = 0;
    public static bool $reentered = false;

    public static function reset(): void {
        self::$written = "";
        self::$mode = "";
        self::$contextName = "";
        self::$options = -1;
        self::$writes = 0;
        self::$flushes = 0;
        self::$closes = 0;
        self::$reentered = false;
    }

    public function stream_open(
        string $path,
        string $mode,
        int $options,
        ?string &$openedPath
    ): bool {
        $context = $this->context
            ? stream_context_get_options($this->context)
            : [];
        self::$mode = $mode;
        self::$options = $options;
        self::$contextName = $context["ctx"]["name"] ?? "none";
        return true;
    }

    public function stream_write(string $data): int {
        self::$writes += 1;
        if (!self::$reentered) {
            self::$reentered = true;
            $nested = new DOMDocument();
            $nested->loadXML("<nested/>");
            if ($nested->documentElement->nodeName !== "nested") {
                throw new Exception("nested DOM re-entry failed");
            }
        }
        $count = min(3, strlen($data));
        self::$written .= substr($data, 0, $count);
        return $count;
    }

    public function stream_flush(): bool {
        self::$flushes += 1;
        return false;
    }

    public function stream_close(): void {
        self::$closes += 1;
    }
}

function report_write(string $label, mixed $count, string $expected): void {
    echo $label, ":";
    echo $count === strlen($expected) ? "T" : "F";
    echo ContextWriteWrapper::$written === $expected ? "T" : "F";
    echo ContextWriteWrapper::$writes > 1 ? "T" : "F";
    echo ContextWriteWrapper::$flushes === 1 ? "T" : "F";
    echo ContextWriteWrapper::$closes === 1 ? "T" : "F";
    echo ContextWriteWrapper::$mode === "wb" ? "T" : "F";
    echo ContextWriteWrapper::$options === 0 ? "T" : "F";
    echo ContextWriteWrapper::$contextName === "selected" ? "T" : "F";
    echo ContextWriteWrapper::$reentered ? "T" : "F";
    echo "|";
}

stream_wrapper_register("ctxwrite", ContextWriteWrapper::class);
$context = stream_context_create(["ctx" => ["name" => "selected"]]);
libxml_set_streams_context($context);

$legacyXml = new DOMDocument();
$legacyXml->loadXML("<root><item/></root>");
$expected = (string) $legacyXml->saveXML();
ContextWriteWrapper::reset();
report_write(
    "legacy-xml",
    $legacyXml->save("ctxwrite://legacy.xml"),
    $expected
);

$legacyHtml = new DOMDocument();
$legacyHtml->loadHTML("<p>legacy</p>");
$expected = (string) $legacyHtml->saveHTML();
ContextWriteWrapper::reset();
report_write(
    "legacy-html",
    $legacyHtml->saveHTMLFile("ctxwrite://legacy.html"),
    $expected
);

$modernXml = Dom\XMLDocument::createFromString("<root><item/></root>");
$expected = (string) $modernXml->saveXml();
ContextWriteWrapper::reset();
report_write(
    "modern-xml",
    $modernXml->saveXmlFile("ctxwrite://modern.xml"),
    $expected
);

$modernHtml = Dom\HTMLDocument::createFromString(
    "<!DOCTYPE html><html><body><p>modern</p></body></html>"
);
$expected = (string) $modernHtml->saveXml();
ContextWriteWrapper::reset();
report_write(
    "html-as-xml",
    $modernHtml->saveXmlFile("ctxwrite://html.xml"),
    $expected
);

$expected = (string) $modernHtml->saveHtml();
ContextWriteWrapper::reset();
report_write(
    "modern-html",
    $modernHtml->saveHtmlFile("ctxwrite://modern.html"),
    $expected
);
ContextWriteWrapper::reset();
unset(
    $expected,
    $modernHtml,
    $modernXml,
    $legacyHtml,
    $legacyXml,
    $context
);
"#,
    );
    assert_eq!(
        out,
        "legacy-xml:TTTTTTTTT|legacy-html:TTTTTTTTT|\
modern-xml:TTTTTTTTT|html-as-xml:TTTTTTTTT|modern-html:TTTTTTTTT|"
    );
}

/// Verifies a re-entrant stream write, ignored false flush, close, and destructor remain balanced.
#[test]
fn dom_file_save_stream_callbacks_are_reentrant_and_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class ReentrantWriteWrapper {
    public $context;
    private bool $reentered = false;

    public function stream_open($path, $mode, $options, &$openedPath): bool {
        echo "O";
        return true;
    }

    public function stream_write($data): int {
        if (!$this->reentered) {
            $this->reentered = true;
            $nested = new DOMDocument();
            $nested->loadXML("<nested/>");
            echo $nested->documentElement->nodeName === "nested" ? "R" : "X";
            unset($nested);
        }
        return strlen($data);
    }

    public function stream_flush(): bool {
        echo "F";
        return false;
    }

    public function stream_close(): void {
        echo "C";
    }

    public function __destruct() {
        echo "D";
    }
}

stream_wrapper_register("reentrantwrite", ReentrantWriteWrapper::class);
$document = new DOMDocument();
$document->loadXML("<root/>");
var_dump($document->save("reentrantwrite://out"));
unset($document);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "ORFCDint(30)\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected re-entrant save callbacks to remain balanced, got: {}",
        out.stderr
    );
}

/// Verifies stream-write false and zero retain their distinct PHP save contracts.
#[test]
fn dom_file_saves_distinguish_stream_write_false_from_zero() {
    let out = compile_and_run(
        r#"<?php
class ResultWriteWrapper {
    public $context;
    public static int $flushes = 0;
    public static int $closes = 0;

    public function stream_open($path, $mode, $options, &$openedPath): bool {
        return true;
    }

    public function stream_write($data): mixed {
        return false;
    }

    public function stream_flush(): bool {
        self::$flushes += 1;
        return true;
    }

    public function stream_close(): void {
        self::$closes += 1;
    }
}

class ZeroWriteWrapper {
    public $context;
    public static int $flushes = 0;
    public static int $closes = 0;

    public function stream_open($path, $mode, $options, &$openedPath): bool {
        return true;
    }

    public function stream_write($data): int {
        return 0;
    }

    public function stream_flush(): bool {
        self::$flushes += 1;
        return true;
    }

    public function stream_close(): void {
        self::$closes += 1;
    }
}

class PartialZeroWriteWrapper {
    public $context;
    public static int $writes = 0;
    public static int $flushes = 0;
    public static int $closes = 0;

    public function stream_open($path, $mode, $options, &$openedPath): bool {
        return true;
    }

    public function stream_write($data): int {
        self::$writes += 1;
        return self::$writes === 1 ? 3 : 0;
    }

    public function stream_flush(): bool {
        self::$flushes += 1;
        return true;
    }

    public function stream_close(): void {
        self::$closes += 1;
    }
}

class NegativeWriteWrapper {
    public $context;
    public static int $flushes = 0;
    public static int $closes = 0;

    public function stream_open($path, $mode, $options, &$openedPath): bool {
        return true;
    }

    public function stream_write($data): int {
        return -2;
    }

    public function stream_flush(): bool {
        self::$flushes += 1;
        return true;
    }

    public function stream_close(): void {
        self::$closes += 1;
    }
}

stream_wrapper_register("resultwrite", ResultWriteWrapper::class);
stream_wrapper_register("zerowrite", ZeroWriteWrapper::class);
stream_wrapper_register("partialzero", PartialZeroWriteWrapper::class);
stream_wrapper_register("negativewrite", NegativeWriteWrapper::class);
$document = new DOMDocument();
$document->loadXML("<root/>");

var_dump($document->save("resultwrite://false"));
echo ResultWriteWrapper::$flushes, ":", ResultWriteWrapper::$closes, "|";

var_dump($document->save("zerowrite://zero"));
echo ZeroWriteWrapper::$flushes, ":", ZeroWriteWrapper::$closes, "|";

var_dump($document->save("partialzero://out"));
echo PartialZeroWriteWrapper::$writes, ":",
    PartialZeroWriteWrapper::$flushes, ":",
    PartialZeroWriteWrapper::$closes, "|";

var_dump($document->save("negativewrite://out"));
echo NegativeWriteWrapper::$flushes, ":", NegativeWriteWrapper::$closes;
unset($document);
"#,
    );
    assert_eq!(
        out,
        "bool(false)\n1:1|int(0)\n0:1|int(3)\n2:1:1|bool(false)\n1:1"
    );
}

/// Verifies oversized wrapper write results warn, clamp, flush, close, and honor suppression.
#[test]
fn dom_file_saves_warn_and_clamp_oversized_stream_write_results() {
    let out = compile_and_run_capture(
        r#"<?php
class OversizedWriteWrapper {
    public $context;

    public function stream_open($path, $mode, $options, &$openedPath): bool {
        return true;
    }

    public function stream_write($data): int {
        echo "W|";
        return strlen($data) + 1;
    }

    public function stream_flush(): bool {
        echo "F|";
        return true;
    }

    public function stream_close(): void {
        echo "C|";
    }
}

stream_wrapper_register("oversizedwrite", OversizedWriteWrapper::class);
$document = new DOMDocument();
$document->loadXML("<root/>");
var_dump($document->save("oversizedwrite://one"));
var_dump(@$document->save("oversizedwrite://two"));
unset($document);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "W|F|C|int(30)\nW|F|C|int(30)\n"
    );
    let warning = "Warning: DOMDocument::save(): \
OversizedWriteWrapper::stream_write wrote 1 bytes more data than requested \
(31 written, 30 max)\n";
    assert_eq!(
        out.stderr.matches(warning).count(),
        1,
        "expected one unsuppressed oversized-write warning, got: {}",
        out.stderr
    );
}

/// Verifies write and flush Throwables rethrow exactly and still close the stream once.
#[test]
fn dom_file_saves_rethrow_stream_callback_throwables_and_close() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class ThrowingWriteWrapper {
    public $context;

    public function stream_open($path, $mode, $options, &$openedPath): bool {
        return true;
    }

    public function stream_write($data): int {
        throw new Exception("write");
    }

    public function stream_flush(): bool {
        return false;
    }

    public function stream_close(): void {
        echo "C|";
    }
}

class ThrowingFlushWrapper {
    public $context;

    public function stream_open($path, $mode, $options, &$openedPath): bool {
        return true;
    }

    public function stream_write($data): int {
        return strlen($data);
    }

    public function stream_flush(): bool {
        throw new Exception("flush");
    }

    public function stream_close(): void {
        echo "C|";
    }
}

stream_wrapper_register("throwwrite", ThrowingWriteWrapper::class);
stream_wrapper_register("throwflush", ThrowingFlushWrapper::class);
$document = new DOMDocument();
$document->loadXML("<root/>");

try {
    $document->save("throwwrite://out");
    echo "X";
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "|";
}
unset($error);
try {
    $document->save("throwflush://out");
    echo "X";
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "|";
}
unset($error, $document);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "C|Exception:write|C|Exception:flush|"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected callback Throwables and streams to unwind cleanly, got: {}",
        out.stderr
    );
}

/// Verifies DOM stream callbacks receive PHP values when wrapper methods omit parameter types.
#[test]
fn dom_file_reads_adapt_untyped_stream_wrapper_parameters() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class UntypedDomWrapper {
    public $context;
    private string $data = "<root>ok</root>";
    private int $offset = 0;

    public function stream_open($path, $mode, $options, &$openedPath) {
        echo "$path|$mode|$options|";
        return true;
    }

    public function stream_read($count) {
        echo "$count|";
        $chunk = substr($this->data, $this->offset, 5);
        $this->offset += strlen($chunk);
        return $chunk;
    }

    public function stream_eof() {
        return $this->offset >= strlen($this->data);
    }

    public function stream_close() {
        echo "C|";
    }

    public function url_stat($path, $flags) {
        echo "$path|$flags|";
        return [];
    }
}

stream_wrapper_register("untypeddom", UntypedDomWrapper::class);
$document = new DOMDocument();
var_dump($document->load("untypeddom://source"));
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(
        out.stdout,
        "untypeddom://source|2|untypeddom://source|rb|0|\
8192|8192|8192|8192|C|bool(true)\n"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected untyped wrapper adapter temporaries to remain balanced, got: {}",
        out.stderr
    );
}

/// Verifies DOM wrapper callbacks normalize declared `mixed` returns to php-src's slot ABI.
#[test]
fn dom_file_reads_normalize_mixed_stream_wrapper_returns() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class MixedReturnDomWrapper {
    public $context;
    private string $data = "<root>ok</root>";
    private int $offset = 0;

    public function stream_open($path, $mode, $options, &$openedPath): mixed {
        return "yes";
    }

    public function stream_read($count): mixed {
        $chunk = substr($this->data, $this->offset, 5);
        $this->offset += strlen($chunk);
        return $chunk;
    }

    public function stream_eof(): mixed {
        return $this->offset >= strlen($this->data);
    }

    public function stream_close(): mixed {
        echo "C|";
        return ["ignored"];
    }

    public function url_stat($path, $flags): mixed {
        return [];
    }
}

stream_wrapper_register("mixeddom", MixedReturnDomWrapper::class);
$document = new DOMDocument();
var_dump($document->load("mixeddom://source"));
echo $document->documentElement->textContent;
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stdout, "C|bool(true)\nok");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected normalized mixed callback returns to remain balanced, got: {}",
        out.stderr
    );
}

/// Verifies callback values requested by reference are warned, isolated, and safely mutable.
#[test]
fn dom_file_reads_materialize_by_ref_stream_wrapper_parameters() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class ByRefDomWrapper {
    public $context;

    public function url_stat($path, $flags) {
        return [];
    }

    public function stream_open(
        string &$path,
        string &$mode,
        int &$options,
        mixed &$openedPath
    ) {
        echo gettype($path), "|", gettype($mode), "|", gettype($options), "|";
        echo gettype($openedPath), "|";
        $path = "changed";
        $mode = "changed";
        $options = 9;
        return false;
    }
}

stream_wrapper_register("byrefdom", ByRefDomWrapper::class);
$document = new DOMDocument();
var_dump($document->load("byrefdom://source"));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "string|string|integer|NULL|bool(false)\n"
    );
    assert!(out.stderr.contains(
        "ByRefDomWrapper::stream_open(): Argument #1 ($path) must be passed by reference, \
value given"
    ));
    assert!(out.stderr.contains(
        "ByRefDomWrapper::stream_open(): Argument #2 ($mode) must be passed by reference, \
value given"
    ));
    assert!(out.stderr.contains(
        "ByRefDomWrapper::stream_open(): Argument #3 ($options) must be passed by reference, \
value given"
    ));
    assert!(!out.stderr.contains(
        "Argument #4 ($openedPath) must be passed by reference"
    ));
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected temporary callback reference cells to remain balanced, got: {}",
        out.stderr
    );
}

/// Verifies untyped by-reference callbacks preserve runtime value kinds and ownership.
#[test]
fn dom_file_reads_materialize_untyped_by_ref_stream_wrapper_parameters() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class UntypedByRefDomWrapper {
    public $context;

    public function url_stat($path, $flags) {
        return [];
    }

    public function stream_open(&$path, &$mode, &$options, &$openedPath) {
        echo gettype($path), "|", gettype($mode), "|", gettype($options), "|";
        echo gettype($openedPath), "|";
        $path = ["changed"];
        $mode = 9;
        $options = "changed";
        $openedPath = "untypedrefdom://opened";
        return false;
    }
}

stream_wrapper_register("untypedrefdom", UntypedByRefDomWrapper::class);
$document = new DOMDocument();
var_dump($document->load("untypedrefdom://source"));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "string|string|integer|NULL|bool(false)\n"
    );
    for warning in [
        "Argument #1 ($path) must be passed by reference, value given",
        "Argument #2 ($mode) must be passed by reference, value given",
        "Argument #3 ($options) must be passed by reference, value given",
    ] {
        assert!(out.stderr.contains(warning), "missing warning: {warning}");
    }
    assert!(!out.stderr.contains(
        "Argument #4 ($openedPath) must be passed by reference"
    ));
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected dynamic callback reference payloads to remain balanced, got: {}",
        out.stderr
    );
}

/// Verifies the opened-path reference starts as null and enforces its declared type.
#[test]
fn dom_file_reads_reject_non_nullable_opened_path_reference() {
    let out = compile_and_run(
        r#"<?php
class TypedOpenedPathDomWrapper {
    public $context;

    public function url_stat($path, $flags) {
        return [];
    }

    public function stream_open($path, $mode, $options, string &$openedPath) {
        return false;
    }
}

stream_wrapper_register("typedopened", TypedOpenedPathDomWrapper::class);
try {
    (new DOMDocument())->load("typedopened://source");
} catch (TypeError $error) {
    echo get_class($error), ":", $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "TypeError:TypedOpenedPathDomWrapper::stream_open(): Argument #4 ($openedPath) \
must be of type string, null given"
    );
}

/// Verifies typed wrapper callbacks apply PHP weak scalar coercions and literal defaults.
#[test]
fn dom_file_reads_coerce_typed_stream_wrapper_parameters() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class TypedDomWrapper {
    public $context;
    private string $data = "<root>ok</root>";
    private int $offset = 0;

    public function stream_open(
        bool $path,
        string $mode,
        float $options,
        &$openedPath,
        string $extra = "default"
    ) {
        echo is_bool($path) ? "bool" : "other", ":", $path ? "1" : "0", "|";
        echo is_string($mode) ? "string" : "other", ":", $mode, "|";
        echo $options === 0.0 ? "float" : "other", ":", $options, "|";
        echo $extra, "|";
        return true;
    }

    public function stream_read(string $count) {
        echo is_string($count) ? "string" : "other", ":", $count, "|";
        $chunk = substr($this->data, $this->offset, 5);
        $this->offset += strlen($chunk);
        return $chunk;
    }

    public function stream_eof() {
        return $this->offset >= strlen($this->data);
    }

    public function stream_close() {
        echo "C|";
    }

    public function url_stat(string $path, bool $flags) {
        echo is_string($path) ? "string" : "other", ":", $path, "|";
        echo is_bool($flags) ? "bool" : "other", ":", $flags ? "1" : "0", "|";
        return [];
    }
}

stream_wrapper_register("typeddom", TypedDomWrapper::class);
$document = new DOMDocument();
var_dump($document->load("typeddom://source"));
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(
        out.stdout,
        "string:typeddom://source|bool:1|bool:1|string:rb|float:0|default|\
string:8192|string:8192|string:8192|string:8192|C|bool(true)\n"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected typed wrapper conversion owners to remain balanced, got: {}",
        out.stderr
    );
}

/// Verifies incompatible typed wrapper callback parameters throw the exact catchable TypeError.
#[test]
fn dom_file_reads_reject_incompatible_typed_stream_wrapper_parameters() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class RejectingDomWrapper {
    public $context;

    public function stream_open($path, $mode, $options, &$openedPath) {
        return true;
    }

    public function stream_read(array $count) {
        return "";
    }

    public function stream_eof() {
        return true;
    }

    public function stream_close() {}

    public function url_stat($path, $flags) {
        return [];
    }
}

stream_wrapper_register("rejectdom", RejectingDomWrapper::class);
$document = new DOMDocument();
try {
    $document->load("rejectdom://source");
} catch (TypeError $error) {
    echo get_class($error), ":", $error->getMessage();
}
unset($document, $error);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "TypeError:RejectingDomWrapper::stream_read(): Argument #1 ($count) \
must be of type array, int given"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected caught wrapper TypeError ownership to remain balanced, got: {}",
        out.stderr
    );
}

/// Verifies non-numeric wrapper paths cannot weakly coerce into a declared int parameter.
#[test]
fn dom_file_reads_reject_non_numeric_string_for_typed_wrapper_int() {
    let out = compile_and_run(
        r#"<?php
class NumericRejectingDomWrapper {
    public $context;

    public function stream_open(int $path, $mode, $options, &$openedPath) {
        return true;
    }

    public function stream_read($count) {
        return "";
    }

    public function stream_eof() {
        return true;
    }

    public function stream_close() {}

    public function url_stat($path, $flags) {
        return [];
    }
}

stream_wrapper_register("numericreject", NumericRejectingDomWrapper::class);
try {
    (new DOMDocument())->load("numericreject://source");
} catch (TypeError $error) {
    echo get_class($error), ":", $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "TypeError:NumericRejectingDomWrapper::stream_open(): Argument #1 ($path) \
must be of type int, string given"
    );
}

/// Verifies callback arguments beyond fixed parameters are packed into a PHP variadic array.
#[test]
fn dom_file_reads_pack_stream_wrapper_variadic_parameters() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class VariadicDomWrapper {
    public $context;
    private string $data = "<root>ok</root>";
    private int $offset = 0;

    public function stream_open($path, ...$arguments) {
        echo count($arguments), ":";
        echo $arguments[0], ":";
        echo $arguments[1], ":";
        echo is_null($arguments[2]) ? "null" : "other", "|";
        return true;
    }

    public function stream_read(...$arguments) {
        echo count($arguments), ":", $arguments[0], "|";
        $chunk = substr($this->data, $this->offset, 5);
        $this->offset += strlen($chunk);
        return $chunk;
    }

    public function stream_eof() {
        return $this->offset >= strlen($this->data);
    }

    public function stream_close() {
        echo "C|";
    }

    public function url_stat($path, ...$arguments) {
        echo count($arguments), ":", $arguments[0], "|";
        return [];
    }
}

stream_wrapper_register("variadicdom", VariadicDomWrapper::class);
$document = new DOMDocument();
var_dump($document->load("variadicdom://source"));
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(
        out.stdout,
        "1:2|3:rb:0:null|1:8192|1:8192|1:8192|1:8192|C|bool(true)\n"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected variadic wrapper arrays to remain balanced, got: {}",
        out.stderr
    );
}

/// Verifies by-reference variadic callback elements remain aliases across array copies.
#[test]
fn dom_file_reads_materialize_by_ref_variadic_wrapper_elements() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class VariadicRefDomWrapper {
    public $context;

    public function url_stat($path, $flags) {
        return [];
    }

    public function stream_open($path, &...$arguments) {
        echo count($arguments), ":";

        $copy = $arguments;
        $copy[0] = "copy-mode";
        $arguments[1] = "changed-options";
        $arguments[2] = "variadicrefdom://opened";
        echo $arguments[0], ":", $copy[1], "|";
        unset($copy);
        return false;
    }
}

stream_wrapper_register("variadicrefdom", VariadicRefDomWrapper::class);
var_dump((new DOMDocument())->load("variadicrefdom://source"));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "3:copy-mode:changed-options|bool(false)\n"
    );
    for warning in [
        "Argument #2 must be passed by reference, value given",
        "Argument #3 must be passed by reference, value given",
    ] {
        assert!(out.stderr.contains(warning), "missing warning: {warning}");
    }
    assert!(!out.stderr.contains(
        "Argument #4 must be passed by reference"
    ));
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected variadic reference cells to remain balanced, got: {}",
        out.stderr
    );
}

/// Verifies exceptional by-reference variadic callbacks release all adapter-owned cells.
#[test]
fn dom_file_reads_unwind_by_ref_variadic_wrapper_elements_on_throw() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class ThrowingVariadicRefDomWrapper {
    public $context;

    public function url_stat($path, $flags) {
        return [];
    }

    public function stream_open($path, &...$arguments) {
        $arguments[2] = "variadicrefthrow://opened";
        throw new Exception("variadic");
    }
}

stream_wrapper_register("variadicrefthrow", ThrowingVariadicRefDomWrapper::class);
try {
    @(new DOMDocument())->load("variadicrefthrow://source");
} catch (Exception $error) {
    echo get_class($error), ":", $error->getMessage();
}
unset($error);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "Exception:variadic");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected exceptional variadic reference cells to unwind cleanly, got: {}",
        out.stderr
    );
}

/// Verifies a by-value stream_open opened-path parameter observes PHP null, not its ref-cell address.
#[test]
fn dom_file_reads_pass_null_to_by_value_stream_open_opened_path() {
    let out = compile_and_run(
        r#"<?php
class OpenedPathValueDomWrapper {
    public $context;

    public function url_stat($path, $flags) {
        return [];
    }

    public function stream_open($path, $mode, $options, $openedPath) {
        echo is_null($openedPath) ? "null|" : "other|";
        return false;
    }
}

stream_wrapper_register("openedvalue", OpenedPathValueDomWrapper::class);
var_dump(@(new DOMDocument())->load("openedvalue://source"));
"#,
    );
    assert_eq!(out, "null|bool(false)\n");
}

/// Verifies too-short internal callback invocation throws PHP's exact ArgumentCountError.
#[test]
fn dom_file_reads_throw_argument_count_error_for_required_extra_wrapper_parameter() {
    let out = compile_and_run(
        r#"<?php
class ArityDomWrapper {
    public $context;

    public function url_stat($path, $flags) {
        return [];
    }

    public function stream_open($path, $mode, $options, &$openedPath, $required) {
        return true;
    }
}

stream_wrapper_register("aritydom", ArityDomWrapper::class);
try {
    (new DOMDocument())->load("aritydom://source");
} catch (ArgumentCountError $error) {
    echo get_class($error), "|";
    echo $error instanceof TypeError ? "type|" : "other|";
    echo $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "ArgumentCountError|type|Too few arguments to function ArityDomWrapper::stream_open(), \
4 passed and exactly 5 expected"
    );
}

/// Verifies typed variadic callback elements use PHP argument positions in TypeError messages.
#[test]
fn dom_file_reads_reject_incompatible_typed_variadic_wrapper_parameter() {
    let out = compile_and_run(
        r#"<?php
class TypedVariadicDomWrapper {
    public $context;

    public function url_stat($path, $flags) {
        return [];
    }

    public function stream_open($path, int ...$arguments) {
        return false;
    }
}

stream_wrapper_register("typedvariadic", TypedVariadicDomWrapper::class);
try {
    @(new DOMDocument())->load("typedvariadic://source");
} catch (TypeError $error) {
    echo get_class($error), ":", $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "TypeError:TypedVariadicDomWrapper::stream_open(): Argument #2 must be of type int, \
string given"
    );
}

/// Verifies typed by-reference variadic callback entries coerce on entry but remain writable.
#[test]
fn dom_file_reads_coerce_typed_by_ref_variadic_wrapper_elements() {
    let out = compile_and_run_capture(
        r#"<?php
class TypedVariadicRefDomWrapper {
    public $context;
    private string $data = "<root>ok</root>";
    private int $offset = 0;
    private bool $seen = false;

    public function stream_open($path, $mode, $options, &$openedPath) {
        return true;
    }

    public function stream_read(string &...$arguments): string {
        if (!$this->seen) {
            echo gettype($arguments[0]), ":", $arguments[0], "|";
            $arguments[0] = 1;
            echo gettype($arguments[0]), ":", $arguments[0], "|";
            $this->seen = true;
        }
        $chunk = substr($this->data, $this->offset, 5);
        $this->offset += strlen($chunk);
        return $chunk;
    }

    public function stream_eof() {
        return $this->offset >= strlen($this->data);
    }

    public function stream_close() {}

    public function url_stat($path, $flags) {
        return [];
    }
}

stream_wrapper_register("typedvariadicref", TypedVariadicRefDomWrapper::class);
var_dump((new DOMDocument())->load("typedvariadicref://source"));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "string:8192|integer:1|bool(true)\n");
    assert!(out.stderr.contains(
        "TypedVariadicRefDomWrapper::stream_read(): Argument #1 must be passed by reference, \
value given"
    ));
}

/// Verifies composite by-reference variadic declarations validate only callback entry values.
#[test]
fn dom_file_reads_validate_composite_by_ref_variadic_wrapper_elements_on_entry() {
    let out = compile_and_run_capture(
        r#"<?php
class CompositeVariadicRefDomWrapper {
    public $context;

    public function url_stat($path, $flags) {
        return [];
    }

    public function stream_open(string|int|null &...$arguments) {
        echo gettype($arguments[0]), ":";
        echo gettype($arguments[1]), ":";
        echo gettype($arguments[2]), ":";
        echo gettype($arguments[3]), "|";
        $arguments[0] = false;
        $arguments[1] = 2.5;
        $arguments[2] = "changed";
        $arguments[3] = [];
        echo gettype($arguments[0]), ":";
        echo gettype($arguments[1]), ":";
        echo gettype($arguments[2]), ":";
        echo gettype($arguments[3]), "|";
        return false;
    }
}

stream_wrapper_register("compositevariadicref", CompositeVariadicRefDomWrapper::class);
var_dump((new DOMDocument())->load("compositevariadicref://source"));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "string:string:integer:NULL|",
            "boolean:double:string:array|bool(false)\n"
        )
    );
    for warning in [
        "Argument #1 must be passed by reference, value given",
        "Argument #2 must be passed by reference, value given",
        "Argument #3 must be passed by reference, value given",
    ] {
        assert!(out.stderr.contains(warning), "missing warning: {warning}");
    }
    assert!(!out.stderr.contains(
        "Argument #4 must be passed by reference"
    ));
}

/// Verifies escaped wrapper Throwables release adapter references and the direct DOM receiver.
#[test]
fn dom_file_reads_unwind_wrapper_callback_temporaries_on_throw() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class ThrowingDomWrapper {
    public $context;

    public function url_stat(string $path, int $flags): array {
        return [];
    }

    public function stream_open(
        string &$path,
        string &$mode,
        int &$options,
        mixed &$openedPath
    ) {
        $path = "changed";
        $openedPath = "throwdom://opened";
        throw new Exception("wrapper");
    }
}

stream_wrapper_register("throwdom", ThrowingDomWrapper::class);
try {
    @(new DOMDocument())->load("throwdom://source");
    echo "X";
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage();
}
unset($error);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "Exception:wrapper");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected callback and direct DOM temporaries to unwind, got: {}",
        out.stderr
    );
}

/// Verifies DOM reads stop after failed url_stat and reproduce php-src wrapper warnings.
#[test]
fn dom_file_reads_report_registered_wrapper_open_failures_like_php() {
    let out = compile_and_run_capture(
        r#"<?php
class MissingStatDomWrapper {
    public $context;

    public function stream_open($path, $mode, $options, &$openedPath) {
        echo "O";
        return true;
    }
}

class FalseStatDomWrapper {
    public $context;

    public function url_stat($path, $flags) {
        echo "S", $flags, "|";
        return false;
    }

    public function stream_open($path, $mode, $options, &$openedPath) {
        echo "O";
        return true;
    }
}

class MissingOpenDomWrapper {
    public $context;

    public function url_stat($path, $flags) {
        echo "S", $flags, "|";
        return [];
    }
}

class FalseOpenDomWrapper {
    public $context;

    public function url_stat($path, $flags) {
        echo "S", $flags, "|";
        return [];
    }

    public function stream_open($path, $mode, $options, &$openedPath) {
        echo "O", $mode, ":", $options, "|";
        return false;
    }
}

stream_wrapper_register("missingstat", MissingStatDomWrapper::class);
stream_wrapper_register("falsestat", FalseStatDomWrapper::class);
stream_wrapper_register("missingopen", MissingOpenDomWrapper::class);
stream_wrapper_register("falseopen", FalseOpenDomWrapper::class);

function report_legacy_failure(string $label, string $path): void {
    echo $label, ":";
    $document = new DOMDocument();
    var_dump($document->load($path));
    unset($document);
}

function report_modern_failure(string $label, string $path): void {
    echo $label, ":";
    try {
        Dom\XMLDocument::createFromFile($path);
        echo "X";
    } catch (Throwable $error) {
        echo get_class($error), ":", $error->getMessage();
    }
    echo "\n";
    unset($error);
}

report_legacy_failure("missingstat", "missingstat://source");
report_legacy_failure("falsestat", "falsestat://source");
report_legacy_failure("missingopen", "missingopen://source");
report_legacy_failure("falseopen", "falseopen://source");
echo "suppressed:";
$suppressed = new DOMDocument();
var_dump(@$suppressed->load("missingstat://source"));
unset($suppressed);

report_modern_failure("modern-missingstat", "missingstat://source");
report_modern_failure("modern-falsestat", "falsestat://source");
report_modern_failure("modern-missingopen", "missingopen://source");
report_modern_failure("modern-falseopen", "falseopen://source");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "missingstat:bool(false)\n",
            "falsestat:S2|bool(false)\n",
            "missingopen:S2|bool(false)\n",
            "falseopen:S2|Orb:0|bool(false)\n",
            "suppressed:bool(false)\n",
            "modern-missingstat:Exception:Cannot open file 'missingstat://source'\n",
            "modern-falsestat:S2|Exception:Cannot open file 'falsestat://source'\n",
            "modern-missingopen:S2|Exception:Cannot open file 'missingopen://source'\n",
            "modern-falseopen:S2|Orb:0|Exception:Cannot open file 'falseopen://source'\n",
        )
    );
    for warning in [
        "Warning: DOMDocument::load(): MissingStatDomWrapper::url_stat is not implemented!\n",
        "Warning: DOMDocument::load(missingopen://source): Failed to open stream: \
\"MissingOpenDomWrapper::stream_open\" is not implemented\n",
        "Warning: DOMDocument::load(falseopen://source): Failed to open stream: \
\"FalseOpenDomWrapper::stream_open\" call failed\n",
        "Warning: Dom\\XMLDocument::createFromFile(): \
MissingStatDomWrapper::url_stat is not implemented!\n",
        "Warning: Dom\\XMLDocument::createFromFile(missingopen://source): \
Failed to open stream: \"MissingOpenDomWrapper::stream_open\" is not implemented\n",
        "Warning: Dom\\XMLDocument::createFromFile(falseopen://source): \
Failed to open stream: \"FalseOpenDomWrapper::stream_open\" call failed\n",
    ] {
        assert_eq!(
            out.stderr.matches(warning).count(),
            1,
            "expected one exact wrapper warning {warning:?}, got: {}",
            out.stderr
        );
    }
}

/// Verifies DOM writes report stream_open failures, hide credentials, and honor suppression.
#[test]
fn dom_file_saves_report_registered_wrapper_open_failures_like_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class MissingWriteOpenDomWrapper {
    public $context;
}

class FalseWriteOpenDomWrapper {
    public $context;

    public function stream_open($path, $mode, $options, &$openedPath) {
        echo "O", $mode, ":", $options, "|";
        return false;
    }
}

stream_wrapper_register("missingwrite", MissingWriteOpenDomWrapper::class);
stream_wrapper_register("falsewrite", FalseWriteOpenDomWrapper::class);

$legacy = new DOMDocument();
$legacy->loadXML("<root/>");
var_dump($legacy->save("missingwrite://user:secret@host/out"));
var_dump($legacy->save("falsewrite://out"));
var_dump(@$legacy->save("missingwrite://suppressed"));

$modern = Dom\XMLDocument::createFromString("<root/>");
var_dump($modern->saveXmlFile("missingwrite://modern"));
unset($modern, $legacy);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(false)\nOwb:0|bool(false)\nbool(false)\nbool(false)\n"
    );
    for warning in [
        "Warning: DOMDocument::save(missingwrite://...@host/out): Failed to open stream: \
\"MissingWriteOpenDomWrapper::stream_open\" is not implemented\n",
        "Warning: DOMDocument::save(falsewrite://out): Failed to open stream: \
\"FalseWriteOpenDomWrapper::stream_open\" call failed\n",
        "Warning: Dom\\XMLDocument::saveXmlFile(missingwrite://modern): \
Failed to open stream: \"MissingWriteOpenDomWrapper::stream_open\" is not implemented\n",
    ] {
        assert_eq!(
            out.stderr.matches(warning).count(),
            1,
            "expected one exact wrapper warning {warning:?}, got: {}",
            out.stderr
        );
    }
    assert!(
        !out.stderr.contains("missingwrite://suppressed"),
        "suppressed wrapper warning leaked: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected failed write wrappers to remain heap-clean, got: {}",
        out.stderr
    );
}

/// Verifies rejected url_stat results release their temporary boxes and wrapper objects.
#[test]
fn dom_file_read_stat_failures_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class MissingStatHeapDomWrapper {
    public $context;

    public function stream_open($path, $mode, $options, &$openedPath) {
        return true;
    }
}

class FalseStatHeapDomWrapper {
    public $context;

    public function url_stat($path, $flags) {
        return false;
    }

    public function stream_open($path, $mode, $options, &$openedPath) {
        return true;
    }
}

stream_wrapper_register("missingstatheap", MissingStatHeapDomWrapper::class);
stream_wrapper_register("falsestatheap", FalseStatHeapDomWrapper::class);
$document = new DOMDocument();
@$document->load("missingstatheap://source");
@$document->load("falsestatheap://source");
unset($document);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected failed stat callbacks to remain heap-clean, got: {}",
        out.stderr
    );
}

/// Verifies a rejected stat wrapper destructor can perform a nested DOM file read.
#[test]
fn dom_file_read_stat_failure_destructors_are_reentrant() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class MissingOuterStatDomWrapper {
    public $context;
    private static bool $reentered = false;

    public function stream_open($path, $mode, $options, &$openedPath) {
        return true;
    }

    public function __destruct() {
        echo "D|";
        if (!self::$reentered) {
            self::$reentered = true;
            $nested = new DOMDocument();
            echo "N:";
            var_dump(@$nested->load("missinginnerstat://source"));
            unset($nested);
        }
    }
}

class MissingInnerStatDomWrapper {
    public $context;

    public function stream_open($path, $mode, $options, &$openedPath) {
        return true;
    }

    public function __destruct() {
        echo "I|";
    }
}

stream_wrapper_register("missingouterstat", MissingOuterStatDomWrapper::class);
stream_wrapper_register("missinginnerstat", MissingInnerStatDomWrapper::class);
$document = new DOMDocument();
echo "O:";
var_dump($document->load("missingouterstat://source"));
unset($document);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "O:D|N:I|bool(false)\nbool(false)\n");
    assert_eq!(
        out.stderr
            .matches(
                "Warning: DOMDocument::load(): \
MissingOuterStatDomWrapper::url_stat is not implemented!\n"
            )
            .count(),
        1,
        "expected only the unsuppressed outer warning, got: {}",
        out.stderr
    );
    assert!(
        !out.stderr.contains("MissingInnerStatDomWrapper::url_stat"),
        "suppressed nested warning leaked: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected nested stat-failure destruction to remain heap-clean, got: {}",
        out.stderr
    );
}

/// Verifies DTD entity and notation accessors match php-src 8.5.8 semantics.
#[test]
fn dom_dtd_entities_and_notations_match_php_oracle() {
    let out = compile_and_run_capture(
        r#"<?php
function show_legacy(string $xml): void {
    $dom = new DOMDocument();
    $dom->loadXML($xml);
    $doctype = $dom->doctype;

    echo "L|cls=", get_class($doctype->entities), "|",
        get_class($doctype->notations), "\n";
    echo "L|len=", $doctype->entities->length, ",",
        $doctype->notations->length, "\n";

    // Canonical entity identity: same wrapper for repeated named lookups.
    $a = $doctype->entities->getNamedItem("sampleExternalPublicWithNotationName1");
    $b = $doctype->entities->getNamedItem("sampleExternalPublicWithNotationName1");
    echo "L|eid=", $a === $b ? "EQ" : "NE", "\n";

    // Notation fresh identity: each access yields a distinct non-identical wrapper.
    $n1 = $doctype->notations->getNamedItem("GIF");
    $n2 = $doctype->notations->getNamedItem("GIF");
    echo "L|nid=", $n1 === $n2 ? "EQ" : "NE", "\n";
    echo "L|ncls=", $n1 === null ? "NULL" : get_class($n1), "\n";
    if ($n1 instanceof DOMNotation) {
        echo "L|npub=", $n1->publicId, "|", $n1->systemId, "\n";
        echo "L|generic=", $n1->nodeType, "|", $n1->nodeName, "|",
            $n1->baseURI, "|", $n1->isConnected ? "T" : "F", "|",
            $n1->ownerDocument === null ? "D" : "X", "|",
            $n1->parentNode === null ? "P" : "X", "|",
            $n1->parentElement === null ? "E" : "X", "|",
            $n1->childNodes->length, "|", var_export($n1->nodeValue, true), "|",
            var_export($n1->textContent, true), "\n";
    } else {
        echo "L|npub=N/A\n";
    }

    // Namespace-ignored lookup: passing a namespace URI must not change result.
    $ns = $doctype->notations->getNamedItemNS("urn:nope", "GIF");
    echo "L|ns=", $ns === null ? "NULL" : ($ns === $n1 ? "EQ" : $ns->nodeName), "\n";

    // Scalar rules per DOMEntity_fields.phpt.
    $names = [
        "sampleExternalPublicWithNotationName1",
        "sampleExternalPublicWithNotationName2",
        "sampleExternalPublicWithoutNotationName1",
        "sampleExternalPublicWithoutNotationName2",
        "sampleExternalSystemWithNotationName",
        "sampleExternalSystemWithoutNotationName",
        "sampleInternalEntity",
    ];
    foreach ($names as $nm) {
        $e = $doctype->entities->getNamedItem($nm);
        if ($e instanceof DOMEntity) {
            echo "E|", $e->nodeName, "|", var_export($e->publicId, true), "|",
                var_export($e->systemId, true), "|",
                var_export($e->notationName, true), "\n";
        } else {
            echo "E|NULL|NULL|NULL|NULL\n";
        }
    }

    $deprecated = $doctype->entities->getNamedItem("sampleInternalEntity");
    if ($deprecated instanceof DOMEntity) {
        echo "D|", var_export($deprecated->actualEncoding, true), "|",
            var_export($deprecated->encoding, true), "|",
            var_export($deprecated->version, true), "\n";
    }

    // Notation item route via item() index.
    $by_idx = $doctype->notations->item(0);
    echo "I|", $by_idx === null ? "NULL" : $by_idx->nodeName, "\n";
    // Out-of-range index returns null.
    echo "O|", $doctype->entities->item(999) === null ? "NULL" : "VAL", "\n";

    // php-src magic dimensions use the DOM map handler, not ArrayAccess:
    // strings select getNamedItem(), integers select item(), and misses are null.
    $dimension_name = $doctype->entities["sampleInternalEntity"];
    $dimension_index = $doctype->entities[0];
    echo "L|dim=", $dimension_name === $doctype->entities->getNamedItem("sampleInternalEntity") ? "EQ" : "NE", "|",
        $dimension_index === $doctype->entities->item(0) ? "EQ" : "NE", "|",
        $doctype->entities["missing"] === null ? "NULL" : "VAL", "|",
        $doctype->entities[999] === null ? "NULL" : "VAL", "|",
        isset($doctype->entities["sampleInternalEntity"]) ? "T" : "F", "|",
        isset($doctype->entities["missing"]) ? "T" : "F", "\n";
    try {
        $dimension_map = $doctype->entities;
        $dimension_map["missing"] = null;
    } catch (Throwable $error) {
        echo "L|write=", get_class($error), ":", $error->getCode(), ":", $error->getMessage(), "\n";
    }
    try {
        $dimension_map = $doctype->entities;
        $dimension_map[] = null;
    } catch (Throwable $error) {
        echo "L|append=", get_class($error), ":", $error->getCode(), ":", $error->getMessage(), "\n";
    }
    try {
        $dimension_map = $doctype->entities;
        unset($dimension_map["sampleInternalEntity"]);
    } catch (Throwable $error) {
        echo "L|unset=", get_class($error), ":", $error->getCode(), ":", $error->getMessage(), "\n";
    }
    echo "L|effects=";
    try {
        $dimension_map = $doctype->entities;
        $dimension_map[dtd_dimension_key()] = dtd_dimension_value();
    } catch (Throwable $error) {
        echo "E";
    }
    echo "|";
    try {
        $dimension_map = $doctype->entities;
        $dimension_map[] = dtd_dimension_value();
    } catch (Throwable $error) {
        echo "E";
    }
    echo "|";
    try {
        $dimension_map = $doctype->entities;
        unset($dimension_map[dtd_dimension_key()]);
    } catch (Throwable $error) {
        echo "E";
    }
    echo "\n";
    $nullable_dimension_map = $doctype->entities;
    $nullable_dimension_map = null;
    echo "L|null=";
    $nullable_dimension_map[dtd_dimension_key()] = dtd_dimension_value();
    echo gettype($nullable_dimension_map), "|";
    $nullable_dimension_map = null;
    unset($nullable_dimension_map[dtd_dimension_key()]);
    echo "N\n";

    // Entity declarations participate in the document-position tree before
    // document children, as asserted by php-src DOMEntity position tests.
    $position_entity = $doctype->entities->getNamedItem("sampleInternalEntity");
    $position_element = $dom->documentElement;
    if (!$position_entity instanceof DOMNode) {
        throw new Exception("position entity missing");
    }
    if (!$position_element instanceof DOMElement) {
        throw new Exception("position element missing");
    }
    echo "L|pos=", $position_entity->compareDocumentPosition($position_element), "|",
        $position_element->compareDocumentPosition($position_entity), "\n";

    // Retained lifetime: the notation wrapper survives the doctype reference.
    $keep = $doctype->notations->getNamedItem("GIF");
    unset($doctype, $dom);
    if ($keep instanceof DOMNotation) {
        echo "K|", $keep->nodeName, ":", $keep->publicId, "\n";
    } else {
        echo "K|NULL\n";
    }
}

function show_modern(string $xml): void {
    $dom = Dom\XMLDocument::createFromString($xml);
    $doctype = $dom->doctype;

    echo "M|cls=", get_class($doctype->entities), "|",
        get_class($doctype->notations), "\n";
    echo "M|len=", $doctype->entities->length, ",",
        $doctype->notations->length, "\n";
    $internal = $doctype->entities->getNamedItem("sampleInternalEntity");
    if ($internal instanceof Dom\Entity) {
        echo "M|test|", var_export($internal->publicId, true), "\n";
    } else {
        echo "M|test|NULL\n";
    }
    $gif = $doctype->notations->getNamedItem("GIF");
    echo "M|gif|", $gif === null ? "NULL" : $gif->nodeName, "\n";
    if ($gif instanceof Dom\Notation) {
        echo "M|generic=", $gif->baseURI, "|", $gif->isConnected ? "T" : "F", "|",
            $gif->ownerDocument === null ? "D" : "X", "|",
            var_export($gif->textContent, true), "\n";
    }
    $modern_entity_name = $doctype->entities["sampleInternalEntity"];
    $modern_entity_index = $doctype->entities[0];
    $modern_notation_name = $doctype->notations["GIF"];
    $modern_notation_index = $doctype->notations[0];
    echo "M|dim=", $modern_entity_name === $doctype->entities->getNamedItem("sampleInternalEntity") ? "EQ" : "NE", "|",
        $modern_entity_index === $doctype->entities->item(0) ? "EQ" : "NE", "|",
        $modern_notation_name instanceof Dom\Notation ? $modern_notation_name->nodeName : "NULL", "|",
        $modern_notation_index instanceof Dom\Notation ? $modern_notation_index->nodeName : "NULL", "|",
        $doctype->notations["missing"] === null ? "NULL" : "VAL", "|",
        $doctype->notations[999] === null ? "NULL" : "VAL", "|",
        isset($doctype->notations["GIF"]) ? "T" : "F", "|",
        isset($doctype->notations["missing"]) ? "T" : "F", "\n";
    try {
        $dimension_map = $doctype->entities;
        $dimension_map["missing"] = null;
    } catch (Throwable $error) {
        echo "M|entity-write=", get_class($error), ":", $error->getCode(), ":", $error->getMessage(), "\n";
    }
    try {
        $dimension_map = $doctype->entities;
        $dimension_map[] = null;
    } catch (Throwable $error) {
        echo "M|entity-append=", get_class($error), ":", $error->getCode(), ":", $error->getMessage(), "\n";
    }
    try {
        $dimension_map = $doctype->entities;
        unset($dimension_map["sampleInternalEntity"]);
    } catch (Throwable $error) {
        echo "M|entity-unset=", get_class($error), ":", $error->getCode(), ":", $error->getMessage(), "\n";
    }
    try {
        $notation_dimension_map = $doctype->notations;
        $notation_dimension_map["missing"] = null;
    } catch (Throwable $error) {
        echo "M|dtd-write=", get_class($error), ":", $error->getCode(), ":", $error->getMessage(), "\n";
    }
    try {
        $notation_dimension_map = $doctype->notations;
        $notation_dimension_map[] = null;
    } catch (Throwable $error) {
        echo "M|dtd-append=", get_class($error), ":", $error->getCode(), ":", $error->getMessage(), "\n";
    }
    try {
        $notation_dimension_map = $doctype->notations;
        unset($notation_dimension_map["GIF"]);
    } catch (Throwable $error) {
        echo "M|dtd-unset=", get_class($error), ":", $error->getCode(), ":", $error->getMessage(), "\n";
    }
    $iterator = $doctype->notations->getIterator();
    $first = $iterator->current();
    $second = $iterator->current();
    $direct = $doctype->notations->item(0);
    $other_iterator = $doctype->notations->getIterator();
    $other = $other_iterator->current();
    echo "M|iter|", $iterator->key(), "|",
        $first instanceof Dom\Notation ? $first->nodeName : "NULL", "|",
        $iterator->valid() ? "T" : "F", "|",
        $first === $second ? "EQ" : "NE", "|",
        $first === $direct ? "EQ" : "NE", "|",
        $first === $other ? "EQ" : "NE", "\n";
}

function dtd_dimension_key(): string {
    echo "K";
    return "key";
}

function dtd_dimension_value(): int {
    echo "V";
    return 42;
}

$xml = <<<XML
<?xml version="1.0"?>
<!DOCTYPE root [
    <!ENTITY sampleInternalEntity "This is a sample entity value.">
    <!ENTITY sampleExternalSystemWithNotationName SYSTEM "external.stuff" NDATA stuff>
    <!ENTITY sampleExternalSystemWithoutNotationName SYSTEM "external.stuff" NDATA >
    <!ENTITY sampleExternalPublicWithNotationName1 PUBLIC "public id" "external.stuff" NDATA stuff>
    <!ENTITY sampleExternalPublicWithNotationName2 PUBLIC "" "external.stuff" NDATA stuff>
    <!ENTITY sampleExternalPublicWithoutNotationName1 PUBLIC "public id" "external.stuff" NDATA >
    <!ENTITY sampleExternalPublicWithoutNotationName2 PUBLIC "" "external.stuff" NDATA >
    <!NOTATION GIF SYSTEM "viewgif.exe">
]>
<root/>
XML;

show_legacy($xml);
show_modern($xml);
"#,
    );
    assert!(
        out.success,
        "program failed after stdout {:?}: {}",
        out.stdout,
        out.stderr
    );
    let expected = concat!(
        "L|cls=DOMNamedNodeMap|DOMNamedNodeMap\n",
        "L|len=7,1\n",
        "L|eid=EQ\n",
        "L|nid=NE\n",
        "L|ncls=DOMNotation\n",
        "L|npub=|viewgif.exe\n",
        "L|generic=12|GIF||F|D|P|E|0|NULL|''\n",
        "L|ns=GIF\n",
        "E|sampleExternalPublicWithNotationName1|'public id'|'external.stuff'|'stuff'\n",
        "E|sampleExternalPublicWithNotationName2|''|'external.stuff'|'stuff'\n",
        "E|sampleExternalPublicWithoutNotationName1|'public id'|'external.stuff'|''\n",
        "E|sampleExternalPublicWithoutNotationName2|''|'external.stuff'|''\n",
        "E|sampleExternalSystemWithNotationName|NULL|'external.stuff'|'stuff'\n",
        "E|sampleExternalSystemWithoutNotationName|NULL|'external.stuff'|''\n",
        "E|sampleInternalEntity|NULL|NULL|NULL\n",
        "D|NULL|NULL|NULL\n",
        "I|GIF\n",
        "O|NULL\n",
        "L|dim=EQ|EQ|NULL|NULL|T|F\n",
        "L|write=Error:0:Cannot use object of type DOMNamedNodeMap as array\n",
        "L|append=Error:0:Cannot use object of type DOMNamedNodeMap as array\n",
        "L|unset=Error:0:Cannot use object of type DOMNamedNodeMap as array\n",
        "L|effects=KVE|VE|KE\n",
        "L|null=KVarray|KN\n",
        "L|pos=4|2\n",
        "K|GIF:\n",
        "M|cls=Dom\\DtdNamedNodeMap|Dom\\DtdNamedNodeMap\n",
        "M|len=7,1\n",
        "M|test|NULL\n",
        "M|gif|GIF\n",
        "M|generic=about:blank|F|D|''\n",
        "M|dim=EQ|EQ|GIF|GIF|NULL|NULL|T|F\n",
        "M|entity-write=Error:0:Cannot use object of type Dom\\DtdNamedNodeMap as array\n",
        "M|entity-append=Error:0:Cannot use object of type Dom\\DtdNamedNodeMap as array\n",
        "M|entity-unset=Error:0:Cannot use object of type Dom\\DtdNamedNodeMap as array\n",
        "M|dtd-write=Error:0:Cannot use object of type Dom\\DtdNamedNodeMap as array\n",
        "M|dtd-append=Error:0:Cannot use object of type Dom\\DtdNamedNodeMap as array\n",
        "M|dtd-unset=Error:0:Cannot use object of type Dom\\DtdNamedNodeMap as array\n",
        "M|iter|GIF|GIF|T|EQ|NE|NE\n",
    );
    assert_eq!(out.stdout, expected, "unexpected stdout:\n{}", out.stdout);
    for warning in [
        "Deprecated: Property DOMEntity::$actualEncoding is deprecated\n",
        "Deprecated: Property DOMEntity::$encoding is deprecated\n",
        "Deprecated: Property DOMEntity::$version is deprecated\n",
    ] {
        assert!(
            out.stderr.contains(warning),
            "expected DOMEntity deprecation warning {warning:?}, got: {}",
            out.stderr,
        );
    }
}

/// Verifies retained DTD maps, declarations, and iterator values release every owned wrapper.
#[test]
fn dom_dtd_entity_and_notation_wrappers_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$legacy = new DOMDocument();
$legacy->loadXML('<!DOCTYPE root [<!ENTITY e "value"><!NOTATION n SYSTEM "n.sys">]><root/>');
$legacyType = $legacy->doctype;
$legacyEntity = $legacyType->entities->getNamedItem("e");
$legacyNotation = $legacyType->notations->getNamedItem("n");
if ($legacyEntity instanceof DOMEntity && $legacyNotation instanceof DOMNotation) {
    echo $legacyEntity->nodeName, "|", $legacyNotation->nodeName, "\n";
}

$modern = Dom\XMLDocument::createFromString(
    '<!DOCTYPE root [<!ENTITY e "value"><!NOTATION n SYSTEM "n.sys">]><root/>'
);
$iterator = $modern->doctype->notations->getIterator();
$first = $iterator->current();
$second = $iterator->current();
$direct = $modern->doctype->notations->item(0);
if ($first instanceof Dom\Notation && $direct instanceof Dom\Notation) {
    echo $first->nodeName, "|", $first === $second ? "EQ" : "NE", "|",
        $first === $direct ? "EQ" : "NE", "\n";
}
"#,
    );
    assert!(out.success, "DTD lifetime program failed: {}", out.stderr);
    assert_eq!(out.stdout, "e|n\nn|EQ|NE\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected DTD wrappers to release every owned allocation, got: {}",
        out.stderr
    );
}

/// Verifies DTD declaration nodes preserve php-src's metadata and readonly edge cases.
#[test]
fn dom_dtd_node_metadata_and_readonly_rules_match_php() {
    let out = compile_and_run_capture(
        r#"<?php
$legacy = new DOMDocument();
$legacy->loadXML(
    '<!DOCTYPE r [<!ENTITY e "replace"><!NOTATION n SYSTEM "n.sys">]><r/>'
);
$legacyEntity = $legacy->doctype->entities->item(0);
$legacyNotation = $legacy->doctype->notations->item(0);
if (!$legacyEntity instanceof DOMEntity || !$legacyNotation instanceof DOMNotation) {
    throw new Exception("legacy DTD declarations missing");
}

echo "LE|", var_export($legacyEntity->namespaceURI, true), "|",
    var_export($legacyEntity->prefix, true), "|",
    var_export($legacyEntity->textContent, true), "|",
    $legacyEntity->hasAttributes() ? "T" : "F", "|",
    $legacyEntity->getLineNo(), "|",
    var_export($legacyEntity->getNodePath(), true), "|",
    var_export($legacyEntity->lookupNamespaceURI(null), true), "|",
    var_export($legacyEntity->lookupPrefix("urn:x"), true), "|",
    $legacyEntity->isDefaultNamespace(null) ? "T" : "F", "|",
    $legacyEntity->cloneNode(false) === false ? "F" : "X", "\n";
$legacyEntity->textContent = "mut";
echo "LE|set=", var_export($legacyEntity->textContent, true), "\n";

echo "LN|", var_export($legacyNotation->namespaceURI, true), "|",
    var_export($legacyNotation->prefix, true), "|",
    var_export($legacyNotation->textContent, true), "|",
    $legacyNotation->hasAttributes() ? "T" : "F", "|",
    $legacyNotation->getLineNo(), "|",
    var_export($legacyNotation->getNodePath(), true), "|",
    var_export($legacyNotation->lookupNamespaceURI(null), true), "|",
    var_export($legacyNotation->lookupPrefix("urn:x"), true), "|",
    $legacyNotation->isDefaultNamespace(null) ? "T" : "F", "|",
    $legacyNotation->cloneNode(false) === false ? "F" : "X", "\n";
$legacyNotation->textContent = "mut";
echo "LN|set=", var_export($legacyNotation->textContent, true), "\n";

$modern = Dom\XMLDocument::createFromString(
    '<!DOCTYPE r [<!ENTITY e "replace"><!NOTATION n SYSTEM "n.sys">]><r/>'
);
$modernEntity = $modern->doctype->entities["e"];
$modernNotation = $modern->doctype->notations["n"];
if (!$modernEntity instanceof Dom\Entity || !$modernNotation instanceof Dom\Notation) {
    throw new Exception("modern DTD declarations missing");
}

echo "ME|", var_export($modernEntity->textContent, true), "|",
    $modernEntity->getLineNo(), "|";
try {
    $modernEntity->getNodePath();
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getCode(), ":", $error->getMessage();
}
echo "|", var_export($modernEntity->lookupNamespaceURI(null), true), "|",
    var_export($modernEntity->lookupPrefix("urn:x"), true), "|",
    $modernEntity->isDefaultNamespace(null) ? "T" : "F", "|",
    $modernEntity->cloneNode(false) === false ? "F" : "X", "\n";
try {
    $modernEntity->textContent = "mut";
} catch (Throwable $error) {
    echo "ME|set=", get_class($error), ":", $error->getMessage(), "\n";
}

try {
    echo $modernNotation->publicId;
} catch (Throwable $error) {
    echo "MN|publicId=", get_class($error), ":", $error->getMessage(), "\n";
}
try {
    echo $modernNotation->systemId;
} catch (Throwable $error) {
    echo "MN|systemId=", get_class($error), ":", $error->getMessage(), "\n";
}
echo "MN|", var_export($modernNotation->textContent, true), "|",
    $modernNotation->getLineNo(), "|";
try {
    $modernNotation->getNodePath();
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getCode(), ":", $error->getMessage();
}
echo "|", var_export($modernNotation->lookupNamespaceURI(null), true), "|",
    var_export($modernNotation->lookupPrefix("urn:x"), true), "|",
    $modernNotation->isDefaultNamespace(null) ? "T" : "F", "|",
    $modernNotation->cloneNode(false) === false ? "F" : "X", "\n";
try {
    $modernNotation->textContent = "mut";
} catch (Throwable $error) {
    echo "MN|set=", get_class($error), ":", $error->getMessage(), "\n";
}
"#,
    );
    assert!(
        out.success,
        "DTD metadata program failed after stdout {:?}: {}",
        out.stdout,
        out.stderr
    );
    assert_eq!(
        out.stdout,
        concat!(
            "LE|NULL|''|''|F|-1|NULL|NULL|NULL|F|F\n",
            "LE|set=''\n",
            "LN|NULL|''|''|F|-1|NULL|NULL|NULL|F|F\n",
            "LN|set=''\n",
            "ME|NULL|-1|DOMException:11:Invalid State Error|NULL|NULL|T|F\n",
            "ME|set=Error:Cannot modify readonly property Dom\\Entity::$textContent\n",
            "MN|publicId=Error:Typed property Dom\\Notation::$publicId must not be accessed before initialization\n",
            "MN|systemId=Error:Typed property Dom\\Notation::$systemId must not be accessed before initialization\n",
            "MN|''|-1|DOMException:11:Invalid State Error|NULL|NULL|T|F\n",
            "MN|set=Error:Cannot modify readonly property Dom\\Notation::$textContent\n",
        )
    );
    let warning =
        "Deprecated: DOMNode::isDefaultNamespace(): Passing null to parameter #1 ($namespace) of type string is deprecated\n";
    assert_eq!(
        out.stderr.matches(warning).count(),
        2,
        "expected one null-namespace deprecation per legacy DTD wrapper: {}",
        out.stderr
    );
}

/// Verifies every inherited tree-mutation result for legacy and modern DTD declaration nodes.
#[test]
fn dom_dtd_tree_mutation_results_match_php() {
    let out = compile_and_run_capture(
        r#"<?php
function showDtdMutation(string $label, callable $operation): void
{
    echo $label, "=";
    try {
        $operation();
    } catch (Throwable $error) {
        echo get_class($error), ":", $error->getCode(), ":", $error->getMessage();
    }
    echo "\n";
}

$legacy = new DOMDocument();
$legacy->loadXML(
    '<!DOCTYPE r [<!ENTITY e "replace"><!NOTATION n SYSTEM "n.sys">]><r/>'
);
$legacyEntity = $legacy->doctype->entities->item(0);
if (!$legacyEntity instanceof DOMEntity) {
    throw new Exception("legacy entity missing");
}
$legacyNotation = $legacy->doctype->notations->item(0);
if (!$legacyNotation instanceof DOMNotation) {
    throw new Exception("legacy notation missing");
}
$legacyChild = $legacy->createElement("x");
if (!$legacyChild instanceof DOMElement) {
    throw new Exception("legacy child missing");
}
$legacyRoot = $legacy->documentElement;
if (!$legacyRoot instanceof DOMElement) {
    throw new Exception("legacy root missing");
}

showDtdMutation("LE|append", function () use ($legacyEntity, $legacyChild): void {
    $result = $legacyEntity->appendChild($legacyChild);
    echo $result === false ? "false" : "object";
});
showDtdMutation("LE|insert", function () use ($legacyEntity, $legacyChild): void {
    $result = $legacyEntity->insertBefore($legacyChild, null);
    echo $result === false ? "false" : "object";
});
showDtdMutation("LE|replace", function () use ($legacyEntity, $legacyChild, $legacyRoot): void {
    $result = $legacyEntity->replaceChild($legacyChild, $legacyRoot);
    echo $result === false ? "false" : "object";
});
showDtdMutation("LE|remove", function () use ($legacyEntity, $legacyRoot): void {
    $result = $legacyEntity->removeChild($legacyRoot);
    echo $result === false ? "false" : "object";
});

showDtdMutation("LN|append", function () use ($legacyNotation, $legacyChild): void {
    $result = $legacyNotation->appendChild($legacyChild);
    echo $result === false ? "false" : "object";
});
showDtdMutation("LN|insert", function () use ($legacyNotation, $legacyChild): void {
    $result = $legacyNotation->insertBefore($legacyChild, null);
    echo $result === false ? "false" : "object";
});
showDtdMutation("LN|replace", function () use ($legacyNotation, $legacyChild, $legacyRoot): void {
    $result = $legacyNotation->replaceChild($legacyChild, $legacyRoot);
    echo $result === false ? "false" : "object";
});
showDtdMutation("LN|remove", function () use ($legacyNotation, $legacyRoot): void {
    $result = $legacyNotation->removeChild($legacyRoot);
    echo $result === false ? "false" : "object";
});

$modern = Dom\XMLDocument::createFromString(
    '<!DOCTYPE r [<!ENTITY e "replace"><!NOTATION n SYSTEM "n.sys">]><r/>'
);
$modernEntity = $modern->doctype->entities["e"];
if (!$modernEntity instanceof Dom\Entity) {
    throw new Exception("modern entity missing");
}
$modernNotation = $modern->doctype->notations["n"];
if (!$modernNotation instanceof Dom\Notation) {
    throw new Exception("modern notation missing");
}
$modernChild = $modern->createElement("x");
$modernRoot = $modern->documentElement;
if (!$modernRoot instanceof Dom\Element) {
    throw new Exception("modern root missing");
}

showDtdMutation("ME|append", function () use ($modernEntity, $modernChild): void {
    $modernEntity->appendChild($modernChild);
});
showDtdMutation("ME|insert", function () use ($modernEntity, $modernChild): void {
    $modernEntity->insertBefore($modernChild, null);
});
showDtdMutation("ME|replace", function () use ($modernEntity, $modernChild, $modernRoot): void {
    $modernEntity->replaceChild($modernChild, $modernRoot);
});
showDtdMutation("ME|remove", function () use ($modernEntity, $modernRoot): void {
    $modernEntity->removeChild($modernRoot);
});

showDtdMutation("MN|append", function () use ($modernNotation, $modernChild): void {
    $modernNotation->appendChild($modernChild);
});
showDtdMutation("MN|insert", function () use ($modernNotation, $modernChild): void {
    $modernNotation->insertBefore($modernChild, null);
});
showDtdMutation("MN|replace", function () use ($modernNotation, $modernChild, $modernRoot): void {
    $modernNotation->replaceChild($modernChild, $modernRoot);
});
showDtdMutation("MN|remove", function () use ($modernNotation, $modernRoot): void {
    $modernNotation->removeChild($modernRoot);
});
"#,
    );
    assert!(
        out.success,
        "DTD mutation program failed after stdout {:?}: {}",
        out.stdout,
        out.stderr
    );
    assert_eq!(
        out.stdout,
        concat!(
            "LE|append=DOMException:7:No Modification Allowed Error\n",
            "LE|insert=DOMException:7:No Modification Allowed Error\n",
            "LE|replace=false\n",
            "LE|remove=DOMException:8:Not Found Error\n",
            "LN|append=false\n",
            "LN|insert=false\n",
            "LN|replace=DOMException:4:Wrong Document Error\n",
            "LN|remove=DOMException:8:Not Found Error\n",
            "ME|append=DOMException:3:Hierarchy Request Error\n",
            "ME|insert=DOMException:3:Hierarchy Request Error\n",
            "ME|replace=DOMException:3:Hierarchy Request Error\n",
            "ME|remove=DOMException:8:Not Found Error\n",
            "MN|append=DOMException:3:Hierarchy Request Error\n",
            "MN|insert=DOMException:3:Hierarchy Request Error\n",
            "MN|replace=DOMException:4:Wrong Document Error\n",
            "MN|remove=DOMException:8:Not Found Error\n",
        )
    );
    assert_eq!(out.stderr, "");
}

/// Verifies legacy and modern HTML wrapper reads use the borrow-free file pipeline.
#[test]
fn dom_html_file_reads_use_registered_wrappers_and_high_bit_options() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class HtmlFileDomWrapper {
    public $context;
    private string $data = "<!doctype html><p>x";
    private int $offset = 0;

    public function url_stat($path, $flags) {
        echo "S", $flags, "|";
        return [];
    }

    public function stream_open($path, $mode, $options, &$openedPath) {
        echo "O", $mode, ":", $options, "|";
        return true;
    }

    public function stream_read($count) {
        echo "R", $count, "|";
        $chunk = substr($this->data, $this->offset);
        $this->offset += strlen($chunk);
        return $chunk;
    }

    public function stream_eof() {
        return $this->offset >= strlen($this->data);
    }

    public function stream_close() {
        echo "C|";
    }
}

stream_wrapper_register("htmlread", HtmlFileDomWrapper::class);
$legacy = new DOMDocument();
var_dump($legacy->loadHTMLFile("htmlread://legacy"));
echo $legacy->documentElement->nodeName, "|";

$modern = Dom\HTMLDocument::createFromFile(
    "htmlread://modern",
    Dom\HTML_NO_DEFAULT_NS
);
echo $modern->documentElement->nodeName, ":";
echo $modern->documentElement->namespaceURI === null ? "N" : "X";
unset($modern, $legacy);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "S2|Orb:0|R8192|R8192|C|bool(true)\n",
            "html|S2|Orb:0|R8192|R8192|C|html:N",
        )
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected HTML wrapper reads to remain heap-clean, got: {}",
        out.stderr
    );
}

/// Verifies the shared `InternalIterator` wrapper over DOM live collections.
/// Covers all six supported classes, numeric vs `nodeName` keys, fresh repeated
/// iterators, live append, token-list mutation, empty collections, iterator class
/// identity, and the rewind-before-next rule.
#[test]
fn dom_internal_iterator_over_live_collections() {
    let out = compile_and_run(
        r#"<?php
function dump_values(Iterator|ArrayIterator $it, string $label): void {
    echo $label, ":";
    $it->rewind();
    $first = true;
    while ($it->valid()) {
        if (!$first) {
            echo ";";
        }
        $first = false;
        $cur = $it->current();
        $key = $it->key();
        echo (is_string($key) ? "s:$key" : "i:$key"), "=";
        if (is_string($cur)) {
            echo "str:$cur";
        } else {
            echo get_class($cur);
        }
        $it->next();
    }
    echo "\n";
}

$legacy = new DOMDocument();
$legacy->loadHTML('<!doctype html><html><body><p class="a" id="p1">X</p><p class="a" id="p2">Y</p></body></html>');
$body_legacy = $legacy->getElementsByTagName("body")->item(0);
if (!$body_legacy instanceof DOMElement) {
    throw new Exception("legacy body missing");
}
$p_list = $body_legacy->getElementsByTagName("p");
$p_el = $p_list->item(0);
if (!$p_el instanceof DOMElement) {
    throw new Exception("legacy p missing");
}
$p_attrs = $p_el->attributes;

dump_values($p_list->getIterator(), "NL");
dump_values($p_attrs->getIterator(), "NNM");

$modern = Dom\HTMLDocument::createFromString('<!doctype html><html><body><p class="a" id="p1">X</p><p class="a" id="p2">Y</p></body></html>');
$body_modern = $modern->getElementsByTagName("body")->item(0);
if (!$body_modern instanceof Dom\HTMLElement) {
    throw new Exception("modern body missing");
}
$mod_p_list = $body_modern->getElementsByTagName("p");
$mod_p_el = $mod_p_list->item(0);
if (!$mod_p_el instanceof Dom\Element) {
    throw new Exception("modern p missing");
}
$mod_p_attrs = $mod_p_el->attributes;
$mod_collection = $body_modern->getElementsByClassName("a");
$mod_tokens = $mod_p_el->classList;

dump_values($mod_p_list->getIterator(), "DNL");
dump_values($mod_p_attrs->getIterator(), "DNNM");
dump_values($mod_collection->getIterator(), "DHC");
dump_values($mod_tokens->getIterator(), "DTL");

$empty_legacy = $body_legacy->getElementsByTagName("span");
$empty_modern = $body_modern->getElementsByTagName("span");
dump_values($empty_legacy->getIterator(), "EL");
dump_values($empty_modern->getIterator(), "EM");

$it1 = $p_list->getIterator();
$it2 = $p_list->getIterator();
$it1->next();
$it2->next();
$it2->next();
echo "cls:", get_class($it1), "|", $it1->key(), "|", $it2->key(), "\n";

$new_p = $legacy->createElement("p");
if (!$new_p instanceof DOMElement) {
    throw new Exception("createElement failed");
}
$body_legacy->appendChild($new_p);
$live_it = $p_list->getIterator();
$live_it->next();
$live_it->next();
echo "live:", ($live_it->valid() ? "y" : "n"), "=", get_class($live_it->current()), "\n";

$el = $body_modern->getElementsByTagName("p")->item(0);
if (!$el instanceof Dom\Element) {
    throw new Exception("modern p2 missing");
}
$tokens = $el->classList;
$tokens->add("b");
$tok_it = $tokens->getIterator();
echo "tok:";
while ($tok_it->valid()) {
    echo $tok_it->current(), ",";
    $tok_it->next();
}
echo "\n";

$rewind_it = $p_list->getIterator();
$rewind_it->rewind();
$rewind_it->rewind();
$rewind_it->next();
try {
    $rewind_it->rewind();
    echo "rewind:noerr\n";
} catch (Error $e) {
    echo "rewind:", $e->getMessage(), "\n";
}

$fresh = $p_list->getIterator();
$fresh->rewind();
$fresh->rewind();
echo "freshok\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "NL:i:0=DOMElement;i:1=DOMElement\n",
            "NNM:s:class=DOMAttr;s:id=DOMAttr\n",
            "DNL:i:0=Dom\\HTMLElement;i:1=Dom\\HTMLElement\n",
            "DNNM:s:class=Dom\\Attr;s:id=Dom\\Attr\n",
            "DHC:i:0=Dom\\HTMLElement;i:1=Dom\\HTMLElement\n",
            "DTL:i:0=str:a\n",
            "EL:\n",
            "EM:\n",
            "cls:InternalIterator|1|2\n",
            "live:y=DOMElement\n",
            "tok:a,b,\n",
            "rewind:Iterator does not support rewinding\n",
            "freshok\n",
        )
    );
}

/// Verifies foreach reaches an empty XPath result through the non-null IteratorAggregate slot.
#[test]
fn dom_xpath_empty_node_list_foreach_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$document = new DOMDocument();
$document->loadXML('<root><child/></root>');
$xpath = new DOMXPath($document);
foreach ($xpath->query('/root/missing') as $child) {
    var_dump($child);
}
echo "okey\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "okey\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected DOM foreach dispatch to stay heap-clean, got: {}",
        out.stderr
    );
}

/// Regression: a native DOM wrapper returned as `Mixed` dispatches directly through the bridge.
///
/// `InternalIterator::current()` deliberately exposes a boxed `Mixed` value. The
/// chained `getAttribute()` call must select the DOMElement candidate and invoke
/// its native opcode rather than dereferencing the absent PHP-method vtable slot.
#[test]
fn dom_mixed_iterator_current_dispatches_native_wrapper_method() {
    let out = compile_and_run(
        r#"<?php
$document = new DOMDocument();
$document->loadXML('<root><item id="first"/></root>');
$iterator = $document->getElementsByTagName("item")->getIterator();
echo $iterator->current()->getAttribute("id");
"#,
    );
    assert_eq!(out, "first");
}

/// Verifies a mixed DOM candidate rejects a colliding userland object argument exactly like PHP.
#[test]
fn dom_mixed_method_collision_reports_internal_parameter_type_error() {
    let out = compile_and_run(
        r#"<?php
class Request {}

function boxed(mixed $value): mixed {
    return $value;
}

$element = Dom\XMLDocument::createFromString("<root/>")->documentElement;
try {
    boxed($element)->matches(new Request());
} catch (TypeError $error) {
    echo $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "Dom\\Element::matches(): Argument #1 ($selectors) must be of type string, Request given"
    );
}

/// Verifies internal string parameters apply PHP's weak `Stringable` coercion.
#[test]
fn dom_mixed_method_collision_accepts_stringable_object_argument() {
    let out = compile_and_run(
        r#"<?php
class Selector {
    public int $calls = 0;

    public function __toString(): string {
        $this->calls = $this->calls + 1;
        return "root";
    }
}

function boxed(mixed $value): mixed {
    return $value;
}

$element = Dom\XMLDocument::createFromString("<root/>")->documentElement;
$selector = new Selector();
echo (boxed($element)->matches($selector) ? "yes" : "no") . ":" . $selector->calls;
"#,
    );
    assert_eq!(out, "yes:1");
}

/// Verifies a boxed `Mixed` argument dispatches through its runtime `Stringable` class once.
#[test]
fn dom_mixed_method_accepts_boxed_stringable_argument() {
    let out = compile_and_run(
        r#"<?php
class DynamicSelector {
    public int $calls = 0;

    public function __toString(): string {
        $this->calls = $this->calls + 1;
        return "root";
    }
}

function boxed_dynamic_selector(mixed $value): mixed {
    return $value;
}

$element = Dom\XMLDocument::createFromString("<root/>")->documentElement;
$selector = new DynamicSelector();
echo (boxed_dynamic_selector($element)->matches(boxed_dynamic_selector($selector))
    ? "yes"
    : "no") . ":" . $selector->calls;
"#,
    );
    assert_eq!(out, "yes:1");
}

/// Verifies a boxed `Stringable` follows PHP's weak coercion for DOM string properties.
#[test]
fn dom_string_property_accepts_boxed_stringable_value() {
    let out = compile_and_run(
        r#"<?php
class DomVersionToken {
    public int $calls = 0;

    public function __toString(): string {
        $this->calls = $this->calls + 1;
        return "1.1";
    }
}

function boxed_dom_version(mixed $value): mixed {
    return $value;
}

$document = new DOMDocument();
$version = new DomVersionToken();
$document->xmlVersion = boxed_dom_version($version);
echo $document->xmlVersion . ":" . $version->calls;
"#,
    );
    assert_eq!(out, "1.1:1");
}

/// Verifies multiple Stringable DOM arguments retain order and run each coercion once.
#[test]
fn dom_mixed_method_prepares_multiple_stringable_arguments_once() {
    let out = compile_and_run(
        r#"<?php
class Token {
    public int $calls = 0;

    public function __construct(public string $value) {}

    public function __toString(): string {
        $this->calls = $this->calls + 1;
        return $this->value;
    }
}

function boxed(mixed $value): mixed {
    return $value;
}

$element = Dom\XMLDocument::createFromString("<root/>")->documentElement;
$namespace = new Token("urn:test");
$name = new Token("p:id");
$value = new Token("value");
boxed($element)->setAttributeNS($namespace, $name, $value);
echo $element->getAttributeNS("urn:test", "id")
    . ":" . $namespace->calls . ":" . $name->calls . ":" . $value->calls;
"#,
    );
    assert_eq!(out, "value:1:1:1");
}

/// Verifies later dynamic failures retain earlier coercion order and exact PHP diagnostics.
#[test]
fn dom_mixed_method_stringable_preflight_preserves_type_error_order() {
    let out = compile_and_run(
        r#"<?php
class OrderedDomToken {
    public function __construct(public string $value) {}

    public function __toString(): string {
        echo $this->value;
        return $this->value;
    }
}

class RejectedDomValue {}

function boxed_ordered_dom_value(mixed $value): mixed {
    return $value;
}

$element = Dom\XMLDocument::createFromString("<root/>")->documentElement;
try {
    boxed_ordered_dom_value($element)->setAttributeNS(
        boxed_ordered_dom_value(new OrderedDomToken("N")),
        boxed_ordered_dom_value(new OrderedDomToken("Q")),
        boxed_ordered_dom_value(new RejectedDomValue()),
    );
} catch (TypeError $error) {
    echo "|" . $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "NQ|Dom\\Element::setAttributeNS(): Argument #3 ($value) must be of type string, RejectedDomValue given"
    );
}

/// Verifies a later boxed `Stringable` throw releases earlier staged strings.
#[test]
fn dom_mixed_method_stringable_throw_unwinds_staged_strings() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class MixedDomValue {
    public mixed $value = null;
}

class RetainedDomToken {
    public function __toString(): string {
        return str_repeat("r", 24);
    }
}

class ThrowingMixedDomToken {
    public function __toString(): string {
        throw new Exception("stop");
    }
}

$document = Dom\XMLDocument::createFromString("<root/>");
$element = $document->documentElement;
$retained = new MixedDomValue();
$retained->value = new RetainedDomToken();
$throwing = new MixedDomValue();
$throwing->value = new ThrowingMixedDomToken();
for ($index = 0; $index < 8; $index++) {
    try {
        $element->setAttributeNS($retained->value, $throwing->value, "value");
    } catch (Exception $error) {
        echo ".";
    }
}
unset($error, $retained, $throwing, $element, $document);
echo "|done";
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stdout, "........|done");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected mixed DOM Stringable unwinding to remain heap-clean, got: {}",
        out.stderr
    );
}

/// Verifies legacy registerNodeClass materializes a mapped grandchild with canonical identity.
#[test]
fn dom_register_node_class_materializes_legacy_grandchild_wrappers() {
    let out = compile_and_run(
        r#"<?php
class SpecialElement extends DOMElement {}
class FinalElement extends SpecialElement {}

$document = new DOMDocument();
echo $document->registerNodeClass(DOMElement::class, FinalElement::class)
    ? "true\n"
    : "false\n";
$document->loadXML("<root><child/></root>");
$root = $document->documentElement;
echo get_class($root) . ":" . $root->tagName . ":"
    . get_class($root->firstElementChild) . ":"
    . ($root === $document->documentElement ? "same" : "different");
"#,
    );
    assert_eq!(out, "true\nFinalElement:root:FinalElement:same");
}

/// Verifies modern registerNodeClass returns void and materializes mapped element wrappers.
#[test]
fn dom_register_node_class_materializes_modern_wrappers() {
    let out = compile_and_run(
        r#"<?php
class ModernElement extends Dom\Element {}

$document = Dom\XMLDocument::createFromString("<root><child/></root>");
$document->registerNodeClass(Dom\Element::class, ModernElement::class);
$root = $document->documentElement;
echo get_class($root) . ":" . $root->localName . ":"
    . get_class($root->firstElementChild);
"#,
    );
    assert_eq!(out, "ModernElement:root:ModernElement");
}

/// Verifies registered wrappers preserve identity, leave weak caches, and release cleanly.
#[test]
fn dom_register_node_class_reinserts_released_wrappers_heap_cleanly() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class RegisterFinalElement extends DOMElement {}

$document = new DOMDocument();
$document->loadXML("<root/>");
$document->registerNodeClass(DOMElement::class, RegisterFinalElement::class);

$root = $document->documentElement;
if (!$root instanceof DOMElement) {
    exit(1);
}
$rootAgain = $document->documentElement;
$firstId = spl_object_id($root);
echo get_class($root), ":", $root->tagName, ":",
    $root === $rootAgain ? "same" : "different";

unset($root);
unset($rootAgain);

$spacer = new RegisterFinalElement("spacer");
$freshRoot = $document->documentElement;
if (!$freshRoot instanceof DOMElement) {
    exit(2);
}
echo "|", get_class($freshRoot), ":", $spacer->tagName, ":",
    $firstId !== spl_object_id($freshRoot) ? "fresh" : "reused", ":",
    $freshRoot === $document->documentElement ? "same" : "different";

unset($freshRoot);
unset($spacer);
unset($document);
echo "|done";
"#,
    );

    assert!(out.success, "program failed: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected registered DOM wrappers and cache entries to release cleanly, stdout={:?}, stderr={}",
        out.stdout,
        out.stderr,
    );
    assert_eq!(
        out.stdout,
        "RegisterFinalElement:root:same|RegisterFinalElement:spacer:fresh:same|done"
    );
}

/// Regression: virtual DOM properties on `Mixed` use bridge contracts without shadowing user slots.
#[test]
fn dom_mixed_receiver_dispatches_native_virtual_properties() {
    let out = compile_and_run(
        r#"<?php
class ShadowNodeName {
    public string $nodeName = "shadow";
}

function boxed(mixed $value): mixed {
    return $value;
}

$document = new DOMDocument();
$document->loadXML('<!DOCTYPE root [<!ENTITY e "value">]><root/>');
$element = $document->documentElement;
$mixedElement = boxed($element);
echo $mixedElement->nodeName, "|", get_class($mixedElement->ownerDocument), "\n";

$mixedDoctype = boxed($document->doctype);
echo get_class($mixedDoctype->entities), "|", $mixedDoctype->entities->length, "\n";

$shadow = boxed(new ShadowNodeName());
echo $shadow->nodeName, "\n";
"#,
    );
    assert_eq!(out, "root|DOMDocument\nDOMNamedNodeMap|1\nshadow\n");
}

/// Verifies that SplFixedArray continues to use the shared InternalIterator wrapper
/// after the DOM collection expansion, including numeric keys and rewind rules.
#[test]
fn spl_fixed_array_internal_iterator_unchanged() {
    let out = compile_and_run(
        r#"<?php
$fixed = new SplFixedArray(3);
$fixed[0] = "a";
$fixed[1] = "b";
$fixed[2] = "c";
$it = $fixed->getIterator();
echo get_class($it), "\n";
$it->rewind();
while ($it->valid()) {
    echo $it->key(), "=", $it->current(), "\n";
    $it->next();
}
$it2 = $fixed->getIterator();
$it2->rewind();
$it2->rewind();
$it2->next();
$it2->rewind();
echo "rewind-ok\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "InternalIterator\n",
            "0=a\n",
            "1=b\n",
            "2=c\n",
            "rewind-ok\n",
        )
    );
}

/// Regression: a nonempty DOMNodeList cursor stops at `count` and becomes
/// valid again after a live append lands exactly at the previous end.
#[test]
fn dom_internal_iterator_live_node_list_repeated_next_and_append() {
    let out = compile_and_run(
        r#"<?php
$doc = new DOMDocument();
$doc->loadHTML('<div><span id="a"></span><span id="b"></span></div>');
$nl = $doc->getElementsByTagName("span");
$it = $nl->getIterator();
$it->next();
$it->next();
echo "end:k=" . $it->key() . ",v=" . ($it->valid() ? "1" : "0") . "\n";
$it->next();
echo "again:k=" . $it->key() . ",v=" . ($it->valid() ? "1" : "0") . "\n";
$span = $doc->createElement("span");
if (!$span instanceof DOMElement) {
    throw new Exception("createElement failed");
}
$span->setAttribute("id", "c");
$nl->item(0)->parentNode->appendChild($span);
echo "append-after-end:k=" . $it->key() . ",v=" . ($it->valid() ? "1" : "0") . ",c=" . ($it->current() === null ? "null" : "node") . "\n";
$it->next();
echo "post:k=" . $it->key() . ",v=" . ($it->valid() ? "1" : "0") . "\n";
$live = $nl->getIterator();
$live->next();
$span2 = $doc->createElement("span");
if (!$span2 instanceof DOMElement) {
    throw new Exception("second createElement failed");
}
$span2->setAttribute("id", "d");
$nl->item(0)->parentNode->appendChild($span2);
$live->next();
$live->next();
$current = $live->current();
if (!$current instanceof DOMElement) {
    throw new Exception("live iterator current element missing");
}
echo "append-before-end:k=" . $live->key() . ",v=" . ($live->valid() ? "1" : "0") . ",c=" . $current->getAttribute("id") . "\n";
try {
    $it->rewind();
    echo "rewind:ok\n";
} catch (Error $e) {
    echo "rewind:" . $e->getMessage() . "\n";
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "end:k=2,v=0\n",
            "again:k=2,v=0\n",
            "append-after-end:k=2,v=0,c=null\n",
            "post:k=2,v=0\n",
            "append-before-end:k=3,v=1,c=d\n",
            "rewind:Iterator does not support rewinding\n",
        )
    );
}

/// Regression: an empty DOMNodeList iterator reports key 0 and refuses rewind.
#[test]
fn dom_internal_iterator_empty_node_list_next_and_rewind() {
    let out = compile_and_run(
        r#"<?php
$doc = new DOMDocument();
$nl = $doc->getElementsByTagName("nonexistent");
$it = $nl->getIterator();
echo "init:k=" . $it->key() . ",v=" . ($it->valid() ? "1" : "0") . "\n";
$it->next();
echo "n1:k=" . $it->key() . ",v=" . ($it->valid() ? "1" : "0") . "\n";
$it->next();
echo "n2:k=" . $it->key() . ",v=" . ($it->valid() ? "1" : "0") . "\n";
try {
    $it->rewind();
    echo "rewind:ok\n";
} catch (Error $e) {
    echo "rewind:" . $e->getMessage() . "\n";
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "init:k=0,v=0\n",
            "n1:k=0,v=0\n",
            "n2:k=0,v=0\n",
            "rewind:Iterator does not support rewinding\n",
        )
    );
}

/// Verifies runtime-named DOM node properties use virtual handlers and remain heap-clean.
#[test]
fn dom_runtime_named_node_properties_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$document = Dom\XMLDocument::createFromString('<root><child/></root>');
$root = $document->documentElement;
$child = $root->firstChild;
foreach (['firstChild', 'lastChild'] as $property) {
    echo get_class($root->$property), '|';
}
foreach (['parentNode', 'ownerDocument'] as $property) {
    echo get_class($child->$property), '|';
}
foreach (['parentElement'] as $property) {
    echo get_class($child->$property), '|';
}
foreach (['previousSibling', 'nextSibling'] as $property) {
    echo $child->$property === null ? 'N|' : 'x|';
}
$property = 'textContent';
echo $child->$property === '' ? 'T|' : 'x|';
$property = 'childNodes';
echo $child->$property->length, "\n";
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(
        out.stdout,
        "Dom\\Element|Dom\\Element|Dom\\Element|Dom\\XMLDocument|Dom\\Element|N|N|T|0\n"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected runtime-named DOM reads to remain heap-clean, got: {}",
        out.stderr
    );
}

/// Verifies legacy namespace declaration removal matches PHP and remains heap-clean.
#[test]
fn legacy_remove_attribute_ns_eliminates_only_the_local_subtree_namespace() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$document = new DOMDocument();
$document->loadXML('<container><child1 xmlns:x="urn:x"><x:foo x:bar=""/></child1><child2 xmlns:x="urn:x"><x:foo x:bar=""/></child2></container>');
$document->documentElement->firstElementChild->removeAttributeNS('urn:x', 'x');
echo $document->saveXML();
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(
        out.stdout,
        "<?xml version=\"1.0\"?>\n<container><child1><foo bar=\"\"/></child1><child2 xmlns:x=\"urn:x\"><x:foo x:bar=\"\"/></child2></container>\n"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected namespace elimination to remain heap-clean, got: {}",
        out.stderr
    );
}

/// Verifies temporary XPath namespace lists preserve php-src's eager wrapper IDs and clean up.
#[test]
fn legacy_xpath_namespace_foreach_reuses_temporary_collection_handles_like_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$document = new DOMDocument();
$document->loadXML('<container><child1 xmlns:x="urn:x"><x:foo/><x:foo/></child1><child2 xmlns:x="urn:x"><x:foo/><x:foo/></child2></container>');
$document->documentElement->firstElementChild->removeAttributeNS('urn:x', 'x');
$xpath = new DOMXPath($document);
foreach ($xpath->query('/container/child1/namespace::*') as $namespace) {
    var_dump($namespace);
}
foreach ($xpath->query('/container/child1/foo/namespace::*') as $namespace) {
    var_dump($namespace);
}
foreach ($xpath->query('/container/child2/namespace::*') as $namespace) {
    var_dump($namespace);
}
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    let object_headers = out
        .stdout
        .lines()
        .filter(|line| line.starts_with("object(DOMNameSpaceNode)#"))
        .collect::<Vec<_>>();
    assert_eq!(
        object_headers,
        vec![
            "object(DOMNameSpaceNode)#4 (10) {",
            "object(DOMNameSpaceNode)#5 (10) {",
            "object(DOMNameSpaceNode)#8 (10) {",
            "object(DOMNameSpaceNode)#9 (10) {",
            "object(DOMNameSpaceNode)#5 (10) {",
        ]
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected eager namespace wrappers to remain heap-clean, got: {}",
        out.stderr
    );
}

/// Verifies php-src's two DOM serialization denial modes and subclass-hook escape hatch.
#[test]
fn dom_serialization_denials_and_subclass_hooks_match_php() {
    let out = compile_and_run_capture(
        r#"<?php
class SerializableDocument extends DOMDocument {
    public function __serialize(): array { return ['value' => 7]; }
}
class PlainDocument extends DOMDocument {}
class BlockedXPath extends DOMXPath {
    public function __serialize(): array { return ['value' => 9]; }
}

$doc = new DOMDocument();
$doc->loadXML('<root><node/></root>');
$values = [
    $doc,
    new DOMXPath($doc),
    new PlainDocument(),
    new BlockedXPath($doc),
];
foreach ($values as $value) {
    try {
        serialize($value);
    } catch (Exception $e) {
        echo $e->getMessage(), "\n";
    }
}
echo serialize(new SerializableDocument()), "\n";
$nested = [$doc];
try {
    serialize($nested);
} catch (Exception $e) {
    echo "nested:", $e->getMessage(), "\n";
}
unset($e, $value, $nested, $values, $doc);
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(
        out.stdout,
        concat!(
            "Serialization of 'DOMDocument' is not allowed, unless serialization methods are implemented in a subclass\n",
            "Serialization of 'DOMXPath' is not allowed\n",
            "Serialization of 'PlainDocument' is not allowed, unless serialization methods are implemented in a subclass\n",
            "Serialization of 'BlockedXPath' is not allowed\n",
            "O:20:\"SerializableDocument\":1:{s:5:\"value\";i:7;}\n",
            "nested:Serialization of 'DOMDocument' is not allowed, unless serialization methods are implemented in a subclass\n",
        )
    );
    assert_eq!(out.stderr, "");
}

/// Verifies both DOM denial modes and recursive serialization unwind without leaks.
#[test]
fn dom_serialization_denials_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$document = new DOMDocument();
try {
    serialize($document);
} catch (Exception $exception) {
    echo $exception->getMessage(), "\n";
}
$xpath = new DOMXPath($document);
try {
    serialize($xpath);
} catch (Exception $exception) {
    echo $exception->getMessage(), "\n";
}
$nested = [$document];
try {
    serialize($nested);
} catch (Exception $exception) {
    echo "nested:", $exception->getMessage(), "\n";
}
unset($exception, $nested, $xpath, $document);
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(
        out.stdout,
        concat!(
            "Serialization of 'DOMDocument' is not allowed, unless serialization methods are implemented in a subclass\n",
            "Serialization of 'DOMXPath' is not allowed\n",
            "nested:Serialization of 'DOMDocument' is not allowed, unless serialization methods are implemented in a subclass\n",
        )
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected DOM serialization denials to remain heap-clean, got: {}",
        out.stderr
    );
}
