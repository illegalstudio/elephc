//! Purpose:
//! PHP 8.5.8 `ext/dom` php-src tests for regressions filed as `bugNNNNN` against the legacy DOM API, as Elephc codegen tests.
//!
//! Called from:
//! - `cargo test` through `tests/codegen/dom_php_src/mod.rs`.
//!
//! Key details:
//! - Each test keeps the php-src program and expected output verbatim; its doc
//!   comment names the PHPT it was migrated from.
//! - A test Elephc does not pass yet is `#[ignore]`d with the missing behaviour, and
//!   php-src's own environment skips keep their `SKIPIF` message.

use super::*;

/// php-src `ext/dom/tests/bug28721.phpt`: Bug #28721 (appendChild() and insertBefore() unset DOMText)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createTextNode"#]
fn test_bug28721() {
    if skip_without_xml_native("test_bug28721") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
function print_node(DomNode $node) {
  echo "name (value): " . $node->nodeName . " (" . $node->nodeValue . ")\n";
}

function print_node_r(DomNode $node) {
  static $indent = "";
  echo "\n" . $indent;
  print_node($node);

  echo $indent . "parent: ";
  if ( $node->parentNode )
    print_node($node->parentNode);
  else
    echo "NULL\n";

  echo $indent . "previousSibling: ";
  if ( $node->previousSibling )
    print_node($node->previousSibling);
  else
    echo "NULL\n";

  echo $indent . "nextSibling: ";
  if ( $node->nextSibling )
    print_node($node->nextSibling);
  else
    echo "NULL\n";

  if ( !$node->hasChildNodes() )
    return;

  foreach ($node->childNodes as $child) {

    $old_indent  = $indent;
    $indent .= "  ";
    print_node_r($child);
    $indent = $old_indent;
  }
}

function err_handler($errno, $errstr, $errfile, $errline) {
  echo "Error ($errno) on line $errline: $errstr\n";
}

// Record 'DocumentFragment is empty' warnings
set_error_handler("err_handler", E_WARNING);

$xml = new DomDocument();

$p = $xml->createElement("p");

$p->appendChild($t1 = $xml->createTextNode(" t1 "));
$p->appendChild($b = $xml->createElement("b"));
$b->appendChild($xml->createTextNode("X"));
$p->appendChild($t2 = $xml->createTextNode(" t2 "));
$p->appendChild($xml->createTextNode(" xxx "));

print_node_r($p);

echo "\nAppend t1 to p:\n";
$ret = $p->appendChild($t1);

print_node_r($p);
echo "\n";

echo "t1 == ret: ";
var_dump( $t1 === $ret );


$d = $xml->createElement("div");
$d->appendChild($t3 = $xml->createTextNode(" t3 "));
$d->appendChild($b = $xml->createElement("b"));
$b->appendChild($xml->createElement("X"));
$d->appendChild($t4 = $xml->createTextNode(" t4 "));
$d->appendChild($xml->createTextNode(" xxx "));

echo "\ndiv:\n";
print_node_r($d);

echo "\nInsert t4 before t3:\n";

$ret = $d->insertBefore($t4, $t3);

print_node_r($d);
echo "\n";

$frag = $xml->createDocumentFragment();

$t5 = $frag->appendChild($xml->createTextNode(" t5 "));
$frag->appendChild($i = $xml->createElement("i"));
$i->appendChild($xml->createTextNode(" frob "));
$frag->appendChild($xml->createTextNOde(" t6 "));

echo "\np:\n";
print_node_r($p);
echo "\nFragment:\n";
print_node_r($frag);

echo "\nAppending fragment to p:\n";
$p->appendChild($frag);

print_node_r($p);
echo "\nFragment:\n";
print_node_r($frag);

echo "\ndiv:\n";
print_node_r($d);
echo "\nInserting fragment before t4\n";
$d->insertBefore($frag, $t4);
print_node_r($d);

echo "\np:\n";
print_node_r($p);

"#);
    assert_exact(&out, r#"name (value): p ( t1 X t2  xxx )
parent: NULL
previousSibling: NULL
nextSibling: NULL

  name (value): #text ( t1 )
  parent: name (value): p ( t1 X t2  xxx )
  previousSibling: NULL
  nextSibling: name (value): b (X)

  name (value): b (X)
  parent: name (value): p ( t1 X t2  xxx )
  previousSibling: name (value): #text ( t1 )
  nextSibling: name (value): #text ( t2 )

    name (value): #text (X)
    parent: name (value): b (X)
    previousSibling: NULL
    nextSibling: NULL

  name (value): #text ( t2 )
  parent: name (value): p ( t1 X t2  xxx )
  previousSibling: name (value): b (X)
  nextSibling: name (value): #text ( xxx )

  name (value): #text ( xxx )
  parent: name (value): p ( t1 X t2  xxx )
  previousSibling: name (value): #text ( t2 )
  nextSibling: NULL

Append t1 to p:

name (value): p (X t2  xxx  t1 )
parent: NULL
previousSibling: NULL
nextSibling: NULL

  name (value): b (X)
  parent: name (value): p (X t2  xxx  t1 )
  previousSibling: NULL
  nextSibling: name (value): #text ( t2 )

    name (value): #text (X)
    parent: name (value): b (X)
    previousSibling: NULL
    nextSibling: NULL

  name (value): #text ( t2 )
  parent: name (value): p (X t2  xxx  t1 )
  previousSibling: name (value): b (X)
  nextSibling: name (value): #text ( xxx )

  name (value): #text ( xxx )
  parent: name (value): p (X t2  xxx  t1 )
  previousSibling: name (value): #text ( t2 )
  nextSibling: name (value): #text ( t1 )

  name (value): #text ( t1 )
  parent: name (value): p (X t2  xxx  t1 )
  previousSibling: name (value): #text ( xxx )
  nextSibling: NULL

t1 == ret: bool(true)

div:

name (value): div ( t3  t4  xxx )
parent: NULL
previousSibling: NULL
nextSibling: NULL

  name (value): #text ( t3 )
  parent: name (value): div ( t3  t4  xxx )
  previousSibling: NULL
  nextSibling: name (value): b ()

  name (value): b ()
  parent: name (value): div ( t3  t4  xxx )
  previousSibling: name (value): #text ( t3 )
  nextSibling: name (value): #text ( t4 )

    name (value): X ()
    parent: name (value): b ()
    previousSibling: NULL
    nextSibling: NULL

  name (value): #text ( t4 )
  parent: name (value): div ( t3  t4  xxx )
  previousSibling: name (value): b ()
  nextSibling: name (value): #text ( xxx )

  name (value): #text ( xxx )
  parent: name (value): div ( t3  t4  xxx )
  previousSibling: name (value): #text ( t4 )
  nextSibling: NULL

Insert t4 before t3:

name (value): div ( t4  t3  xxx )
parent: NULL
previousSibling: NULL
nextSibling: NULL

  name (value): #text ( t4 )
  parent: name (value): div ( t4  t3  xxx )
  previousSibling: NULL
  nextSibling: name (value): #text ( t3 )

  name (value): #text ( t3 )
  parent: name (value): div ( t4  t3  xxx )
  previousSibling: name (value): #text ( t4 )
  nextSibling: name (value): b ()

  name (value): b ()
  parent: name (value): div ( t4  t3  xxx )
  previousSibling: name (value): #text ( t3 )
  nextSibling: name (value): #text ( xxx )

    name (value): X ()
    parent: name (value): b ()
    previousSibling: NULL
    nextSibling: NULL

  name (value): #text ( xxx )
  parent: name (value): div ( t4  t3  xxx )
  previousSibling: name (value): b ()
  nextSibling: NULL


p:

name (value): p (X t2  xxx  t1 )
parent: NULL
previousSibling: NULL
nextSibling: NULL

  name (value): b (X)
  parent: name (value): p (X t2  xxx  t1 )
  previousSibling: NULL
  nextSibling: name (value): #text ( t2 )

    name (value): #text (X)
    parent: name (value): b (X)
    previousSibling: NULL
    nextSibling: NULL

  name (value): #text ( t2 )
  parent: name (value): p (X t2  xxx  t1 )
  previousSibling: name (value): b (X)
  nextSibling: name (value): #text ( xxx )

  name (value): #text ( xxx )
  parent: name (value): p (X t2  xxx  t1 )
  previousSibling: name (value): #text ( t2 )
  nextSibling: name (value): #text ( t1 )

  name (value): #text ( t1 )
  parent: name (value): p (X t2  xxx  t1 )
  previousSibling: name (value): #text ( xxx )
  nextSibling: NULL

Fragment:

name (value): #document-fragment ()
parent: NULL
previousSibling: NULL
nextSibling: NULL

  name (value): #text ( t5 )
  parent: name (value): #document-fragment ()
  previousSibling: NULL
  nextSibling: name (value): i ( frob )

  name (value): i ( frob )
  parent: name (value): #document-fragment ()
  previousSibling: name (value): #text ( t5 )
  nextSibling: name (value): #text ( t6 )

    name (value): #text ( frob )
    parent: name (value): i ( frob )
    previousSibling: NULL
    nextSibling: NULL

  name (value): #text ( t6 )
  parent: name (value): #document-fragment ()
  previousSibling: name (value): i ( frob )
  nextSibling: NULL

Appending fragment to p:

name (value): p (X t2  xxx  t1  t5  frob  t6 )
parent: NULL
previousSibling: NULL
nextSibling: NULL

  name (value): b (X)
  parent: name (value): p (X t2  xxx  t1  t5  frob  t6 )
  previousSibling: NULL
  nextSibling: name (value): #text ( t2 )

    name (value): #text (X)
    parent: name (value): b (X)
    previousSibling: NULL
    nextSibling: NULL

  name (value): #text ( t2 )
  parent: name (value): p (X t2  xxx  t1  t5  frob  t6 )
  previousSibling: name (value): b (X)
  nextSibling: name (value): #text ( xxx )

  name (value): #text ( xxx )
  parent: name (value): p (X t2  xxx  t1  t5  frob  t6 )
  previousSibling: name (value): #text ( t2 )
  nextSibling: name (value): #text ( t1 )

  name (value): #text ( t1 )
  parent: name (value): p (X t2  xxx  t1  t5  frob  t6 )
  previousSibling: name (value): #text ( xxx )
  nextSibling: name (value): #text ( t5 )

  name (value): #text ( t5 )
  parent: name (value): p (X t2  xxx  t1  t5  frob  t6 )
  previousSibling: name (value): #text ( t1 )
  nextSibling: name (value): i ( frob )

  name (value): i ( frob )
  parent: name (value): p (X t2  xxx  t1  t5  frob  t6 )
  previousSibling: name (value): #text ( t5 )
  nextSibling: name (value): #text ( t6 )

    name (value): #text ( frob )
    parent: name (value): i ( frob )
    previousSibling: NULL
    nextSibling: NULL

  name (value): #text ( t6 )
  parent: name (value): p (X t2  xxx  t1  t5  frob  t6 )
  previousSibling: name (value): i ( frob )
  nextSibling: NULL

Fragment:

name (value): #document-fragment ()
parent: NULL
previousSibling: NULL
nextSibling: NULL

div:

name (value): div ( t4  t3  xxx )
parent: NULL
previousSibling: NULL
nextSibling: NULL

  name (value): #text ( t4 )
  parent: name (value): div ( t4  t3  xxx )
  previousSibling: NULL
  nextSibling: name (value): #text ( t3 )

  name (value): #text ( t3 )
  parent: name (value): div ( t4  t3  xxx )
  previousSibling: name (value): #text ( t4 )
  nextSibling: name (value): b ()

  name (value): b ()
  parent: name (value): div ( t4  t3  xxx )
  previousSibling: name (value): #text ( t3 )
  nextSibling: name (value): #text ( xxx )

    name (value): X ()
    parent: name (value): b ()
    previousSibling: NULL
    nextSibling: NULL

  name (value): #text ( xxx )
  parent: name (value): div ( t4  t3  xxx )
  previousSibling: name (value): b ()
  nextSibling: NULL

Inserting fragment before t4
Error (2) on line 109: DOMNode::insertBefore(): Document Fragment is empty

name (value): div ( t4  t3  xxx )
parent: NULL
previousSibling: NULL
nextSibling: NULL

  name (value): #text ( t4 )
  parent: name (value): div ( t4  t3  xxx )
  previousSibling: NULL
  nextSibling: name (value): #text ( t3 )

  name (value): #text ( t3 )
  parent: name (value): div ( t4  t3  xxx )
  previousSibling: name (value): #text ( t4 )
  nextSibling: name (value): b ()

  name (value): b ()
  parent: name (value): div ( t4  t3  xxx )
  previousSibling: name (value): #text ( t3 )
  nextSibling: name (value): #text ( xxx )

    name (value): X ()
    parent: name (value): b ()
    previousSibling: NULL
    nextSibling: NULL

  name (value): #text ( xxx )
  parent: name (value): div ( t4  t3  xxx )
  previousSibling: name (value): b ()
  nextSibling: NULL

p:

name (value): p (X t2  xxx  t1  t5  frob  t6 )
parent: NULL
previousSibling: NULL
nextSibling: NULL

  name (value): b (X)
  parent: name (value): p (X t2  xxx  t1  t5  frob  t6 )
  previousSibling: NULL
  nextSibling: name (value): #text ( t2 )

    name (value): #text (X)
    parent: name (value): b (X)
    previousSibling: NULL
    nextSibling: NULL

  name (value): #text ( t2 )
  parent: name (value): p (X t2  xxx  t1  t5  frob  t6 )
  previousSibling: name (value): b (X)
  nextSibling: name (value): #text ( xxx )

  name (value): #text ( xxx )
  parent: name (value): p (X t2  xxx  t1  t5  frob  t6 )
  previousSibling: name (value): #text ( t2 )
  nextSibling: name (value): #text ( t1 )

  name (value): #text ( t1 )
  parent: name (value): p (X t2  xxx  t1  t5  frob  t6 )
  previousSibling: name (value): #text ( xxx )
  nextSibling: name (value): #text ( t5 )

  name (value): #text ( t5 )
  parent: name (value): p (X t2  xxx  t1  t5  frob  t6 )
  previousSibling: name (value): #text ( t1 )
  nextSibling: name (value): i ( frob )

  name (value): i ( frob )
  parent: name (value): p (X t2  xxx  t1  t5  frob  t6 )
  previousSibling: name (value): #text ( t5 )
  nextSibling: name (value): #text ( t6 )

    name (value): #text ( frob )
    parent: name (value): i ( frob )
    previousSibling: NULL
    nextSibling: NULL

  name (value): #text ( t6 )
  parent: name (value): p (X t2  xxx  t1  t5  frob  t6 )
  previousSibling: name (value): i ( frob )
  nextSibling: NULL"#);
}

/// php-src `ext/dom/tests/bug28817.phpt`: Bug #28817 (properties in extended class)
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_bug28817() {
    if skip_without_xml_native("test_bug28817") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

class z extends domDocument{
    /** variable can have name */
    public $p_array;
    public $p_variable;

    function __construct(){
        $this->p_array[] = 'bonus';
        $this->p_array[] = 'vir';
        $this->p_array[] = 'semper';
        $this->p_array[] = 'tiro';

        $this->p_variable = 'Cessante causa cessat effectus';
    }
}

$z=new z();
var_dump($z->p_array);
var_dump($z->p_variable);
"#);
    assert_exact(&out, r#"array(4) {
  [0]=>
  string(5) "bonus"
  [1]=>
  string(3) "vir"
  [2]=>
  string(6) "semper"
  [3]=>
  string(4) "tiro"
}
string(30) "Cessante causa cessat effectus""#);
}

/// php-src `ext/dom/tests/bug32615.phpt`: Bug #32615 (Replacing and inserting Fragments)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createDocumentFragment"#]
fn test_bug32615() {
    if skip_without_xml_native("test_bug32615") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = new DomDocument;
$frag = $dom->createDocumentFragment();
$frag->appendChild(new DOMElement('root'));
$dom->appendChild($frag);
$root = $dom->documentElement;

$frag->appendChild(new DOMElement('first'));
$root->appendChild($frag);

$frag->appendChild(new DOMElement('second'));
$root->appendChild($frag);

$node = $dom->createElement('newfirst');
$frag->appendChild($node);
$root->replaceChild($frag, $root->firstChild);

unset($frag);
$frag = $dom->createDocumentFragment();

$frag->appendChild(new DOMElement('newsecond'));
$root->replaceChild($frag, $root->lastChild);

$node = $frag->appendChild(new DOMElement('fourth'));
$root->insertBefore($frag, NULL);

$frag->appendChild(new DOMElement('third'));
$node = $root->insertBefore($frag, $node);

$frag->appendChild(new DOMElement('start'));
$root->insertBefore($frag, $root->firstChild);

$frag->appendChild(new DOMElement('newthird'));
$root->replaceChild($frag, $node);

$frag->appendChild(new DOMElement('newfourth'));
$root->replaceChild($frag, $root->lastChild);

$frag->appendChild(new DOMElement('first'));
$root->replaceChild($frag, $root->firstChild->nextSibling);

$root->removeChild($root->firstChild);

echo $dom->saveXML()."\n";

while ($root->hasChildNodes()) {
   $root->removeChild($root->firstChild);
}

$frag->appendChild(new DOMElement('first'));
$root->insertBefore($frag, $root->firstChild);

$node = $frag->appendChild(new DOMElement('fourth'));
$root->appendChild($frag);

$frag->appendChild(new DOMElement('second'));
$frag->appendChild(new DOMElement('third'));
$root->insertBefore($frag, $node);

echo $dom->saveXML()."\n";

$frag = $dom->createDocumentFragment();
$root = $dom->documentElement;
$root->replaceChild($frag, $root->firstChild);

echo $dom->saveXML();

"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<root><first/><newsecond/><newthird/><newfourth/></root>

<?xml version="1.0"?>
<root><first/><second/><third/><fourth/></root>

<?xml version="1.0"?>
<root><second/><third/><fourth/></root>"#);
}

/// php-src `ext/dom/tests/bug34276.phpt`: Bug #34276 (setAttributeNS and default namespace)
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_bug34276() {
    if skip_without_xml_native("test_bug34276") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$xml = <<<HERE
<?xml version="1.0" encoding="ISO-8859-1" ?>
<foo xmlns="http://www.example.com/ns/foo"
     xmlns:fubar="http://www.example.com/ns/fubar" attra="attra" />
HERE;

function dump($elems) {
    foreach ($elems as $elem) {
        var_dump($elem->nodeName);
        dump($elem->childNodes);
    }
}

$dom = new DOMDocument();
$dom->loadXML($xml);
$foo = $dom->documentElement;
var_dump($foo->hasAttributeNS('http://www.example.com/ns/foo', 'attra'));
var_dump($foo->getAttributeNS('http://www.example.com/ns/foo', 'attra'));

$foo->setAttributeNS('http://www.example.com/ns/foo', 'attra', 'attranew');
$foo->setAttributeNS('http://www.example.com/ns/fubar', 'attrb', 'attrbnew');
$foo->setAttributeNS('http://www.example.com/ns/foo', 'attrc', 'attrc');

var_dump($foo->getAttributeNS('http://www.example.com/ns/foo', 'attra'));
var_dump($foo->getAttributeNS('http://www.example.com/ns/fubar', 'attrb'));
var_dump($foo->getAttributeNS('http://www.example.com/ns/foo', 'attrc'));

print $dom->saveXML();
"#);
    assert_exact(&out, r#"bool(false)
string(0) ""
string(8) "attranew"
string(8) "attrbnew"
string(5) "attrc"
<?xml version="1.0" encoding="ISO-8859-1"?>
<foo xmlns="http://www.example.com/ns/foo" xmlns:fubar="http://www.example.com/ns/fubar" xmlns:default="http://www.example.com/ns/foo" attra="attra" default:attra="attranew" fubar:attrb="attrbnew" default:attrc="attrc"/>"#);
}

/// php-src `ext/dom/tests/bug35342.phpt`: Bug #35342 (isset(DOMNodeList->length) returns false)
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMNodeList::foo"#]
fn test_bug35342() {
    if skip_without_xml_native("test_bug35342") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = new DOMDocument();
$dom->loadXML("<root><foo>foobar</foo><foo>foobar#2</foo></root>");

$nodelist = $dom->getElementsByTagName("foo");

var_dump($nodelist->length, isset($nodelist->length), isset($nodelist->foo));
var_dump(empty($nodelist->length), empty($nodelist->foo));
"#);
    assert_exact(&out, r#"int(2)
bool(true)
bool(false)
bool(false)
bool(true)"#);
}

/// php-src `ext/dom/tests/bug35673.phpt`: Bug #35673 (formatOutput does not work with saveHTML).
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::formatOutput"#]
fn test_bug35673() {
    if skip_without_xml_native("test_bug35673") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$html = '<html><head><meta http-equiv="Content-Type" content="text/html; charset=UTF-8">
<title>This is the title</title></head></html>';

$htmldoc = new DOMDocument();
$htmldoc->loadHTML($html);
$htmldoc->formatOutput = true;
echo $htmldoc->saveHTML();
"#);
    assert_exact(&out, r#"<!DOCTYPE html PUBLIC "-//W3C//DTD HTML 4.0 Transitional//EN" "http://www.w3.org/TR/REC-html40/loose.dtd">
<html><head>
<meta http-equiv="Content-Type" content="text/html; charset=UTF-8">
<title>This is the title</title>
</head></html>"#);
}

/// php-src `ext/dom/tests/bug36756.phpt`: Bug #36756 (DOMDocument::removeChild corrupts node)
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXpath"#]
fn test_bug36756() {
    if skip_without_xml_native("test_bug36756") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

/* Node is preserved from removeChild */
$dom = new DOMDocument();
$dom->loadXML('<root><child/></root>');
$xpath = new DOMXpath($dom);
$node = $xpath->query('/root')->item(0);
echo $node->nodeName . "\n";
$dom->removeChild($GLOBALS['dom']->firstChild);
echo "nodeType: " . $node->nodeType . "\n";
/* Node gets destroyed during removeChild */
$dom->loadXML('<root><child/></root>');
$xpath = new DOMXpath($dom);
$node = $xpath->query('//child')->item(0);
echo $node->nodeName . "\n";
$GLOBALS['dom']->removeChild($GLOBALS['dom']->firstChild);

echo "nodeType: " . $node->nodeType . "\n";

"#);
    assert_exact(&out, r#"root
nodeType: 1
child
nodeType: 1"#);
}

/// php-src `ext/dom/tests/bug37277.phpt`: Bug #37277 (cloning Dom Documents or Nodes does not work)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::importNode"#]
fn test_bug37277() {
    if skip_without_xml_native("test_bug37277") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom1 = new DomDocument('1.0', 'UTF-8');

$xml = '<foo />';
$dom1->loadXml($xml);

$node = clone $dom1->documentElement;

$dom2 = new DomDocument('1.0', 'UTF-8');
$dom2->appendChild($dom2->importNode($node->cloneNode(true), TRUE));

print $dom2->saveXML();


"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<foo/>"#);
}

/// php-src `ext/dom/tests/bug37456.phpt`: Bug #37456 (DOMElement->setAttribute() loops forever)
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::resolveExternals"#]
fn test_bug37456() {
    if skip_without_xml_native("test_bug37456") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument();
$doc->resolveExternals = true;
$doc->load(__DIR__."/dom.xml");

$root = $doc->getElementsByTagName('foo')->item(0);
$root->setAttribute('bar', '&gt;');
$attr = $root->setAttribute('bar', 'newval');
print $attr->nodeValue;


"#);
    assert_exact(&out, r#"newval"#);
}

/// php-src `ext/dom/tests/bug38438.phpt`: Bug #38438 (DOMNodeList->item(0) segfault on empty NodeList)
#[test]
fn test_bug38438() {
    if skip_without_xml_native("test_bug38438") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$list = new DOMNodeList();
var_dump($list->item(0));
echo "OK\n";
"#);
    assert_exact(&out, r#"NULL
OK"#);
}

/// php-src `ext/dom/tests/bug38474.phpt`: Bug #38474 (getAttribute select attribute by order, even when prefixed) (OK to fail with libxml2 < 2.6.2x)
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::firstChild"#]
fn test_bug38474() {
    if skip_without_xml_native("test_bug38474") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$xml = '<node xmlns:pre="http://foo.com/tr/pre"
              xmlns:post="http://foo.com/tr/post"
              pre:type="bar" type="foo" ><sub /></node>';
$dom = new DomDocument();
$dom->loadXML($xml);
echo $dom->firstChild->getAttribute('type')."\n";
echo $dom->firstChild->getAttribute('pre:type')."\n";

$dom->firstChild->setAttribute('pre:type', 'bar2');
$dom->firstChild->setAttribute('type', 'foo2');
$dom->firstChild->setAttribute('post:type', 'baz');
$dom->firstChild->setAttribute('new:type', 'baz2');

echo $dom->firstChild->getAttribute('type')."\n";
echo $dom->firstChild->getAttribute('pre:type')."\n";
echo $dom->firstChild->getAttribute('post:type')."\n";

$dom->firstChild->removeAttribute('pre:type');
$dom->firstChild->removeAttribute('type');

echo $dom->firstChild->getAttribute('type')."\n";
echo $dom->firstChild->getAttribute('pre:type')."\n";
echo $dom->firstChild->getAttribute('post:type')."\n";
echo $dom->firstChild->getAttribute('new:type');
"#);
    assert_exact(&out, r#"foo
bar
foo2
bar2
baz


baz
baz2"#);
}

/// php-src `ext/dom/tests/bug38850.phpt`: Bug #38850 (lookupNamespaceURI does not return default namespace)
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_bug38850() {
    if skip_without_xml_native("test_bug38850") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$xml = <<<HERE
<?xml version="1.0" ?>
<foo xmlns="http://www.example.com/ns/foo" />
HERE;

$doc = new DOMDocument();
$doc->loadXML($xml);

$root = $doc->documentElement;

print $root->lookupNamespaceURI(NULL);


"#);
    assert_exact(&out, r#"http://www.example.com/ns/foo"#);
}

/// php-src `ext/dom/tests/bug38949.phpt`: Bug #38949 (Cannot get xmlns value attribute)
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_bug38949() {
    if skip_without_xml_native("test_bug38949") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument();
$doc->load(__DIR__."/nsdoc.xml");

$root = $doc->documentElement;

echo $root->getAttribute("xmlns")."\n";
echo $root->getAttribute("xmlns:ns2")."\n";

$child = $root->firstChild->nextSibling;
echo $child->getAttribute("xmlns")."\n";
echo $child->getAttribute("xmlns:ns2")."\n";

echo "DONE\n";
"#);
    assert_exact(&out, r#"http://ns
http://ns2


DONE"#);
}

/// php-src `ext/dom/tests/bug40836.phpt`: Bug #40836 (Segfault in insertBefore)
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::preserveWhiteSpace"#]
fn test_bug40836() {
    if skip_without_xml_native("test_bug40836") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = new DOMDocument("1.0", "UTF-8");
$dom->preserveWhiteSpace = false;
$xml = '<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <entry xmlns="http://www.w3.org/2005/Atom">
    <updated>2007-02-14T00:00:00+01:00</updated>
    <content>
      <div xmlns="http://www.w3.org/1999/xhtml">
        <p>paragraph</p>
      </div>
    </content>
  </entry>
</feed>';
$dom->loadXML($xml);
$entry = $dom->getElementsByTagNameNS("http://www.w3.org/2005/Atom", "entry")->item(0);
$contentNode = $entry->getElementsByTagName("content")->item(0)->firstChild;
$dateNode = $entry->getElementsByTagName("updated")->item(0)->firstChild;
$contentNode->firstChild->insertBefore($dateNode);
echo $dom->saveXML();
"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom"><entry xmlns="http://www.w3.org/2005/Atom"><updated/><content><div xmlns="http://www.w3.org/1999/xhtml"><p>paragraph2007-02-14T00:00:00+01:00</p></div></content></entry></feed>"#);
}

/// php-src `ext/dom/tests/bug41257.phpt`: Bug #41257 (lookupNamespaceURI does not work as expected)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::lookupNamespaceURI"#]
fn test_bug41257() {
    if skip_without_xml_native("test_bug41257") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument();
$doc->load(__DIR__."/nsdoc.xml");

$root = $doc->documentElement;

$duri = $doc->lookupNamespaceURI("ns2")."\n";
$euri = $root->lookupNamespaceURI("ns2")."\n";

var_dump($duri == $euri);

$dpref = $doc->lookupPrefix("http://ns2")."\n";
$epref = $root->lookupPrefix("http://ns2")."\n";

var_dump($dpref == $epref);

$disdef = $doc->isDefaultNamespace("http://ns")."\n";
$eisdef = $root->isDefaultNamespace("http://ns")."\n";

var_dump($dpref === $epref);
"#);
    assert_exact(&out, r#"bool(true)
bool(true)
bool(true)"#);
}

/// php-src `ext/dom/tests/bug41374.phpt`: Bug #41374 (wholetext concats values of wrong nodes)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createTextNode"#]
fn test_bug41374() {
    if skip_without_xml_native("test_bug41374") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$xml = <<<EOXML
<?xml version="1.0" encoding="ISO-8859-1" ?>
<root>foo<child />baz</root>
EOXML;

$doc = new DOMDocument();
$doc->loadXML($xml);

$root = $doc->documentElement;
$foo = $root->firstChild;

var_dump($foo->wholeText == "foo");

$bar = $root->insertBefore($doc->createTextNode("bar"), $foo->nextSibling);

var_dump($foo->wholeText == "foobar");
var_dump($foo->wholeText == $bar->wholeText);
$baz = $bar->nextSibling->nextSibling;

var_dump($baz->wholeText === $foo->wholeText);
"#);
    assert_exact(&out, r#"bool(true)
bool(true)
bool(true)
bool(false)"#);
}

/// php-src `ext/dom/tests/bug42082.phpt`: Bug #42082 (NodeList length zero should be empty)
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXPath"#]
fn test_bug42082() {
    if skip_without_xml_native("test_bug42082") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument();
$xpath = new DOMXPath($doc);
$nodes = $xpath->query('*');
echo get_class($nodes), "\n";
var_dump($nodes->length);
$length = $nodes->length;
var_dump(empty($nodes->length), empty($length));

$doc->loadXML("<element></element>");
var_dump($doc->firstChild->nodeValue, empty($doc->firstChild->nodeValue), isset($doc->firstChild->nodeValue));
var_dump(empty($doc->nodeType), empty($doc->firstChild->nodeType));"#);
    assert_exact(&out, r#"DOMNodeList
int(0)
bool(true)
bool(true)
string(0) ""
bool(true)
bool(true)
bool(false)
bool(false)"#);
}

/// php-src `ext/dom/tests/bug43364.phpt`: Bug #43364 (recursive xincludes don't remove internal xml nodes properly)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::xinclude"#]
fn test_bug43364() {
    if skip_without_xml_native("test_bug43364") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
function loopElements($nodes)
{
    $count = 0;
    foreach($nodes as $node) {
        if($node instanceof DOMElement) {
            $count++;
            if($node->childNodes->length > 0) {
                $count += loopElements($node->childNodes);
            }
        }
    }
    return $count;
}

$xml = <<<DOC
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns:xi="http://www.w3.org/2001/XInclude">
    <a>
        <a_child1>ac1</a_child1>
        <a_child2>ac2</a_child2>
    </a>
    <b><xi:include xpointer="xpointer(/root/a)" /></b>
    <c><xi:include xpointer="xpointer(/root/b)" /></c>
</root>
DOC;

$doc = new DomDocument();
$doc->loadXml($xml);
$doc->xinclude();

$count = loopElements(array($doc->documentElement));

var_dump($count == 13 || $count == 11);
"#);
    assert_exact(&out, r#"bool(true)"#);
}

/// php-src `ext/dom/tests/bug44648.phpt`: Bug #44648 (Attribute names not checked for well formedness)
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMAttr"#]
fn test_bug44648() {
    if skip_without_xml_native("test_bug44648") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument();
$doc->loadXML('<root/>');

$root = $doc->documentElement;

try {
  $attr = new DOMAttr('@acb', '123');
  $root->setAttributeNode($attr);
} catch (DOMException $e) {
  echo $e->getMessage()."\n";
}

try {
  $root->setAttribute('@def', '456');
} catch (DOMException $e) {
  echo $e->getMessage()."\n";
}

try {
  $root->setAttributeNS(NULL, '@ghi', '789');
} catch (DOMException $e) {
  echo $e->getMessage()."\n";
}

try {
  $root->setAttributeNS('urn::test', 'a:g@hi', '789');
} catch (DOMException $e) {
  echo $e->getMessage()."\n";
}

echo $doc->saveXML($root);
"#);
    assert_exact(&out, r#"Invalid Character Error
Invalid Character Error
Invalid Character Error
Namespace Error
<root/>"#);
}

/// php-src `ext/dom/tests/bug45251.phpt`: Bug #45251 (double free or corruption with setAttributeNode())
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXPath"#]
fn test_bug45251() {
    if skip_without_xml_native("test_bug45251") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument;
$doc->loadXml(<<<EOF
<?xml version="1.0" encoding="utf-8" ?>
<aaa>
  <bbb foo="bar"/>
</aaa>
EOF
);

$xpath = new DOMXPath($doc);

$bbb = $xpath->query('bbb', $doc->documentElement)->item(0);

$ccc = $doc->createElement('ccc');
foreach ($bbb->attributes as $attr)
{
  $ccc->setAttributeNode($attr);
}

echo $attr->parentNode->localName;

"#);
    assert_exact(&out, r#"ccc"#);
}

/// php-src `ext/dom/tests/bug46185.phpt`: Bug #46185 (importNode changes the namespace of an XML element).
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::firstChild"#]
fn test_bug46185() {
    if skip_without_xml_native("test_bug46185") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$aDOM = new DOMDocument();
$aDOM->loadXML('<?xml version="1.0"?>
<ns1:a xmlns:ns1="urn::ns"/>');
$a= $aDOM->firstChild;

$ok = new DOMDocument();
$ok->loadXML('<?xml version="1.0"?>
<ns1:ok xmlns:ns1="urn::ns" xmlns="urn::REAL"><watch-me xmlns:default="urn::BOGUS"/></ns1:ok>');

$imported= $aDOM->importNode($ok->firstChild, true);
$a->appendChild($imported);

echo $aDOM->saveXML();
"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<ns1:a xmlns:ns1="urn::ns"><ns1:ok xmlns="urn::REAL"><watch-me xmlns:default="urn::BOGUS"/></ns1:ok></ns1:a>"#);
}

/// php-src `ext/dom/tests/bug46335.phpt`: Bug #46335 (DOMText::splitText doesn't handle multibyte characters).
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createTextNode"#]
fn test_bug46335() {
    if skip_without_xml_native("test_bug46335") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$textascii = 'This is an "example" of using DOM splitText';
$text = 'This is an ‘example’ of using DOM splitText';
$start = 30;
$length = 3;

$dom = new DOMDocument('1.0', 'UTF-8');
$node = $dom->createTextNode($textascii);
$dom->appendChild($node);

print "Text: $node->textContent\n";

$matched = $node->splitText($start);
$matched->splitText($length);
print "splitText (ASCII): $matched->textContent\n";

$node = $dom->createTextNode($text);
$dom->appendChild($node);

print "Text: $node->textContent\n";

$matched = $node->splitText($start);
$matched->splitText($length);
print "splitText (UTF-8): $matched->textContent\n";
"#);
    assert_exact(&out, r#"Text: This is an "example" of using DOM splitText
splitText (ASCII): DOM
Text: This is an ‘example’ of using DOM splitText
splitText (UTF-8): DOM"#);
}

/// php-src `ext/dom/tests/bug46849.phpt`: Bug #46849 (Cloning DOMDocument doesn't clone the properties).
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::formatOutput"#]
fn test_bug46849() {
    if skip_without_xml_native("test_bug46849") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = new DOMDocument;
$dom->formatOutput = true;
var_dump($dom->formatOutput);

$dom2 = clone $dom;
var_dump($dom2->formatOutput);
"#);
    assert_exact(&out, r#"bool(true)
bool(true)"#);
}

/// php-src `ext/dom/tests/bug47430.phpt`: Bug #47430 (Errors after writing to nodeValue parameter of an absent previousSibling).
#[test]
fn test_bug47430() {
    if skip_without_xml_native("test_bug47430") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$xml = '<?xml
version="1.0"?><html><p><i>Hello</i></p><p><i>World!</i></p></html>';
$dom = new DOMDocument();
$dom->loadXML($xml);

$elements = $dom->getElementsByTagName('i');
foreach ($elements as $i) {
    try {
        $i->previousSibling->nodeValue = '';
    } catch (Error $e) {
        echo $e->getMessage(), "\n";
    }
}

$arr = array();
$arr[0] = 'Value';

print_r($arr);

"#);
    assert_exact(&out, r#"Attempt to assign property "nodeValue" on null
Attempt to assign property "nodeValue" on null
Array
(
    [0] => Value
)"#);
}

/// php-src `ext/dom/tests/bug47530.phpt`: Bug #47530 (Importing objects into document fragments creates bogus "default" namespace)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createDocumentFragment"#]
fn test_bug47530() {
    if skip_without_xml_native("test_bug47530") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

function test_document_fragment_with_import() {
    $doc = new DOMDocument;
    $doc->loadXML('<html xmlns="https://php.net/something" xmlns:ns="https://php.net/whatever"><element ns:foo="https://php.net/bar"/></html>');
    $root = $doc->documentElement;
    $frag = $doc->createDocumentFragment();
    $frag->appendChild($doc->importNode($root->firstChild));
    $root->appendChild($frag);
    echo $doc->saveXML();
}

function test_document_fragment_without_import() {
    $doc = new DOMDocument;
    $doc->loadXML('<html xmlns=""><element xmlns:foo="https://php.net/bar"/></html>');
    $frag = $doc->createDocumentFragment();
    $frag->appendChild($doc->createElementNS('https://php.net/bar', 'bar'));
    $frag->appendChild($doc->createElementNS('', 'bar'));
    $element = $doc->documentElement->firstChild;
    $element->appendChild($frag);
    unset($frag); // Free fragment, should not break getting the namespaceURI below
    echo $doc->saveXML();
    unset($doc);
    var_dump($element->firstChild->tagName);
    var_dump($element->firstChild->namespaceURI);
}

function test_document_import() {
    $xml = <<<XML
<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
<div xmlns="http://www.w3.org/1999/xhtml">
    <p>Test-Text</p>
</div>
</feed>
XML;

    $dom = new DOMDocument();
    $dom->loadXML($xml);

    $dom2 = new DOMDocument();
    $importedNode = $dom2->importNode($dom->documentElement, true);
    $dom2->appendChild($importedNode);

    echo $dom2->saveXML();
}

function test_partial_document_import() {
    $xml = <<<XML
<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/1999/xhtml" xmlns:test="https://php.net/test" xmlns:example="https://php.net/example">
<div>
    <p>Test-Text</p>
    <example:p>More test text</example:p>
    <test:p>Even more test text</test:p>
</div>
</feed>
XML;

    $dom = new DOMDocument();
    $dom->loadXML($xml);

    $dom2 = new DOMDocument();
    $dom2->loadXML('<?xml version="1.0"?><container xmlns:test="https://php.net/test" xmlns="https://php.net/example"/>');
    $importedNode = $dom2->importNode($dom->documentElement, true);
    $dom2->documentElement->appendChild($importedNode);

    // Freeing the original document shouldn't break the other document
    unset($importedNode);
    unset($dom);

    echo $dom2->saveXML();
}

function test_document_import_with_attributes() {
    $dom = new DOMDocument();
    $dom->loadXML('<?xml version="1.0"?><div xmlns="https://php.net/default" xmlns:example="https://php.net/example"><p example:test="test"/><i/></div>');
    $dom2 = new DOMDocument();
    $dom2->loadXML('<?xml version="1.0"?><div xmlns:example="https://php.net/somethingelse"/>');
    $dom2->documentElement->appendChild($dom2->importNode($dom->documentElement->firstChild));
    echo $dom2->saveXML(), "\n";

    $dom2->documentElement->firstChild->appendChild($dom2->importNode($dom->documentElement->firstChild->nextSibling));
    echo $dom2->saveXML(), "\n";
}

function test_appendChild_with_shadowing() {
    $dom = new DOMDocument();
    $dom->loadXML('<?xml version="1.0"?><container xmlns:default="http://php.net/default"><a xmlns:foo="http://php.net/bar"/><b xmlns:foo="http://php.net/foo"><default:test foo:bar=""/><foo:test2/></b></container>');

    $a = $dom->documentElement->firstElementChild;
    $b = $a->nextSibling;
    $b->remove();
    $a->appendChild($b);

    echo $dom->saveXML(), "\n";
}

echo "-- Test document fragment with import --\n";
test_document_fragment_with_import();
echo "-- Test document fragment without import --\n";
test_document_fragment_without_import();
echo "-- Test document import --\n";
test_document_import();
echo "-- Test partial document import --\n";
test_partial_document_import();
echo "-- Test document import with attributes --\n";
test_document_import_with_attributes();
echo "-- Test appendChild with shadowing --\n";
test_appendChild_with_shadowing();

"#);
    assert_exact(&out, r#"-- Test document fragment with import --
<?xml version="1.0"?>
<html xmlns="https://php.net/something" xmlns:ns="https://php.net/whatever"><default:element xmlns:default="https://php.net/something" ns:foo="https://php.net/bar"/></html>
-- Test document fragment without import --
<?xml version="1.0"?>
<html xmlns=""><element xmlns:foo="https://php.net/bar"><foo:bar/><bar/></element></html>
string(7) "foo:bar"
string(19) "https://php.net/bar"
-- Test document import --
<?xml version="1.0"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:default="http://www.w3.org/1999/xhtml">
<default:div xmlns="http://www.w3.org/1999/xhtml">
    <default:p>Test-Text</default:p>
</default:div>
</feed>
-- Test partial document import --
<?xml version="1.0"?>
<container xmlns:test="https://php.net/test" xmlns="https://php.net/example"><feed xmlns="http://www.w3.org/1999/xhtml" xmlns:example="https://php.net/example">
<div>
    <p>Test-Text</p>
    <example:p>More test text</example:p>
    <test:p>Even more test text</test:p>
</div>
</feed></container>
-- Test document import with attributes --
<?xml version="1.0"?>
<div xmlns:example="https://php.net/somethingelse"><p xmlns="https://php.net/default" xmlns:example="https://php.net/example" example:test="test"/></div>

<?xml version="1.0"?>
<div xmlns:example="https://php.net/somethingelse"><p xmlns="https://php.net/default" xmlns:example="https://php.net/example" example:test="test"><i/></p></div>

-- Test appendChild with shadowing --
<?xml version="1.0"?>
<container xmlns:default="http://php.net/default"><a xmlns:foo="http://php.net/bar"><b xmlns:foo="http://php.net/foo"><default:test foo:bar=""/><foo:test2/></b></a></container>"#);
}

/// php-src `ext/dom/tests/bug47531_a.phpt`: Bug #47531 (No way of removing redundant xmlns: declarations)
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_bug47531_a() {
    if skip_without_xml_native("test_bug47531_a") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;
$doc->loadXML(<<<XML
<container xmlns:foo="some:ns">
    <foo:first/>
    <second>
        <foo:child1/>
        <foo:child2/>
        <!-- comment -->
        <child3>
            <foo:child4/>
            <foo:child5>
                <p xmlns:foo="other:ns">
                    <span foo:foo="bar"/>
                    <span foo:foo="bar"/>
                </p>
                <foo:child6 foo:foo="bar">
                    <span foo:foo="bar"/>
                    <span foo:foo="bar"/>
                </foo:child6>
            </foo:child5>
        </child3>
        <child7 foo:foo="bar">
            <foo:child8/>
        </child7>
    </second>
</container>
XML);
$root = $doc->documentElement;
var_dump($root->removeAttribute("xmlns:foo"));
echo $doc->saveXML();

"#);
    assert_exact(&out, r#"bool(true)
<?xml version="1.0"?>
<container>
    <foo:first xmlns:foo="some:ns"/>
    <second>
        <foo:child1 xmlns:foo="some:ns"/>
        <foo:child2 xmlns:foo="some:ns"/>
        <!-- comment -->
        <child3>
            <foo:child4 xmlns:foo="some:ns"/>
            <foo:child5 xmlns:foo="some:ns">
                <p xmlns:foo="other:ns">
                    <span foo:foo="bar"/>
                    <span foo:foo="bar"/>
                </p>
                <foo:child6 foo:foo="bar">
                    <span foo:foo="bar"/>
                    <span foo:foo="bar"/>
                </foo:child6>
            </foo:child5>
        </child3>
        <child7 xmlns:foo="some:ns" foo:foo="bar">
            <foo:child8/>
        </child7>
    </second>
</container>"#);
}

/// php-src `ext/dom/tests/bug47531_b.phpt`: Bug #47531 (No way of removing redundant xmlns: declarations)
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_bug47531_b() {
    if skip_without_xml_native("test_bug47531_b") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;
$doc->loadXML(<<<XML
<container xmlns:foo="some:ns">
    <foo:first/>
    <foo:second xmlns:foo="some:ns2">
        <foo:child1/>
        <foo:child2/>
        <!-- comment -->
        <child3>
            <foo:child4/>
            <foo:child5 xmlns:foo="some:ns3">
                <p xmlns:foo="other:ns">
                    <span foo:foo="bar"/>
                    <span foo:foo="bar"/>
                </p>
                <foo:child6 foo:foo="bar">
                    <span foo:foo="bar"/>
                    <span foo:foo="bar"/>
                </foo:child6>
            </foo:child5>
        </child3>
        <child7 xmlns:foo="some:ns" foo:foo="bar">
            <foo:child8/>
        </child7>
    </foo:second>
</container>
XML);
$root = $doc->documentElement;
$elem = $root->firstElementChild->nextElementSibling;
var_dump($elem->removeAttribute("xmlns:foo"));
echo $doc->saveXML();

"#);
    assert_exact(&out, r#"bool(true)
<?xml version="1.0"?>
<container xmlns:foo="some:ns">
    <foo:first/>
    <foo:second xmlns:foo="some:ns2">
        <foo:child1/>
        <foo:child2/>
        <!-- comment -->
        <child3>
            <foo:child4/>
            <foo:child5 xmlns:foo="some:ns3">
                <p xmlns:foo="other:ns">
                    <span foo:foo="bar"/>
                    <span foo:foo="bar"/>
                </p>
                <foo:child6 foo:foo="bar">
                    <span foo:foo="bar"/>
                    <span foo:foo="bar"/>
                </foo:child6>
            </foo:child5>
        </child3>
        <child7 xmlns:foo="some:ns" foo:foo="bar">
            <foo:child8/>
        </child7>
    </foo:second>
</container>"#);
}

/// php-src `ext/dom/tests/bug47848.phpt`: Bug #47848 (importNode doesn't preserve attribute namespaces)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createElementNS"#]
fn test_bug47848() {
    if skip_without_xml_native("test_bug47848") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$aDOM = new DOMDocument();
$aDOM->appendChild($aDOM->createElementNS('http://friend2friend.net/','f2f:a'));

$fromdom = new DOMDocument();
$fromdom->loadXML('<data xmlns:ai="http://altruists.org" ai:attr="namespaced" />');

$attr= $fromdom->firstChild->attributes->item(0);

$att = $aDOM->importNode($attr);

$aDOM->documentElement->appendChild($aDOM->importNode($attr, true));

echo $aDOM->saveXML();

"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<f2f:a xmlns:f2f="http://friend2friend.net/" xmlns:ai="http://altruists.org" ai:attr="namespaced"/>"#);
}

/// php-src `ext/dom/tests/bug47849.phpt`: Bug #47849 (Non-deep import loses the namespace).
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createElementNS"#]
fn test_bug47849() {
    if skip_without_xml_native("test_bug47849") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$aDOM= new DOMDocument();
$aDOM->appendChild($aDOM->createElementNS('urn::root','r:root'));

$fromdom= new DOMDocument();
$fromdom->loadXML('<data xmlns="urn::data">aaa</data>');

$data= $fromdom->documentElement;
$aDOM->documentElement->appendChild($aDOM->importNode($data));

echo $aDOM->saveXML();

"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<r:root xmlns:r="urn::root"><data xmlns="urn::data"/></r:root>"#);
}

/// php-src `ext/dom/tests/bug49463.phpt`: Bug #49463 (setAttributeNS fails setting default namespace).
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createElementNS"#]
fn test_bug49463() {
    if skip_without_xml_native("test_bug49463") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument('1.0', 'utf-8');
$root = $doc->createElementNS('http://purl.org/rss/1.0/','rdf:RDF');
$doc->appendChild($root);
$root->setAttributeNS("http://www.w3.org/2000/xmlns/","xmlns","http://purl.org/rss/1.0/" );

echo $doc->saveXML()."\n";
"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="utf-8"?>
<rdf:RDF xmlns:rdf="http://purl.org/rss/1.0/" xmlns="http://purl.org/rss/1.0/"/>"#);
}

/// php-src `ext/dom/tests/bug49490.phpt`: Bug #49490 (XPath namespace prefix conflict).
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXPath"#]
fn test_bug49490() {
    if skip_without_xml_native("test_bug49490") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument();
$doc->loadXML('<prefix:root xmlns:prefix="urn:a" />');

$xp = new DOMXPath($doc);
$xp->registerNamespace('prefix', 'urn:b');

echo($xp->query('//prefix:root', null, false)->length . "\n");

"#);
    assert_exact(&out, r#"0"#);
}

/// php-src `ext/dom/tests/bug50661.phpt`: Bug #50661 (DOMDocument::loadXML does not allow UTF-16).
#[test]
fn test_bug50661() {
    if skip_without_xml_native("test_bug50661") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$data = "\xFE\xFF\x00\x3C\x00\x66\x00\x6F\x00\x6F\x00\x2F\x00\x3E";

$dom = new DOMDocument();
$dom->loadXML($data);
echo $dom->saveXML();

"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<foo/>"#);
}

/// php-src `ext/dom/tests/bug52656.phpt`: Bug #52656 (DOMCdataSection does not work with splitText).
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMCdataSection"#]
fn test_bug52656() {
    if skip_without_xml_native("test_bug52656") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$CData = new DOMCdataSection('splithere!');
$CDataSplit = $CData->splitText(5);
echo get_class($CDataSplit), "\n";
var_dump($CDataSplit->data);
"#);
    assert_exact(&out, r#"DOMText
string(5) "here!""#);
}

/// php-src `ext/dom/tests/bug54382.phpt`: Bug #54382 DOMNode::getAttributeNodeNS doesn't get xmlns* attributes
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_bug54382() {
    if skip_without_xml_native("test_bug54382") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$xmlString = '<?xml version="1.0" encoding="utf-8" ?>
<root xmlns="http://ns" xmlns:ns2="http://ns2">
    <ns2:child />
</root>';

$xml=new DOMDocument();
$xml->loadXML($xmlString);
$de = $xml->documentElement;

$ns2 = $de->getAttributeNodeNS("http://www.w3.org/2000/xmlns/", "ns2");
if ($ns2 == NULL) {
  echo 'namespace node does not exist.' . "\n";
} else {
  echo 'namespace node prefix=' . $ns2->prefix . "\n";
  echo 'namespace node namespaceURI=' . $ns2->namespaceURI . "\n";
}
"#);
    assert_exact(&out, r#"namespace node prefix=ns2
namespace node namespaceURI=http://ns2"#);
}

/// php-src `ext/dom/tests/bug54601.phpt`: Segfault when removing the Doctype node
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::doctype"#]
fn test_bug54601() {
    if skip_without_xml_native("test_bug54601") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$xml = <<< XML
<?xml version='1.0' encoding='utf-8' ?>
<!DOCTYPE set PUBLIC "-//OASIS//DTD DocBook XML V5.0//EN" "http://www.docbook.org/xml/5.0/dtd/docbook.dtd" [
<!ENTITY foo '<foo>footext</foo>'>
<!ENTITY bar '<bar>bartext</bar>'>
]>
<set>&foo;&bar;</set>
XML;

$doc = new DOMDocument();
$doc->loadXML($xml, LIBXML_NOENT);
$n = $doc->doctype;
$doc->removeChild($n);
echo get_class($n), "\n";
print $doc->saveXML();
"#);
    assert_exact(&out, r#"DOMDocumentType
<?xml version="1.0" encoding="utf-8"?>
<set><foo>footext</foo><bar>bartext</bar></set>"#);
}

/// php-src `ext/dom/tests/bug55700.phpt`: Bug #55700 (XPath namespace prefix conflict, global registerNodeNS flag)
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXPath"#]
fn test_bug55700() {
    if skip_without_xml_native("test_bug55700") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument();
$doc->loadXML('<prefix:root xmlns:prefix="urn:a" />');

$xp = new DOMXPath($doc, true);
$xp->registerNamespace('prefix', 'urn:b');

echo($xp->query('//prefix:root')->length . "\n");

$xp = new DOMXPath($doc, false);
$xp->registerNamespace('prefix', 'urn:b');

echo($xp->query('//prefix:root')->length . "\n");

var_dump($xp->registerNodeNamespaces);
$xp->registerNodeNamespaces = true;

var_dump($xp->registerNodeNamespaces);

echo($xp->query('//prefix:root')->length . "\n");

var_dump($xp);
"#);
    assert_exact(&out, r#"1
0
bool(false)
bool(true)
1
object(DOMXPath)#4 (2) {
  ["document"]=>
  string(22) "(object value omitted)"
  ["registerNodeNamespaces"]=>
  bool(true)
}"#);
}

/// php-src `ext/dom/tests/bug61858.phpt`: Bug #61858 DOMAttr debug info generates E_WARNING
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMNode::getAttributeNode"#]
fn test_bug61858() {
    if skip_without_xml_native("test_bug61858") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument();
$doc->loadXML('<example a="b">Test</example>');

$example = $doc->getElementsByTagName('example')->item(0);
$attr    = $example->getAttributeNode('a');

var_dump($attr);
print_r($attr);
"#);
    assert_format(&out, r#"object(DOMAttr)#%d (%d) {
%A
}
DOMAttr Object
(
%A
)"#);
}

/// php-src `ext/dom/tests/bug65196.phpt`: bug #65196 (Passing DOMDocumentFragment to DOMDocument::saveHTML() Produces invalid Markup)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createDocumentFragment"#]
fn test_bug65196() {
    if skip_without_xml_native("test_bug65196") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = new DOMDocument();

$frag1 = $dom->createDocumentFragment();
var_dump($dom->saveHTML($frag1));

$frag2 = $dom->createDocumentFragment();
$div = $dom->createElement('div');
$div->appendChild($dom->createElement('span'));
$frag2->appendChild($div);
$frag2->appendChild($dom->createElement('div'));
$frag2->appendChild($dom->createElement('div'));
var_dump($dom->saveHTML($frag2));
"#);
    assert_exact(&out, r#"string(0) ""
string(46) "<div><span></span></div><div></div><div></div>""#);
}

/// php-src `ext/dom/tests/bug66502.phpt`: Bug #66502 (DOM document dangling reference)
#[test]
#[ignore = r#"DOM port pending: Constructor 'DOMElement::__construct' expects 0 arguments, got 1"#]
fn test_bug66502() {
    if skip_without_xml_native("test_bug66502") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = new DOMDocument('1.0', 'UTF-8');
$element = $dom->appendChild(new DOMElement('root'));
$comment = new DOMComment("Comment 0");
$comment = $element->appendChild($comment);

$comment->__construct("Comment 1");
$comment->__construct("Comment 2");
$comment->__construct("Comment 3");
echo 'DONE', PHP_EOL;
"#);
    assert_exact(&out, r#"DONE"#);
}

/// php-src `ext/dom/tests/bug66783.phpt`: Bug #66783 (UAF when appending DOMDocument to element)
#[test]
fn test_bug66783() {
    if skip_without_xml_native("test_bug66783") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DomDocument;
$doc->loadXML('<root></root>');
$e = $doc->createElement('e');
try {
    $e->appendChild($doc);
} catch (DOMException $ex) {
    echo $ex->getMessage(), PHP_EOL;
}
"#);
    assert_exact(&out, r#"Hierarchy Request Error"#);
}

/// php-src `ext/dom/tests/bug67081.phpt`: Bug #67081 DOMDocumentType->internalSubset returns entire DOCTYPE tag, not only the subset
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::substituteEntities"#]
fn test_bug67081() {
    if skip_without_xml_native("test_bug67081") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
    $domDocument = new DOMDocument();
    $domDocument->substituteEntities = true;
    $domDocument->load(__DIR__ . DIRECTORY_SEPARATOR . "bug67081_0.xml");
    var_dump($domDocument->doctype->internalSubset);

    $domDocument = new DOMDocument();
    $domDocument->substituteEntities = true;
    $domDocument->load(__DIR__ . DIRECTORY_SEPARATOR . "bug67081_1.xml");
    var_dump($domDocument->doctype->internalSubset);

    $domDocument = new DOMDocument();
    $domDocument->substituteEntities = true;
    $domDocument->load(__DIR__ . DIRECTORY_SEPARATOR . "bug67081_2.xml");
    var_dump($domDocument->doctype->internalSubset);

    $domDocument = new DOMDocument();
    $domDocument->substituteEntities = true;
    $domDocument->load(__DIR__ . DIRECTORY_SEPARATOR . "dom.xml");
    var_dump($domDocument->doctype->internalSubset);
"#);
    assert_exact(&out, r#"string(19) "<!ELEMENT a EMPTY>
"
string(38) "<!ELEMENT a EMPTY>
<!ELEMENT b EMPTY>
"
NULL
string(277) "<!ENTITY % incent SYSTEM "dom.ent">
<!ENTITY amp "&#38;#38;">
<!ENTITY gt "&#62;">
<!ENTITY % coreattrs "title CDATA #IMPLIED">
<!ENTITY % attrs "%coreattrs;">
<!ATTLIST foo bar CDATA #IMPLIED>
<!ELEMENT foo (#PCDATA)>
<!ELEMENT root (foo)+>
<!ATTLIST th title CDATA #IMPLIED>
""#);
}

/// php-src `ext/dom/tests/bug67440.phpt`: Bug #67440 (append_node of a DOMDocumentFragment does not reconcile namespaces)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createDocumentFragment"#]
fn test_bug67440() {
    if skip_without_xml_native("test_bug67440") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

function createDocument() {
    $document = new DOMDocument();
    $document->loadXML('<?xml version="1.0"?><rootElement xmlns:myns="http://example/ns" xmlns:myns2="http://example/ns2"></rootElement>');
    $fragment = $document->createDocumentFragment();
    $fragment->appendChild($document->createTextNode("\n"));
    $fragment->appendChild($document->createElementNS('http://example/ns', 'myns:childNode', '1'));
    $fragment->appendChild($document->createTextNode("\n"));
    $fragment->appendChild($document->createElementNS('http://example/ns', 'myns:childNode', '2'));
    $fragment->appendChild($document->createTextNode("\n"));
    return array($document, $fragment);
}

function case1($method) {
    list($document, $fragment) = createDocument();
    $document->documentElement->{$method}($fragment);
    echo $document->saveXML();
}

function case2($method) {
    list($document, $fragment) = createDocument();
    $childNodes = iterator_to_array($fragment->childNodes);
    foreach ($childNodes as $childNode) {
        $document->documentElement->{$method}($childNode);
    }
    echo $document->saveXML();
}

function case3($method) {
    list($document, $fragment) = createDocument();
    $fragment->removeChild($fragment->firstChild);
    $document->documentElement->{$method}($fragment);
    echo $document->saveXML();
}

function case4($method) {
    list($document, $fragment) = createDocument();
    $fragment->childNodes[1]->appendChild($document->createElementNS('http://example/ns2', 'myns2:childNode', '3'));
    $document->documentElement->{$method}($fragment);
    echo $document->saveXML();
}

echo "== appendChild ==\n";
echo "-- fragment to document element --\n"; case1('appendChild'); echo "\n";
echo "-- children manually document element --\n"; case2('appendChild'); echo "\n";
echo "-- fragment to document where first element is not a text node --\n"; case3('appendChild'); echo "\n";
echo "-- fragment with namespace declarations in children --\n"; case4('appendChild'); echo "\n";

echo "== insertBefore ==\n";
echo "-- fragment to document element --\n"; case1('insertBefore'); echo "\n";
echo "-- children manually document element --\n"; case2('insertBefore'); echo "\n";
echo "-- fragment to document where first element is not a text node --\n"; case3('insertBefore'); echo "\n";
echo "-- fragment with namespace declarations in children --\n"; case4('insertBefore'); echo "\n";

echo "== insertAfter ==\n";
echo "-- fragment to document element --\n"; case1('insertBefore'); echo "\n";
echo "-- children manually document element --\n"; case2('insertBefore'); echo "\n";
echo "-- fragment to document where first element is not a text node --\n"; case3('insertBefore'); echo "\n";
echo "-- fragment with namespace declarations in children --\n"; case4('insertBefore'); echo "\n";

"#);
    assert_exact(&out, r#"== appendChild ==
-- fragment to document element --
<?xml version="1.0"?>
<rootElement xmlns:myns="http://example/ns" xmlns:myns2="http://example/ns2">
<myns:childNode>1</myns:childNode>
<myns:childNode>2</myns:childNode>
</rootElement>

-- children manually document element --
<?xml version="1.0"?>
<rootElement xmlns:myns="http://example/ns" xmlns:myns2="http://example/ns2">
<myns:childNode>1</myns:childNode>
<myns:childNode>2</myns:childNode>
</rootElement>

-- fragment to document where first element is not a text node --
<?xml version="1.0"?>
<rootElement xmlns:myns="http://example/ns" xmlns:myns2="http://example/ns2"><myns:childNode>1</myns:childNode>
<myns:childNode>2</myns:childNode>
</rootElement>

-- fragment with namespace declarations in children --
<?xml version="1.0"?>
<rootElement xmlns:myns="http://example/ns" xmlns:myns2="http://example/ns2">
<myns:childNode>1<myns2:childNode>3</myns2:childNode></myns:childNode>
<myns:childNode>2</myns:childNode>
</rootElement>

== insertBefore ==
-- fragment to document element --
<?xml version="1.0"?>
<rootElement xmlns:myns="http://example/ns" xmlns:myns2="http://example/ns2">
<myns:childNode>1</myns:childNode>
<myns:childNode>2</myns:childNode>
</rootElement>

-- children manually document element --
<?xml version="1.0"?>
<rootElement xmlns:myns="http://example/ns" xmlns:myns2="http://example/ns2">
<myns:childNode>1</myns:childNode>
<myns:childNode>2</myns:childNode>
</rootElement>

-- fragment to document where first element is not a text node --
<?xml version="1.0"?>
<rootElement xmlns:myns="http://example/ns" xmlns:myns2="http://example/ns2"><myns:childNode>1</myns:childNode>
<myns:childNode>2</myns:childNode>
</rootElement>

-- fragment with namespace declarations in children --
<?xml version="1.0"?>
<rootElement xmlns:myns="http://example/ns" xmlns:myns2="http://example/ns2">
<myns:childNode>1<myns2:childNode>3</myns2:childNode></myns:childNode>
<myns:childNode>2</myns:childNode>
</rootElement>

== insertAfter ==
-- fragment to document element --
<?xml version="1.0"?>
<rootElement xmlns:myns="http://example/ns" xmlns:myns2="http://example/ns2">
<myns:childNode>1</myns:childNode>
<myns:childNode>2</myns:childNode>
</rootElement>

-- children manually document element --
<?xml version="1.0"?>
<rootElement xmlns:myns="http://example/ns" xmlns:myns2="http://example/ns2">
<myns:childNode>1</myns:childNode>
<myns:childNode>2</myns:childNode>
</rootElement>

-- fragment to document where first element is not a text node --
<?xml version="1.0"?>
<rootElement xmlns:myns="http://example/ns" xmlns:myns2="http://example/ns2"><myns:childNode>1</myns:childNode>
<myns:childNode>2</myns:childNode>
</rootElement>

-- fragment with namespace declarations in children --
<?xml version="1.0"?>
<rootElement xmlns:myns="http://example/ns" xmlns:myns2="http://example/ns2">
<myns:childNode>1<myns2:childNode>3</myns2:childNode></myns:childNode>
<myns:childNode>2</myns:childNode>
</rootElement>"#);
}

/// php-src `ext/dom/tests/bug67474.phpt`: Bug #67474 getElementsByTagNameNS and default namespace
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::getElementsByTagNameNS"#]
fn test_bug67474() {
    if skip_without_xml_native("test_bug67474") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
declare(strict_types=1);

$doc = new DOMDocument();
$doc->loadXML('<root xmlns:x="x"><a/><x:a/></root>');
$list = $doc->getElementsByTagNameNS('', 'a');
var_dump($list->length);
$list = $doc->getElementsByTagNameNS(null, 'a');
var_dump($list->length);

$elem = $doc->documentElement;
$list = $elem->getElementsByTagNameNS('', 'a');
var_dump($list->length);
$list = $elem->getElementsByTagNameNS(null, 'a');
var_dump($list->length);

"#);
    assert_exact(&out, r#"int(1)
int(1)
int(1)
int(1)"#);
}

/// php-src `ext/dom/tests/bug67949.phpt`: Bug #67949: DOMNodeList elements should be accessible through array notation
#[test]
#[ignore = r#"DOM port pending: Cannot index non-array"#]
fn test_bug67949() {
    if skip_without_xml_native("test_bug67949") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

// Note: non-numeric string accesses fail on NodeLists (as per spec).

$html = <<<HTML
<div>data</div>
<a href="test">hello world</a>
HTML;
$doc = new DOMDocument;
$doc->loadHTML($html);

$nodes = $doc->getElementsByTagName('div');

echo "--- testing has_dimension ---\n";
var_dump(isset($nodes[0]));
var_dump(isset($nodes[1]));
var_dump(isset($nodes[-1]));

echo "--- testing property access ---\n";
var_dump($nodes[0]->textContent);
var_dump($nodes[1]->textContent);

echo "--- testing offset not a long: array ---\n";
$offset = ['test'];
var_dump($offset);
var_dump(isset($nodes[$offset]), $nodes[$offset]->textContent);

echo "--- testing offset not a long: Reference to string ---\n";
$something = 'href';
$offset = &$something;

var_dump($offset);
var_dump(isset($nodes[$offset]), $nodes[$offset]->textContent);

echo "--- testing offset not a long: string ---\n";
$offset = 'test';
var_dump($offset);
var_dump(isset($nodes[$offset]), $nodes[$offset]->textContent);

echo "--- testing read_dimension with null offset ---\n";
try {
    var_dump($nodes[][] = 1);
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}

echo "--- testing attribute access ---\n";
$anchor = $doc->getElementsByTagName('a')[0];
var_dump($anchor->attributes[0]->name);

echo "==DONE==\n";
"#);
    assert_format(&out, r#"--- testing has_dimension ---
bool(true)
bool(false)
bool(false)
--- testing property access ---
string(4) "data"

Warning: Attempt to read property "textContent" on null in %s on line %d
NULL
--- testing offset not a long: array ---
array(1) {
  [0]=>
  string(4) "test"
}

Warning: Attempt to read property "textContent" on null in %s on line %d
bool(false)
NULL
--- testing offset not a long: Reference to string ---
string(4) "href"

Warning: Attempt to read property "textContent" on null in %s on line %d
bool(false)
NULL
--- testing offset not a long: string ---
string(4) "test"

Warning: Attempt to read property "textContent" on null in %s on line %d
bool(false)
NULL
--- testing read_dimension with null offset ---
Cannot access DOMNodeList without offset
--- testing attribute access ---
string(4) "href"
==DONE=="#);
}

/// php-src `ext/dom/tests/bug69373.phpt`: Bug #69373 References to deleted XPath query results
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXpath"#]
fn test_bug69373() {
    if skip_without_xml_native("test_bug69373") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument();
for( $i=0; $i<20; $i++ ) {
    $doc->loadXML("<parent><child /><child /></parent>");
    $xpath = new DOMXpath($doc);
    $all = $xpath->query('//*');
    $doc->firstChild->nodeValue = '';
}
echo 'DONE', PHP_EOL;
"#);
    assert_exact(&out, r#"DONE"#);
}

/// php-src `ext/dom/tests/bug69679.phpt`: Bug #69679 (DOMDocument::loadHTML refuses to accept NULL bytes)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::saveHTML"#]
fn test_bug69679() {
    if skip_without_xml_native("test_bug69679") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument();
$html = "<!DOCTYPE html><html><head><meta charset='UTF-8'></head><body>U+0000 <span>\x0</span></body></html>";
$doc->loadHTML($html);
print($doc->saveHTML());
"#);
    assert_format(&out, r#"<!DOCTYPE html>
<html><head><meta charset="UTF-8"></head><body>U+0000 <span>%r(&#65533;|)%r</span></body></html>"#);
}

/// php-src `ext/dom/tests/bug69846.phpt`: Bug #69846 Segmentation fault (access violation) when iterating over DOMNodeList
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::firstChild"#]
fn test_bug69846() {
    if skip_without_xml_native("test_bug69846") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$tmpDomDocument = new DOMDocument();

$xml = '<?xml version="1.0" encoding="UTF-8"?><dummy xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"><xfa:data>
  <form1>
    <TextField1>Value A</TextField1>
    <TextField1>Value B</TextField1>
    <TextField1>Value C</TextField1>
  </form1>
</xfa:data></dummy>';

$tmpDomDocument->loadXML($xml);

$dataNodes = $tmpDomDocument->firstChild->childNodes->item(0)->childNodes;

var_dump($dataNodes->length);
$datasetDom = new DOMDocument();

foreach ($dataNodes AS $node) {
    $node = $datasetDom->importNode($node, true);
    var_dump($node);
}

"#);
    assert_format(&out, r##"int(3)
object(DOMText)#%d (23) {
  ["wholeText"]=>
  string(3) "
  "
  ["data"]=>
  string(3) "
  "
  ["length"]=>
  int(3)
  ["previousElementSibling"]=>
  NULL
  ["nextElementSibling"]=>
  NULL
  ["nodeName"]=>
  string(5) "#text"
  ["nodeValue"]=>
  string(3) "
  "
  ["nodeType"]=>
  int(3)
  ["parentNode"]=>
  NULL
  ["parentElement"]=>
  NULL
  ["childNodes"]=>
  string(22) "(object value omitted)"
  ["firstChild"]=>
  NULL
  ["lastChild"]=>
  NULL
  ["previousSibling"]=>
  NULL
  ["nextSibling"]=>
  NULL
  ["attributes"]=>
  NULL
  ["isConnected"]=>
  bool(false)
  ["ownerDocument"]=>
  string(22) "(object value omitted)"
  ["namespaceURI"]=>
  NULL
  ["prefix"]=>
  string(0) ""
  ["localName"]=>
  NULL
  ["baseURI"]=>
  NULL
  ["textContent"]=>
  string(3) "
  "
}
object(DOMElement)#%d (27) {
  ["tagName"]=>
  string(5) "form1"
  ["className"]=>
  string(0) ""
  ["id"]=>
  string(0) ""
  ["schemaTypeInfo"]=>
  NULL
  ["firstElementChild"]=>
  string(22) "(object value omitted)"
  ["lastElementChild"]=>
  string(22) "(object value omitted)"
  ["childElementCount"]=>
  int(3)
  ["previousElementSibling"]=>
  NULL
  ["nextElementSibling"]=>
  NULL
  ["nodeName"]=>
  string(5) "form1"
  ["nodeValue"]=>
  string(39) "
    Value A
    Value B
    Value C
  "
  ["nodeType"]=>
  int(1)
  ["parentNode"]=>
  NULL
  ["parentElement"]=>
  NULL
  ["childNodes"]=>
  string(22) "(object value omitted)"
  ["firstChild"]=>
  string(22) "(object value omitted)"
  ["lastChild"]=>
  string(22) "(object value omitted)"
  ["previousSibling"]=>
  NULL
  ["nextSibling"]=>
  NULL
  ["attributes"]=>
  string(22) "(object value omitted)"
  ["isConnected"]=>
  bool(false)
  ["ownerDocument"]=>
  string(22) "(object value omitted)"
  ["namespaceURI"]=>
  NULL
  ["prefix"]=>
  string(0) ""
  ["localName"]=>
  string(5) "form1"
  ["baseURI"]=>
  NULL
  ["textContent"]=>
  string(39) "
    Value A
    Value B
    Value C
  "
}
object(DOMText)#%d (23) {
  ["wholeText"]=>
  string(1) "
"
  ["data"]=>
  string(1) "
"
  ["length"]=>
  int(1)
  ["previousElementSibling"]=>
  NULL
  ["nextElementSibling"]=>
  NULL
  ["nodeName"]=>
  string(5) "#text"
  ["nodeValue"]=>
  string(1) "
"
  ["nodeType"]=>
  int(3)
  ["parentNode"]=>
  NULL
  ["parentElement"]=>
  NULL
  ["childNodes"]=>
  string(22) "(object value omitted)"
  ["firstChild"]=>
  NULL
  ["lastChild"]=>
  NULL
  ["previousSibling"]=>
  NULL
  ["nextSibling"]=>
  NULL
  ["attributes"]=>
  NULL
  ["isConnected"]=>
  bool(false)
  ["ownerDocument"]=>
  string(22) "(object value omitted)"
  ["namespaceURI"]=>
  NULL
  ["prefix"]=>
  string(0) ""
  ["localName"]=>
  NULL
  ["baseURI"]=>
  NULL
  ["textContent"]=>
  string(1) "
"
}"##);
}

/// php-src `ext/dom/tests/bug70001.phpt`: Bug #70001 (Assigning to DOMNode::textContent does additional entity encoding)
#[test]
#[ignore = r#"DOM port pending: Constructor 'DOMText::__construct' expects 0 arguments, got 1"#]
fn test_bug70001() {
    if skip_without_xml_native("test_bug70001") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$element = new DOMText('<p>foo & bar</p>');
var_dump($element->textContent);
$element = (new DOMDocument())->createTextNode('<p>foo & bar</p>');
var_dump($element->textContent);
$element->textContent = ('<p>foo & bar</p>');
var_dump($element->textContent);
"#);
    assert_exact(&out, r#"string(16) "<p>foo & bar</p>"
string(16) "<p>foo & bar</p>"
string(16) "<p>foo & bar</p>""#);
}

/// php-src `ext/dom/tests/bug70359.phpt`: Bug #70359 (print_r() on DOMAttr causes Segfault in php_libxml_node_free_list())
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_bug70359() {
    if skip_without_xml_native("test_bug70359") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

echo "-- Test without parent --\n";

$dom = new DOMDocument();
$dom->loadXML(<<<XML
<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xsi="fooooooooooooooooooooo"/>
XML);
$spaceNode = $dom->documentElement->getAttributeNode('xmlns');
print_r($spaceNode);

echo "-- Test with parent and non-ns attribute --\n";

$dom = new DOMDocument();
$dom->loadXML(<<<XML
<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
<url xmlns:xsi="fooooooooooooooooooooo" myattrib="bar"/>
</urlset>
XML);
$spaceNode = $dom->documentElement->firstElementChild->getAttributeNode('myattrib');
var_dump($spaceNode->nodeType);
var_dump($spaceNode->nodeValue);

$dom->documentElement->firstElementChild->remove();
var_dump($spaceNode->parentNode);

echo "-- Test with parent and ns attribute --\n";

$dom = new DOMDocument();
$dom->loadXML(<<<XML
<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
<url xmlns:xsi="fooooooooooooooooooooo" myattrib="bar"/>
</urlset>
XML);
$spaceNode = $dom->documentElement->firstElementChild->getAttributeNode('xmlns:xsi');
print_r($spaceNode);

$dom->documentElement->firstElementChild->remove();
var_dump($spaceNode->parentNode->nodeName); // Shouldn't crash;"#);
    assert_exact(&out, r#"-- Test without parent --
DOMNameSpaceNode Object
(
    [nodeName] => xmlns
    [nodeValue] => http://www.sitemaps.org/schemas/sitemap/0.9
    [nodeType] => 18
    [prefix] => 
    [localName] => xmlns
    [namespaceURI] => http://www.sitemaps.org/schemas/sitemap/0.9
    [isConnected] => 1
    [ownerDocument] => (object value omitted)
    [parentNode] => (object value omitted)
    [parentElement] => (object value omitted)
)
-- Test with parent and non-ns attribute --
int(2)
string(3) "bar"
NULL
-- Test with parent and ns attribute --
DOMNameSpaceNode Object
(
    [nodeName] => xmlns:xsi
    [nodeValue] => fooooooooooooooooooooo
    [nodeType] => 18
    [prefix] => xsi
    [localName] => xsi
    [namespaceURI] => fooooooooooooooooooooo
    [isConnected] => 1
    [ownerDocument] => (object value omitted)
    [parentNode] => (object value omitted)
    [parentElement] => (object value omitted)
)
string(3) "url""#);
}

/// php-src `ext/dom/tests/bug70558.phpt`: Bug #70558 ("Couldn't fetch" error in DOMDocument::registerNodeClass())
#[test]
#[ignore = r#"DOM port pending: Undefined method: X::registerNodeClass"#]
fn test_bug70558() {
    if skip_without_xml_native("test_bug70558") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

class X extends \DOMDocument {

    public function __clone() {
        var_dump($this->registerNodeClass('DOMDocument', 'X'));
    }
}

$dom = clone (new X());
"#);
    assert_exact(&out, r#"bool(true)"#);
}

/// php-src `ext/dom/tests/bug73907.phpt`: Bug #73907 nextSibling property not included in var_dump of DOMNode
#[test]
fn test_bug73907() {
    if skip_without_xml_native("test_bug73907") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$xmlString = '<?xml version="1.0" encoding="utf-8" ?>
<root>
</root>';

$doc = new DOMDocument();
$doc->loadXML($xmlString);
$attr = $doc->documentElement;

var_dump($attr);
"#);
    assert_format(&out, r#"object(DOMElement)#%d (%d) {%A
  ["nextSibling"]=>
  NULL
%A}"#);
}

/// php-src `ext/dom/tests/bug74004.phpt`: Bug #74004 (DOMDocument->loadHTML and ->loadHTMLFile do not heed LIBXML_NOWARNING and LIBXML_NOERROR options)
#[test]
fn test_bug74004() {
    if skip_without_xml_native("test_bug74004") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc=new DOMDocument();
$doc->loadHTML("<tag-throw></tag-throw>",LIBXML_NOERROR);

echo '===DONE===
';"#);
    assert_exact(&out, r#"===DONE==="#);
}

/// php-src `ext/dom/tests/bug74416.phpt`: Bug #74416 Wrong reflection on DOMNode::cloneNode
#[test]
#[ignore = r#"DOM port pending: ReflectionMethod::__construct(): undefined class 'DOMNode'"#]
fn test_bug74416() {
    if skip_without_xml_native("test_bug74416") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$rm = new ReflectionMethod(DOMNode::class, "cloneNode");
printf("%d\n%d\n", $rm->getNumberOfParameters(), $rm->getNumberOfRequiredParameters());
foreach ($rm->getParameters() as $param) {
    printf("Parameter #%d %s OPTIONAL\n", $param->getPosition(), $param->isOptional() ? "IS" : "IS NOT");
}
"#);
    assert_exact(&out, r#"1
0
Parameter #0 IS OPTIONAL"#);
}

/// php-src `ext/dom/tests/bug75451.phpt`: Bug #75451 (Assertion fails while foreach on empty xpath query)
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXpath"#]
fn test_bug75451() {
    if skip_without_xml_native("test_bug75451") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = new DOMDocument();
$dom->loadXML('<root><child/></root>');
$xpath = new DOMXpath($dom);
foreach($xpath->query('/root/noexist') as $child) {
    var_dump($child);
}
echo 'okey
';"#);
    assert_exact(&out, r#"okey"#);
}

/// php-src `ext/dom/tests/bug76285.phpt`: Bug #76285 DOMDocument::formatOutput attribute sometimes ignored
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::formatOutput"#]
fn test_bug76285() {
    if skip_without_xml_native("test_bug76285") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument();
$dom->formatOutput = false;
$html = '<div><div><a>test</a></div><div><a>test2</a></div></div>';
$dom->loadHTML($html, LIBXML_HTML_NOIMPLIED | LIBXML_HTML_NODEFDTD);
$rootNode = $dom->documentElement;
var_dump($dom->saveHTML($rootNode));
var_dump($dom->saveHTML());

"#);
    assert_exact(&out, r#"string(56) "<div><div><a>test</a></div><div><a>test2</a></div></div>"
string(57) "<div><div><a>test</a></div><div><a>test2</a></div></div>
""#);
}

/// php-src `ext/dom/tests/bug77569.phpt`: Bug #77569 (Write Access Violation in DomImplementation)
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMImplementation"#]
fn test_bug77569() {
    if skip_without_xml_native("test_bug77569") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$imp = new DOMImplementation;
$dom = $imp->createDocument("", "");
try {
    $dom->encoding = null;
} catch (\ValueError $e) {
    echo $e->getMessage() . \PHP_EOL;
}
"#);
    assert_exact(&out, r#"Invalid document encoding"#);
}

/// php-src `ext/dom/tests/bug77686.phpt`: Bug #77686 (Removed elements are still returned by getElementById)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::getElementById"#]
fn test_bug77686() {
    if skip_without_xml_native("test_bug77686") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;
$doc->loadHTML('<html id="htmlelement"><body id="x">before<div id="y">hello</div>after</body></html>');
$body = $doc->getElementById('x');
$div = $doc->getElementById('y');
var_dump($doc->getElementById('y')->textContent);

// Detached from document, should not find it anymore
$body->removeChild($div);
var_dump($doc->getElementById('y'));

// Added again, should find it
$body->appendChild($div);
var_dump($doc->getElementById('y')->textContent);

// Should find root element without a problem
var_dump($doc->getElementById('htmlelement')->textContent);

// Created element but not yet attached, should not find it before it is added
$new_element = $doc->createElement('p');
$new_element->textContent = 'my new text';
$new_element->setAttribute('id', 'myp');
var_dump($doc->getElementById('myp'));
$body->appendChild($new_element);
var_dump($doc->getElementById('myp')->textContent);

"#);
    assert_exact(&out, r#"string(5) "hello"
NULL
string(5) "hello"
string(16) "beforeafterhello"
NULL
string(11) "my new text""#);
}

/// php-src `ext/dom/tests/bug78025.phpt`: Bug #78025 (segfault when accessing properties of DOMDocumentType)
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::doctype"#]
fn test_bug78025() {
    if skip_without_xml_native("test_bug78025") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$htm = "<!DOCTYPE><html></html>";
$dom = new DOMDocument;
$dom->loadHTML($htm, LIBXML_NOERROR);
var_dump($dom->doctype->name);
"#);
    assert_format(&out, r#"string(0) """#);
}

/// php-src `ext/dom/tests/bug78221.phpt`: Bug #78221 (DOMNode::normalize() doesn't remove empty text nodes)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::getElementById"#]
fn test_bug78221() {
    if skip_without_xml_native("test_bug78221") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument();
$doc->loadHTML('<p id=x>foo</p>');
$p = $doc->getElementById('x');
$p->childNodes[0]->textContent = '';
$p->normalize();
var_dump($p->childNodes->length);
"#);
    assert_exact(&out, r#"int(0)"#);
}

/// php-src `ext/dom/tests/bug78577.phpt`: Bug #78577 (Crash in DOMNameSpace debug info handlers)
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_bug78577() {
    if skip_without_xml_native("test_bug78577") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;
$doc->loadXML('<foo xmlns="http://php.net/test" xmlns:foo="urn:foo" />');

$attr = $doc->documentElement->getAttributeNode('xmlns');
var_dump($attr);

"#);
    assert_exact(&out, r#"object(DOMNameSpaceNode)#3 (10) {
  ["nodeName"]=>
  string(5) "xmlns"
  ["nodeValue"]=>
  string(19) "http://php.net/test"
  ["nodeType"]=>
  int(18)
  ["prefix"]=>
  string(0) ""
  ["localName"]=>
  string(5) "xmlns"
  ["namespaceURI"]=>
  string(19) "http://php.net/test"
  ["isConnected"]=>
  bool(true)
  ["ownerDocument"]=>
  string(22) "(object value omitted)"
  ["parentNode"]=>
  string(22) "(object value omitted)"
  ["parentElement"]=>
  string(22) "(object value omitted)"
}"#);
}

/// php-src `ext/dom/tests/bug79271.phpt`: Bug #79271 (DOMDocumentType::$childNodes is NULL)
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMImplementation"#]
fn test_bug79271() {
    if skip_without_xml_native("test_bug79271") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = new DOMImplementation();
$type = $dom->createDocumentType('html');
var_dump($type->childNodes);
"#);
    assert_format(&out, r#"object(DOMNodeList)#%d (1) {
  ["length"]=>
  int(0)
}"#);
}

/// php-src `ext/dom/tests/bug79451.phpt`: Bug #79451 (Using DOMDocument->replaceChild on doctype causes double free)
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMImplementation"#]
fn test_bug79451() {
    if skip_without_xml_native("test_bug79451") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = new \DOMDocument();
$dom->loadHTML("<!DOCTYPE html><p>hello</p>");
$impl = new \DOMImplementation();
$dt = $impl->createDocumentType("html_replace", "", "");
$dom->replaceChild($dt, $dom->doctype);

var_dump($dom->doctype->name);
echo $dom->saveXML();
"#);
    assert_format(&out, r#"string(12) "html_replace"
<?xml version="1.0" standalone="yes"?>
<!DOCTYPE html_replace>
<html><body><p>hello</p></body></html>"#);
}

/// php-src `ext/dom/tests/bug79852.phpt`: Bug #79852: count(DOMNodeList) doesn't match count(IteratorIterator(DOMNodeList))
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_bug79852() {
    if skip_without_xml_native("test_bug79852") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$XML = <<< XML
<root>
  <item>1</item>
  <item>2</item>
  <item>3</item>
</root>
XML;

$dom = new DomDocument();
$dom->loadXml($XML);
$items = $dom->getElementsByTagName('item');

echo "Count: ".count($items)."\n";
echo "Count: ".iterator_count($items->getIterator())."\n";
$it = new IteratorIterator($items);
echo "Count: ".iterator_count($it)."\n";
echo "Count: ".iterator_count($it)."\n";

"#);
    assert_format(&out, r#"Count: 3
Count: 3
Count: 3

Fatal error: Uncaught Error: Iterator does not support rewinding in %s:%d
Stack trace:
#0 [internal function]: InternalIterator->rewind()
#1 [internal function]: IteratorIterator->rewind()
#2 %s(%d): iterator_count(Object(IteratorIterator))
#3 {main}
  thrown in %s on line %d"#);
}

/// php-src `ext/dom/tests/bug79968.phpt`: dom: Bug #79968 - Crash when calling before without valid hierarchy
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMText::before"#]
fn test_bug79968() {
    if skip_without_xml_native("test_bug79968") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$cdata = new DOMText;

$cdata->before("string");
$cdata->after("string");
$cdata->replaceWith("string");

$dom = new DOMDocument();
$dom->adoptNode($cdata);
var_dump($dom->saveXML($cdata));

"#);
    assert_exact(&out, r#"string(0) """#);
}

/// php-src `ext/dom/tests/bug79971_2.phpt`: Bug #79971 (special character is breaking the path in xml function)
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMImplementation"#]
fn test_bug79971_2() {
    if skip_without_xml_native("test_bug79971_2") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$imp = new DOMImplementation;
if (PHP_OS_FAMILY === 'Windows') {
    $path = '/' . str_replace('\\', '/', __DIR__);
} else {
    $path = __DIR__;
}
$uri = "file://$path/bug79971_2.xml";
var_dump($imp->createDocumentType("$uri%00foo"));
"#);
    assert_format(&out, r#"Warning: DOMImplementation::createDocumentType(): URI must not contain percent-encoded NUL bytes in %s on line %d
bool(false)"#);
}

/// php-src `ext/dom/tests/bug80268.phpt`: Bug #80268 (loadHTML() truncates at NUL bytes)
#[test]
#[ignore = r#"php-src SKIPIF: skip For libxml2 < 2.9.12 only"#]
fn test_bug80268() {
    if skip_without_xml_native("test_bug80268") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument;
$doc->loadHTML("<p>foo\0bar</p>");
$html = $doc->saveHTML();
var_dump(strpos($html, '<p>foo</p>') !== false);

file_put_contents(__DIR__ . '/80268.html', "<p>foo\0bar</p>");
$doc = new DOMDocument;
$doc->loadHTMLFile(__DIR__ . '/80268.html');
$html = $doc->saveHTML();
var_dump(strpos($html, '<p>foo</p>') !== false);
"#);
    assert_exact(&out, r#"bool(true)
bool(true)"#);
}

/// php-src `ext/dom/tests/bug80268_2.phpt`: Bug #80268 (loadHTML() truncates at NUL bytes)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::saveHTML"#]
fn test_bug80268_2() {
    if skip_without_xml_native("test_bug80268_2") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument;
$doc->loadHTML("<p>foo\0bar</p>", LIBXML_NOERROR);
$html = $doc->saveHTML();
var_dump(strpos($html, '<p>foo</p>') !== false);

file_put_contents(__DIR__ . '/80268.html', "<p>foo\0bar</p>");
$doc = new DOMDocument;
$doc->loadHTMLFile(__DIR__ . '/80268.html', LIBXML_NOERROR);
$html = $doc->saveHTML();
var_dump(strpos($html, '<p>foo</p>') !== false);
"#);
    assert_format(&out, r#"bool(false)
bool(false)"#);
}

/// php-src `ext/dom/tests/bug80332_1.phpt`: Bug #80332 (Completely broken array access functionality with DOMNamedNodeMap) - DOMNamedNodeMap variation
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXPath"#]
fn test_bug80332_1() {
    if skip_without_xml_native("test_bug80332_1") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;

$doc->loadHTML('<span attr1="value1" attr2="value2"></span>');

$x = new DOMXPath($doc);
$span = $x->query('//span')[0];

print "Node name: {$span->nodeName}\n";

function test($span, $key) {
    $key_formatted = match ($key) {
        false => 'false',
        true => 'true',
        null => 'null',
        default => is_string($key) ? "'$key'" : $key,
    };
    echo "Attribute [{$key_formatted}] name: ", $span->attributes[$key]->nodeName ?? '/', "\n";
    echo "Attribute [{$key_formatted}] value: ", $span->attributes[$key]->nodeValue ?? '/', "\n";
    echo "Attribute [{$key_formatted}] isset: ", isset($span->attributes[$key]) ? "yes" : "no", "\n";
    echo "\n";
}

test($span, 0);
test($span, false);
test($span, true);
test($span, null);
test($span, 'attr2');
// This one should fail because there is no 'hi' attribute
test($span, 'hi');
test($span, '0');
test($span, '0.5');
test($span, '1');
// This one should fail because it's out of bounds
test($span, '2147483647');

"#);
    assert_exact(&out, r#"Node name: span
Attribute [0] name: attr1
Attribute [0] value: value1
Attribute [0] isset: yes

Attribute [false] name: attr1
Attribute [false] value: value1
Attribute [false] isset: yes

Attribute [true] name: attr2
Attribute [true] value: value2
Attribute [true] isset: yes

Attribute [null] name: attr1
Attribute [null] value: value1
Attribute [null] isset: yes

Attribute ['attr2'] name: attr2
Attribute ['attr2'] value: value2
Attribute ['attr2'] isset: yes

Attribute ['hi'] name: /
Attribute ['hi'] value: /
Attribute ['hi'] isset: no

Attribute ['0'] name: attr1
Attribute ['0'] value: value1
Attribute ['0'] isset: yes

Attribute ['0.5'] name: attr1
Attribute ['0.5'] value: value1
Attribute ['0.5'] isset: yes

Attribute ['1'] name: attr2
Attribute ['1'] value: value2
Attribute ['1'] isset: yes

Attribute ['2147483647'] name: /
Attribute ['2147483647'] value: /
Attribute ['2147483647'] isset: no"#);
}

/// php-src `ext/dom/tests/bug80332_2.phpt`: Bug #80332 (Completely broken array access functionality with DOMNamedNodeMap) - DOMNodeList variation
#[test]
#[ignore = r#"DOM port pending: Cannot index non-array"#]
fn test_bug80332_2() {
    if skip_without_xml_native("test_bug80332_2") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;
$doc->loadXML('<?xml version="1.0"?><span><strong id="1"/><strong id="2"/></span>');

$list = $doc->getElementsByTagName('strong');

function test($list, $key) {
    $key_formatted = match ($key) {
        false => 'false',
        true => 'true',
        null => 'null',
        default => is_string($key) ? "'$key'" : $key,
    };
    echo "list[$key_formatted] id attribute: ", $list[$key] ? $list[$key]->getAttribute('id') : '/', "\n";
}

test($list, 0);
test($list, false);
test($list, true);
test($list, null);
test($list, '0');
test($list, '0.5');
test($list, '1');
// These two should fail because there's no named lookup possible here
test($list, 'attr2');
test($list, 'hi');
// This one should fail because it's out of bounds
test($list, '2147483647');

"#);
    assert_exact(&out, r#"list[0] id attribute: 1
list[false] id attribute: 1
list[true] id attribute: 2
list[null] id attribute: 1
list['0'] id attribute: 1
list['0.5'] id attribute: 1
list['1'] id attribute: 2
list['attr2'] id attribute: /
list['hi'] id attribute: /
list['2147483647'] id attribute: /"#);
}

/// php-src `ext/dom/tests/bug80600.phpt`: dom: DOMChildNode::remove does not work on character data
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_bug80600() {
    if skip_without_xml_native("test_bug80600") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new \DOMDocument();
$doc->loadXML('<a><!-- foo --></a>');
$doc->documentElement->firstChild->remove();
echo $doc->saveXML($doc->documentElement);
"#);
    assert_exact(&out, r#"<a/>"#);
}

/// php-src `ext/dom/tests/bug80602.phpt`: Bug #80602 (Segfault when using DOMChildNode::before())
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_bug80602() {
    if skip_without_xml_native("test_bug80602") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
declare(strict_types=1);

$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->firstChild;
$target->before($target);
echo "1 ", $doc->saveXML($doc->documentElement).PHP_EOL;

$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->lastChild;
$target->before($target);
echo "2 ", $doc->saveXML($doc->documentElement).PHP_EOL;


$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->firstChild;
$target->before($doc->documentElement->lastChild);
echo "3 ", $doc->saveXML($doc->documentElement).PHP_EOL;

$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->lastChild;
$target->before($doc->documentElement->firstChild);
echo "4 ", $doc->saveXML($doc->documentElement).PHP_EOL;


$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->firstChild;
$target->before($target, $doc->documentElement->lastChild);
echo "5 ", $doc->saveXML($doc->documentElement).PHP_EOL;

$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->firstChild;
$target->before($doc->documentElement->lastChild, $target);
echo "6 ", $doc->saveXML($doc->documentElement).PHP_EOL;


$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->lastChild;
$target->before($target, $doc->documentElement->firstChild);
echo "7 ", $doc->saveXML($doc->documentElement).PHP_EOL;

$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->lastChild;
$target->before($doc->documentElement->firstChild, $target);
echo "8 ", $doc->saveXML($doc->documentElement).PHP_EOL;


$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->firstChild;
$target->before('bar','baz');
echo "9 ", $doc->saveXML($doc->documentElement).PHP_EOL;

$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->lastChild;
$target->before('bar','baz');
echo "10 ", $doc->saveXML($doc->documentElement).PHP_EOL;


$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->firstChild;
$target->before($target, 'bar','baz');
echo "11 ", $doc->saveXML($doc->documentElement).PHP_EOL;

$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->firstChild;
$target->before('bar', $target, 'baz');
echo "12 ", $doc->saveXML($doc->documentElement).PHP_EOL;

$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->firstChild;
$target->before('bar', 'baz', $target);
echo "13 ", $doc->saveXML($doc->documentElement).PHP_EOL;



$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->lastChild;
$target->before($target, 'bar','baz');
echo "14 ", $doc->saveXML($doc->documentElement).PHP_EOL;

$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->lastChild;
$target->before('bar', $target, 'baz');
echo "15 ", $doc->saveXML($doc->documentElement).PHP_EOL;

$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->lastChild;
$target->before('bar', 'baz', $target);
echo "16 ", $doc->saveXML($doc->documentElement).PHP_EOL;



$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->firstChild;
$target->before('bar', $target, $doc->documentElement->lastChild);
echo "17 ", $doc->saveXML($doc->documentElement).PHP_EOL;


$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->firstChild;
$target->before($target, 'bar', $doc->documentElement->lastChild);
echo "18 ", $doc->saveXML($doc->documentElement).PHP_EOL;


$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->firstChild;
$target->before($target, $doc->documentElement->lastChild, 'bar');
echo "19 ", $doc->saveXML($doc->documentElement).PHP_EOL;




$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->lastChild;
$target->before('bar', $doc->documentElement->firstChild, $target);
echo "20 ", $doc->saveXML($doc->documentElement).PHP_EOL;


$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->lastChild;
$target->before($doc->documentElement->firstChild, 'bar', $target);
echo "21 ", $doc->saveXML($doc->documentElement).PHP_EOL;


$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->lastChild;
$target->before($doc->documentElement->firstChild, $target, 'bar');
echo "22 ", $doc->saveXML($doc->documentElement).PHP_EOL;

"#);
    assert_format(&out, r#"1 <a>foo<last/></a>
2 <a>foo<last/></a>
3 <a><last/>foo</a>
4 <a>foo<last/></a>
5 <a>foo<last/></a>
6 <a><last/>foo</a>
7 <a><last/>foo</a>
8 <a>foo<last/></a>
9 <a>barbazfoo<last/></a>
10 <a>foobarbaz<last/></a>
11 <a>foobarbaz<last/></a>
12 <a>barfoobaz<last/></a>
13 <a>barbazfoo<last/></a>
14 <a>foo<last/>barbaz</a>
15 <a>foobar<last/>baz</a>
16 <a>foobarbaz<last/></a>
17 <a>barfoo<last/></a>
18 <a>foobar<last/></a>
19 <a>foo<last/>bar</a>
20 <a>barfoo<last/></a>
21 <a>foobar<last/></a>
22 <a>foo<last/>bar</a>"#);
}

/// php-src `ext/dom/tests/bug80602_2.phpt`: Bug #80602 (Segfault when using DOMChildNode::after())
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_bug80602_2() {
    if skip_without_xml_native("test_bug80602_2") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
declare(strict_types=1);

$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->firstChild;
$target->after($target);
echo "1 ", $doc->saveXML($doc->documentElement).PHP_EOL;

$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->lastChild;
$target->after($target);
echo "2 ", $doc->saveXML($doc->documentElement).PHP_EOL;


$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->firstChild;
$target->after($doc->documentElement->lastChild);
echo "3 ", $doc->saveXML($doc->documentElement).PHP_EOL;

$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->lastChild;
$target->after($doc->documentElement->firstChild);
echo "4 ", $doc->saveXML($doc->documentElement).PHP_EOL;


$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->firstChild;
$target->after($target, $doc->documentElement->lastChild);
echo "5 ", $doc->saveXML($doc->documentElement).PHP_EOL;

$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->firstChild;
$target->after($doc->documentElement->lastChild, $target);
echo "6 ", $doc->saveXML($doc->documentElement).PHP_EOL;


$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->lastChild;
$target->after($target, $doc->documentElement->firstChild);
echo "7 ", $doc->saveXML($doc->documentElement).PHP_EOL;

$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->lastChild;
$target->after($doc->documentElement->firstChild, $target);
echo "8 ", $doc->saveXML($doc->documentElement).PHP_EOL;


$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->firstChild;
$target->after('bar','baz');
echo "9 ", $doc->saveXML($doc->documentElement).PHP_EOL;

$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->lastChild;
$target->after('bar','baz');
echo "10 ", $doc->saveXML($doc->documentElement).PHP_EOL;


$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->firstChild;
$target->after($target, 'bar','baz');
echo "11 ", $doc->saveXML($doc->documentElement).PHP_EOL;

$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->firstChild;
$target->after('bar', $target, 'baz');
echo "12 ", $doc->saveXML($doc->documentElement).PHP_EOL;

$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->firstChild;
$target->after('bar', 'baz', $target);
echo "13 ", $doc->saveXML($doc->documentElement).PHP_EOL;



$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->lastChild;
$target->after($target, 'bar','baz');
echo "14 ", $doc->saveXML($doc->documentElement).PHP_EOL;

$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->lastChild;
$target->after('bar', $target, 'baz');
echo "15 ", $doc->saveXML($doc->documentElement).PHP_EOL;

$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->lastChild;
$target->after('bar', 'baz', $target);
echo "16 ", $doc->saveXML($doc->documentElement).PHP_EOL;



$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->firstChild;
$target->after('bar', $target, $doc->documentElement->lastChild);
echo "17 ", $doc->saveXML($doc->documentElement).PHP_EOL;


$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->firstChild;
$target->after($target, 'bar', $doc->documentElement->lastChild);
echo "18 ", $doc->saveXML($doc->documentElement).PHP_EOL;


$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->firstChild;
$target->after($target, $doc->documentElement->lastChild, 'bar');
echo "19 ", $doc->saveXML($doc->documentElement).PHP_EOL;




$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->lastChild;
$target->after('bar', $doc->documentElement->firstChild, $target);
echo "20 ", $doc->saveXML($doc->documentElement).PHP_EOL;


$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->lastChild;
$target->after($doc->documentElement->firstChild, 'bar', $target);
echo "21 ", $doc->saveXML($doc->documentElement).PHP_EOL;


$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->lastChild;
$target->after($doc->documentElement->firstChild, $target, 'bar');
echo "22 ", $doc->saveXML($doc->documentElement).PHP_EOL;

"#);
    assert_format(&out, r#"1 <a>foo<last/></a>
2 <a>foo<last/></a>
3 <a>foo<last/></a>
4 <a><last/>foo</a>
5 <a>foo<last/></a>
6 <a><last/>foo</a>
7 <a><last/>foo</a>
8 <a>foo<last/></a>
9 <a>foobarbaz<last/></a>
10 <a>foo<last/>barbaz</a>
11 <a>foobarbaz<last/></a>
12 <a>barfoobaz<last/></a>
13 <a>barbazfoo<last/></a>
14 <a>foo<last/>barbaz</a>
15 <a>foobar<last/>baz</a>
16 <a>foobarbaz<last/></a>
17 <a>barfoo<last/></a>
18 <a>foobar<last/></a>
19 <a>foo<last/>bar</a>
20 <a>barfoo<last/></a>
21 <a>foobar<last/></a>
22 <a>foo<last/>bar</a>"#);
}

/// php-src `ext/dom/tests/bug80602_3.phpt`: Bug #80602 (Segfault when using DOMChildNode::before()) - use-after-free variation
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_bug80602_3() {
    if skip_without_xml_native("test_bug80602_3") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->lastChild;
$target->before('bar', $doc->documentElement->firstChild, 'baz');
echo $doc->saveXML($doc->documentElement), "\n";
var_dump($target);

$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/></a>');
$target = $doc->documentElement->lastChild;
// Note: after instead of before
$target->after('bar', $doc->documentElement->firstChild, 'baz');
echo $doc->saveXML($doc->documentElement), "\n";
var_dump($target);

"#);
    assert_format(&out, r#"<a>barfoobaz<last/></a>
object(DOMElement)#3 (27) {
  ["tagName"]=>
  string(4) "last"
  ["className"]=>
  string(0) ""
  ["id"]=>
  string(0) ""
  ["schemaTypeInfo"]=>
  NULL
  ["firstElementChild"]=>
  NULL
  ["lastElementChild"]=>
  NULL
  ["childElementCount"]=>
  int(0)
  ["previousElementSibling"]=>
  NULL
  ["nextElementSibling"]=>
  NULL
  ["nodeName"]=>
  string(4) "last"
  ["nodeValue"]=>
  string(0) ""
  ["nodeType"]=>
  int(1)
  ["parentNode"]=>
  string(22) "(object value omitted)"
  ["parentElement"]=>
  string(22) "(object value omitted)"
  ["childNodes"]=>
  string(22) "(object value omitted)"
  ["firstChild"]=>
  NULL
  ["lastChild"]=>
  NULL
  ["previousSibling"]=>
  string(22) "(object value omitted)"
  ["nextSibling"]=>
  NULL
  ["attributes"]=>
  string(22) "(object value omitted)"
  ["isConnected"]=>
  bool(true)
  ["ownerDocument"]=>
  string(22) "(object value omitted)"
  ["namespaceURI"]=>
  NULL
  ["prefix"]=>
  string(0) ""
  ["localName"]=>
  string(4) "last"
  ["baseURI"]=>
  string(%d) %s
  ["textContent"]=>
  string(0) ""
}
<a><last/>barfoobaz</a>
object(DOMElement)#2 (27) {
  ["tagName"]=>
  string(4) "last"
  ["className"]=>
  string(0) ""
  ["id"]=>
  string(0) ""
  ["schemaTypeInfo"]=>
  NULL
  ["firstElementChild"]=>
  NULL
  ["lastElementChild"]=>
  NULL
  ["childElementCount"]=>
  int(0)
  ["previousElementSibling"]=>
  NULL
  ["nextElementSibling"]=>
  NULL
  ["nodeName"]=>
  string(4) "last"
  ["nodeValue"]=>
  string(0) ""
  ["nodeType"]=>
  int(1)
  ["parentNode"]=>
  string(22) "(object value omitted)"
  ["parentElement"]=>
  string(22) "(object value omitted)"
  ["childNodes"]=>
  string(22) "(object value omitted)"
  ["firstChild"]=>
  NULL
  ["lastChild"]=>
  NULL
  ["previousSibling"]=>
  NULL
  ["nextSibling"]=>
  string(22) "(object value omitted)"
  ["attributes"]=>
  string(22) "(object value omitted)"
  ["isConnected"]=>
  bool(true)
  ["ownerDocument"]=>
  string(22) "(object value omitted)"
  ["namespaceURI"]=>
  NULL
  ["prefix"]=>
  string(0) ""
  ["localName"]=>
  string(4) "last"
  ["baseURI"]=>
  string(%d) %s
  ["textContent"]=>
  string(0) ""
}"#);
}

/// php-src `ext/dom/tests/bug80602_4.phpt`: Bug #80602 (Segfault when using DOMChildNode::before()) - after text merge variation
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::firstChild"#]
fn test_bug80602_4() {
    if skip_without_xml_native("test_bug80602_4") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new \DOMDocument();
$doc->loadXML('<a>foo<last/>bar</a>');
$foo = $doc->firstChild->firstChild;
$bar = $doc->firstChild->lastChild;

$foo->after($bar);

var_dump($doc->saveXML());

$foo->nodeValue = "x";

var_dump($doc->saveXML());

$bar->nodeValue = "y";

var_dump($doc->saveXML());

"#);
    assert_exact(&out, r#"string(43) "<?xml version="1.0"?>
<a>foobar<last/></a>
"
string(41) "<?xml version="1.0"?>
<a>xbar<last/></a>
"
string(39) "<?xml version="1.0"?>
<a>xy<last/></a>
""#);
}

/// php-src `ext/dom/tests/bug80927.phpt`: Bug #80927 (Removing documentElement after creating attribute node: possible use-after-free)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createAttributeNS"#]
fn test_bug80927() {
    if skip_without_xml_native("test_bug80927") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

function test1() {
    $dom = new DOMDocument();
    $dom->appendChild($dom->createElement("html"));
    $a = $dom->createAttributeNS("fake_ns", "test:test");
    $dom->removeChild($dom->documentElement);

    echo $dom->saveXML();

    var_dump($a->namespaceURI);
    var_dump($a->prefix);
}

enum Test2Variation {
    case REMOVE_DOCUMENT;
    case REMOVE_CHILD;
}

function test2(Test2Variation $variation) {
    $dom = new DOMDocument();
    $dom->appendChild($dom->createElement("html"));
    $a = $dom->createAttributeNS("fake_ns", "test:test");

    $foo = $dom->appendChild($dom->createElement('foo'));
    $foo->appendChild($dom->documentElement);

    unset($foo);

    match ($variation) {
        Test2Variation::REMOVE_DOCUMENT => $dom->documentElement->remove(),
        Test2Variation::REMOVE_CHILD => $dom->documentElement->firstElementChild->remove(),
    };

    echo $dom->saveXML();

    var_dump($a->namespaceURI);
    var_dump($a->prefix);
}

function test3() {
    $dom = new DOMDocument();
    $dom->appendChild($dom->createElement('html'));
    $foobar = $dom->documentElement->appendChild($dom->createElementNS('some:ns', 'foo:bar'));
    $foobar2 = $foobar->appendChild($dom->createElementNS('some:ns', 'foo:bar2'));
    $foobar->remove();
    unset($foobar);
    $dom->documentElement->appendChild($foobar2);

    echo $dom->saveXML();

    var_dump($foobar2->namespaceURI);
    var_dump($foobar2->prefix);
}

echo "--- Remove namespace declarator for attribute, root ---\n";
test1();
echo "--- Remove namespace declarator for attribute, moved root ---\n";
test2(Test2Variation::REMOVE_DOCUMENT);
echo "--- Remove namespace declarator for attribute, moved root child ---\n";
test2(Test2Variation::REMOVE_CHILD);
echo "--- Remove namespace declarator for element ---\n";
test3();

"#);
    assert_exact(&out, r#"--- Remove namespace declarator for attribute, root ---
<?xml version="1.0"?>
string(7) "fake_ns"
string(4) "test"
--- Remove namespace declarator for attribute, moved root ---
<?xml version="1.0"?>
string(7) "fake_ns"
string(4) "test"
--- Remove namespace declarator for attribute, moved root child ---
<?xml version="1.0"?>
<foo/>
string(7) "fake_ns"
string(4) "test"
--- Remove namespace declarator for element ---
<?xml version="1.0"?>
<html><foo:bar2 xmlns:foo="some:ns"/></html>
string(7) "some:ns"
string(3) "foo""#);
}

/// php-src `ext/dom/tests/bug81433.phpt`: Bug #81433 (DOMElement::setIdAttribute(attr, true) called twice removes ID)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMElement::setIdAttribute"#]
fn test_bug81433() {
    if skip_without_xml_native("test_bug81433") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = new DOMDocument('1.0', 'utf-8');

$element = $dom->createElement('test', 'root');

$dom->appendChild($element);

$element->setAttribute("id", 123);
$element->setIdAttribute("id", true);

$node = $element->getAttributeNode("id");
var_dump($node->isId());

$element->setIdAttribute("id", true);
var_dump($node->isId());
"#);
    assert_exact(&out, r#"bool(true)
bool(true)"#);
}

/// php-src `ext/dom/tests/bug81642.phpt`: Bug #81642 (DOMChildNode::replaceWith() bug when replacing a node with itself)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMElement::replaceWith"#]
fn test_bug81642() {
    if skip_without_xml_native("test_bug81642") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

// Replace with itself
$doc = new DOMDocument();
$doc->appendChild($target = $doc->createElement('test'));
$target->replaceWith($target);
var_dump($doc->saveXML());

// Replace with itself + another element
$doc = new DOMDocument();
$doc->appendChild($target = $doc->createElement('test'));
$target->replaceWith($target, $doc->createElement('foo'));
var_dump($doc->saveXML());

// Replace with text node
$doc = new DOMDocument();
$doc->appendChild($target = $doc->createElement('test'));
$target->replaceWith($target, 'foo');
var_dump($doc->saveXML());

// Replace with text node variant 2
$doc = new DOMDocument();
$doc->appendChild($target = $doc->createElement('test'));
$target->replaceWith('bar', $target, 'foo');
var_dump($doc->saveXML());

"#);
    assert_exact(&out, r#"string(30) "<?xml version="1.0"?>
<test/>
"
string(37) "<?xml version="1.0"?>
<test/>
<foo/>
"
string(34) "<?xml version="1.0"?>
<test/>
foo
"
string(38) "<?xml version="1.0"?>
bar
<test/>
foo
""#);
}
