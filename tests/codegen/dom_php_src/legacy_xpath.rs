//! Purpose:
//! PHP 8.5.8 `ext/dom` php-src tests for `DOMXPath` queries, namespaces and PHP callbacks, as Elephc codegen tests.
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

/// php-src `ext/dom/tests/DOMXPath_callables.phpt`: registerPHPFunctions() with callables - legit cases
#[test]
#[ignore = r#"DOM port pending: Unknown class referenced during inheritance flattening: DOMXPath"#]
fn test_domxpath_callables() {
    if skip_without_xml_native("test_domxpath_callables") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

class MyClass {
    public static function dump(string $var) {
        var_dump($var);
    }
}

class MyDOMXPath extends DOMXPath {
    public function registerCycle() {
        $this->registerPhpFunctions(["cycle" => array($this, "dummy")]);
    }

    public function dummy(string $var) {
        echo "dummy: $var\n";
    }
}

$doc = new DOMDocument();
$doc->loadHTML('<a href="https://php.net">hello</a>');

echo "--- Legit cases: none ---\n";

$xpath = new DOMXPath($doc);
$xpath->registerNamespace("php", "http://php.net/xpath");
try {
    $xpath->evaluate("//a[php:function('var_dump', string(@href))]");
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}

echo "--- Legit cases: all ---\n";

$xpath->registerPHPFunctions(null);
$xpath->evaluate("//a[php:function('var_dump', string(@href))]");
$xpath->evaluate("//a[php:function('MyClass::dump', string(@href))]");

echo "--- Legit cases: set ---\n";

$xpath = new DOMXPath($doc);
$xpath->registerNamespace("php", "http://php.net/xpath");
$xpath->registerPhpFunctions([]);
$xpath->registerPHPFunctions(["xyz" => MyClass::dump(...), "mydump" => function (string $x) {
    var_dump($x);
}]);
$xpath->registerPhpFunctions(str_repeat("var_dump", mt_rand(1, 1) /* defeat SCCP */));
$xpath->evaluate("//a[php:function('mydump', string(@href))]");
$xpath->evaluate("//a[php:function('xyz', string(@href))]");
$xpath->evaluate("//a[php:function('var_dump', string(@href))]");
try {
    $xpath->evaluate("//a[php:function('notinset', string(@href))]");
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}

echo "--- Legit cases: set with cycle ---\n";

$xpath = new MyDOMXPath($doc);
$xpath->registerNamespace("php", "http://php.net/xpath");
$xpath->registerCycle();
$xpath->evaluate("//a[php:function('cycle', string(@href))]");

echo "--- Legit cases: reset to null ---\n";

$xpath->registerPhpFunctions(null);
$xpath->evaluate("//a[php:function('var_dump', string(@href))]");

"#);
    assert_exact(&out, r#"--- Legit cases: none ---
No callbacks were registered
--- Legit cases: all ---
string(15) "https://php.net"
string(15) "https://php.net"
--- Legit cases: set ---
string(15) "https://php.net"
string(15) "https://php.net"
string(15) "https://php.net"
No callback handler "notinset" registered
--- Legit cases: set with cycle ---
dummy: https://php.net
--- Legit cases: reset to null ---
string(15) "https://php.net""#);
}

/// php-src `ext/dom/tests/DOMXPath_callables_errors.phpt`: registerPHPFunctions() with callables - error cases
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXPath"#]
fn test_domxpath_callables_errors() {
    if skip_without_xml_native("test_domxpath_callables_errors") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument();
$doc->loadHTML('<a href="https://php.net">hello</a>');

$xpath = new DOMXPath($doc);
try {
    $xpath->registerPhpFunctions("nonexistent");
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}

try {
    $xpath->registerPhpFunctions(function () {});
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}

try {
    $xpath->registerPhpFunctions([function () {}]);
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}

try {
    $xpath->registerPhpFunctions([var_dump(...)]);
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}

try {
    $xpath->registerPhpFunctions(["nonexistent"]);
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}

try {
    $xpath->registerPhpFunctions(["" => var_dump(...)]);
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}

try {
    $xpath->registerPhpFunctions(["\0" => var_dump(...)]);
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}

try {
    $xpath->registerPhpFunctions("");
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}

$x = new class {
    public static function dump() {}
};

$classes = get_declared_classes();

try {
    $str = str_repeat($classes[count($classes) - 1] . '::dump', random_int(1, 1));
    $xpath->registerPhpFunctions([$str]);
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"DOMXPath::registerPhpFunctions(): Argument #1 ($restrict) must be a callable, function "nonexistent" not found or invalid function name
DOMXPath::registerPhpFunctions(): Argument #1 ($restrict) must be of type array|string|null, Closure given
Object of class Closure could not be converted to string
Object of class Closure could not be converted to string
DOMXPath::registerPhpFunctions(): Argument #1 ($restrict) must be an array with valid callbacks as values, function "nonexistent" not found or invalid function name
DOMXPath::registerPhpFunctions(): Argument #1 ($restrict) must be an array containing valid callback names
DOMXPath::registerPhpFunctions(): Argument #1 ($restrict) must be an array containing valid callback names
DOMXPath::registerPhpFunctions(): Argument #1 ($restrict) must be a valid callback name
DOMXPath::registerPhpFunctions(): Argument #1 ($restrict) must be an array containing valid callback names"#);
}

/// php-src `ext/dom/tests/DOMXPath_clone.phpt`: DOMXPath: Cloning a DOMXPath object
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXPath"#]
fn test_domxpath_clone() {
    if skip_without_xml_native("test_domxpath_clone") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument;
$xpath = new DOMXPath($dom);
try {
    clone $xpath;
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"Trying to clone an uncloneable object of class DOMXPath"#);
}

/// php-src `ext/dom/tests/DOMXPath_constructor_registered_functions.phpt`: DOMXPath: Calling __construct() again when functions were already registered
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXPath"#]
fn test_domxpath_constructor_registered_functions() {
    if skip_without_xml_native("test_domxpath_constructor_registered_functions") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument;
$dom->loadXML('<root/>');

class Test {
    public function __destruct() {
        echo "destruct\n";
    }

    public function test() {
        echo "test\n";
    }
}

echo "=== First run ===\n";

$xpath = new DOMXPath($dom);
$xpath->registerNamespace('foo', 'urn:foo');
$xpath->registerPhpFunctionNS('urn:foo', 'test', [new Test, 'test']);

echo "=== Reconstruct ===\n";

$xpath->__construct($dom, true);

echo "=== Second run ===\n";

$xpath->registerNamespace('foo', 'urn:foo');
// Note: since libxml2 commit aca16fb3d45e0b2c45364ffc1cea8eb4abaca87d this only outputs 1 warning. This seems intentional.
// Easiest workaround is silencing the warnings
@$xpath->query('//*[foo:test()]');
$xpath->registerPhpFunctionNS('urn:foo', 'test', [new Test, 'test']);
$xpath->query('//*[foo:test()]');

"#);
    assert_exact(&out, r#"=== First run ===
=== Reconstruct ===
destruct
=== Second run ===
test
destruct"#);
}

/// php-src `ext/dom/tests/DOMXPath_evaluate_namespace_node_set.phpt`: DOMXPath::evaluate() with PHP function passing a namespace node-set
#[test]
#[ignore = r#"DOM port pending: count() argument must be array or Countable object"#]
fn test_domxpath_evaluate_namespace_node_set() {
    if skip_without_xml_native("test_domxpath_evaluate_namespace_node_set") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument();
$dom->loadXML(<<<XML
<?xml version="1.0"?>
<container>
    <p>hi</p>
</container>
XML);

$xpath = new DOMXPath($dom);

function node_test($nodes) {
    echo "nodes count: ", count($nodes), "\n";
    return array_sum(array_map(fn ($node) => strlen($node->nodeName), $nodes));
}

$xpath->registerNamespace("php", "http://php.net/xpath");
$xpath->registerPhpFunctions(['node_test']);
var_dump($xpath->evaluate('number(php:function("node_test", //namespace::*))'));
var_dump($xpath->evaluate('boolean(php:function("node_test", //namespace::*))'));

"#);
    assert_exact(&out, r#"nodes count: 2
float(18)
nodes count: 2
bool(true)"#);
}

/// php-src `ext/dom/tests/DOMXPath_evaluate_node_set_to_string.phpt`: DOMXPath::evaluate() with PHP function passing node-set returning a string
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXPath"#]
fn test_domxpath_evaluate_node_set_to_string() {
    if skip_without_xml_native("test_domxpath_evaluate_node_set_to_string") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument();
$dom->loadXML(<<<XML
<?xml version="1.0"?>
<container>
    <p>hi</p>
</container>
XML);

$xpath = new DOMXPath($dom);

$xpath->registerNamespace("php", "http://php.net/xpath");
$xpath->registerPhpFunctions(['strrev']);
var_dump($xpath->evaluate('php:functionString("strrev", //p)'));
var_dump($xpath->evaluate('php:functionString("strrev", //namespace::*)'));

"#);
    assert_exact(&out, r#"string(2) "ih"
string(36) "ecapseman/8991/LMX/gro.3w.www//:ptth""#);
}

/// php-src `ext/dom/tests/DOMXPath_quote.phpt`: Test DOMXPath::quote with various inputs
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXPath"#]
fn test_domxpath_quote() {
    if skip_without_xml_native("test_domxpath_quote") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = new DOMDocument();
$xpath = new DOMXPath($dom);


/**
 * Quote a string for use in an XPath expression.
 * 
 * Example: $xp->query("//span[contains(text()," . $xp->quote($string) . ")]")
 * 
 * @param string $string string to quote.
 * @return string quoted string.
 */
function UserlandDOMXPathQuote(string $string): string
{
    if (false === \strpos($string, '\'')) {
        return '\'' . $string . '\'';
    }
    if (false === \strpos($string, '"')) {
        return '"' . $string . '"';
    }
    // if the string contains both single and double quotes, construct an
    // expression that concatenates all non-double-quote substrings with
    // the quotes, e.g.:
    //  'foo'"bar => concat("'foo'", '"bar")
    $sb = [];
    while ($string !== '') {
        $bytesUntilSingleQuote = \strcspn($string, '\'');
        $bytesUntilDoubleQuote = \strcspn($string, '"');
        $quoteMethod = ($bytesUntilSingleQuote > $bytesUntilDoubleQuote) ? "'" : '"';
        $bytesUntilQuote = \max($bytesUntilSingleQuote, $bytesUntilDoubleQuote);
        $sb[] = $quoteMethod . \substr($string, 0, $bytesUntilQuote) . $quoteMethod;
        $string = \substr($string, $bytesUntilQuote);
    }
    $sb = \implode(',', $sb);
    return 'concat(' . $sb . ')';
}



$tests = [
    '' => "''", // empty string
    'foo' => "'foo'", // no quotes
    '"foo' => '\'"foo\'', // double quotes only
    '\'foo' => '"\'foo"', // single quotes only
    '\'foo"bar' => 'concat("\'foo",\'"bar\')', // both; double quotes in mid-string
    '\'foo"bar"baz' => 'concat("\'foo",\'"bar"baz\')', // multiple double quotes in mid-string
    '\'foo"' => 'concat("\'foo",\'"\')', // string ends with double quotes
    '\'foo""' => 'concat("\'foo",\'""\')', // string ends with run of double quotes
    '"\'foo' => 'concat(\'"\',"\'foo")', // string begins with double quotes
    '""\'foo' => 'concat(\'""\',"\'foo")', // string begins with run of double quotes
    '\'foo""bar' => 'concat("\'foo",\'""bar\')', // run of double quotes in mid-string
];

foreach ($tests as $input => $expected) {
    $result = $xpath->quote($input);
    if ($result === $expected) {
        echo "Pass: {$input} => {$result}\n";
    } else {
        echo 'Fail: ';
        var_dump([
            'input' => $input,
            'expected' => $expected,
            'result' => $result,
            'userland_implementation_result' => UserlandDOMXPathQuote($input),
        ]);
    }
}
"#);
    assert_exact(&out, r#"Pass:  => ''
Pass: foo => 'foo'
Pass: "foo => '"foo'
Pass: 'foo => "'foo"
Pass: 'foo"bar => concat("'foo",'"bar')
Pass: 'foo"bar"baz => concat("'foo",'"bar"baz')
Pass: 'foo" => concat("'foo",'"')
Pass: 'foo"" => concat("'foo",'""')
Pass: "'foo => concat('"',"'foo")
Pass: ""'foo => concat('""',"'foo")
Pass: 'foo""bar => concat("'foo",'""bar')"#);
}

/// php-src `ext/dom/tests/registerPhpFunctionNS.phpt`: registerPhpFunctionNS() function - legit cases
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXPath"#]
fn test_register_php_function_ns() {
    if skip_without_xml_native("test_register_php_function_ns") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

class TrampolineClass {
    public static function __callStatic(string $name, array $arguments): mixed {
        var_dump($name, $arguments);
        return strtoupper($arguments[0]);
    }
}

class StatefulClass {
    public array $state = [];

    public function __call(string $name, array $arguments): mixed {
        $this->state[] = [$name, $arguments[0]];
        return $arguments[0];
    }
}

$doc = new DOMDocument();
$doc->loadHTML('<a href="https://PHP.net">hello</a>');

$xpath = new DOMXPath($doc);

$xpath->registerNamespace('foo', 'urn:foo');

echo "--- Legit cases: global function callable ---\n";

$xpath->registerPhpFunctionNS('urn:foo', 'strtolower', strtolower(...));
var_dump($xpath->query('//a[foo:strtolower(string(@href)) = "https://php.net"]'));

echo "--- Legit cases: string callable ---\n";

$xpath->registerPhpFunctionNS('urn:foo', 'strtolower', 'strtolower');
var_dump($xpath->query('//a[foo:strtolower(string(@href)) = "https://php.net"]'));

echo "--- Legit cases: trampoline callable ---\n";

$xpath->registerPhpFunctionNS('urn:foo', 'test', TrampolineClass::test(...));
var_dump($xpath->query('//a[foo:test(string(@href)) = "https://php.net"]'));

echo "--- Legit cases: instance class method callable ---\n";

$state = new StatefulClass;
$xpath->registerPhpFunctionNS('urn:foo', 'test', $state->test(...));
var_dump($xpath->query('//a[foo:test(string(@href))]'));
var_dump($state->state);

echo "--- Legit cases: global function callable that returns nothing ---\n";

$xpath->registerPhpFunctionNS('urn:foo', 'test', var_dump(...));
$xpath->query('//a[foo:test(string(@href))]');

echo "--- Legit cases: multiple namespaces ---\n";

$xpath->registerNamespace('bar', 'urn:bar');
$xpath->registerPhpFunctionNS('urn:bar', 'test', 'strtolower');
var_dump($xpath->query('//a[bar:test(string(@href)) = "https://php.net"]'));

"#);
    assert_exact(&out, r#"--- Legit cases: global function callable ---
object(DOMNodeList)#5 (1) {
  ["length"]=>
  int(1)
}
--- Legit cases: string callable ---
object(DOMNodeList)#5 (1) {
  ["length"]=>
  int(1)
}
--- Legit cases: trampoline callable ---
string(4) "test"
array(1) {
  [0]=>
  string(15) "https://PHP.net"
}
object(DOMNodeList)#3 (1) {
  ["length"]=>
  int(0)
}
--- Legit cases: instance class method callable ---
object(DOMNodeList)#6 (1) {
  ["length"]=>
  int(1)
}
array(1) {
  [0]=>
  array(2) {
    [0]=>
    string(4) "test"
    [1]=>
    string(15) "https://PHP.net"
  }
}
--- Legit cases: global function callable that returns nothing ---
string(15) "https://PHP.net"
--- Legit cases: multiple namespaces ---
object(DOMNodeList)#5 (1) {
  ["length"]=>
  int(1)
}"#);
}

/// php-src `ext/dom/tests/registerPhpFunctionNS_errors.phpt`: registerPhpFunctionNS() function - error cases
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXPath"#]
fn test_register_php_function_ns_errors() {
    if skip_without_xml_native("test_register_php_function_ns_errors") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

class TrampolineClass {
    public function __call($name, $args) {
    }
}

$doc = new DOMDocument();
$doc->loadHTML('<a href="https://PHP.net">hello</a>');

$xpath = new DOMXPath($doc);

try {
    $xpath->registerPhpFunctionNS('http://php.net/xpath', 'strtolower', strtolower(...));
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}

try {
    $xpath->registerPhpFunctionNS('http://php.net/xpath', 'test', [new TrampolineClass, 'test']);
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}

try {
    $xpath->registerPhpFunctionNS('urn:foo', '$$$', [new TrampolineClass, 'test']);
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}

try {
    $xpath->registerPhpFunctionNS('urn:foo', 'x:a', strtolower(...));
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}

try {
    $xpath->registerPhpFunctionNS("urn:foo", "\0", strtolower(...));
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}

try {
    $xpath->registerPhpFunctionNS("\0", 'strtolower', strtolower(...));
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"DOMXPath::registerPhpFunctionNS(): Argument #1 ($namespaceURI) must not be "http://php.net/xpath" because it is reserved by PHP
DOMXPath::registerPhpFunctionNS(): Argument #1 ($namespaceURI) must not be "http://php.net/xpath" because it is reserved by PHP
DOMXPath::registerPhpFunctionNS(): Argument #2 ($name) must be a valid callback name
DOMXPath::registerPhpFunctionNS(): Argument #2 ($name) must be a valid callback name
DOMXPath::registerPhpFunctionNS(): Argument #2 ($name) must not contain any null bytes
DOMXPath::registerPhpFunctionNS(): Argument #1 ($namespaceURI) must not contain any null bytes"#);
}

/// php-src `ext/dom/tests/xpath_context_node.phpt`: XPath: with a context node
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXpath"#]
fn test_xpath_context_node() {
    if skip_without_xml_native("test_xpath_context_node") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = new DOMDocument();
$dom->loadXML(<<<XML
<root>
    <child>
        <p>bar</p>
    </child>
    <child>
        <p>foo1</p>
        <p>foo2</p>
    </child>
</root>
XML);
$xpath = new DOMXpath($dom);

foreach ($xpath->query("p", $dom->documentElement->firstElementChild->nextElementSibling) as $p) {
    echo $p->textContent, "\n";
}

var_dump($xpath->evaluate("count(p)", $dom->documentElement->firstElementChild->nextElementSibling));

"#);
    assert_exact(&out, r#"foo1
foo2
float(2)"#);
}

/// php-src `ext/dom/tests/xpath_domnamespacenode.phpt`: DOMXPath::query() can return DOMNodeList with DOMNameSpaceNode items
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXPath"#]
fn test_xpath_domnamespacenode() {
    if skip_without_xml_native("test_xpath_domnamespacenode") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$xml = <<<'XML'
<?xml version="1.0" encoding="UTF-8"?>
<test></test>
XML;
$doc = new DomDocument;
$doc->loadXML($xml);
$xpath = new DOMXPath($doc);
$nodes = $xpath->query('//namespace::*');
var_dump($nodes->item(0));

"#);
    assert_exact(&out, r#"object(DOMNameSpaceNode)#4 (10) {
  ["nodeName"]=>
  string(9) "xmlns:xml"
  ["nodeValue"]=>
  string(36) "http://www.w3.org/XML/1998/namespace"
  ["nodeType"]=>
  int(18)
  ["prefix"]=>
  string(3) "xml"
  ["localName"]=>
  string(3) "xml"
  ["namespaceURI"]=>
  string(36) "http://www.w3.org/XML/1998/namespace"
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

/// php-src `ext/dom/tests/xpath_domnamespacenode_advanced.phpt`: DOMXPath::query() can return DOMNodeList with DOMNameSpaceNode items - advanced variation
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXPath"#]
fn test_xpath_domnamespacenode_advanced() {
    if skip_without_xml_native("test_xpath_domnamespacenode_advanced") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument();
$dom->loadXML(<<<'XML'
<root xmlns:foo="http://example.com/foo" xmlns:bar="http://example.com/bar">
    <child xmlns:baz="http://example.com/baz">Hello PHP!</child>
</root>
XML);

$xpath = new DOMXPath($dom);
$query = '//namespace::*';

echo "-- All namespace attributes --\n";

foreach ($xpath->query($query) as $attribute) {
    echo $attribute->nodeName . ' = ' . $attribute->nodeValue . PHP_EOL;
    var_dump($attribute->parentNode->tagName);
}

echo "-- All namespace attributes with removal attempt --\n";

foreach ($xpath->query($query) as $attribute) {
    echo "Before: ", $attribute->parentNode->tagName, "\n";
    // Second & third attempt should fail because it's no longer in the document
    try {
        $attribute->parentNode->remove();
    } catch (\DOMException $e) {
        echo $e->getMessage(), "\n";
    }
    // However, it should not cause a use-after-free
    echo "After: ", $attribute->parentNode->tagName, "\n";
}

"#);
    assert_exact(&out, r#"-- All namespace attributes --
xmlns:xml = http://www.w3.org/XML/1998/namespace
string(4) "root"
xmlns:bar = http://example.com/bar
string(4) "root"
xmlns:foo = http://example.com/foo
string(4) "root"
xmlns:xml = http://www.w3.org/XML/1998/namespace
string(5) "child"
xmlns:bar = http://example.com/bar
string(5) "child"
xmlns:foo = http://example.com/foo
string(5) "child"
xmlns:baz = http://example.com/baz
string(5) "child"
-- All namespace attributes with removal attempt --
Before: root
After: root
Before: root
Not Found Error
After: root
Before: root
Not Found Error
After: root
Before: child
After: child
Before: child
Not Found Error
After: child
Before: child
Not Found Error
After: child
Before: child
Not Found Error
After: child"#);
}

/// php-src `ext/dom/tests/xpath_evaluate_basic_types.phpt`: XPath: basic types evaluation
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXpath"#]
fn test_xpath_evaluate_basic_types() {
    if skip_without_xml_native("test_xpath_evaluate_basic_types") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument();
$dom->loadHTML('<p align="center">foo</p>');
$xpath = new DOMXpath($dom);
var_dump($xpath->evaluate("count(//p) > 0"));
var_dump($xpath->evaluate("string(//p/@align)"));

"#);
    assert_exact(&out, r#"bool(true)
string(6) "center""#);
}
