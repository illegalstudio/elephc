//! Purpose:
//! Integration coverage for the standard DOM class surface injected by the compiler.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Type checks pin inheritance, iteration, properties, signatures, and named parameters.
//! - Runtime checks prove the declarations reach EIR lowering and method dispatch.
//! - The tests after the `php-verified` marker are generated from probe programs: each program was
//!   run under php 8.5.10 (libxml 2.9.13) and its stdout is the expected string, so they pin the
//!   tree, parser and serializer behaviour against php itself rather than against the model.

use crate::codegen::oop::compile_and_run;

/// Runs the frontend through type checking with the standard DOM declarations injected.
fn type_checks_cleanly(source: &str) -> Result<(), String> {
    let tokens = elephc::lexer::tokenize(source).map_err(|error| error.message.clone())?;
    let ast = elephc::parser::parse(&tokens).map_err(|error| error.message.clone())?;
    let ast = elephc::autoload::collect_aliases(ast).map_err(|error| error.message.clone())?;
    let ast = elephc::dom_prelude::inject(ast);
    let ast = elephc::name_resolver::resolve(ast).map_err(|error| error.message.clone())?;
    let ast = elephc::optimize::fold_constants(ast);
    elephc::types::check(&ast).map_err(|error| error.message.clone())?;
    Ok(())
}

/// Verifies the node hierarchy and optional serialization parameter type-check.
#[test]
fn test_dom_node_and_document_typed_signatures_type_check() {
    let result = type_checks_cleanly(
        r#"<?php
function accept_node(\DOMNode $node): void {}
function serialize_document(\DOMDocument $document): string { return $document->saveXML(); }
"#,
    );
    assert!(result.is_ok(), "expected type-check success, got: {result:?}");
}

/// Verifies the standard construction, mutation, append, and serialization call surface, with the
/// parameter names php itself declares (`localName`, `qualifiedName`).
#[test]
fn test_dom_document_mutation_surface_type_checks() {
    let result = type_checks_cleanly(
        r#"<?php
$document = new \DOMDocument();
$element = $document->createElement(localName: 'item', value: 'content');
$element->setAttribute(qualifiedName: 'id', value: '7');
$document->appendChild($element);
echo $document->saveXML(node: $element);
"#,
    );
    assert!(result.is_ok(), "expected type-check success, got: {result:?}");
}

/// Verifies node collections and named-node maps are iterable with inherited node properties.
#[test]
fn test_dom_collection_and_property_surface_type_checks() {
    let result = type_checks_cleanly(
        r#"<?php
function inspect_element(\DOMElement $element): array {
    $result = [];
    foreach ($element->attributes as $name => $attribute) {
        $result[$name] = $attribute->nodeValue;
    }
    foreach ($element->childNodes as $node) {
        if ($node instanceof \DOMText) {
            $result['text'] = $node->nodeValue;
        }
    }
    return $result;
}
"#,
    );
    assert!(result.is_ok(), "expected type-check success, got: {result:?}");
}

/// Verifies DOM declarations and constants reach EIR with executable method bodies.
#[test]
fn test_dom_document_constructs_and_runs() {
    let out = compile_and_run(
        r#"<?php
$document = new \DOMDocument('1.0', 'UTF-8');
$document->formatOutput = true;
$element = $document->createElement('item', 'value');
$text = $document->createTextNode('content');
echo $element->ownerDocument === $document ? 'owner' : 'missing';
echo '|', $text->nodeValue, '|', $document->saveXML(), '|', XML_DOCUMENT_TYPE_NODE;
"#,
    );
    assert_eq!(out, "owner|content|<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n|10");
}

/// Verifies the parsing predicates: well-formed XML loads, validation is not provided.
#[test]
fn test_dom_parse_predicates() {
    let out = compile_and_run(
        r#"<?php
$document = new \DOMDocument();
$document->validateOnParse = true;
$loaded = $document->loadXML('<root/>');
$document->normalizeDocument();
$valid = $document->schemaValidateSource('<schema/>');
echo $document->validateOnParse ? 'set' : 'unset';
echo '|', $loaded ? 'yes' : 'no', '|', $valid ? 'yes' : 'no';
"#,
    );
    assert_eq!(out, "set|yes|no");
}

// ---- php-verified DOM behaviour (generated from snippets run under php 8.5) ----

/// Tree building keeps the parent, child and sibling links, the live child list and the owner document in step with every mutation.
#[test]
fn test_dom_tree_building() {
    let out = compile_and_run(
        r####"<?php
// TEST: Tree building keeps the parent, child and sibling links, the live child list and the owner document in step with every mutation.
$d = new DOMDocument();
$root = $d->appendChild($d->createElement('root'));
$a = $d->createElement('a');
$b = $d->createElement('b');
$c = $d->createElement('c');
$root->appendChild($a);
$root->appendChild($c);
$root->insertBefore($b, $c);
echo $root->childNodes->length, count($root->childNodes), $root->hasChildNodes() ? 'Y' : 'N', "\n";
echo $root->firstChild->nodeName, $root->lastChild->nodeName, $b->previousSibling->nodeName, $b->nextSibling->nodeName, $b->parentNode->nodeName, "\n";
var_dump($a->previousSibling, $c->nextSibling, $d->documentElement === $root, $d->firstChild === $root, $root->ownerDocument === $d, $d->ownerDocument);
$listing = [];
foreach ($root->childNodes as $i => $n) {
    $listing[] = $i . '=' . $n->nodeName . ':' . $n->nodeType;
}
echo implode(' ', $listing), "\n", $root->childNodes->item(1)->nodeName, var_export($root->childNodes->item(3), true), "\n";
$removed = $root->removeChild($b);
echo $removed->nodeName, $root->childNodes->length, var_export($b->parentNode, true), "\n";
$root->replaceChild($b, $c);
echo $root->lastChild->nodeName, $c->parentNode === null ? ' detached' : ' attached', "\n";
$moved = $d->createElement('moved');
$a->appendChild($moved);
$root->appendChild($moved);
echo $a->childNodes->length, $root->childNodes->length, $moved->parentNode->nodeName, "\n";
$copy = $root->cloneNode(true);
var_dump($copy->parentNode, $copy->childNodes->length, $copy->ownerDocument === $d, $copy === $root);
$shallow = $root->cloneNode(false);
echo $shallow->childNodes->length, "\n";
echo $d->saveXML();
$d->appendChild($d->createComment('after'));
echo $d->saveXML();
"####,
    );
    assert_eq!(
        out,
        r####"33Y
acacroot
NULL
NULL
bool(true)
bool(true)
bool(true)
NULL
0=a:1 1=b:1 2=c:1
bNULL
b2NULL
b detached
03root
NULL
int(3)
bool(true)
bool(false)
0
<?xml version="1.0"?>
<root><a/><b/><moved/></root>
<?xml version="1.0"?>
<root><a/><b/><moved/></root>
<!--after-->
"####
    );
}

/// nodeName, nodeType, nodeValue and textContent follow the node kind, and text assigned straight to a node reaches the tree.
#[test]
fn test_dom_node_values() {
    let out = compile_and_run(
        r####"<?php
// TEST: nodeName, nodeType, nodeValue and textContent follow the node kind, and text assigned straight to a node reaches the tree.
$d = new DOMDocument('1.0', 'UTF-8');
$e = $d->appendChild($d->createElement('item', 'hello'));
$e->appendChild($d->createElement('sub', ' world'));
$t = $d->createTextNode('txt');
$c = $d->createComment('note');
$x = $d->createCDATASection('<raw>');
$p = $d->createProcessingInstruction('target', 'data');
$f = $d->createDocumentFragment();
foreach ([$d, $e, $t, $c, $x, $p, $f] as $n) {
    echo str_pad($n->nodeName, 20), $n->nodeType, ' ', var_export($n->nodeValue, true), ' ', var_export($n->textContent, true), "\n";
}
$e->textContent = 'replaced';
echo $d->saveXML($e), $e->childNodes->length, "\n";
$e->nodeValue = 'again';
echo $d->saveXML($e), "\n";
$t->data = 'viaData';
echo $t->nodeValue, $t->textContent, $t->length, "\n";
$t->nodeValue = 'viaNodeValue';
echo $t->data, "\n";
$t->appendData('!');
$t->insertData(0, '>');
echo $t->data, $t->substringData(1, 3), "\n";
$t->replaceData(0, 1, '<<');
$t->deleteData(0, 1);
echo $t->data, "\n";
$wrap = $d->createElement('wrap');
$wrap->appendChild($t);
$wrap->appendChild($d->createElement('b', 'B'));
$wrap->appendChild($d->createTextNode('tail'));
echo $wrap->textContent, '|', $wrap->nodeValue, "\n";
$wrap->firstChild->data = 'changed';
echo $d->saveXML($wrap), "\n";
$wrap->normalize();
echo $wrap->childNodes->length, "\n";
echo var_export($d->documentElement->nodeValue, true), "\n";
"####,
    );
    assert_eq!(
        out,
        r####"#document           9 NULL 'hello world'
item                1 'hello world' 'hello world'
#text               3 'txt' 'txt'
#comment            8 'note' 'note'
#cdata-section      4 '<raw>' '<raw>'
target              7 'data' 'data'
#document-fragment  11 NULL ''
<item>replaced</item>1
<item>again</item>
viaDataviaData7
viaNodeValue
>viaNodeValue!via
<viaNodeValue!
<viaNodeValue!Btail|<viaNodeValue!Btail
<wrap>changed<b>B</b>tail</wrap>
3
'again'
"####
    );
}

/// Attributes keep insertion order, coerce scalars to strings, and are reachable through the DOMNamedNodeMap and the DOMAttr nodes.
#[test]
fn test_dom_attributes() {
    let out = compile_and_run(
        r####"<?php
// TEST: Attributes keep insertion order, coerce scalars to strings, and are reachable through the DOMNamedNodeMap and the DOMAttr nodes.
$d = new DOMDocument();
$r = $d->appendChild($d->createElement('root'));
$r->setAttribute('b', 2);
$r->setAttribute('a', 'one');
$r->setAttribute('b', 3.5);
$r->setAttribute('flag', true);
echo $r->getAttribute('b'), '|', $r->getAttribute('a'), '|', $r->getAttribute('flag'), '|', var_export($r->getAttribute('missing'), true), "\n";
var_dump($r->hasAttribute('a'), $r->hasAttribute('zz'), $r->hasAttributes(), $d->createElement('x')->hasAttributes());
echo $r->attributes->length, count($r->attributes), "\n";
foreach ($r->attributes as $name => $attr) {
    echo $name, '=', $attr->value, ' ', $attr->name, ' ', $attr->nodeName, ' ', $attr->nodeType, ' ', get_class($attr), ' ', $attr->ownerElement->nodeName, "\n";
}
echo $r->attributes->item(0)->name, var_export($r->attributes->item(9), true), $r->attributes->getNamedItem('a')->value, var_export($r->attributes->getNamedItem('zz'), true), "\n";
$node = $r->getAttributeNode('a');
echo $node->value, var_export($r->getAttributeNode('zz'), true), "\n";
$node->value = 'uno';
echo $r->getAttribute('a'), "\n";
$extra = $d->createAttribute('extra');
$extra->value = 'cv';
$r->setAttributeNode($extra);
echo $r->getAttribute('extra'), $extra->ownerElement === $r ? 'T' : 'F', "\n";
var_dump($r->removeAttribute('b'), $r->removeAttribute('b'), $r->removeAttributeNode($extra) === $extra, $r->hasAttribute('extra'));
echo $d->saveXML($r), "\n";
$r->setAttribute('q', "a\"b<c>&d\n");
echo $d->saveXML($r), "\n";
$clone = $r->cloneNode(false);
echo $clone->getAttribute('a'), $clone->attributes->length, "\n";
$ns = $d->createElementNS('urn:x', 'p:el');
$ns->setAttributeNS('urn:y', 'q:at', 'v');
echo $ns->getAttributeNS('urn:y', 'at'), var_export($ns->hasAttributeNS('urn:y', 'at'), true), $ns->namespaceURI, $ns->prefix, $ns->localName, $ns->nodeName, "\n";
echo $d->saveXML($ns), "\n";
"####,
    );
    assert_eq!(
        out,
        r####"3.5|one|1|''
bool(true)
bool(false)
bool(true)
bool(false)
33
b=3.5 b b 2 DOMAttr root
a=one a a 2 DOMAttr root
flag=1 flag flag 2 DOMAttr root
bNULLoneNULL
onefalse
uno
cvT
bool(true)
bool(false)
bool(true)
bool(false)
<root a="uno" flag="1"/>
<root a="uno" flag="1" q="a&quot;b&lt;c&gt;&amp;d&#10;"/>
uno3
vtrueurn:xpelp:el
<p:el xmlns:p="urn:x" xmlns:q="urn:y" q:at="v"/>
"####
    );
}

/// The Symfony XmlDescriptor shape: documents built from createElement, new DOMText and importNode, merged through firstChild and childNodes.
#[test]
fn test_dom_import_and_text_nodes() {
    let out = compile_and_run(
        r####"<?php
// TEST: The Symfony XmlDescriptor shape: documents built from createElement, new DOMText and importNode, merged through firstChild and childNodes.
function routeDocument(string $name, string $path): DOMDocument
{
    $dom = new DOMDocument('1.0', 'UTF-8');
    $dom->appendChild($routeXML = $dom->createElement('route'));
    $routeXML->setAttribute('name', $name);
    $routeXML->setAttribute('class', 'Symfony\Component\Routing\Route');
    $routeXML->appendChild($pathXML = $dom->createElement('path'));
    $pathXML->setAttribute('regex', '{^' . $path . '$}sD');
    $pathXML->appendChild(new \DOMText($path));
    $routeXML->appendChild($defaultsXML = $dom->createElement('defaults'));
    $defaultsXML->appendChild($defaultXML = $dom->createElement('default'));
    $defaultXML->setAttribute('key', '_controller');
    $defaultXML->appendChild(new \DOMText('App\Controller::index'));
    return $dom;
}

$dom = new \DOMDocument('1.0', 'UTF-8');
$dom->appendChild($routesXML = $dom->createElement('routes'));
foreach (['home' => '/', 'blog' => '/blog/{slug}'] as $name => $path) {
    $routeXML = routeDocument($name, $path);
    $routeXML->firstChild->appendChild($aliasesXML = $routeXML->createElement('aliases'));
    $aliasesXML->appendChild($aliasXML = $routeXML->createElement('alias'));
    $aliasXML->appendChild(new \DOMText($name . '_alias'));
    $routesXML->appendChild($routesXML->ownerDocument->importNode($routeXML->childNodes->item(0), true));
}
$dom->formatOutput = true;
echo $dom->saveXML();

$alias = new \DOMDocument('1.0', 'UTF-8');
$alias->appendChild($alias->importNode($dom->childNodes->item(0)->firstChild, true));
echo $alias->saveXML();
echo $alias->documentElement->getAttribute('name'), ' ', $alias->firstChild->childNodes->length, "\n";

$deprecations = new \DOMDocument('1.0', 'UTF-8');
$deprecations->appendChild($deprecationsXML = $deprecations->createElement('deprecations'));
foreach ([['message' => 'Use B instead', 'file' => 'src/A.php', 'line' => 12, 'count' => 3]] as $log) {
    $deprecationsXML->appendChild($deprecationXML = $deprecations->createElement('deprecation'));
    $deprecationXML->setAttribute('count', $log['count']);
    $deprecationXML->appendChild($deprecations->createElement('message', $log['message']));
    $deprecationXML->appendChild($deprecations->createElement('file', $log['file']));
    $deprecationXML->appendChild($deprecations->createElement('line', $log['line']));
}
$deprecationsXML->setAttribute('remainingCount', 0);
$deprecations->formatOutput = true;
echo $deprecations->saveXML();
$loose = new \DOMText('loose');
var_dump($loose->ownerDocument, $loose->parentNode);
$into = $deprecations->createElement('into');
$into->appendChild($loose);
var_dump($loose->ownerDocument === $deprecations);
"####,
    );
    assert_eq!(
        out,
        r####"<?xml version="1.0" encoding="UTF-8"?>
<routes>
  <route name="home" class="Symfony\Component\Routing\Route">
    <path regex="{^/$}sD">/</path>
    <defaults>
      <default key="_controller">App\Controller::index</default>
    </defaults>
    <aliases>
      <alias>home_alias</alias>
    </aliases>
  </route>
  <route name="blog" class="Symfony\Component\Routing\Route">
    <path regex="{^/blog/{slug}$}sD">/blog/{slug}</path>
    <defaults>
      <default key="_controller">App\Controller::index</default>
    </defaults>
    <aliases>
      <alias>blog_alias</alias>
    </aliases>
  </route>
</routes>
<?xml version="1.0" encoding="UTF-8"?>
<route name="home" class="Symfony\Component\Routing\Route"><path regex="{^/$}sD">/</path><defaults><default key="_controller">App\Controller::index</default></defaults><aliases><alias>home_alias</alias></aliases></route>
home 3
<?xml version="1.0" encoding="UTF-8"?>
<deprecations remainingCount="0">
  <deprecation count="3">
    <message>Use B instead</message>
    <file>src/A.php</file>
    <line>12</line>
  </deprecation>
</deprecations>
NULL
NULL
bool(true)
"####
    );
}

/// saveXML() header, escaping, empty-element and non-ASCII rules, formatOutput indentation, and the single-node form.
#[test]
fn test_dom_save_xml() {
    let out = compile_and_run(
        r####"<?php
// TEST: saveXML() header, escaping, empty-element and non-ASCII rules, formatOutput indentation, and the single-node form.
$d = new DOMDocument();
echo $d->saveXML(), "|\n";
$r = $d->appendChild($d->createElement('r'));
echo $d->saveXML();
$r->appendChild($d->createElement('empty'));
$r->appendChild($d->createElement('t', 'plain'));
$r->appendChild($d->createTextNode('a<b>&"c\''));
$r->appendChild($d->createTextNode("x\ry"));
$r->appendChild($d->createComment(' c '));
$r->appendChild($d->createCDATASection('1 < 2'));
$r->appendChild($d->createProcessingInstruction('pi', 'v'));
$r->appendChild($d->createElement('u', 'é日'));
echo $d->saveXML();
echo $d->saveXML($r->childNodes->item(1)), "\n";
echo $d->saveXML($r->childNodes->item(2)), "\n";
echo $d->saveXML($r->lastChild), "\n";
$utf = new DOMDocument('1.0', 'UTF-8');
$utf->appendChild($utf->createElement('é', 'é日'));
echo $utf->saveXML();
$latin = new DOMDocument('1.0', 'ISO-8859-1');
$latin->appendChild($latin->createElement('l'));
echo $latin->saveXML();
$d->formatOutput = true;
echo $d->saveXML();
$f = new DOMDocument();
$f->formatOutput = true;
$f->appendChild($root = $f->createElement('root'));
$root->appendChild($a = $f->createElement('a'));
$a->appendChild($f->createElement('b'));
$a->appendChild($c = $f->createElement('c'));
$c->appendChild($f->createElement('d', 'text'));
$root->appendChild($f->createElement('mixed'))->appendChild($f->createTextNode('t'));
$root->lastChild->appendChild($f->createElement('inner'));
$root->appendChild($f->createComment('trailing'));
echo $f->saveXML();
echo $f->saveXML($a);
$f->formatOutput = false;
echo $f->saveXML();
$ns = new DOMDocument();
$ns->appendChild($e = $ns->createElementNS('urn:a', 'p:root'));
$e->appendChild($ns->createElementNS('urn:a', 'p:child'));
$e->appendChild($ns->createElementNS('urn:b', 'q:other'));
$e->appendChild($ns->createElement('plain'));
echo $ns->saveXML();
$standalone = new DOMDocument();
$standalone->loadXML('<?xml version="1.0" encoding="UTF-8" standalone="yes"?><s/>');
echo $standalone->saveXML(), $standalone->encoding, $standalone->xmlStandalone ? 'Y' : 'N', "\n";
echo $standalone->saveXML(null, LIBXML_NOEMPTYTAG);
echo $standalone->saveXML(null, LIBXML_NOXMLDECL);
"####,
    );
    assert_eq!(
        out,
        r####"<?xml version="1.0"?>
|
<?xml version="1.0"?>
<r/>
<?xml version="1.0"?>
<r><empty/><t>plain</t>a&lt;b&gt;&amp;"c'x&#xD;y<!-- c --><![CDATA[1 < 2]]><?pi v?><u>&#xE9;&#x65E5;</u></r>
<t>plain</t>
a&lt;b&gt;&amp;"c'
<u>é日</u>
<?xml version="1.0" encoding="UTF-8"?>
<é>é日</é>
<?xml version="1.0" encoding="ISO-8859-1"?>
<l/>
<?xml version="1.0"?>
<r><empty/><t>plain</t>a&lt;b&gt;&amp;"c'x&#xD;y<!-- c --><![CDATA[1 < 2]]><?pi v?><u>&#xE9;&#x65E5;</u></r>
<?xml version="1.0"?>
<root>
  <a>
    <b/>
    <c>
      <d>text</d>
    </c>
  </a>
  <mixed>t<inner/></mixed>
  <!--trailing-->
</root>
<a>
  <b/>
  <c>
    <d>text</d>
  </c>
</a><?xml version="1.0"?>
<root><a><b/><c><d>text</d></c></a><mixed>t<inner/></mixed><!--trailing--></root>
<?xml version="1.0"?>
<p:root xmlns:p="urn:a"><p:child/><q:other xmlns:q="urn:b"/><plain/></p:root>
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<s/>
UTF-8Y
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<s></s>
<s/>
"####
    );
}

/// loadXML() parses well-formed documents into the same tree the builders make, and reports malformed input as false.
#[test]
fn test_dom_load_xml() {
    let out = compile_and_run(
        r####"<?php
// TEST: loadXML() parses well-formed documents into the same tree the builders make, and reports malformed input as false.
function dumpTree(DOMNode $n, int $depth = 0): string
{
    $line = str_repeat('  ', $depth) . $n->nodeType . ' ' . $n->nodeName;
    if ($n instanceof DOMElement) {
        foreach ($n->attributes as $name => $attr) {
            $line .= ' ' . $name . '=' . $attr->value;
        }
        if ($n->namespaceURI !== null) {
            $line .= ' ns=' . $n->namespaceURI;
        }
    } elseif ($n->nodeType !== 9) {
        $line .= ' ' . json_encode($n->nodeValue);
    }
    $line .= "\n";
    foreach ($n->childNodes as $child) {
        $line .= dumpTree($child, $depth + 1);
    }
    return $line;
}

$xml = <<<'XML'
<?xml version="1.0" encoding="UTF-8"?>
<!-- lead -->
<catalog xmlns="urn:books" xmlns:x="urn:extra" id="c1">
  <book isbn="1" x:flag="y">
    <title>A &amp; B &lt;C&gt; &#65;&#x42;</title>
    <note><![CDATA[1 < 2 & 3]]></note>
    <?render fast?>
    <empty/>
  </book>
  <x:other>text<!-- inner --></x:other>
</catalog>
XML;
$d = new DOMDocument();
var_dump($d->loadXML($xml));
echo dumpTree($d);
echo $d->encoding, ' ', $d->xmlVersion, ' ', $d->documentElement->nodeName, ' ', $d->documentElement->namespaceURI, "\n";
echo $d->documentElement->getElementsByTagName('title')->item(0)->textContent, "\n";
echo $d->getElementsByTagNameNS('urn:extra', 'other')->length, $d->getElementsByTagName('book')->length, "\n";
$d->formatOutput = true;
echo $d->saveXML();

$blanks = new DOMDocument();
$blanks->loadXML("<r>\n  <a/>\n  <b>  </b>\n  <c>x</c>\n</r>", LIBXML_NOBLANKS);
echo dumpTree($blanks);
$keep = new DOMDocument();
$keep->loadXML("<r>\n  <a/>\n</r>");
echo $keep->documentElement->childNodes->length, "\n";
$nocdata = new DOMDocument();
$nocdata->loadXML('<r><![CDATA[x]]></r>', LIBXML_NOCDATA);
echo $nocdata->documentElement->firstChild->nodeName, "\n";

$verdicts = [];
foreach (['<a>', '<a></b>', '<a/><b/>', 'plain text', '<a b=1/>', '<a>&unknown;</a>', '<a b="1" b="2"/>', '<a><b></a></b>', '<1a/>'] as $bad) {
    $x = new DOMDocument();
    $verdicts[] = @$x->loadXML($bad) ? 'loaded' : 'failed';
}
echo implode(' ', $verdicts), "\n";
$doc = new DOMDocument();
$doc->loadXML('<!DOCTYPE note SYSTEM "note.dtd"><note/>');
echo $doc->doctype->name, '|', $doc->doctype->systemId, '|', $doc->saveXML();
"####,
    );
    assert_eq!(
        out,
        r####"bool(true)
9 #document
  8 #comment " lead "
  1 catalog id=c1 ns=urn:books
    3 #text "\n  "
    1 book isbn=1 flag=y ns=urn:books
      3 #text "\n    "
      1 title ns=urn:books
        3 #text "A & B <C> AB"
      3 #text "\n    "
      1 note ns=urn:books
        4 #cdata-section "1 < 2 & 3"
      3 #text "\n    "
      7 render "fast"
      3 #text "\n    "
      1 empty ns=urn:books
      3 #text "\n  "
    3 #text "\n  "
    1 x:other ns=urn:extra
      3 #text "text"
      8 #comment " inner "
    3 #text "\n"
UTF-8 1.0 catalog urn:books
A & B <C> AB
11
<?xml version="1.0" encoding="UTF-8"?>
<!-- lead -->
<catalog xmlns="urn:books" xmlns:x="urn:extra" id="c1">
  <book isbn="1" x:flag="y">
    <title>A &amp; B &lt;C&gt; AB</title>
    <note><![CDATA[1 < 2 & 3]]></note>
    <?render fast?>
    <empty/>
  </book>
  <x:other>text<!-- inner --></x:other>
</catalog>
9 #document
  1 r
    1 a
    1 b
      3 #text "  "
    1 c
      3 #text "x"
3
#text
failed failed failed failed failed failed failed failed failed
note|note.dtd|<?xml version="1.0"?>
<!DOCTYPE note SYSTEM "note.dtd">
<note/>
"####
    );
}

/// loadHTML() applies libxml2's HTML 4 rules: implied html/head/body, implied paragraphs, auto-closed tags, void elements, raw script text and entity decoding.
#[test]
fn test_dom_load_html() {
    let out = compile_and_run(
        r####"<?php
// TEST: loadHTML() applies libxml2's HTML 4 rules: implied html/head/body, implied paragraphs, auto-closed tags, void elements, raw script text and entity decoding.
function dumpTree(DOMNode $n, int $depth = 0): string
{
    $line = str_repeat('  ', $depth) . $n->nodeType . ' ' . $n->nodeName;
    if ($n instanceof DOMElement) {
        foreach ($n->attributes as $name => $attr) {
            $line .= ' [' . $name . '=' . $attr->value . ']';
        }
    } elseif ($n instanceof DOMDocumentType) {
        $line .= ' ' . $n->name . '|' . $n->publicId . '|' . $n->systemId;
    } elseif ($n->nodeType !== 13) {
        $line .= ' ' . json_encode($n->nodeValue);
    }
    $line .= "\n";
    foreach ($n->childNodes as $child) {
        $line .= dumpTree($child, $depth + 1);
    }
    return $line;
}

$inputs = [
    'hello world',
    '<p>one<p>two',
    '<ul><li>a<li>b</ul>',
    '<table><tr><td>1<td>2<tr><td>3</table>',
    '<div class="a b" id=x disabled data-n=1 class=ignored>text<br>more<img src=a.png alt="x y"><hr/></div>',
    '<script>if (a < b && c) { x = "</p>"; }</script><style>p > a {}</style><p>after',
    '<p>&amp; &lt; &gt; &nbsp; &eacute; &#65; &#x42; &unknown; &amp bare & amp</p>',
    '<DIV CLASS="Up"><B>bold<I>both</B>italic</I></DIV>',
    "<!DOCTYPE html>\n<html>\n<head><title>T</title></head>\n<body>\n<p>x</p>\n</body>\n</html>\n",
    '<!-- lead --><p>x</p><!-- trail -->',
    '<select><option>a<option>b</select><p>1<br>2</p>',
    '<a href="1">one<a href="2">two</a></a><h1>a<h2>b</h2></h1>',
    '<foo:bar x:y="1">t</foo:bar><custom-el>z</custom-el><x>y',
    'text before <b>bold</b> and <i>italic</i> after',
    '<span>a</span> <span>b</span>',
];
foreach ($inputs as $i => $html) {
    $d = new DOMDocument();
    @$d->loadHTML($html);
    echo '#', $i, ' ', json_encode($html), "\n", dumpTree($d);
}
$d = new DOMDocument();
var_dump(@$d->loadHTML('<p>x</p>'), $d->nodeType, $d->documentElement->nodeName, $d->doctype->publicId);
try {
    $d->loadHTML('');
} catch (ValueError $e) {
    echo get_class($e), ': ', $e->getMessage(), "\n";
}
"####,
    );
    assert_eq!(
        out,
        r####"#0 "hello world"
13 #document
  10 html html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
  1 html
    1 body
      1 p
        3 #text "hello world"
#1 "<p>one<p>two"
13 #document
  10 html html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
  1 html
    1 body
      1 p
        3 #text "one"
      1 p
        3 #text "two"
#2 "<ul><li>a<li>b<\/ul>"
13 #document
  10 html html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
  1 html
    1 body
      1 ul
        1 li
          3 #text "a"
        1 li
          3 #text "b"
#3 "<table><tr><td>1<td>2<tr><td>3<\/table>"
13 #document
  10 html html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
  1 html
    1 body
      1 table
        1 tr
          1 td
            3 #text "1"
          1 td
            3 #text "2"
        1 tr
          1 td
            3 #text "3"
#4 "<div class=\"a b\" id=x disabled data-n=1 class=ignored>text<br>more<img src=a.png alt=\"x y\"><hr\/><\/div>"
13 #document
  10 html html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
  1 html
    1 body
      1 div [class=a b] [id=x] [disabled=disabled] [data-n=1]
        3 #text "text"
        1 br
        3 #text "more"
        1 img [src=a.png] [alt=x y]
        1 hr
#5 "<script>if (a < b && c) { x = \"<\/p>\"; }<\/script><style>p > a {}<\/style><p>after"
13 #document
  10 html html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
  1 html
    1 head
      1 script
        4 #cdata-section "if (a < b && c) { x = \"\"; }"
      1 style
        4 #cdata-section "p > a {}"
    1 body
      1 p
        3 #text "after"
#6 "<p>&amp; &lt; &gt; &nbsp; &eacute; &#65; &#x42; &unknown; &amp bare & amp<\/p>"
13 #document
  10 html html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
  1 html
    1 body
      1 p
        3 #text "& < > \u00a0 \u00e9 A B &unknown; &amp bare & amp"
#7 "<DIV CLASS=\"Up\"><B>bold<I>both<\/B>italic<\/I><\/DIV>"
13 #document
  10 html html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
  1 html
    1 body
      1 div [class=Up]
        1 b
          3 #text "bold"
          1 i
            3 #text "both"
        3 #text "italic"
#8 "<!DOCTYPE html>\n<html>\n<head><title>T<\/title><\/head>\n<body>\n<p>x<\/p>\n<\/body>\n<\/html>\n"
13 #document
  10 html html||
  1 html
    3 #text "\n"
    1 head
      1 title
        3 #text "T"
    3 #text "\n"
    1 body
      3 #text "\n"
      1 p
        3 #text "x"
      3 #text "\n"
    3 #text "\n"
#9 "<!-- lead --><p>x<\/p><!-- trail -->"
13 #document
  10 html html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
  8 #comment " lead "
  1 html
    1 body
      1 p
        3 #text "x"
      8 #comment " trail "
#10 "<select><option>a<option>b<\/select><p>1<br>2<\/p>"
13 #document
  10 html html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
  1 html
    1 body
      1 select
        1 option
          3 #text "a"
        1 option
          3 #text "b"
      1 p
        3 #text "1"
        1 br
        3 #text "2"
#11 "<a href=\"1\">one<a href=\"2\">two<\/a><\/a><h1>a<h2>b<\/h2><\/h1>"
13 #document
  10 html html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
  1 html
    1 body
      1 a [href=1]
        3 #text "one"
      1 a [href=2]
        3 #text "two"
      1 h1
        3 #text "a"
        1 h2
          3 #text "b"
#12 "<foo:bar x:y=\"1\">t<\/foo:bar><custom-el>z<\/custom-el><x>y"
13 #document
  10 html html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
  1 html
    1 body
      1 bar [x:y=1]
        3 #text "t"
      1 custom-el
        3 #text "z"
      1 x
        3 #text "y"
#13 "text before <b>bold<\/b> and <i>italic<\/i> after"
13 #document
  10 html html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
  1 html
    1 body
      1 p
        3 #text "text before "
        1 b
          3 #text "bold"
        3 #text " and "
        1 i
          3 #text "italic"
        3 #text " after"
#14 "<span>a<\/span> <span>b<\/span>"
13 #document
  10 html html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
  1 html
    1 body
      1 span
        3 #text "a"
      3 #text " "
      1 span
        3 #text "b"
bool(true)
int(13)
string(4) "html"
string(37) "-//W3C//DTD HTML 4.0 Transitional//EN"
ValueError: DOMDocument::loadHTML(): Argument #1 ($source) must not be empty
"####
    );
}

/// The LIBXML_HTML_NOIMPLIED, LIBXML_HTML_NODEFDTD and LIBXML_NOBLANKS flags, the UTF-8 prolog trick, and a Termwind-style walk of the parsed tree.
#[test]
fn test_dom_load_html_flags() {
    let out = compile_and_run(
        r####"<?php
// TEST: The LIBXML_HTML_NOIMPLIED, LIBXML_HTML_NODEFDTD and LIBXML_NOBLANKS flags, the UTF-8 prolog trick, and a Termwind-style walk of the parsed tree.
function dumpTree(DOMNode $n, int $depth = 0): string
{
    $line = str_repeat('  ', $depth) . $n->nodeType . ' ' . $n->nodeName;
    if ($n instanceof DOMElement) {
        foreach ($n->attributes as $name => $attr) {
            $line .= ' [' . $name . '=' . $attr->value . ']';
        }
    } elseif ($n->nodeType !== 13) {
        $line .= ' ' . json_encode($n->nodeValue);
    }
    $line .= "\n";
    foreach ($n->childNodes as $child) {
        $line .= dumpTree($child, $depth + 1);
    }
    return $line;
}

$src = '<div><p>one</p><p>two</p></div><span>tail</span>';
foreach ([
    'default' => 0,
    'nodefdtd' => LIBXML_HTML_NODEFDTD,
    'noimplied' => LIBXML_HTML_NOIMPLIED,
    'both' => LIBXML_HTML_NOIMPLIED | LIBXML_HTML_NODEFDTD,
] as $label => $flags) {
    $d = new DOMDocument();
    $d->loadHTML($src, $flags);
    echo $label, "\n", dumpTree($d);
}

$ws = "<ul>\n  <li>a</li>\n  <li>b</li>\n</ul>\n<table>\n<tr><td>x</td></tr>\n</table>";
foreach (['keep' => 0, 'noblanks' => LIBXML_NOBLANKS] as $label => $flags) {
    $d = new DOMDocument();
    $d->loadHTML($ws, $flags | LIBXML_HTML_NOIMPLIED | LIBXML_HTML_NODEFDTD);
    echo $label, "\n", dumpTree($d);
}

// Without a charset hint the bytes are read as ISO-8859-1; the xml prolog (or a meta charset) says UTF-8.
$utf8 = '<p>é ü €</p>';
$plain = new DOMDocument();
$plain->loadHTML($utf8);
$prolog = new DOMDocument();
$prolog->loadHTML('<?xml encoding="UTF-8">' . $utf8);
$meta = new DOMDocument();
$meta->loadHTML('<meta charset="utf-8">' . $utf8);
echo bin2hex($plain->getElementsByTagName('p')->item(0)->textContent), "\n";
echo $prolog->getElementsByTagName('p')->item(0)->textContent, "\n";
echo $meta->getElementsByTagName('p')->item(0)->textContent, "\n";

// How Termwind drives it.
$html = '<div class="bg-red-500 px-1"><span class="font-bold">Hello</span> <b>wor&amp;ld</b><br><ul><li>one</li><li>two</li></ul></div>';
$dom = new DOMDocument();
$dom->loadHTML('<?xml encoding="UTF-8">' . trim($html), LIBXML_NOERROR | LIBXML_COMPACT | LIBXML_HTML_NODEFDTD | LIBXML_NOBLANKS | LIBXML_NOXMLDECL);
$body = $dom->getElementsByTagName('body')->item(0);
function walk(DOMNode $node, int $depth): void
{
    $class = $node instanceof DOMElement ? $node->getAttribute('class') : '';
    $parent = $node->parentNode === null ? '-' : $node->parentNode->nodeName;
    echo str_repeat('.', $depth), $node->nodeName, ' type=', $node->nodeType, ' class=[', $class, '] value=', json_encode($node->nodeValue), ' parent=', $parent, ' text=', json_encode($node->textContent), $node instanceof DOMElement && $node->hasAttribute('class') ? ' hasClass' : '', "\n";
    foreach ($node->childNodes as $child) {
        walk($child, $depth + 1);
    }
}
walk($body, 0);
echo $dom->saveHTML($body->firstChild), "\n";
"####,
    );
    assert_eq!(
        out,
        r####"default
13 #document
  10 html null
  1 html
    1 body
      1 div
        1 p
          3 #text "one"
        1 p
          3 #text "two"
      1 span
        3 #text "tail"
nodefdtd
13 #document
  1 html
    1 body
      1 div
        1 p
          3 #text "one"
        1 p
          3 #text "two"
      1 span
        3 #text "tail"
noimplied
13 #document
  10 html null
  1 div
    1 p
      3 #text "one"
    1 p
      3 #text "two"
    1 span
      3 #text "tail"
both
13 #document
  1 div
    1 p
      3 #text "one"
    1 p
      3 #text "two"
    1 span
      3 #text "tail"
keep
13 #document
  1 ul
    3 #text "\n  "
    1 li
      3 #text "a"
    3 #text "\n  "
    1 li
      3 #text "b"
    3 #text "\n"
    1 table
      3 #text "\n"
      1 tr
        1 td
          3 #text "x"
      3 #text "\n"
noblanks
13 #document
  1 ul
    1 li
      3 #text "a"
    3 #text "\n  "
    1 li
      3 #text "b"
    3 #text "\n"
    1 table
      1 tr
        1 td
          3 #text "x"
c383c2a920c383c2bc20c3a2c282c2ac
é ü €
é ü €
body type=1 class=[] value="Hello wor&ldonetwo" parent=html text="Hello wor&ldonetwo"
.div type=1 class=[bg-red-500 px-1] value="Hello wor&ldonetwo" parent=body text="Hello wor&ldonetwo" hasClass
..span type=1 class=[font-bold] value="Hello" parent=div text="Hello" hasClass
...#text type=3 class=[] value="Hello" parent=span text="Hello"
..#text type=3 class=[] value=" " parent=div text=" "
..b type=1 class=[] value="wor&ld" parent=div text="wor&ld"
...#text type=3 class=[] value="wor&ld" parent=b text="wor&ld"
..br type=1 class=[] value="" parent=div text=""
..ul type=1 class=[] value="onetwo" parent=div text="onetwo"
...li type=1 class=[] value="one" parent=ul text="one"
....#text type=3 class=[] value="one" parent=li text="one"
...li type=1 class=[] value="two" parent=ul text="two"
....#text type=3 class=[] value="two" parent=li text="two"
<div class="bg-red-500 px-1"><span class="font-bold">Hello</span> <b>wor&amp;ld</b><br><ul><li>one</li><li>two</li></ul></div>
"####
    );
}

/// saveHTML() serializes with HTML 4 rules: the doctype line, void elements, minimized boolean attributes, quoting, URI escaping, entities, raw script text and formatOutput line breaks.
#[test]
fn test_dom_save_html() {
    let out = compile_and_run(
        r####"<?php
// TEST: saveHTML() serializes with HTML 4 rules: the doctype line, void elements, minimized boolean attributes, quoting, URI escaping, entities, raw script text and formatOutput line breaks.
$d = new DOMDocument();
@$d->loadHTML('<div foo="" bar baz="1" checked class="a&amp;b &quot;q&quot;" title=\'say "hi"\'>x</div><br><input value="" disabled><a href="/x y?a=1&b=2#f" name="n m">link</a><img src="a b.png">');
echo $d->saveHTML();
echo $d->saveHTML($d->getElementsByTagName('a')->item(0)), "\n";
$d->formatOutput = true;
echo $d->saveHTML();

$full = new DOMDocument();
$full->loadHTML('<!DOCTYPE html><html><head><title>T</title><meta charset="utf-8"></head><body><div><p>one</p><p>two</p></div><ul><li>a</li><li></li></ul><table><tr><td>1</td></tr></table><script>var a = "<b>" && 1;</script><!-- c --></body></html>');
echo $full->saveHTML();
$full->formatOutput = true;
echo $full->saveHTML();
echo $full->saveHTML($full->getElementsByTagName('div')->item(0));
echo "\n";

$utf = new DOMDocument();
$utf->loadHTML('<?xml encoding="UTF-8"><p title="é">é ü € 日本 &nbsp; &lt;</p>');
echo $utf->saveHTML();

$built = new DOMDocument();
$div = $built->appendChild($built->createElement('div'));
$div->setAttribute('empty', '');
$div->setAttribute('data-x', 'a"b');
$div->setAttribute('href', 'a b');
$div->appendChild($built->createTextNode('é < &'));
$div->appendChild($built->createElement('br'));
$div->appendChild($built->createComment('c'));
$div->appendChild($built->createCDATASection('cd<>'));
$div->appendChild($built->createProcessingInstruction('pi', 'x'));
echo $built->saveHTML();
echo $built->saveHTML($div->firstChild), "|\n";
$built->formatOutput = true;
echo $built->saveHTML();
"####,
    );
    assert_eq!(
        out,
        r####"<!DOCTYPE html PUBLIC "-//W3C//DTD HTML 4.0 Transitional//EN" "http://www.w3.org/TR/REC-html40/loose.dtd">
<html><body><div foo="" bar baz="1" checked class='a&amp;b "q"' title='say "hi"'>x</div><br><input value="" disabled><a href="/x%20y?a=1&amp;b=2#f" name="n%20m">link</a><img src="a%20b.png"></body></html>
<a href="/x%20y?a=1&amp;b=2#f" name="n%20m">link</a>
<!DOCTYPE html PUBLIC "-//W3C//DTD HTML 4.0 Transitional//EN" "http://www.w3.org/TR/REC-html40/loose.dtd">
<html><body>
<div foo="" bar baz="1" checked class='a&amp;b "q"' title='say "hi"'>x</div>
<br><input value="" disabled><a href="/x%20y?a=1&amp;b=2#f" name="n%20m">link</a><img src="a%20b.png">
</body></html>
<!DOCTYPE html>
<html><head><title>T</title><meta charset="utf-8"></head><body><div><p>one</p><p>two</p></div><ul><li>a</li><li></ul><table><tr><td>1</td></tr></table><script>var a = "<b>" && 1;</script><!-- c --></body></html>
<!DOCTYPE html>
<html>
<head>
<title>T</title>
<meta charset="utf-8">
</head>
<body>
<div>
<p>one</p>
<p>two</p>
</div>
<ul>
<li>a</li>
<li>
</ul>
<table><tr><td>1</td></tr></table>
<script>var a = "<b>" && 1;</script><!-- c -->
</body>
</html>
<div>
<p>one</p>
<p>two</p>
</div>

<!DOCTYPE html PUBLIC "-//W3C//DTD HTML 4.0 Transitional//EN" "http://www.w3.org/TR/REC-html40/loose.dtd">
<?xml encoding="UTF-8"><html><body><p title="&eacute;">&eacute; &uuml; &euro; &#26085;&#26412; &nbsp; &lt;</p></body></html>
<div empty="" data-x='a"b' href="a%20b">&eacute; &lt; &amp;<br><!--c-->cd<><?pi x></div>
&#xE9; &lt; &amp;|
<div empty="" data-x='a"b' href="a%20b">&eacute; &lt; &amp;<br><!--c-->cd<><?pi x>
</div>
"####
    );
}

/// DOMException codes and messages for invalid operations, and the predefined LIBXML_*, XML_*_NODE and DOM_*_ERR constants.
#[test]
fn test_dom_exceptions_and_constants() {
    let out = compile_and_run(
        r####"<?php
// TEST: DOMException codes and messages for invalid operations, and the predefined LIBXML_*, XML_*_NODE and DOM_*_ERR constants.
function report(string $label, DOMException $e): void
{
    echo $label, ': DOMException ', $e->getCode(), ' ', $e->getMessage(), "\n";
}
$d = new DOMDocument();
$other = new DOMDocument();
$p = $d->createElement('p');
$k = $d->createElement('k');
$p->appendChild($k);
try { $p->removeChild($d->createElement('z')); } catch (DOMException $e) { report('remove stranger', $e); }
try { $p->appendChild($other->createElement('n')); } catch (DOMException $e) { report('foreign document', $e); }
try { $k->appendChild($p); } catch (DOMException $e) { report('cycle', $e); }
try { $p->appendChild($p); } catch (DOMException $e) { report('self', $e); }
try { $p->insertBefore($d->createElement('n'), $d->createElement('ref')); } catch (DOMException $e) { report('insert before stranger', $e); }
try { $p->replaceChild($d->createElement('n'), $d->createElement('old')); } catch (DOMException $e) { report('replace stranger', $e); }
try { $d->createElement('1x'); } catch (DOMException $e) { report('bad element name', $e); }
try { $d->createElement(''); } catch (DOMException $e) { report('empty element name', $e); }
try { $p->setAttribute('a b', 'v'); } catch (DOMException $e) { report('bad attribute name', $e); }
try { $d->createTextNode('abc')->substringData(9, 1); } catch (DOMException $e) { report('text index', $e); }
$fine = $p->appendChild($d->createElement('fine'));
echo get_class($fine), ' ', $p->childNodes->length, "\n";
$attr = $p->appendChild($d->createAttribute('moved'));
echo get_class($attr), var_export($p->hasAttribute('moved'), true), "\n";
echo get_parent_class(new DOMException('m', 3)), ' ', (new DOMException('m', 3))->getCode(), "\n";

var_dump(LIBXML_NOERROR, LIBXML_NOWARNING, LIBXML_NOBLANKS, LIBXML_NOCDATA, LIBXML_NOXMLDECL, LIBXML_NOEMPTYTAG, LIBXML_HTML_NOIMPLIED, LIBXML_HTML_NODEFDTD, LIBXML_COMPACT, LIBXML_PARSEHUGE, LIBXML_NONET, LIBXML_RECOVER, LIBXML_NOENT, LIBXML_DTDLOAD, LIBXML_DTDATTR, LIBXML_DTDVALID, LIBXML_XINCLUDE, LIBXML_NSCLEAN, LIBXML_PEDANTIC, LIBXML_ERR_FATAL);
var_dump(XML_ELEMENT_NODE, XML_ATTRIBUTE_NODE, XML_TEXT_NODE, XML_CDATA_SECTION_NODE, XML_PI_NODE, XML_COMMENT_NODE, XML_DOCUMENT_NODE, XML_DOCUMENT_TYPE_NODE, XML_DOCUMENT_FRAG_NODE, XML_HTML_DOCUMENT_NODE);
var_dump(DOM_INDEX_SIZE_ERR, DOM_HIERARCHY_REQUEST_ERR, DOM_WRONG_DOCUMENT_ERR, DOM_INVALID_CHARACTER_ERR, DOM_NOT_FOUND_ERR, DOM_NOT_SUPPORTED_ERR, DOM_INUSE_ATTRIBUTE_ERR, DOM_SYNTAX_ERR, DOM_VALIDATION_ERR);
define('MY_OWN', 1);
echo implode(',', array_keys(get_defined_constants(true)['user'])), "\n";

var_dump(libxml_use_internal_errors(true), libxml_use_internal_errors(false), libxml_use_internal_errors());
libxml_clear_errors();
var_dump(libxml_get_errors());
$x = new DOMDocument();
libxml_use_internal_errors(true);
var_dump($x->loadXML('<a><b></a>'));
libxml_use_internal_errors(false);
"####,
    );
    assert_eq!(
        out,
        r####"remove stranger: DOMException 8 Not Found Error
foreign document: DOMException 4 Wrong Document Error
cycle: DOMException 3 Hierarchy Request Error
self: DOMException 3 Hierarchy Request Error
insert before stranger: DOMException 8 Not Found Error
replace stranger: DOMException 8 Not Found Error
bad element name: DOMException 5 Invalid Character Error
empty element name: DOMException 5 Invalid Character Error
bad attribute name: DOMException 5 Invalid Character Error
text index: DOMException 1 Index Size Error
DOMElement 2
DOMAttrtrue
Exception 3
int(32)
int(64)
int(256)
int(16384)
int(2)
int(4)
int(8192)
int(4)
int(65536)
int(524288)
int(2048)
int(1)
int(2)
int(4)
int(8)
int(16)
int(1024)
int(8192)
int(128)
int(3)
int(1)
int(2)
int(3)
int(4)
int(7)
int(8)
int(9)
int(10)
int(11)
int(13)
int(1)
int(3)
int(4)
int(5)
int(8)
int(9)
int(10)
int(12)
int(16)
MY_OWN
bool(false)
bool(true)
bool(false)
array(0) {
}
bool(false)
"####
    );
}

/// symfony/config's `XmlUtils::getXmlErrors()` reads `$error->level` ... `$error->column` off each
/// `libxml_get_errors()` entry: the result must be a list of `LibXMLError`, empty at run time.
#[test]
fn test_dom_libxml_get_errors_elements_are_typed() {
    let out = compile_and_run(
        r####"<?php
// Shaped like symfony/config XmlUtils::getXmlErrors().
function getXmlErrors(bool $internalErrors): array
{
    $errors = [];
    foreach (libxml_get_errors() as $error) {
        $errors[] = sprintf('[%s %s] %s (in %s - line %d, column %d)',
            LIBXML_ERR_WARNING == $error->level ? 'WARNING' : 'ERROR',
            $error->code,
            trim($error->message),
            $error->file ?: 'n/a',
            $error->line,
            $error->column
        );
    }

    libxml_clear_errors();
    libxml_use_internal_errors($internalErrors);

    return $errors;
}

$previous = libxml_use_internal_errors(true);
$doc = new DOMDocument();
$doc->loadXML('<a><b/></a>');
var_dump(getXmlErrors($previous));
"####,
    );
    assert_eq!(
        out,
        r####"array(0) {
}
"####
    );
}

/// `LibXMLError` has php's typed, uninitialized public properties and no others.
#[test]
fn test_dom_libxml_error_class_matches_php() {
    let out = compile_and_run(
        r####"<?php
var_dump(new LibXMLError());
$e = new LibXMLError();
$e->level = 2;
$e->message = "m";
echo $e->level, $e->message, "\n";
var_dump($e);
"####,
    );
    assert_eq!(
        out,
        r####"object(LibXMLError)#1 (0) {
  ["level"]=>
  uninitialized(int)
  ["code"]=>
  uninitialized(int)
  ["column"]=>
  uninitialized(int)
  ["message"]=>
  uninitialized(string)
  ["file"]=>
  uninitialized(string)
  ["line"]=>
  uninitialized(int)
}
2m
object(LibXMLError)#1 (2) {
  ["level"]=>
  int(2)
  ["code"]=>
  uninitialized(int)
  ["column"]=>
  uninitialized(int)
  ["message"]=>
  string(1) "m"
  ["file"]=>
  uninitialized(string)
  ["line"]=>
  uninitialized(int)
}
"####
    );
}
