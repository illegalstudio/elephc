//! Purpose:
//! Covers SimpleXML DOM import failures, namespace contexts, and iterator diagnostics.
//!
//! Called from:
//! - Focused codegen CI with managed libxml2 and optional PHP reference checks.
//!
//! Key details:
//! - Bundles compiled and runtime-eval cases with heap-debug ownership checks.

use crate::support::*;

/// Scoped loader calls retain the XML bridge without any ordinary instance loadXML trigger.
#[test]
fn test_simplexml_dom_scoped_load_xml_detection() {
    if skip_without_xml_native("test_simplexml_dom_scoped_load_xml_detection") { return; }
    let out = compile_and_run_with_heap_debug(r#"<?php
class ScopedFeed extends DOMDocument {
    public function loadParent(string $xml): bool { return parent::loadXML($xml); }
    public function loadSelf(string $xml): bool { return self::loadXML($xml); }
}
$feed = new ScopedFeed();
var_dump($feed->loadParent('<r/>'), $feed->loadSelf('<s/>'), $feed->documentElement->nodeName);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "bool(true)\nbool(true)\nstring(1) \"s\"\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Iterator steps create subclass wrappers eagerly and native methods bypass other public overrides.
#[test]
fn test_simplexml_iterator_eager_destruction_and_native_method_hooks() {
    if skip_without_xml_native("test_simplexml_iterator_eager_destruction_and_native_method_hooks") { return; }
    let out = compile_and_run_with_heap_debug(r#"<?php
class EagerXmlCursor extends SimpleXMLElement {
    public function __destruct() { echo '[', parent::getName(), ']'; }
}
$xml = new EagerXmlCursor('<r><a/><b/></r>');
$xml->rewind(); echo 'rewound|';
$xml->next(); echo 'advanced|';
$xml->next(); echo 'ended|'; unset($xml);
echo '|';
class HookXmlCursor extends SimpleXMLElement {
    public function valid(): bool { echo 'valid|'; return parent::valid(); }
    public function current(): SimpleXMLElement { echo 'current|'; return parent::current(); }
    public function getName(): string { echo 'name|'; return parent::getName(); }
    public function count(): int { echo 'count|'; return parent::count(); }
}
$xml = new HookXmlCursor('<r><a><v/></a><b/></r>');
$xml->rewind(); echo $xml->key(), ':'; var_dump($xml->hasChildren());
$child = $xml->getChildren(); echo get_class($child), '|'; unset($child);
$xml->next(); echo $xml->key(), ':'; var_dump($xml->hasChildren());
$xml->next(); var_dump($xml->getChildren());
echo (int)$xml->valid();
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "rewound|[a]advanced|[b]ended|[r]|a:bool(true)\nHookXmlCursor|b:bool(false)\nNULL\nvalid|0");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// XPath results reset namespace filters and nested groups retain their selected parent element.
#[test]
fn test_simplexml_xpath_filters_and_detached_nested_groups() {
    if skip_without_xml_native("test_simplexml_xpath_filters_and_detached_nested_groups") { return; }
    let out = compile_and_run_with_heap_debug(r#"<?php
$xml = simplexml_load_string('<r xmlns:p="urn:p"><p:a><b>B</b><p:c>C</p:c></p:a></r>', null, 0, 'p', true);
foreach ($xml->xpath('//p:a')[0] as $key => $value) { echo $key, '=', $value; }
echo '|';
$xml = new SimpleXMLElement('<r id="v"><a><i>1</i><i>2</i></a><a><i>3</i></a></r>');
$attributes = $xml->attributes(); $attributes->rewind();
var_dump($attributes->hasChildren(), $attributes->getChildren());
$group = $xml->a; $nested = $xml->a->i;
$children = $xml->a->children(); $attributes = $xml->a->attributes();
unset($xml->a[0]);
echo count($group), ':', count($nested), ':', count($children), ':', count($attributes), '|';
foreach ($nested as $value) { echo $value; }
$copied = clone $nested;
echo '|'; foreach ($copied as $value) { echo $value; }
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "b=B|bool(false)\nNULL\n1:2:2:0|12|12");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Keeps failed remote DOM imports reviewable without dumping eval's complete method catalog.
fn import_failure_assembly(assembly: &str) -> String {
    let mut result = String::new();
    for name in ["simplexml_import_dom", "__elephc_simplexml_class"] {
        let start = format!("{}:\n", elephc::names::function_symbol(name));
        let end = format!("{}:", elephc::names::function_epilogue_symbol(name));
        if let Some((_, body)) = assembly.split_once(&start) {
            result.push_str(&start);
            result.push_str(body.split_once(&end).map_or(body, |(body, _)| body));
        }
    }
    let lines: Vec<_> = assembly.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        if line.contains(&elephc::names::function_symbol("simplexml_import_dom"))
            && matches!(line.split_whitespace().next(), Some("bl" | "call"))
        {
            result.push_str(&lines[index.saturating_sub(24)..(index + 28).min(lines.len())].join("\n"));
            result.push('\n');
        }
    }
    result
}

/// Dynamic method syntax and callback arrays inject the XML loader used by DOMDocument.
#[test]
fn test_simplexml_dynamic_dom_load_xml() {
    if skip_without_xml_native("test_simplexml_dynamic_dom_load_xml") { return; }
    let out = compile_and_run(r#"<?php
$document = new DOMDocument();
$method = 'loadXML';
var_dump($document->$method('<r>dynamic</r>'));
var_dump($document?->{'loadXML'}('<r>nullsafe</r>'));
var_dump(call_user_func([$document, 'loadXML'], '<r>callback</r>'));
echo $document->documentElement->textContent;
"#);
    assert_eq!(out, "bool(true)\nbool(true)\nbool(true)\ncallback");
}

/// DOM loaders retain native capability when reached through open receiver declarations.
#[test]
fn test_simplexml_dom_load_xml_trait_interface_and_node() {
    if skip_without_xml_native("test_simplexml_dom_load_xml_trait_interface_and_node") { return; }
    let out = compile_and_run_with_heap_debug(r#"<?php
trait LoadsXml {
    public function loadFrom(string $xml): void { $this->loadXML($xml); }
}
class Feed extends DOMDocument { use LoadsXml; }
$feed = new Feed();
$feed->loadFrom('<rss><item>a</item></rss>');
echo $feed->documentElement->nodeName, "\n";
interface XmlLoader { public function loadXML(string $source, int $options = 0): bool; }
class LoaderDocument extends DOMDocument implements XmlLoader {}
function loadThroughInterface(XmlLoader $loader): void { var_dump($loader->loadXML('<r>interface</r>')); }
loadThroughInterface(new LoaderDocument());
function loadThroughNode(DOMNode $node): void {
    if ($node instanceof DOMDocument) { var_dump($node->loadXML('<n/>')); }
}
loadThroughNode(new DOMDocument());
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "rss\nbool(true)\nbool(true)\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A DOMNode annotation alone must inject the loader before guarded method calls are lowered.
#[test]
fn test_simplexml_dom_load_xml_through_dom_node_only() {
    if skip_without_xml_native("test_simplexml_dom_load_xml_through_dom_node_only") { return; }
    let out = compile_and_run_with_heap_debug(r#"<?php
function loadOnlyThroughNode(DOMNode $node): void {
    if ($node instanceof DOMDocument) { var_dump($node->loadXML('<n/>')); }
}
loadOnlyThroughNode(new DOMDocument());
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "bool(true)\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Builtin interface hints remain open to DOMDocument subclasses without another XML trigger.
#[test]
fn test_simplexml_dom_load_xml_through_builtin_interface_only() {
    if skip_without_xml_native("test_simplexml_dom_load_xml_through_builtin_interface_only") { return; }
    let out = compile_and_run_with_heap_debug(r#"<?php
class CountedDocument extends DOMDocument implements Countable {
    public function count(): int { return 0; }
}
function loadOnlyThroughCountable(Countable $document): void {
    if ($document instanceof DOMDocument) { var_dump($document->loadXML('<r/>')); }
}
loadOnlyThroughCountable(new CountedDocument());
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "bool(true)\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Callable arrays inject native XML support, and eval's callable factory shares the DOM receiver.
#[test]
fn test_simplexml_callable_array_dom_load_xml_detection_bundle() {
    if skip_without_xml_native("test_simplexml_callable_array_dom_load_xml_detection_bundle") { return; }
    let out = compile_and_run(r#"<?php
$document = new DOMDocument();
$load = [$document, 'loadXML'];
var_dump($load('<r>stored</r>'));
$results = array_map([$document, 'loadXML'], ['<r>mapped</r>']);
echo $document->documentElement->textContent, '|';
$source = $argc > 0 ? '$closure = Closure::fromCallable([$document, "loadXML"]); var_dump($closure("<r>closed</r>"));' : '';
eval($source);
echo $document->documentElement->textContent;
"#);
    assert_eq!(out, "bool(true)\nmapped|bool(true)\nclosed");
}

/// A by-reference callback can replace a non-DOM local before its first XML load.
#[test]
fn test_simplexml_dom_load_after_captured_reassignment() {
    if skip_without_xml_native("test_simplexml_dom_load_after_captured_reassignment") { return; }
    let out = compile_and_run(r#"<?php
class InitialConfig {}
function initialDocumentConfig(): mixed { return new InitialConfig(); }
$document = initialDocumentConfig();
$replace = function () use (&$document): void { $document = new DOMDocument(); };
$replace();
var_dump($document->loadXML('<r>captured</r>'));
echo $document->documentElement->textContent;
$alias =& $document;
$document = initialDocumentConfig();
$alias = new DOMDocument();
var_dump($document->loadXML('<r>alias</r>'));
echo $document->documentElement->textContent;
"#);
    assert_eq!(out, "bool(true)\ncapturedbool(true)\nalias");
}

/// Loading HTML replaces a document's XML binding without invalidating retained old nodes.
#[test]
fn test_simplexml_dom_html_reload_bundle() {
    if skip_without_xml_native("test_simplexml_dom_html_reload_bundle") { return; }
    let out = compile_and_run_with_heap_debug(r###"<?php
$document = new DOMDocument();
$document->loadXML('<old>before</old>');
$old = simplexml_import_dom($document);
$document->loadHTML('<html><body><p>after</p></body></html>');
var_dump($document->documentElement->nodeName, str_contains($document->saveXML(), 'after'), str_contains($document->saveXML(), '<old>'));
echo $old->getName(), ':', (string)$old, '|';
$document->loadHTML('<html><body><p>first</p></body></html>');
$imported = simplexml_import_dom($document);
$document->loadHTML('<html><body><p>second</p></body></html>');
var_dump(str_contains($document->saveXML(), 'second'), str_contains($document->saveXML(), 'first'));
$source = $argc > 0 ? '$ed = new DOMDocument(); $ed->loadXML("<old/>"); $ed->loadHTML("<html><body><p>new</p></body></html>"); var_dump($ed->documentElement->nodeName, str_contains($ed->saveXML(), "new"), str_contains($ed->saveXML(), "<old"));' : '';
eval($source);
"###);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(out.stdout, "string(4) \"html\"\nbool(true)\nbool(false)\nold:before|bool(true)\nbool(false)\nstring(4) \"html\"\nbool(true)\nbool(false)\n");
}

/// Separates parser trees, DOM construction, retained imports and eval reload ownership.
#[test]
fn test_simplexml_dom_reload_owners_isolated_bundle() {
    if skip_without_xml_native("test_simplexml_dom_reload_owners_isolated_bundle") { return; }
    let args: Vec<_> = (0..25).map(|count| vec!["x"; count]).collect();
    let argvs: Vec<_> = args.iter().map(Vec::as_slice).collect();
    let (runs, assembly) = compile_and_run_with_heap_debug_and_asm_per_argv(r###"<?php
if ($argc === 1) {
    $document = new DOMDocument(); echo $document->childNodes->length;
} elseif ($argc === 2) {
    for ($i = 0; $i < 3; $i++) {
        $tree = __elephc_dom_parse_html('<html><body><p>x</p></body></html>', 0);
        echo count($tree['children']); unset($tree);
    }
} elseif ($argc === 3) {
    $document = new DOMDocument();
    for ($i = 0; $i < 3; $i++) { $document->loadHTML('<html><body><p>x</p></body></html>'); }
    echo $document->documentElement->nodeName;
} elseif ($argc === 4) {
    $document = new DOMDocument(); $document->loadXML('<old>before</old>');
    $old = simplexml_import_dom($document);
    $document->loadHTML('<html><body><p>after</p></body></html>');
    echo $old->getName(), ':', (string)$old, '|', $document->documentElement->nodeName;
} elseif ($argc === 5) {
    $document = new DOMDocument();
    $document->loadHTML('<html><body><p>first</p></body></html>');
    $imported = simplexml_import_dom($document);
    $document->loadHTML('<html><body><p>second</p></body></html>');
    echo $imported->getName(), '|', $document->documentElement->nodeName;
} elseif ($argc === 6) {
    $source = $argc > 0 ? '$document = new DOMDocument(); $document->loadXML("<old/>"); $document->loadHTML("<html><body><p>new</p></body></html>"); echo $document->documentElement->nodeName;' : '';
    eval($source);
} elseif ($argc === 7) {
    $document = new DOMDocument(); $document->loadHTML('<html><body><p>x</p></body></html>');
    echo $document->documentElement->nodeName;
} elseif ($argc === 8) {
    $document = new DOMDocument(); $document->loadHTML('<html><body><p>x</p></body></html>');
    $document->childNodes = new DOMNodeList([]); echo $document->childNodes->length;
} elseif ($argc === 9) {
    $source = $argc > 0 ? '$document = new DOMDocument(); $document->loadHTML("<html><body><p>x</p></body></html>"); echo $document->documentElement->nodeName;' : '';
    eval($source);
} elseif ($argc === 10) {
    $document = new DOMDocument(); $list = new DOMNodeList([$document]); echo $list->length;
} elseif ($argc === 11) {
    $list = new DOMNodeList([]); $list->__construct([]); $list->__construct([]); echo $list->length;
} elseif ($argc === 12) {
    $document = new DOMDocument(); $document->childNodes = new DOMNodeList([]); echo $document->childNodes->length;
} elseif ($argc === 13) {
    $document = new DOMDocument(); $document->childNodes = new DOMNodeList([$document]);
    $document->childNodes = new DOMNodeList([]); echo $document->childNodes->length;
} elseif ($argc === 14) {
    $document = new DOMDocument(); $document->loadXML('<r xmlns:p="urn:p" p:id="8">text<c/></r>');
    $text = $document->documentElement->childNodes->item(0); echo $text->nodeName;
} elseif ($argc === 15 || $argc === 16) {
    $document = new DOMDocument(); $document->loadXML('<r>text<c/></r>');
    $text = $document->documentElement->childNodes->item(0);
    if ($text instanceof DOMNode) {
        if ($argc === 15) {
            set_error_handler(function(int $level, string $message): bool { return true; });
            echo (int)(simplexml_import_dom($text) === null);
        } else {
            set_error_handler(function(int $level, string $message): bool { throw new RuntimeException('caught'); });
            try { simplexml_import_dom($text); } catch (RuntimeException $error) { echo $error->getMessage(); }
        }
        restore_error_handler();
    } else { echo 'missing-node'; }
} elseif ($argc === 17) {
    set_error_handler(function(int $level, string $message): bool { return true; });
    $document = new DOMDocument(); $document->loadXML('<r xmlns:p="urn:p" p:id="8">text<c/></r>');
    if ($document instanceof DOMNode) {
        $first = simplexml_import_dom($document); $first->registerXPathNamespace('q', 'urn:p');
        if ($document instanceof DOMNode) {
            $second = simplexml_import_dom($document);
            echo (int)($second->xpath('//@q:id') === false), ':', count($first->xpath('//@q:id')), ':';
            echo json_encode($first->attributes('p', true)->id->getNamespaces());
        } else { echo 'missing-node'; }
    } else { echo 'missing-node'; }
    restore_error_handler();
} elseif ($argc === 18) {
    $document = new DOMDocument();
    set_error_handler(function(int $level, string $message): bool { return true; });
    echo (int)$document->loadXML('<r>'), (int)$document->loadXML('<?xml version="1.1"?><r/>');
    restore_error_handler();
    set_error_handler(function(int $level, string $message): bool { throw new RuntimeException('caught'); });
    try { $document->loadXML('<?xml version="1.1"?><r/>'); }
    catch (RuntimeException $error) { echo $error->getMessage(); }
    restore_error_handler();
} elseif ($argc === 19) {
    set_error_handler(function(int $level, string $message): bool { return true; });
    echo (int)(simplexml_import_dom(new DOMDocument()) === null);
    $uninitialized = (new ReflectionClass('DOMElement'))->newInstanceWithoutConstructor();
    if ($uninitialized instanceof DOMNode) {
        try { simplexml_import_dom($uninitialized); } catch (TypeError $error) { echo 'E:'; }
    } else { echo 'wrong-node'; }
    $uninitializedDocument = (new ReflectionClass('DOMDocument'))->newInstanceWithoutConstructor();
    if ($uninitializedDocument instanceof DOMNode) {
        try { simplexml_import_dom($uninitializedDocument); } catch (TypeError $error) { echo 'D:'; }
    } else { echo 'wrong-node'; }
    try { simplexml_import_dom(new stdClass()); } catch (TypeError $error) { echo get_class($error); }
    $iterator = new SimpleXMLElement('<r><a/></r>');
    try { $iterator->current(); } catch (Error $error) { echo 'E'; }
    try { $iterator->key(); } catch (Error $error) { echo 'E'; }
    restore_error_handler();
} elseif ($argc === 20) {
    $source = $argc > 0 ? 'set_error_handler(function(int $level, string $message): bool { return true; }); $document = new DOMDocument(); echo (int)(simplexml_import_dom($document) === null), (int)$document->loadXML("<r>"); $iterator = new SimpleXMLElement("<r/>"); try { $iterator->key(); } catch (Error $error) { echo "E"; } $uninitialized = (new ReflectionClass("DOMElement"))->newInstanceWithoutConstructor(); try { simplexml_import_dom($uninitialized); } catch (TypeError $error) { echo "T"; } restore_error_handler();' : '';
    eval($source);
} elseif ($argc === 21 || $argc === 22) {
    set_error_handler(function(int $level, string $message): bool { return true; });
    $document = new DOMDocument();
    echo (int)$document->loadXML($argc === 21 ? '<r>' : '<?xml version="1.1"?><r/>');
    restore_error_handler();
} elseif ($argc === 23) {
    $document = new DOMDocument();
    set_error_handler(function(int $level, string $message): bool { throw new RuntimeException('caught'); });
    try { $document->loadXML('<?xml version="1.1"?><r/>'); }
    catch (RuntimeException $error) { echo $error->getMessage(); }
    restore_error_handler();
} else {
    $iterator = new SimpleXMLElement('<r><a/></r>');
    try {
        if ($argc === 24) { $iterator->current(); } else { $iterator->key(); }
    } catch (Error $error) { echo 'E'; }
}
"###, &argvs);
    let expected = ["0", "111", "html", "old:before|html", "html|html", "html", "html", "0", "html", "1", "0", "0", "0", "#text", "1", "caught", "1:1:{\"p\":\"urn:p\"}", "01caught", "1E:D:TypeErrorEE", "10ET", "0", "1", "caught", "E", "E"];
    assert_eq!(runs.len(), expected.len());
    let mut failures = Vec::new();
    for (mode, (out, expected)) in runs.iter().zip(expected).enumerate() {
        if !out.success || out.stdout != expected || !out.stderr.contains("HEAP DEBUG: leak summary: clean") {
            failures.push(format!("mode={mode} success={} stdout={:?} expected={expected:?}\n{}", out.success, out.stdout, out.stderr));
        }
    }
    let symbols = [
        "main".into(),
        elephc::names::function_symbol("__elephc_simplexml_load_dom"),
        elephc::names::method_symbol("DOMDocument", "loadXML"),
        elephc::names::method_symbol("SimpleXMLElement", "current"),
        elephc::names::method_symbol("SimpleXMLElement", "key"),
        elephc::names::method_symbol("SimpleXMLElement", "__elephc_iterator_current"),
        elephc::names::method_symbol("DOMDocument", "loadHTML"),
        elephc::names::method_symbol("DOMNode", "__propset_childNodes"),
        elephc::names::join_php_symbol("_method_source_abi", &["DOMNode", "__propset_childNodes"]),
        elephc::names::method_symbol("DOMNodeList", "__construct"),
        elephc::names::join_php_symbol("_method_source_abi", &["DOMNodeList", "__construct"]),
    ];
    assert!(failures.is_empty(), "{}\nDOM replacement assembly:\n{}", failures.join("\n"), selected_function_assembly(&assembly, &symbols));
}

/// DOM failures warn before returning, imported wrappers isolate prefixes, and iterator errors match PHP.
#[test]
fn test_simplexml_dom_namespace_and_iterator_bundle() {
    if skip_without_xml_native("test_simplexml_dom_namespace_and_iterator_bundle") { return; }
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(r###"<?php
set_error_handler(function(int $level, string $message): bool { echo $level, ':', $message, '|'; return true; });
$doc = new DOMDocument(); $doc->loadXML('<r xmlns:p="urn:p" p:id="8">text<c/></r>');
$text = $doc->documentElement->childNodes->item(0);
if ($text instanceof DOMNode) { var_dump(simplexml_import_dom($text)); }
$unattached = (new ReflectionClass('DOMElement'))->newInstanceWithoutConstructor();
if ($unattached instanceof DOMNode) {
    try { simplexml_import_dom($unattached); } catch (TypeError $error) { echo 'uninitialized:', $error->getMessage(), '|'; }
} else { echo 'wrong-node'; }
var_dump(simplexml_import_dom(new DOMDocument()));
try { simplexml_import_dom(new stdClass()); } catch (TypeError $error) { echo $error->getMessage(), '|'; }
$first = simplexml_import_dom($doc);
$first->registerXPathNamespace('q', 'urn:p');
$second = simplexml_import_dom($doc);
var_dump($second->xpath('//@q:id'));
$second->registerXPathNamespace('q', 'urn:other');
echo count($first->xpath('//@q:id')), '|';
echo json_encode($first->attributes('p', true)->id->getNamespaces()), '|';
echo json_encode($first->attributes('p', true)->id->getNamespaces(true)), '|';
echo json_encode($first->attributes()->getNamespaces()), '|';
$iterator = new SimpleXMLElement('<r><a/></r>');
try { $iterator->current(); } catch (Error $error) { echo $error->getMessage(), '|'; }
try { $iterator->key(); } catch (Error $error) { echo $error->getMessage(), '|'; }
$iterator->rewind(); $iterator->next();
try { $iterator->current(); } catch (Error $error) { echo $error->getMessage(), '|'; }
try { $iterator->key(); } catch (Error $error) { echo $error->getMessage(), '|'; }
$bad = new DOMDocument();
var_dump($bad->loadXML('<r>'));
var_dump($bad->loadXML('<?xml version="1.1"?><r/>'));
try { $bad->loadXML(''); } catch (ValueError $error) { echo $error->getMessage(), '|'; }
restore_error_handler();
set_error_handler(function(int $level, string $message): bool { throw new RuntimeException('caught'); });
try { if ($text instanceof DOMNode) { simplexml_import_dom($text); } } catch (RuntimeException $error) { echo $error->getMessage(), '|'; }
try { $bad->loadXML('<?xml version="1.1"?><r/>'); } catch (RuntimeException $error) { echo $error->getMessage(), '|'; }
restore_error_handler();
set_error_handler(function(int $level, string $message): bool { echo $level, ':', $message, '|'; return true; });
$source = $argc > 0 ? '$ed = new DOMDocument(); var_dump(simplexml_import_dom($ed)); var_dump($ed->loadXML("<r>")); $ei = new SimpleXMLElement("<r/>"); try { $ei->key(); } catch (Error $error) { echo $error->getMessage(), "|"; }' : '';
eval($source);
restore_error_handler();
"###);
    let invalid = "2:simplexml_import_dom(): Invalid Nodetype to import|NULL\n";
    let uninitialized = "uninitialized:simplexml_import_dom(): Argument #1 ($node) must be a valid XML node|";
    let consumed = "Iterator not initialized or already consumed|";
    let parse = "2:DOMDocument::loadXML(): Premature end of data in tag r line 1 in Entity, line: 1|bool(false)\n";
    assert!(out.success, "stdout={} stderr={}\nDOM import assembly:\n{}", out.stdout, out.stderr, import_failure_assembly(&assembly));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(out.stdout, format!("{invalid}{uninitialized}{invalid}simplexml_import_dom(): Argument #1 ($node) must be a valid XML node|2:SimpleXMLElement::xpath(): Undefined namespace prefix: q|bool(false)\n1|{{\"p\":\"urn:p\"}}|{{\"p\":\"urn:p\"}}|[]|{}{parse}8:DOMDocument::loadXML(): Unsupported version '1.1' in Entity, line: 1|bool(true)\nDOMDocument::loadXML(): Argument #1 ($source) must not be empty|caught|caught|{invalid}{parse}{consumed}", consumed.repeat(4)));
}

/// Separates construction, selection, and foreach ownership in compiled and runtime-eval paths.
#[test]
fn test_simplexml_foreach_view_owners_isolated_bundle() {
    if skip_without_xml_native("test_simplexml_foreach_view_owners_isolated_bundle") { return; }
    let runs = compile_and_run_with_heap_debug_per_argv(r###"<?php
$xml = new SimpleXMLElement('<r><item>a</item><item>b</item><item>c</item><item>d</item><item>e</item><item>f</item><item>g</item><item>h</item></r>');
if ($argc === 1) { echo 'baseline'; }
elseif ($argc === 2) { echo count($xml->item); }
elseif ($argc === 3) { foreach ($xml->item as $item) { echo (string)$item, ','; } }
elseif ($argc === 4) {
    $items = $xml->item;
    foreach ($items as $item) { echo (string)$item, ','; }
} else {
    $source = $argc > 0 ? 'foreach ($xml->item as $item) { echo (string)$item, ","; }' : '';
    eval($source);
}
"###, &[&[], &["x"], &["x", "x"], &["x", "x", "x"], &["x", "x", "x", "x"]]);
    let expected = ["baseline", "8", "a,b,c,d,e,f,g,h,", "a,b,c,d,e,f,g,h,", "a,b,c,d,e,f,g,h,"];
    assert_eq!(runs.len(), expected.len());
    let mut failures = Vec::new();
    for (mode, (out, expected)) in runs.iter().zip(expected).enumerate() {
        if !out.success || out.stdout != expected || !out.stderr.contains("HEAP DEBUG: leak summary: clean") {
            failures.push(format!("mode={mode} success={} stdout={:?} expected={expected:?}\n{}", out.success, out.stdout, out.stderr));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Native cursors preserve PHP live sibling mutation, initialization, clone and alias behavior.
#[test]
fn test_simplexml_iterator_mutations_and_aliases_bundle() {
    if skip_without_xml_native("test_simplexml_iterator_mutations_and_aliases_bundle") { return; }
    let out = compile_and_run_with_heap_debug(r#"<?php
$xml = new SimpleXMLElement('<r><i>a</i><i>b</i><i>c</i></r>');
$n = 0;
foreach ($xml->i as $item) { echo (string)$item, ','; if ($n++ === 0) { $cached = $item; unset($xml->i[0]); } }
echo '|';
$xml = new SimpleXMLElement('<r><i>a</i><i>b</i><i>c</i></r>'); $n = 0;
foreach ($xml->i as $item) { echo (string)$item, ','; if ($n++ === 0) { unset($xml->i[1]); } }
echo '|';
$xml = new SimpleXMLElement('<r><i>a</i><i>b</i><i>c</i></r>'); $n = 0;
foreach ($xml->i as $item) { echo (string)$item, ','; if ($n++ === 0) { $xml->addChild('i', 'd'); } }
echo '|';
$xml = new SimpleXMLElement('<r><i>a</i><i>b</i><i>c</i></r>'); $view = $xml->i;
$view->rewind(); echo (string)$view->current(), ':';
unset($xml->i[0]); echo (int)$view->valid(), ':'; $view->next(); echo (int)$view->valid(), '|';
$xml = new SimpleXMLElement('<r><i>a</i><i>b</i></r>'); $view = $xml->i;
echo (int)$view->valid(), ':'; $view->next(); echo (int)$view->valid(), '|';
$view->rewind(); $view->next(); $copy = clone $view;
echo (int)$copy->valid(), ':', (string)$view->current(), '|';
$view->rewind(); $alias = $view; $alias->next(); echo (string)$view->current(), '|';
$first = $xml->i; $second = $xml->i; $first->rewind(); $second->rewind(); $second->next();
echo (string)$first->current(), ':', (string)$second->current(), '|';
$attributes = new SimpleXMLElement('<r a="1" b="2" c="3"/>'); $n = 0;
foreach ($attributes->attributes() as $key => $attribute) {
    echo $key, '=', (string)$attribute, ',';
    if ($n++ === 0) { unset($attributes['a']); }
}
echo '|', (string)$cached;
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "a,|a,c,|a,b,c,d,|a:1:0|0:0|0:b|b|a:b|a=1,|a");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Large sibling sets must complete within the ordinary CI budget instead of quadratic scans.
#[test]
fn test_simplexml_large_sibling_iteration() {
    if skip_without_xml_native("test_simplexml_large_sibling_iteration") { return; }
    let out = compile_and_run(r#"<?php
$xml = new SimpleXMLElement('<r>' . str_repeat('<i>x</i>', 50000) . '</r>');
$count = 0;
foreach ($xml->i as $item) { $count++; }
echo $count;
"#);
    assert_eq!(out, "50000");
}

/// Native XML capability preserves ordinary virtual getters through boxed storage.
#[test]
fn test_simplexml_boxed_ordinary_property_getter() {
    if skip_without_xml_native("test_simplexml_boxed_ordinary_property_getter") { return; }
    let out = compile_and_run_with_heap_debug(r#"<?php
class OrdinaryXmlGetter { public string $value { get { return 'getter'; } } }
function readOrdinaryXmlGetter(mixed $object): void { echo $object->value; }
$xml = new SimpleXMLElement('<r/>');
readOrdinaryXmlGetter(new OrdinaryXmlGetter());
echo '|', $xml->getName();
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "getter|r");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
