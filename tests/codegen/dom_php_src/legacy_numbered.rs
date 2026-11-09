//! Purpose:
//! PHP 8.5.8 `ext/dom` php-src tests for the original numbered `domNNN` smoke tests, as Elephc codegen tests.
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

/// php-src `ext/dom/tests/dom001.phpt`: Test 1: Accessing single node
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_dom001() {
    if skip_without_xml_native("test_dom001") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
require_once("dom_test.inc");

echo "Test 1: accessing single nodes from php\n";
$dom = new domDocument;
$dom->loadxml($xmlstr);
if(!$dom) {
  echo "Error while parsing the document\n";
  exit(0);
}

// children() of of document would result in a memleak
//$children = $dom->children();
//print_node_list($children);

echo "--------- root\n";
$rootnode = $dom->documentElement;
print_node($rootnode);

echo "--------- children of root\n";
$children = $rootnode->childNodes;
print_node_list($children);

// The last node should be identical with the last entry in the children array
echo "--------- last\n";
$last = $rootnode->lastChild;
print_node($last);

// The parent of this last node is the root again
echo "--------- parent\n";
$parent = $last->parentNode;
print_node($parent);

// The children of this parent are the same children as one above
echo "--------- children of parent\n";
$children = $parent->childNodes;
print_node_list($children);

echo "--------- creating a new attribute\n";
//This is worthless
//$attr = $dom->createAttribute("src", "picture.gif");
//print_r($attr);

//$rootnode->set_attributeNode($attr);
$attr = $rootnode->setAttribute("src", "picture.gif");
$attr = $rootnode->getAttribute("src");
print_r($attr);
print "\n";

echo "--------- Get Attribute Node\n";
$attr = $rootnode->getAttributeNode("src");
print_node($attr);

echo "--------- Remove Attribute Node\n";
$attr = $rootnode->removeAttribute("src");
print "Removed " . $attr . " attributes.\n";

echo "--------- attributes of rootnode\n";
$attrs = $rootnode->attributes;
print_node_list($attrs);

echo "--------- children of an attribute\n";
$children = $attrs->item(0)->childNodes;
print_node_list($children);

echo "--------- Add child to root\n";
$myelement = new domElement("Silly", "Symphony");
$newchild = $rootnode->appendChild($myelement);
print_node($newchild);
print $dom->saveXML();
print "\n";

echo "--------- Find element by tagname\n";
echo "    Using dom\n";
$children = $dom->getElementsByTagname("Silly");
print_node_list($children);

echo "    Using elem\n";
$children = $rootnode->getElementsByTagName("Silly");
print_node_list($children);

echo "--------- Unlink Node\n";
print_node($children->item(0));
$rootnode->removeChild($children->item(0));
print_node_list($rootnode->childNodes);
print $dom->savexml();

echo "--------- Find element by id\n";
print ("Not implemented\n");

echo "--------- Check various node_name return values\n";
print ("Not needed\n");

"#);
    assert_exact(&out, r#"Test 1: accessing single nodes from php
--------- root
Node Name: chapter
Node Type: 1
Num Children: 4

--------- children of root
Node Name: title
Node Type: 1
Num Children: 1
Node Content: Title

Node Name: #text
Node Type: 3
Num Children: 0
Node Content:

Node Name: para
Node Type: 1
Num Children: 7

Node Name: #text
Node Type: 3
Num Children: 0
Node Content:

--------- last
Node Name: #text
Node Type: 3
Num Children: 0
Node Content:

--------- parent
Node Name: chapter
Node Type: 1
Num Children: 4

--------- children of parent
Node Name: title
Node Type: 1
Num Children: 1
Node Content: Title

Node Name: #text
Node Type: 3
Num Children: 0
Node Content:

Node Name: para
Node Type: 1
Num Children: 7

Node Name: #text
Node Type: 3
Num Children: 0
Node Content:

--------- creating a new attribute
picture.gif
--------- Get Attribute Node
Node Name: src
Node Type: 2
Num Children: 1
Node Content: picture.gif

--------- Remove Attribute Node
Removed 1 attributes.
--------- attributes of rootnode
Node Name: language
Node Type: 2
Num Children: 1
Node Content: en

--------- children of an attribute
Node Name: #text
Node Type: 3
Num Children: 0
Node Content: en

--------- Add child to root
Node Name: Silly
Node Type: 1
Num Children: 1
Node Content: Symphony

<?xml version="1.0" standalone="yes"?>
<!DOCTYPE chapter SYSTEM "/share/sgml/Norman_Walsh/db3xml10/db3xml10.dtd" [
<!ENTITY sp "spanish">
]>
<!-- lsfj  -->
<chapter language="en"><title language="en">Title</title>
<para language="ge">
&sp;
<!-- comment -->
<informaltable language="&sp;kkk">
<tgroup cols="3">
<tbody>
<row><entry>a1</entry><entry morerows="1">b1</entry><entry>c1</entry></row>
<row><entry>a2</entry><entry>c2</entry></row>
<row><entry>a3</entry><entry>b3</entry><entry>c3</entry></row>
</tbody>
</tgroup>
</informaltable>
</para>
<Silly>Symphony</Silly></chapter>

--------- Find element by tagname
    Using dom
Node Name: Silly
Node Type: 1
Num Children: 1
Node Content: Symphony

    Using elem
Node Name: Silly
Node Type: 1
Num Children: 1
Node Content: Symphony

--------- Unlink Node
Node Name: Silly
Node Type: 1
Num Children: 1
Node Content: Symphony

Node Name: title
Node Type: 1
Num Children: 1
Node Content: Title

Node Name: #text
Node Type: 3
Num Children: 0
Node Content:

Node Name: para
Node Type: 1
Num Children: 7

Node Name: #text
Node Type: 3
Num Children: 0
Node Content:

<?xml version="1.0" standalone="yes"?>
<!DOCTYPE chapter SYSTEM "/share/sgml/Norman_Walsh/db3xml10/db3xml10.dtd" [
<!ENTITY sp "spanish">
]>
<!-- lsfj  -->
<chapter language="en"><title language="en">Title</title>
<para language="ge">
&sp;
<!-- comment -->
<informaltable language="&sp;kkk">
<tgroup cols="3">
<tbody>
<row><entry>a1</entry><entry morerows="1">b1</entry><entry>c1</entry></row>
<row><entry>a2</entry><entry>c2</entry></row>
<row><entry>a3</entry><entry>b3</entry><entry>c3</entry></row>
</tbody>
</tgroup>
</informaltable>
</para>
</chapter>
--------- Find element by id
Not implemented
--------- Check various node_name return values
Not needed"#);
}

/// php-src `ext/dom/tests/dom002.phpt`: Test 2: getElementsByTagName() / getElementsByTagNameNS()
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::getElementsByTagNameNS"#]
fn test_dom002() {
    if skip_without_xml_native("test_dom002") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$xml = <<<HERE
<?xml version="1.0" encoding="ISO-8859-1" ?>
<foo xmlns="http://www.example.com/ns/foo"
     xmlns:fubar="http://www.example.com/ns/fubar">
  <bar><test1 /></bar>
  <bar><test2 /></bar>
  <fubar:bar><test3 /></fubar:bar>
  <fubar:bar><test4 /></fubar:bar>
</foo>
HERE;

function dump($elems) {
    foreach ($elems as $elem) {
        var_dump($elem->nodeName);
        dump($elem->childNodes);
    }
}

$dom = new DOMDocument();
$dom->loadXML($xml);
$doc = $dom->documentElement;
dump($dom->getElementsByTagName('bar'));
dump($doc->getElementsByTagName('bar'));
dump($dom->getElementsByTagNameNS('http://www.example.com/ns/fubar', 'bar'));
dump($doc->getElementsByTagNameNS('http://www.example.com/ns/fubar', 'bar'));
"#);
    assert_exact(&out, r#"string(3) "bar"
string(5) "test1"
string(3) "bar"
string(5) "test2"
string(9) "fubar:bar"
string(5) "test3"
string(9) "fubar:bar"
string(5) "test4"
string(3) "bar"
string(5) "test1"
string(3) "bar"
string(5) "test2"
string(9) "fubar:bar"
string(5) "test3"
string(9) "fubar:bar"
string(5) "test4"
string(9) "fubar:bar"
string(5) "test3"
string(9) "fubar:bar"
string(5) "test4"
string(9) "fubar:bar"
string(5) "test3"
string(9) "fubar:bar"
string(5) "test4""#);
}

/// php-src `ext/dom/tests/dom003.phpt`: Test 3: Exception Test
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_dom003() {
    if skip_without_xml_native("test_dom003") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new domdocument;
$dom->load(__DIR__."/book.xml");
$rootNode = $dom->documentElement;
print "--- Catch exception with try/catch\n";
try {
    $rootNode->appendChild($rootNode);
} catch (domexception $e) {
    ob_start();
    var_dump($e);
    $contents = ob_get_contents();
    ob_end_clean();
    echo preg_replace('/object\(DOMElement\).+\{.*?\}/s', 'DOMElement', $contents);
}
print "--- Don't catch exception with try/catch\n";
$rootNode->appendChild($rootNode);


"#);
    assert_format(&out, r#"--- Catch exception with try/catch
object(DOMException)#%d (%d) {
  ["message":protected]=>
  string(23) "Hierarchy Request Error"
  ["string":"Exception":private]=>
  string(0) ""
  ["code"]=>
  int(3)
  ["file":protected]=>
  string(%d) "%stest.php"
  ["line":protected]=>
  int(8)
  ["trace":"Exception":private]=>
  array(1) {
    [0]=>
    array(6) {
      ["file"]=>
      string(%d) "%stest.php"
      ["line"]=>
      int(8)
      ["function"]=>
      string(11) "appendChild"
      ["class"]=>
      string(7) "DOMNode"
      ["type"]=>
      string(2) "->"
      ["args"]=>
      array(1) {
        [0]=>
        DOMElement
      }
    }
  }
  ["previous":"Exception":private]=>
  NULL
}
--- Don't catch exception with try/catch

Fatal error: Uncaught DOMException: Hierarchy Request Error in %stest.php:%d
Stack trace:
#0 %stest.php(%d): DOMNode->appendChild(Object(DOMElement))
#1 {main}
  thrown in %stest.php on line %d"#);
}

/// php-src `ext/dom/tests/dom004.phpt`: Test 4: Streams Test
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_dom004() {
    if skip_without_xml_native("test_dom004") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = new domdocument;
$dom->load("compress.zlib://" . str_replace("\\", "/", __DIR__) . "/book.xml.gz");
print $dom->saveXML();
"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<books>
 <book>
  <title>The Grapes of Wrath</title>
  <author>John Steinbeck</author>
 </book>
 <book>
  <title>The Pearl</title>
  <author>John Steinbeck</author>
 </book>
</books>"#);
}

/// php-src `ext/dom/tests/dom005.phpt`: Test 5: HTML Test
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::loadHTMLFile"#]
fn test_dom005() {
    if skip_without_xml_native("test_dom005") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = new domdocument;
$dom->loadHTMLFile(__DIR__."/test.html", LIBXML_NOBLANKS);
print  "--- save as XML\n";

print adjustDoctype($dom->saveXML());
print  "--- save as HTML\n";

print adjustDoctype($dom->saveHTML());

function adjustDoctype($xml) {
    return str_replace(array("DOCTYPE HTML",'<p>','</p>'),array("DOCTYPE html",'',''),$xml);
}
"#);
    assert_exact(&out, r#"--- save as XML
<?xml version="1.0" standalone="yes"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD HTML 4.0 Transitional//EN" "http://www.w3.org/TR/REC-html40/loose.dtd">
<html><head><title>Hello world</title></head><body>
This is a not well-formed<br/>
html files with undeclared entities&#xA0;
</body></html>
--- save as HTML
<!DOCTYPE html PUBLIC "-//W3C//DTD HTML 4.0 Transitional//EN" "http://www.w3.org/TR/REC-html40/loose.dtd">
<html><head><title>Hello world</title></head><body>
This is a not well-formed<br>
html files with undeclared entities&nbsp;
</body></html>"#);
}

/// php-src `ext/dom/tests/dom006.phpt`: Test 6: Extends Test
#[test]
fn test_dom006() {
    if skip_without_xml_native("test_dom006") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

Class books extends domDocument {
    function addBook($title, $author) {
        $titleElement = $this->createElement("title");
        $titleElement->appendChild($this->createTextNode($title));
        $authorElement = $this->createElement("author");
        $authorElement->appendChild($this->createTextNode($author));

        $bookElement = $this->createElement("book");

        $bookElement->appendChild($titleElement);
        $bookElement->appendChild($authorElement);
        $this->documentElement->appendChild($bookElement);
    }

}

$dom = new books;

$dom->load(__DIR__."/book.xml");
$dom->addBook("PHP de Luxe", "Richard Samar, Christian Stocker");
print $dom->saveXML();
"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<books>
 <book>
  <title>The Grapes of Wrath</title>
  <author>John Steinbeck</author>
 </book>
 <book>
  <title>The Pearl</title>
  <author>John Steinbeck</author>
 </book>
<book><title>PHP de Luxe</title><author>Richard Samar, Christian Stocker</author></book></books>"#);
}

/// php-src `ext/dom/tests/dom007.phpt`: Test 7: DTD tests
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::doctype"#]
fn test_dom007() {
    if skip_without_xml_native("test_dom007") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$xml = <<< EOXML
<?xml version="1.0" encoding="ISO-8859-1"?>
<!DOCTYPE courses [
<!ELEMENT courses (course+)>
<!ELEMENT course (title, description, temp*)>
<!ATTLIST course cid ID #REQUIRED>
<!ELEMENT title (#PCDATA)>
<!ELEMENT description (#PCDATA)>
<!ELEMENT temp (#PCDATA)>
<!ATTLIST temp vid ID #REQUIRED>
<!ENTITY test 'http://www.hpl.hp.com/semweb/2003/query_tester#'>
<!ENTITY rdf  'http://www.w3.org/1999/02/22-rdf-syntax-ns#'>
<!NOTATION GIF PUBLIC "-" "image/gif">
<!ENTITY myimage PUBLIC "-" "mypicture.gif" NDATA GIF>
]>
<courses>
   <course cid="c1">
      <title>Basic Languages</title>
      <description>Introduction to Languages</description>
   </course>
   <course cid="c6">
      <title>French I</title>
      <description>Introduction to French</description>
      <temp vid="c7">
      </temp>
   </course>
</courses>
EOXML;

$dom = new DOMDocument();
$dom->loadXML($xml);

$dtd = $dom->doctype;

/* Notation Tests */
$nots = $dtd->notations;

$length = $nots->length;
echo "Length: ".$length."\n";

foreach ($nots AS $key=>$node) {
    echo "Key $key: ".$node->nodeName." (".$node->systemId.") (".$node->publicId.")\n";
}
print "\n";
for($x=0; $x < $length; $x++) {
    echo "Index $x: ".$nots->item($x)->nodeName." (".$nots->item($x)->systemId.") (".$nots->item($x)->publicId.")\n";
}

echo "\n";
$node = $nots->getNamedItem('xxx');
var_dump($node);

echo "\n";
/* Entity Decl Tests */
$ents = $dtd->entities;
$length = $ents->length;
echo "Length: ".$length."\n";

$xkeys = array();
foreach ($ents AS $key=>$node) {
    $xkeys[] = "Key: $key Name: ".$node->nodeName."\n";
}
sort($xkeys);  // fix inconsistent output ordering (bug #61810)
foreach ($xkeys as $key => $node) {
    echo $node;
}
echo "\n";

$xkeys = array();
for($x=0; $x < $length; $x++) {
    $xkeys[] = "Index: ".$ents->item($x)->nodeName."\n";
}
sort($xkeys);  // fix inconsistent output ordering (bug #61810)
foreach ($xkeys as $key => $node) {
    echo $node;
}

echo "\n";
$node = $ents->item(3);
var_dump($node);
$node = $ents->getNamedItem('xxx');
var_dump($node);
"#);
    assert_exact(&out, r#"Length: 1
Key GIF: GIF (image/gif) (-)

Index 0: GIF (image/gif) (-)

NULL

Length: 3
Key: myimage Name: myimage
Key: rdf Name: rdf
Key: test Name: test

Index: myimage
Index: rdf
Index: test

NULL
NULL"#);
}
