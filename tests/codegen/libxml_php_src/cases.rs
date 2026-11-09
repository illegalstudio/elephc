//! Purpose:
//! PHP 8.5.8 `ext/libxml` php-src tests for the `libxml_*` functions and libxml2 global state, as Elephc codegen tests.
//!
//! Called from:
//! - `cargo test` through `tests/codegen/libxml_php_src/mod.rs`.
//!
//! Key details:
//! - Each test keeps the php-src program and expected output verbatim; its doc
//!   comment names the PHPT it was migrated from.
//! - A test Elephc does not pass yet is `#[ignore]`d with the missing behaviour, and
//!   php-src's own environment skips keep their `SKIPIF` message.

use super::*;

/// php-src `ext/libxml/tests/001.phpt`: libxml_use_internal_errors()
#[test]
fn test_001() {
    if skip_without_xml_native("test_001") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

var_dump(libxml_use_internal_errors(false));
var_dump(libxml_use_internal_errors(true));
var_dump(libxml_use_internal_errors());

var_dump(libxml_get_errors());
var_dump(libxml_get_last_error());

var_dump(libxml_clear_errors());

echo "Done\n";
"#);
    assert_exact(&out, r#"bool(false)
bool(false)
bool(true)
array(0) {
}
bool(false)
NULL
Done"#);
}

/// php-src `ext/libxml/tests/002.phpt`: libxml_get_errors()
#[test]
fn test_002() {
    if skip_without_xml_native("test_002") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

var_dump(libxml_use_internal_errors(true));

$xmlstr = <<< XML
<?xml version='1.0' standalone='yes'?>
    <movies>
        <movie>
            <titles>PHP: Behind the Parser</title>
        </movie>
    </movies>
XML;

$doc = simplexml_load_string($xmlstr);
$xml = explode("\n", $xmlstr);

if (!$doc) {
    $errors = libxml_get_errors();

    foreach ($errors as $error) {
        echo display_xml_error($error, $xml);
    }

    var_dump(libxml_get_last_error());
}


function display_xml_error($error, $xml)
{
    $return  = $xml[$error->line - 1] . "\n";
    $return .= str_repeat('-', $error->column) . "^\n";

    switch ($error->level) {
        case LIBXML_ERR_WARNING:
            $return .= "Warning $error->code: ";
            break;
        case LIBXML_ERR_ERROR:
            $return .= "Error $error->code: ";
            break;
        case LIBXML_ERR_FATAL:
            $return .= "Fatal Error $error->code: ";
            break;
    }

    $return .= trim($error->message) . "\n  Line: $error->line" . "\n  Column: $error->column";

    if ($error->file) {
        $return .= "\n  File: $error->file";
    }

    return "$return\n\n--------------------------------------------\n\n";
}


echo "Done\n";
"#);
    assert_format(&out, r#"bool(false)
            <titles>PHP: Behind the Parser</title>
%s
Fatal Error 76: Opening and ending tag mismatch: titles line 4 and title
  Line: 4
  Column: %d

--------------------------------------------

object(LibXMLError)#%d (6) {
  ["level"]=>
  int(3)
  ["code"]=>
  int(76)
  ["column"]=>
  int(%d)
  ["message"]=>
  string(57) "Opening and ending tag mismatch: titles line 4 and title
"
  ["file"]=>
  string(0) ""
  ["line"]=>
  int(4)
}
Done"#);
}

/// php-src `ext/libxml/tests/003.phpt`: libxml_use_internal_errors() memory leaks
#[test]
fn test_003() {
    if skip_without_xml_native("test_003") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
var_dump(libxml_use_internal_errors(true));

$xmlstr = <<< XML
<?xml version='1.0' standalone='yes'?>
    <movies>
        <movie>
            <titles>PHP: Behind the Parser</title>
        </movie>
    </movies>
XML;

simplexml_load_string($xmlstr);

// test memleaks here
var_dump(libxml_use_internal_errors(false));

echo "Done\n";
"#);
    assert_exact(&out, r#"bool(false)
bool(true)
Done"#);
}

/// php-src `ext/libxml/tests/004.phpt`: libxml_set_streams_context()
#[test]
#[ignore = r#"DOM port pending: Undefined function: libxml_set_streams_context"#]
fn test_004() {
    if skip_without_xml_native("test_004") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$ctxs = array(
    NULL,
    'bogus',
    123,
    new stdclass,
    array('a'),
    stream_context_create(),
);

foreach ($ctxs as $ctx) {
    try {
        var_dump(libxml_set_streams_context($ctx));
    } catch (TypeError $e) {
        echo $e->getMessage(), "\n";
    }
    $dom = new DOMDocument();
    var_dump($dom->load(__DIR__.'/test.xml'));
}

echo "Done\n";

"#);
    assert_exact(&out, r#"libxml_set_streams_context(): Argument #1 ($context) must be of type resource, null given
bool(true)
libxml_set_streams_context(): Argument #1 ($context) must be of type resource, string given
bool(true)
libxml_set_streams_context(): Argument #1 ($context) must be of type resource, int given
bool(true)
libxml_set_streams_context(): Argument #1 ($context) must be of type resource, stdClass given
bool(true)
libxml_set_streams_context(): Argument #1 ($context) must be of type resource, array given
bool(true)
NULL
bool(true)
Done"#);
}

/// php-src `ext/libxml/tests/bug42112.phpt`: Bug #42112 (deleting a node produces memory corruption)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::getElementById"#]
fn test_bug42112() {
    if skip_without_xml_native("test_bug42112") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$xml = <<<EOXML
<root><child xml:id="id1">baz</child></root>
EOXML;

function remove_node($doc) {
    $node = $doc->getElementById( 'id1' );
    print 'Deleting Node: '.$node->nodeName."\n";
    $node->parentNode->removeChild( $node );
}

$doc = new DOMDocument();
$doc->loadXML($xml);

remove_node($doc);

$node = $doc->getElementById( 'id1' );
if ($node) {
    print 'Found Node: '.$node->nodeName."\n";
}
$root = $doc->documentElement;
print 'Root Node: '.$root->nodeName."\n";
"#);
    assert_exact(&out, r#"Deleting Node: child
Root Node: root"#);
}

/// php-src `ext/libxml/tests/bug51903.phpt`: Bug #51903 (simplexml_load_file() doesn't use HTTP headers)
#[test]
#[ignore = r#"needs php-src's ext/standard/tests/http/server.inc test HTTP server"#]
fn test_bug51903() {
    if skip_without_xml_native("test_bug51903") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
require "./ext/standard/tests/http/server.inc";
$responses = [
    "data://text/plain,HTTP/1.1 200 OK\r\n"
    . "Content-Type: text/xml; charset=ISO-8859-1\r\n\r\n"
    . "<?xml version=\"1.0\"?>\n"
    . "<root>\xE4\xF6\xFC</root>\n",
    "data://text/plain,HTTP/1.1 200 OK\r\n"
    . "Content-Type: text/xml; charset=ISO-8859-1; foo=bar\r\n\r\n"
    . "<?xml version=\"1.0\"?>\n"
    . "<root>\xE4\xF6\xFC</root>\n",
    "data://text/plain,HTTP/1.1 200 OK\r\n"
    . "Content-Type: text/xml; charset=\"ISO-8859-1\" ; foo=bar\r\n\r\n"
    . "<?xml version=\"1.0\"?>\n"
    . "<root>\xE4\xF6\xFC</root>\n",
];
['pid' => $pid, 'uri' => $uri] = http_server($responses);

for ($i = 0; $i < count($responses); $i++) {
    $sxe = simplexml_load_file($uri);
    echo "$sxe\n";
}

http_server_kill($pid);
"#);
    assert_exact(&out, r#"äöü
äöü
äöü"#);
}

/// php-src `ext/libxml/tests/bug54138.phpt`: Bug #54138 - DOMNode::getLineNo() doesn't return line number higher than 65535
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMNode::getLineNo"#]
fn test_bug54138() {
    if skip_without_xml_native("test_bug54138") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$foos = str_repeat('<foo/>' . PHP_EOL, 65535);
$xml = <<<XML
<?xml version="1.0" encoding="UTF-8"?>
<root>
$foos
<bar/>
</root>
XML;
$dom = new DOMDocument();
$dom->loadXML($xml, LIBXML_BIGLINES);
var_dump($dom->getElementsByTagName('bar')->item(0)->getLineNo());
"#);
    assert_exact(&out, r#"int(65540)"#);
}

/// php-src `ext/libxml/tests/bug54138_2.phpt`: Bug #54138 - DOMNode::getLineNo() doesn't return line number higher than 65535
#[test]
#[ignore = r#"php-src SKIPIF: skip this test is for LIBXML_BIGLINES being undefined"#]
fn test_bug54138_2() {
    if skip_without_xml_native("test_bug54138_2") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
define('LIBXML_BIGLINES', 1<<22);
$foos = str_repeat('<foo/>' . PHP_EOL, 65535);
$xml = <<<XML
<?xml version="1.0" encoding="UTF-8"?>
<root>
$foos
<bar/>
</root>
XML;
$dom = new DOMDocument();
$dom->loadXML($xml, LIBXML_BIGLINES);
var_dump($dom->getElementsByTagName('bar')->item(0)->getLineNo());
"#);
    assert_exact(&out, r#"int(65540)"#);
}

/// php-src `ext/libxml/tests/bug54440.phpt`: Bug #54440: libxml extension ignores default context
#[test]
#[ignore = r#"DOM port pending: Undefined property: TestWrapper::context"#]
fn test_bug54440() {
    if skip_without_xml_native("test_bug54440") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

class TestWrapper {

function stream_open($path, $mode, $options, &$opened_path)
{
    if ($this->context)
        print_r(stream_context_get_options($this->context));
    return false;
}

function url_stat($path, $flags)
{
return array();
}

}

stream_wrapper_register("test", "TestWrapper")
    or die("Failed to register protocol");

$ctx1 = stream_context_create(array('test'=>array('test'=>'test 1')));
$ctx2 = stream_context_create(array('test'=>array('test'=>'test 2')));

stream_context_set_default(stream_context_get_options($ctx1));
@simplexml_load_file('test://sdfsdf');

libxml_set_streams_context($ctx2);
@simplexml_load_file('test://sdfsdf');
"#);
    assert_exact(&out, r#"Array
(
    [test] => Array
        (
            [test] => test 1
        )

)
Array
(
    [test] => Array
        (
            [test] => test 2
        )

)"#);
}

/// php-src `ext/libxml/tests/bug61367-read.phpt`: Bug #61367: open_basedir bypass in libxml RSHUTDOWN: read test
#[test]
#[ignore = r#"php-src SKIPIF: skip For libxml2 < 2.9.12 only"#]
fn test_bug61367_read() {
    if skip_without_xml_native("test_bug61367_read") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php ini_set('open_basedir', '.');
/*
 * Note: Using error_reporting=E_ALL & ~E_NOTICE to suppress "Trying to get property of non-object" notices.
 */
class StreamExploiter {
    public $context;
    public function stream_close (  ) {
        $doc = new DOMDocument;
        $doc->resolveExternals = true;
        $doc->substituteEntities = true;
        $dir = htmlspecialchars(dirname(getcwd()));
        $dir = str_replace('\\', '/', $dir); // fix for windows
        $doc->loadXML( <<<XML
<!DOCTYPE doc [
    <!ENTITY file SYSTEM "file:///$dir/bad">
]>
<doc>&file;</doc>
XML
        );
        print $doc->documentElement->firstChild->nodeValue;
    }

    public function stream_open (  $path ,  $mode ,  $options ,  &$opened_path ) {
        return true;
    }
}

var_dump(mkdir('test_bug_61367-read'));
var_dump(mkdir('test_bug_61367-read/base'));
var_dump(file_put_contents('test_bug_61367-read/bad', 'blah'));
var_dump(chdir('test_bug_61367-read/base'));

stream_wrapper_register( 'exploit', 'StreamExploiter' );
$s = fopen( 'exploit://', 'r' );

"#);
    assert_format(&out, r#"bool(true)
bool(true)
int(4)
bool(true)

Warning: DOMDocument::loadXML(): I/O warning : failed to load external entity "file:///%s/test_bug_61367-read/bad" in %s on line %d

Warning: DOMDocument::loadXML(): Failure to process entity file in Entity, line: 4 in %s on line %d

Warning: DOMDocument::loadXML(): Entity 'file' not defined in Entity, line: 4 in %s on line %d

Warning: Attempt to read property "firstChild" on null in %s on line %d

Warning: Attempt to read property "nodeValue" on null in %s on line %d"#);
}

/// php-src `ext/libxml/tests/bug61367-read_2.phpt`: Bug #61367: open_basedir bypass in libxml RSHUTDOWN: read test
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::resolveExternals"#]
fn test_bug61367_read_2() {
    if skip_without_xml_native("test_bug61367_read_2") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php ini_set('open_basedir', '.');
/*
 * Note: Using error_reporting=E_ALL & ~E_NOTICE to suppress "Trying to get property of non-object" notices.
 */
class StreamExploiter {
    public $context;

    public function stream_close (  ) {
        $doc = new DOMDocument;
        $doc->resolveExternals = true;
        $doc->substituteEntities = true;
        $dir = htmlspecialchars(dirname(getcwd()));
        $dir = str_replace('\\', '/', $dir); // fix for windows
        $doc->loadXML( <<<XML
<!DOCTYPE doc [
    <!ENTITY file SYSTEM "file:///$dir/bad">
]>
<doc>&file;</doc>
XML
        );
        print $doc->documentElement->firstChild->nodeValue;
    }

    public function stream_open (  $path ,  $mode ,  $options ,  &$opened_path ) {
        return true;
    }
}

var_dump(mkdir('test_bug_61367-read'));
var_dump(mkdir('test_bug_61367-read/base'));
var_dump(file_put_contents('test_bug_61367-read/bad', 'blah'));
var_dump(chdir('test_bug_61367-read/base'));

stream_wrapper_register( 'exploit', 'StreamExploiter' );
$s = fopen( 'exploit://', 'r' );

"#);
    assert_format(&out, r#"bool(true)
bool(true)
int(4)
bool(true)

%s: DOMDocument::loadXML(): %s

Warning: Attempt to read property "nodeValue" on null in %s on line %d"#);
}

/// php-src `ext/libxml/tests/bug61367-write.phpt`: Bug #61367: open_basedir bypass in libxml RSHUTDOWN: write test
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createTextNode"#]
fn test_bug61367_write() {
    if skip_without_xml_native("test_bug61367_write") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php ini_set('open_basedir', '.');

class StreamExploiter {
    public $context;

    public function stream_close (  ) {
        $doc = new DOMDocument;
        $doc->appendChild($doc->createTextNode('hello'));
        var_dump($doc->save(dirname(getcwd()) . '/bad'));
    }

    public function stream_open (  $path ,  $mode ,  $options ,  &$opened_path ) {
        return true;
    }
}

var_dump(mkdir('test_bug_61367-write'));
var_dump(mkdir('test_bug_61367-write/base'));
var_dump(file_put_contents('test_bug_61367-write/bad', 'blah'));
var_dump(chdir('test_bug_61367-write/base'));

stream_wrapper_register( 'exploit', 'StreamExploiter' );
$s = fopen( 'exploit://', 'r' );

"#);
    assert_format(&out, r#"bool(true)
bool(true)
int(4)
bool(true)

Warning: DOMDocument::save(): open_basedir restriction in effect. File(%s) is not within the allowed path(s): (.) in %s on line %d

Warning: DOMDocument::save(%s): Failed to open stream: Operation not permitted in %s on line %d
bool(false)"#);
}

/// php-src `ext/libxml/tests/bug63389.phpt`: Bug #63389 (Missing context check on libxml_set_streams_context() causes memleak)
#[test]
#[ignore = r#"DOM port pending: Undefined function: libxml_set_streams_context"#]
fn test_bug63389() {
    if skip_without_xml_native("test_bug63389") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
try {
    libxml_set_streams_context("a");
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
$fp = fopen("php://input", "r");
try {
    libxml_set_streams_context($fp);
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
echo "okey";
"#);
    assert_exact(&out, r#"libxml_set_streams_context(): Argument #1 ($context) must be of type resource, string given
libxml_set_streams_context(): supplied resource is not a valid Stream-Context resource
okey"#);
}

/// php-src `ext/libxml/tests/bug69753-mb.phpt`: Bug #69753 - libXMLError::file contains invalid URI
#[test]
#[ignore = r#"php-src SKIPIF: skip this test is for Windows platforms only"#]
fn test_bug69753_mb() {
    if skip_without_xml_native("test_bug69753_mb") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
libxml_use_internal_errors(true);
$doc = new DomDocument();
$doc->load(__DIR__ . DIRECTORY_SEPARATOR . 'bug69753私はガラスを食べられます.xml');
$error = libxml_get_last_error();
var_dump($error->file);
"#);
    assert_format(&out, r#"string(%d) "%s\ext\libxml\tests\bug69753私はガラスを食べられます.xml""#);
}

/// php-src `ext/libxml/tests/bug69753.phpt`: Bug #69753 - libXMLError::file contains invalid URI
#[test]
#[ignore = r#"php-src SKIPIF: skip this test is for Windows platforms only"#]
fn test_bug69753() {
    if skip_without_xml_native("test_bug69753") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
libxml_use_internal_errors(true);
$doc = new DomDocument();
$doc->load(__DIR__ . DIRECTORY_SEPARATOR . 'bug69753.xml');
$error = libxml_get_last_error();
var_dump($error->file);
"#);
    assert_format(&out, r#"string(%d) "%s\ext\libxml\tests\bug69753.xml""#);
}

/// php-src `ext/libxml/tests/bug76777.phpt`: Bug #76777 (first parameter of libxml_set_external_entity_loader callback undefined)
#[test]
#[ignore = r#"DOM port pending: Undefined function: libxml_set_external_entity_loader"#]
fn test_bug76777() {
    if skip_without_xml_native("test_bug76777") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$xml=<<<EOF
<?xml version="1.0"?>
<test/>
EOF;

$xsd=<<<EOF
<?xml version="1.0"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:include schemaLocation="nonexistent.xsd"/>
  <xs:element name="test"/>
</xs:schema>
EOF;

libxml_set_external_entity_loader(function($p,$s,$c) {
    var_dump($p,$s,$c);
    die();
});

$dom=new DOMDocument($xml);
$dom->schemaValidateSource($xsd);
"#);
    assert_exact(&out, r#"NULL
string(15) "nonexistent.xsd"
array(4) {
  ["directory"]=>
  NULL
  ["intSubName"]=>
  NULL
  ["extSubURI"]=>
  NULL
  ["extSubSystem"]=>
  NULL
}"#);
}

/// php-src `ext/libxml/tests/bug79191.phpt`: Bug #79191 (Error in SoapClient ctor disables DOMDocument::save())
#[test]
#[ignore = r#"requires the unsupported ext/soap extension"#]
fn test_bug79191() {
    if skip_without_xml_native("test_bug79191") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
try {
    new \SoapClient('does-not-exist.wsdl');
} catch (Throwable $t) {
}

$dom = new DOMDocument;
$dom->loadxml('<?xml version="1.0" ?><root />');
var_dump($dom->save(__DIR__ . '/bug79191.xml'));
"#);
    assert_format(&out, r#"int(%d)"#);
}

/// php-src `ext/libxml/tests/image/getimagesize.phpt`: getimagesize() with svg input
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_getimagesize() {
    if skip_without_xml_native("test_getimagesize") {
        return;
    }
    let out = run_case(r#"image"#, &[], r#"<?php

$inputs = [
    "<?xml version=\"1.0\"?>",
    "svg width=\"1\" height=\"1\"",
    <<<XML
<?xml version="1.0" standalone="yes"?>
<!-- foo <svg width="1" height="1"/> -->
<x:svg width='4cm' height="8cm" xmlns:x="http://www.w3.org/2000/svg">
XML,
    "<SVG width='1' height=\"1\"/>",
    "<SVG width='1px' height=\"1\"/>",
    "<SVG width='1' height=\"1mm\"/>",
    "<SVG width='1' height=\"cm\"/>",
    "<SVG width='' height=\"\"/>",
    "<SVG width=\"é\" height='1'/>",
    "<foo width='1' height=\"1\"/>",
    "<foo foo='1' height=\"1\"/>",
];

foreach ($inputs as $input) {
    var_dump(getimagesizefromstring($input));
}

"#);
    assert_format(&out, r#"bool(false)
bool(false)
array(6) {
  [0]=>
  int(4)
  [1]=>
  int(8)
  [2]=>
  int(%d)
  ["mime"]=>
  string(13) "image/svg+xml"
  ["width_unit"]=>
  string(2) "cm"
  ["height_unit"]=>
  string(2) "cm"
}
array(7) {
  [0]=>
  int(1)
  [1]=>
  int(1)
  [2]=>
  int(%d)
  [3]=>
  string(20) "width="1" height="1""
  ["mime"]=>
  string(13) "image/svg+xml"
  ["width_unit"]=>
  string(2) "px"
  ["height_unit"]=>
  string(2) "px"
}
array(7) {
  [0]=>
  int(1)
  [1]=>
  int(1)
  [2]=>
  int(%d)
  [3]=>
  string(20) "width="1" height="1""
  ["mime"]=>
  string(13) "image/svg+xml"
  ["width_unit"]=>
  string(2) "px"
  ["height_unit"]=>
  string(2) "px"
}
array(6) {
  [0]=>
  int(1)
  [1]=>
  int(1)
  [2]=>
  int(%d)
  ["mime"]=>
  string(13) "image/svg+xml"
  ["width_unit"]=>
  string(2) "px"
  ["height_unit"]=>
  string(2) "mm"
}
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)"#);
}

/// php-src `ext/libxml/tests/image/imagetype_svg.phpt`: imagetype API with svg extension
#[test]
#[ignore = r#"DOM port pending: Undefined constant: IMAGETYPE_SVG"#]
fn test_imagetype_svg() {
    if skip_without_xml_native("test_imagetype_svg") {
        return;
    }
    let out = run_case(r#"image"#, &[], r#"<?php

var_dump(IMAGETYPE_COUNT > IMAGETYPE_SVG);
var_dump(image_type_to_extension(IMAGETYPE_SVG));
var_dump(image_type_to_mime_type(IMAGETYPE_SVG));

"#);
    assert_exact(&out, r#"bool(true)
string(4) ".svg"
string(13) "image/svg+xml""#);
}

/// php-src `ext/libxml/tests/libxml_disable_entity_loader.phpt`: libxml_disable_entity_loader()
#[test]
#[ignore = r#"php-src SKIPIF: skip For libxml2 < 2.9.12 only"#]
fn test_libxml_disable_entity_loader() {
    if skip_without_xml_native("test_libxml_disable_entity_loader") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$xml = <<<EOT
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE test [<!ENTITY xxe SYSTEM "XXE_URI">]>
<foo>&xxe;</foo>
EOT;

$dir = str_replace('\\', '/', __DIR__);
$xml = str_replace('XXE_URI', $dir . '/libxml_disable_entity_loader_payload.txt', $xml);

function parseXML($xml) {
  $doc = new DOMDocument();
  $doc->resolveExternals = true;
  $doc->substituteEntities = true;
  $doc->validateOnParse = false;
  $doc->loadXML($xml, 0);
  return $doc->saveXML();
}

var_dump(strpos(parseXML($xml), 'SECRET_DATA') !== false);
var_dump(libxml_disable_entity_loader(true));
var_dump(strpos(parseXML($xml), 'SECRET_DATA') === false);

echo "Done\n";
"#);
    assert_format(&out, r#"bool(true)

Deprecated: Function libxml_disable_entity_loader() is deprecated since 8.0, as external entity loading is disabled by default in %s on line %d
bool(false)

Warning: DOMDocument::loadXML(): I/O warning : failed to load external entity "%s" in %s on line %d

Warning: DOMDocument::loadXML(): Failure to process entity xxe in Entity, line: %d in %s on line %d

Warning: DOMDocument::loadXML(): Entity 'xxe' not defined in Entity, line: %d in %s on line %d
bool(true)
Done"#);
}

/// php-src `ext/libxml/tests/libxml_disable_entity_loader_2.phpt`: libxml_disable_entity_loader()
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::resolveExternals"#]
fn test_libxml_disable_entity_loader_2() {
    if skip_without_xml_native("test_libxml_disable_entity_loader_2") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$xml = <<<EOT
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE test [<!ENTITY xxe SYSTEM "XXE_URI">]>
<foo>&xxe;</foo>
EOT;

$dir = str_replace('\\', '/', __DIR__);
$xml = str_replace('XXE_URI', $dir . '/libxml_disable_entity_loader_payload.txt', $xml);

function parseXML($xml) {
  $doc = new DOMDocument();
  $doc->resolveExternals = true;
  $doc->substituteEntities = true;
  $doc->validateOnParse = false;
  $doc->loadXML($xml, 0);
  return $doc->saveXML();
}

var_dump(strpos(parseXML($xml), 'SECRET_DATA') !== false);
var_dump(libxml_disable_entity_loader(true));
var_dump(strpos(parseXML($xml), 'SECRET_DATA') === false);

echo "Done\n";
"#);
    assert_format(&out, r#"bool(true)

Deprecated: Function libxml_disable_entity_loader() is deprecated since 8.0, as external entity loading is disabled by default in %s on line %d
bool(false)

%s: DOMDocument::loadXML(): %s
bool(true)
Done"#);
}

/// php-src `ext/libxml/tests/libxml_entity_loading_disabled_by_default.phpt`: libxml_disable_entity_loader()
#[test]
fn test_libxml_entity_loading_disabled_by_default() {
    if skip_without_xml_native("test_libxml_entity_loading_disabled_by_default") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$xml = <<<EOT
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE test [<!ENTITY xxe SYSTEM "XXE_URI">]>
<foo>&xxe;</foo>
EOT;

$dir = str_replace('\\', '/', __DIR__);
$xml = str_replace('XXE_URI', $dir . '/libxml_disable_entity_loader_payload.txt', $xml);

function parseXML1($xml) {
  $doc = new DOMDocument();
  $doc->loadXML($xml, 0);
  return $doc->saveXML();
}

function parseXML2($xml) {
  return simplexml_load_string($xml);
}

function parseXML3($xml) {
  $p = xml_parser_create();
  xml_parse_into_struct($p, $xml, $vals, $index);
  return var_export($vals, true);
}

function parseXML4($xml) {
  // This is the only time we enable external entity loading.
  return simplexml_load_string($xml, 'SimpleXMLElement', LIBXML_NOENT);
}

var_dump(strpos(parseXML1($xml), 'SECRET_DATA') === false);
var_dump(strpos(parseXML2($xml), 'SECRET_DATA') === false);
var_dump(strpos(parseXML3($xml), 'SECRET_DATA') === false);
var_dump(strpos(parseXML4($xml), 'SECRET_DATA') === false);

echo "Done\n";
"#);
    assert_exact(&out, r#"bool(true)
bool(true)
bool(true)
bool(false)
Done"#);
}

/// php-src `ext/libxml/tests/libxml_get_external_entity_loader.phpt`: libxml_get_external_entity_loader() returns current handler
#[test]
#[ignore = r#"DOM port pending: Undefined function: libxml_get_external_entity_loader"#]
fn test_libxml_get_external_entity_loader() {
    if skip_without_xml_native("test_libxml_get_external_entity_loader") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

class Handler {
	private $name;

	public function __construct($name) {
		$this->name = $name;
	}

	public function handle($public, $system, $context) {
		return null;
	}

	public function __toString() {
		return "Handler#{$this->name}";
	}
}

var_dump(libxml_get_external_entity_loader());
libxml_set_external_entity_loader([new Handler('A'), 'handle']);
print libxml_get_external_entity_loader()[0] . "\n";
libxml_set_external_entity_loader([new Handler('B'), 'handle']);
print libxml_get_external_entity_loader()[0] . "\n";
libxml_set_external_entity_loader(null);
var_dump(libxml_get_external_entity_loader());

"#);
    assert_exact(&out, r#"NULL
Handler#A
Handler#B
NULL"#);
}

/// php-src `ext/libxml/tests/libxml_get_external_entity_loader_error_callback_name.phpt`: libxml_get_external_entity_loader() display correct callable name
#[test]
#[ignore = r#"DOM port pending: Undefined function: proc_open"#]
fn test_libxml_get_external_entity_loader_error_callback_name() {
    if skip_without_xml_native("test_libxml_get_external_entity_loader_error_callback_name") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$xml = <<<XML
<!DOCTYPE foo PUBLIC "-//FOO/BAR" "http://example.com/foobar">
<foo>bar</foo>
XML;

$dtd = <<<DTD
<!ELEMENT foo (#PCDATA)>
DTD;

class Handler {
	public function handle($public, $system, $context) {
        $descriptors = [["pty"], ["pty"], ["pty"], ["pipe", "w"]];
        $pipes = [];
        return proc_open('echo "foo";', $descriptors, $pipes);
	}
}

$o = new Handler();
libxml_set_external_entity_loader([$o, 'handle']);
$dd = new DOMDocument;
$r = $dd->loadXML($xml);

try {
    var_dump($dd->validate());
} catch (\Throwable $e) {
    var_dump($e->getMessage());
}

"#);
    assert_exact(&out, r#"string(122) "DOMDocument::validate(): The user entity loader callback "Handler::handle" has returned a resource, but it is not a stream""#);
}

/// php-src `ext/libxml/tests/libxml_set_external_entity_loader_basic.phpt`: libxml_set_external_entity_loader() basic test
#[test]
#[ignore = r#"DOM port pending: Undefined function: libxml_set_external_entity_loader"#]
fn test_libxml_set_external_entity_loader_basic() {
    if skip_without_xml_native("test_libxml_set_external_entity_loader_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$xml = <<<XML
<!DOCTYPE foo PUBLIC "-//FOO/BAR" "http://example.com/foobar">
<foo>bar</foo>
XML;

$dtd = <<<DTD
<!ELEMENT foo (#PCDATA)>
DTD;

libxml_set_external_entity_loader(
    function ($public, $system, $context) use($dtd){
        var_dump($public);
        var_dump($system);
        var_dump($context);
        $f = fopen("php://temp", "r+");
        fwrite($f, $dtd);
        rewind($f);
        return $f;
    }
);

$dd = new DOMDocument;
$r = $dd->loadXML($xml);
var_dump($dd->validate());

echo "Done.\n";

"#);
    assert_exact(&out, r#"string(10) "-//FOO/BAR"
string(25) "http://example.com/foobar"
array(4) {
  ["directory"]=>
  NULL
  ["intSubName"]=>
  NULL
  ["extSubURI"]=>
  NULL
  ["extSubSystem"]=>
  NULL
}
bool(true)
Done."#);
}

/// php-src `ext/libxml/tests/libxml_set_external_entity_loader_error1.phpt`: libxml_set_external_entity_loader() error: bad arguments
#[test]
#[ignore = r#"DOM port pending: Undefined function: libxml_set_external_entity_loader"#]
fn test_libxml_set_external_entity_loader_error1() {
    if skip_without_xml_native("test_libxml_set_external_entity_loader_error1") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$xml = <<<XML
<!DOCTYPE foo PUBLIC "-//FOO/BAR" "http://example.com/foobar">
<foo>bar</foo>
XML;

$dd = new DOMDocument;
$r = $dd->loadXML($xml);

var_dump(libxml_set_external_entity_loader(function($a, $b, $c, $d) {}));
try {
    var_dump($dd->validate());
} catch (Throwable $e) {
    echo "Exception: " . $e->getMessage() . "\n";
}

echo "Done.\n";
"#);
    assert_format(&out, r#"bool(true)
Exception: Too few arguments to function {closure:%s:%d}(), 3 passed and exactly 4 expected
Done."#);
}

/// php-src `ext/libxml/tests/libxml_set_external_entity_loader_error2.phpt`: libxml_set_external_entity_loader() with non-callable argument
#[test]
#[ignore = r#"DOM port pending: Undefined function: libxml_set_external_entity_loader"#]
fn test_libxml_set_external_entity_loader_error2() {
    if skip_without_xml_native("test_libxml_set_external_entity_loader_error2") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
try {
	libxml_set_external_entity_loader('nonexistent_function');
} catch (Throwable $e) {
	echo "Exception: " . $e->getMessage() . "\n";
}
"#);
    assert_exact(&out, r#"Exception: libxml_set_external_entity_loader(): Argument #1 ($resolver_function) must be a valid callback or null, function "nonexistent_function" not found or invalid function name"#);
}

/// php-src `ext/libxml/tests/libxml_set_external_entity_loader_trampoline.phpt`: libxml_set_external_entity_loader() trampoline callback
#[test]
#[ignore = r#"DOM port pending: Undefined function: libxml_set_external_entity_loader"#]
fn test_libxml_set_external_entity_loader_trampoline() {
    if skip_without_xml_native("test_libxml_set_external_entity_loader_trampoline") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$xml = <<<XML
<!DOCTYPE foo PUBLIC "-//FOO/BAR" "http://example.com/foobar">
<foo>bar</foo>
XML;

class TrampolineTest {
    const DTD = '<!ELEMENT foo (#PCDATA)>';

    public function __call(string $name, array $arguments) {
        echo 'Trampoline for ', $name, PHP_EOL;
        var_dump($arguments);
        $f = fopen("php://temp", "r+");
        fwrite($f, self::DTD);
        rewind($f);
        return $f;
    }
}
$o = new TrampolineTest();
$callback = [$o, 'entity_loader'];

libxml_set_external_entity_loader($callback);

$dd = new DOMDocument;
$r = $dd->loadXML($xml);
var_dump($dd->validate());

echo "Done.\n";

"#);
    assert_exact(&out, r#"Trampoline for entity_loader
array(3) {
  [0]=>
  string(10) "-//FOO/BAR"
  [1]=>
  string(25) "http://example.com/foobar"
  [2]=>
  array(4) {
    ["directory"]=>
    NULL
    ["intSubName"]=>
    NULL
    ["extSubURI"]=>
    NULL
    ["extSubSystem"]=>
    NULL
  }
}
bool(true)
Done."#);
}

/// php-src `ext/libxml/tests/libxml_set_external_entity_loader_variation1.phpt`: libxml_set_external_entity_loader() variation: resolve externals and entities
#[test]
#[ignore = r#"DOM port pending: Undefined function: libxml_set_external_entity_loader"#]
fn test_libxml_set_external_entity_loader_variation1() {
    if skip_without_xml_native("test_libxml_set_external_entity_loader_variation1") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
chdir(__DIR__);
$xml = <<<XML
<!DOCTYPE foo PUBLIC "-//FOO/BAR" "http://example.com/foobar">
<foo>bar&fooz;</foo>
XML;

$dtd = <<<DTD
<!ELEMENT foo (#PCDATA)>
<!ENTITY % fooentity PUBLIC
   "-//FOO/ENTITY"
   "fooentity.ent">
%fooentity;
DTD;

$entity = <<<ENT
<!ENTITY fooz "baz">
ENT;

libxml_set_external_entity_loader(
    function ($public, $system, $context) use($dtd,$entity){
        static $first = true;
        var_dump($public);
        var_dump($system);
        var_dump($context);
        $f = fopen("php://temp", "r+");
        fwrite($f, $first ? $dtd : $entity);
        $first = false;
        rewind($f);
        return $f;
    }
);

$dd = new DOMDocument;
$dd->substituteEntities = true;
$dd->resolveExternals = true;
$r = $dd->loadXML($xml);
var_dump($dd->validate());

echo "Done.\n";
"#);
    assert_format(&out, r#"string(10) "-//FOO/BAR"
string(25) "http://example.com/foobar"
array(4) {
  ["directory"]=>
  string(%d) "%s"
  ["intSubName"]=>
  string(3) "foo"
  ["extSubURI"]=>
  string(25) "http://example.com/foobar"
  ["extSubSystem"]=>
  string(10) "-//FOO/BAR"
}
string(13) "-//FOO/ENTITY"
string(32) "http://example.com/fooentity.ent"
array(4) {
  ["directory"]=>
  %r(NULL|string\(%d\) "%s")%r
  ["intSubName"]=>
  string(3) "foo"
  ["extSubURI"]=>
  string(25) "http://example.com/foobar"
  ["extSubSystem"]=>
  string(10) "-//FOO/BAR"
}
bool(true)
Done."#);
}

/// php-src `ext/libxml/tests/libxml_set_external_entity_loader_variation2.phpt`: libxml_set_external_entity_loader() variation: restore original handler; returning NULL
#[test]
#[ignore = r#"DOM port pending: Undefined function: libxml_set_external_entity_loader"#]
fn test_libxml_set_external_entity_loader_variation2() {
    if skip_without_xml_native("test_libxml_set_external_entity_loader_variation2") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
chdir(__DIR__);
$xml = <<<XML
<!DOCTYPE foo PUBLIC "-//FOO/BAR" "foobar.dtd">
<foo>bar</foo>
XML;

$dtd = <<<DTD
<!ELEMENT foo (#PCDATA)>
DTD;


libxml_set_external_entity_loader(
    function ($public, $system, $context) {
        var_dump($public,$system);
        return null;
    }
);

$dd = new DOMDocument;
$r = $dd->loadXML($xml);
var_dump($dd->validate());

libxml_set_external_entity_loader(NULL);
file_put_contents(__DIR__ . "/foobar.dtd", $dtd);
var_dump($dd->validate());

echo "Done.\n";
"#);
    assert_format(&out, r#"string(10) "-//FOO/BAR"
string(%d) "%sfoobar.dtd"

Warning: DOMDocument::validate(): Failed to load external entity "-//FOO/BAR" in %s on line %d

Warning: DOMDocument::validate(): Could not load the external subset "foobar.dtd" in %s on line %d
bool(false)
bool(true)
Done."#);
}

/// php-src `ext/libxml/tests/libxml_set_streams_context_overwrite.phpt`: Overwrite libxml_set_streams_context
#[test]
#[ignore = r#"DOM port pending: Undefined property: TestWrapper::context"#]
fn test_libxml_set_streams_context_overwrite() {
    if skip_without_xml_native("test_libxml_set_streams_context_overwrite") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

// Based on bug54440.phpt

class TestWrapper {
    function stream_open($path, $mode, $options, &$opened_path)
    {
        print_r(stream_context_get_options($this->context));
        return false;
    }

    function url_stat($path, $flags)
    {
        return array();
    }
}

stream_wrapper_register("test", "TestWrapper");

$ctx1 = stream_context_create(array('test'=>array('test'=>'test 1')));
$ctx2 = stream_context_create(array('test'=>array('test'=>'test 2')));

libxml_set_streams_context($ctx2);
@simplexml_load_file('test://sdfsdf');

libxml_set_streams_context($ctx1);
@simplexml_load_file('test://sdfsdf');
"#);
    assert_exact(&out, r#"Array
(
    [test] => Array
        (
            [test] => test 2
        )

)
Array
(
    [test] => Array
        (
            [test] => test 1
        )

)"#);
}

/// php-src `ext/libxml/tests/null_returned_by_resolver.phpt`: null returned by resolver function
#[test]
#[ignore = r#"DOM port pending: Undefined function: libxml_set_external_entity_loader"#]
fn test_null_returned_by_resolver() {
    if skip_without_xml_native("test_null_returned_by_resolver") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

libxml_set_external_entity_loader(function ($public_id, $system_id, $context) {
    var_dump($public_id, $system_id, $context);
    return null;
});

$doc = new DOMDocument();
$doc->loadHTMLFile("foobar");

"#);
    assert_format(&out, r#"NULL
string(6) "foobar"
array(4) {
  ["directory"]=>
  NULL
  ["intSubName"]=>
  NULL
  ["extSubURI"]=>
  NULL
  ["extSubSystem"]=>
  NULL
}

Warning: DOMDocument::loadHTMLFile(): Failed to load external entity because the resolver function returned null in %s on line %d"#);
}
