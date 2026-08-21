//! Purpose:
//! End-to-end regressions for legacy `DOMXPath` and modern `Dom\XPath`.
//!
//! Called from:
//! - `cargo test --test codegen_tests codegen::dom_xpath` through Rust's test harness.
//!
//! Key details:
//! - Tests cover scalar/node-set results, context nodes, namespaces, properties, and exact errors.
//! - Custom namespace callbacks cover scalar/node marshalling, DOM returns, and exact Throwable transport.

use crate::support::{
    compile_and_run, compile_and_run_capture, compile_and_run_with_heap_debug,
};

/// Verifies custom namespace callbacks preserve scalar order and exact Throwable identity.
#[test]
fn xpath_custom_namespace_callbacks_are_reentrant() {
    let out = compile_and_run_capture(
        r#"<?php
$document = Dom\XMLDocument::createFromString("<root/>");
$xpath = new Dom\XPath($document);
$xpath->registerNamespace("cb", "urn:callback");
$xpath->registerPhpFunctionNS(
    "urn:callback",
    "render",
    function (bool $flag, float $number, string $text) use ($document): string {
        echo $document->documentElement->nodeName, ":";
        echo gettype($flag), ":", gettype($number), ":", gettype($text), "|";
        return ($flag ? "1" : "0") . ":" . $number . ":" . $text;
    }
);
echo $xpath->evaluate("cb:render(true(), 12.5, 'abc')"), "\n";

$xpath->registerPhpFunctionNS(
    "urn:callback",
    "flag",
    function (): bool {
        return true;
    }
);
var_dump($xpath->evaluate("cb:flag()"));

$expected = new Exception("callback failure");
$xpath->registerPhpFunctionNS(
    "urn:callback",
    "fail",
    function () use ($expected): string {
        throw $expected;
    }
);
try {
    $xpath->evaluate("cb:fail()");
} catch (Throwable $actual) {
    var_dump($actual === $expected);
}
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={} stderr={}",
        out.stdout,
        out.stderr,
    );
    assert_eq!(
        out.stdout,
        "root:boolean:double:string|1:12.5:abc\nbool(true)\nbool(true)\n"
    );
    assert_eq!(out.stderr, "");
}

/// Verifies node-set arguments become canonical wrappers and DOM callback results stay live.
#[test]
fn xpath_custom_namespace_callbacks_preserve_dom_nodes() {
    let out = compile_and_run_capture(
        r#"<?php
$document = Dom\XMLDocument::createFromString(
    "<root><item>A</item><item>B</item></root>"
);
$xpath = new Dom\XPath($document);
$xpath->registerNamespace("cb", "urn:callback");
$xpath->registerPhpFunctionNS(
    "urn:callback",
    "inspect",
    function (array $nodes) use ($document): string {
        echo get_class($nodes[0]), ":", count($nodes), ":";
        var_dump($nodes[0] === $document->documentElement->firstChild);
        return "B";
    }
);
echo $xpath->evaluate("cb:inspect(//item)"), "\n";

$xpath->registerPhpFunctionNS(
    "urn:callback",
    "first",
    function (array $nodes) {
        return $nodes[0];
    }
);
$roundTrip = $xpath->evaluate("cb:first(//item)");
echo $roundTrip->item(0)->textContent, ":";
var_dump($roundTrip->item(0) === $document->documentElement->firstChild);

$other = Dom\XMLDocument::createFromString("<other><picked/></other>");
$picked = $other->documentElement->firstChild;
$xpath->registerPhpFunctionNS(
    "urn:callback",
    "node",
    function () use ($picked) {
        return $picked;
    }
);
$returned = $xpath->evaluate("cb:node()");
echo get_class($returned), ":", $returned->length, ":";
echo $returned->item(0)->nodeName, ":";
var_dump($returned->item(0) === $picked);

$xpath->registerPhpFunctionNS(
    "urn:callback",
    "invalid",
    function (): stdClass {
        return new stdClass();
    }
);
try {
    $xpath->evaluate("cb:invalid()");
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={} stderr={}",
        out.stdout,
        out.stderr,
    );
    assert_eq!(
        out.stdout,
        concat!(
            "Dom\\Element:2:bool(true)\n",
            "B\n",
            "A:bool(true)\n",
            "Dom\\NodeList:1:picked:bool(true)\n",
            "Only objects that are instances of DOM nodes can be converted ",
            "to an XPath expression\n",
        )
    );
    assert_eq!(out.stderr, "");
}

/// Verifies custom namespace callback registration uses php-src's exact ValueError messages.
#[test]
fn xpath_custom_namespace_registration_errors_match_php() {
    let out = compile_and_run(
        r#"<?php
$document = Dom\XMLDocument::createFromString("<root/>");
$xpath = new Dom\XPath($document);
$callback = function (): string {
    return "";
};
foreach ([
    ["http://php.net/xpath", "valid"],
    ["urn:test", "bad:name"],
    ["urn:\0test", "valid"],
    ["urn:test", "bad\0name"],
] as $arguments) {
    try {
        $xpath->registerPhpFunctionNS(
            $arguments[0],
            $arguments[1],
            $callback
        );
    } catch (ValueError $error) {
        echo $error->getMessage(), "\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "Dom\\XPath::registerPhpFunctionNS(): Argument #1 ($namespaceURI) ",
            "must not be \"http://php.net/xpath\" because it is reserved by PHP\n",
            "Dom\\XPath::registerPhpFunctionNS(): Argument #2 ($name) ",
            "must be a valid callback name\n",
            "Dom\\XPath::registerPhpFunctionNS(): Argument #1 ($namespaceURI) ",
            "must not contain any null bytes\n",
            "Dom\\XPath::registerPhpFunctionNS(): Argument #2 ($name) ",
            "must not contain any null bytes\n",
        )
    );
}

/// Verifies php-src's none, all, and restricted reserved callback modes.
#[test]
fn xpath_reserved_php_callbacks_preserve_registration_modes() {
    let out = compile_and_run_capture(
        r#"<?php
function xpath_render(string $value): string {
    return "[" . $value . "]";
}

$document = Dom\XMLDocument::createFromString("<root/>");
$xpath = new Dom\XPath($document);
$xpath->registerNamespace("php", "http://php.net/xpath");
try {
    $xpath->evaluate("php:function('xpath_render', 'none')");
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}

$xpath->registerPhpFunctions();
echo $xpath->evaluate("php:function('xpath_render', 'all')"), "\n";
echo $xpath->evaluate("php:function('strtoupper', 'builtin')"), "\n";

$xpath = new Dom\XPath($document);
$xpath->registerNamespace("php", "http://php.net/xpath");
$xpath->registerPhpFunctions([]);
$xpath->registerPhpFunctions("xpath_render");
echo $xpath->evaluate("php:function('xpath_render', 'set')"), "\n";
try {
    $xpath->evaluate("php:function('missing', 'value')");
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
$xpath->registerPhpFunctions(null);
echo $xpath->evaluate("php:function('strtoupper', 'reset')"), "\n";
$xpath->registerPhpFunctions([]);
echo $xpath->evaluate("php:function('xpath_render', 'retained')"), "\n";

$legacyDocument = new DOMDocument();
$legacyDocument->loadXML("<root/>");
$legacy = new DOMXPath($legacyDocument);
$legacy->registerNamespace("php", "http://php.net/xpath");
$legacy->registerPhpFunctions("xpath_render");
echo $legacy->evaluate("php:function('xpath_render', 'legacy')"), "\n";
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={} stderr={}",
        out.stdout,
        out.stderr,
    );
    assert_eq!(
        out.stdout,
        concat!(
            "No callbacks were registered\n",
            "[all]\n",
            "BUILTIN\n",
            "[set]\n",
            "No callback handler \"missing\" registered\n",
            "RESET\n",
            "[retained]\n",
            "[legacy]\n",
        )
    );
    assert_eq!(out.stderr, "");
}

/// Verifies aliases, node-set conversion modes, and reserved callback argument errors.
#[test]
fn xpath_reserved_php_callbacks_preserve_aliases_and_nodesets() {
    let out = compile_and_run_capture(
        r#"<?php
$document = Dom\XMLDocument::createFromString(
    "<root><item>A</item><item>B</item></root>"
);
$xpath = new Dom\XPath($document);
$xpath->registerNamespace("php", "http://php.net/xpath");
$xpath->registerPhpFunctions([
    "showString" => function (string $value): string {
        return gettype($value) . ":" . $value;
    },
    "showNodes" => function (array $nodes): string {
        $second = $nodes[1];
        return gettype($nodes) . ":" . count($nodes)
            . ":" . get_class($second);
    },
]);
echo $xpath->evaluate(
    "php:functionString('showString', //item)"
), "\n";
echo $xpath->evaluate(
    "php:function('showNodes', //item)"
), "\n";

foreach ([
    "php:function()",
    "php:function(12)",
] as $expression) {
    try {
        $xpath->evaluate($expression);
    } catch (Throwable $error) {
        echo get_class($error), ":", $error->getMessage(), "\n";
    }
}
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={} stderr={}",
        out.stdout,
        out.stderr,
    );
    assert_eq!(
        out.stdout,
        concat!(
            "string:A\n",
            "array:2:Dom\\Element\n",
            "Error:Function name must be passed as the first argument\n",
            "TypeError:Handler name must be a string\n",
        )
    );
    assert_eq!(out.stderr, "");
}

/// Verifies reserved callback maps preserve instance and static callable-array identity.
#[test]
fn xpath_reserved_php_callbacks_accept_callable_arrays() {
    let out = compile_and_run_capture(
        r#"<?php
class XPathCallbackHandler {
    public function decorate(string $value): string {
        return "instance:" . $value;
    }

    public static function render(string $value): string {
        return "static:" . $value;
    }
}

$document = Dom\XMLDocument::createFromString("<root/>");
$xpath = new Dom\XPath($document);
$xpath->registerNamespace("php", "http://php.net/xpath");
$handler = new XPathCallbackHandler();
$callbacks = [
    "decorate" => [$handler, "decorate"],
    "render" => [XPathCallbackHandler::class, "render"],
];
$xpath->registerPhpFunctions($callbacks);
echo $xpath->evaluate("php:function('decorate', 'one')"), "\n";
echo $xpath->evaluate("php:function('render', 'two')"), "\n";
try {
    $xpath->registerPhpFunctions([
        "invalid" => [new stdClass(), "missing"],
    ]);
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
echo $xpath->evaluate("php:function('decorate', 'retained')"), "\n";
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={} stderr={}",
        out.stdout,
        out.stderr,
    );
    assert_eq!(
        out.stdout,
        concat!(
            "instance:one\n",
            "static:two\n",
            "Dom\\XPath::registerPhpFunctions(): Argument #1 ($restrict) ",
            "must be an array with valid callbacks as values, class stdClass ",
            "does not have a method \"missing\"\n",
            "instance:retained\n",
        )
    );
    assert_eq!(out.stderr, "");
}

/// Verifies reserved callback registration reproduces php-src's primary error cases.
#[test]
fn xpath_reserved_php_callback_registration_errors_match_php() {
    let out = compile_and_run(
        r#"<?php
$document = Dom\XMLDocument::createFromString("<root/>");
$xpath = new Dom\XPath($document);

try {
    $xpath->registerPhpFunctions("nonexistent");
} catch (Throwable $error) {
    echo $error->getMessage(), "\n";
}
try {
    $xpath->registerPhpFunctions(function () {});
} catch (Throwable $error) {
    echo $error->getMessage(), "\n";
}
try {
    $xpath->registerPhpFunctions([function () {}]);
} catch (Throwable $error) {
    echo $error->getMessage(), "\n";
}
try {
    $xpath->registerPhpFunctions(["nonexistent"]);
} catch (Throwable $error) {
    echo $error->getMessage(), "\n";
}
try {
    $xpath->registerPhpFunctions(["" => var_dump(...)]);
} catch (Throwable $error) {
    echo $error->getMessage(), "\n";
}
try {
    $xpath->registerPhpFunctions(["\0" => var_dump(...)]);
} catch (Throwable $error) {
    echo $error->getMessage(), "\n";
}
try {
    $xpath->registerPhpFunctions("");
} catch (Throwable $error) {
    echo $error->getMessage(), "\n";
}
try {
    $xpath->registerPhpFunctions(1);
} catch (Throwable $error) {
    echo $error->getMessage(), "\n";
}
try {
    $xpath->registerPhpFunctions(false);
} catch (Throwable $error) {
    echo $error->getMessage(), "\n";
}
try {
    $xpath->registerPhpFunctions(1.2);
} catch (Throwable $error) {
    echo $error->getMessage(), "\n";
}
try {
    $xpath->registerPhpFunctions(new stdClass());
} catch (Throwable $error) {
    echo $error->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "Dom\\XPath::registerPhpFunctions(): Argument #1 ($restrict) ",
            "must be a callable, function \"nonexistent\" not found or ",
            "invalid function name\n",
            "Dom\\XPath::registerPhpFunctions(): Argument #1 ($restrict) ",
            "must be of type array|string|null, Closure given\n",
            "Object of class Closure could not be converted to string\n",
            "Dom\\XPath::registerPhpFunctions(): Argument #1 ($restrict) ",
            "must be an array with valid callbacks as values, function ",
            "\"nonexistent\" not found or invalid function name\n",
            "Dom\\XPath::registerPhpFunctions(): Argument #1 ($restrict) ",
            "must be an array containing valid callback names\n",
            "Dom\\XPath::registerPhpFunctions(): Argument #1 ($restrict) ",
            "must be an array containing valid callback names\n",
            "Dom\\XPath::registerPhpFunctions(): Argument #1 ($restrict) ",
            "must be a valid callback name\n",
            "Dom\\XPath::registerPhpFunctions(): Argument #1 ($restrict) ",
            "must be a callable, function \"1\" not found or invalid function name\n",
            "Dom\\XPath::registerPhpFunctions(): Argument #1 ($restrict) ",
            "must be a valid callback name\n",
            "Dom\\XPath::registerPhpFunctions(): Argument #1 ($restrict) ",
            "must be a callable, function \"1.2\" not found or invalid function name\n",
            "Dom\\XPath::registerPhpFunctions(): Argument #1 ($restrict) ",
            "must be of type array|string|null, stdClass given\n",
        )
    );
}

/// Verifies legacy XPath node-set/scalar results, context nodes, and document identity.
#[test]
fn legacy_xpath_evaluates_nodes_scalars_and_contexts() {
    let out = compile_and_run_capture(
        r#"<?php
$document = new DOMDocument();
$document->loadXML(
    '<root xmlns:p="urn:p">'
    . '<group><p:item>A</p:item></group>'
    . '<group><p:item>B</p:item><p:item>C</p:item></group>'
    . '</root>'
);
$xpath = new DOMXpath($document);
var_dump($xpath->registerNamespace("p", "urn:p"));
$items = $xpath->query("//p:item");
echo get_class($items), ":", $items->length, ":";
echo $items->item(0)->nodeName;
echo $items->item(1)->nodeName;
echo $items->item(2)->nodeName;
echo "\n";
$group = $document->documentElement->lastChild;
echo gettype($xpath->evaluate("count(p:item)", $group)), ":";
echo $xpath->evaluate("count(p:item)", $group), "\n";
echo gettype($xpath->evaluate("boolean(//p:item)")), ":";
var_dump($xpath->evaluate("boolean(//p:item)"));
echo gettype($xpath->evaluate("string(//p:item)")), ":";
var_dump($xpath->evaluate("string(//p:item)"));
var_dump($xpath->document === $document);
"#,
    );
    assert!(out.success, "program failed: stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "bool(true)\n",
            "DOMNodeList:3:p:itemp:itemp:item\n",
            "double:2\n",
            "boolean:bool(true)\n",
            "string:string(1) \"A\"\n",
            "bool(true)\n",
        )
    );
}

/// Verifies modern XPath results and rejection of namespace-axis nodes.
#[test]
fn modern_xpath_evaluates_nodes_and_rejects_namespace_axis() {
    let out = compile_and_run(
        r#"<?php
$document = Dom\XMLDocument::createFromString(
    '<root><p>hi</p><?target data?><!-- comment --></root>'
);
$xpath = new Dom\XPath($document);
$nodes = $xpath->evaluate("//p");
echo get_class($nodes), ":", $nodes->length, ":";
echo $nodes->item(0)->textContent, "\n";
echo gettype($xpath->evaluate("string-length(//p)")), ":";
echo $xpath->evaluate("string-length(//p)"), "\n";
$special = $xpath->query("//processing-instruction()|//comment()");
for ($index = 0; $index < $special->length; $index++) {
    $node = $special->item($index);
    echo get_class($node), ":", $node->textContent, "\n";
}
try {
    $xpath->evaluate("//*/namespace::*");
} catch (DOMException $error) {
    echo $error->getCode(), ":", $error->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "Dom\\NodeList:1:hi\n",
            "double:2\n",
            "Dom\\ProcessingInstruction:data\n",
            "Dom\\Comment: comment \n",
            "9:The namespace axis is not well-defined in the living DOM ",
            "specification. Use Dom\\Element::getInScopeNamespaces() or ",
            "Dom\\Element::getDescendantNamespaces() instead.\n",
        )
    );
}

/// Verifies persistent, automatic, disabled, and per-call namespace behavior.
#[test]
fn xpath_registers_persistent_and_context_node_namespaces() {
    let out = compile_and_run_capture(
        r#"<?php
$document = Dom\XMLDocument::createFromString(
    '<root><scope xmlns:p="urn:one"><p:item/></scope></root>'
);
$scope = $document->documentElement->firstChild;
$xpath = new Dom\XPath($document);
echo $xpath->query("//p:item", $scope)->length, ":";
$xpath->registerNodeNamespaces = false;
var_dump($xpath->registerNodeNamespaces);
try {
    $xpath->query("//p:item", $scope);
} catch (Error $error) {
    echo $error->getMessage(), ":";
}
try {
    $xpath->query(contextNode: $scope, expression: "//p:item");
} catch (Error $error) {
    echo "named:", $error->getMessage(), ":";
}
try {
    $xpath->query(...[
        "contextNode" => $scope,
        "expression" => "//p:item",
    ]);
} catch (Error $error) {
    echo "spread:", $error->getMessage(), ":";
}
echo $xpath->query("//p:item", $scope, true)->length, ":";
var_dump($xpath->registerNamespace("", "urn:none"));
var_dump($xpath->registerNamespace("p", "urn:one"));
echo $xpath->query("//p:item", null, false)->length, "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "1:bool(false)\n",
            "Could not evaluate XPath expression:",
            "named:Could not evaluate XPath expression:",
            "spread:Could not evaluate XPath expression:1:",
            "bool(false)\n",
            "bool(true)\n",
            "1\n",
        )
    );
    assert_eq!(
        out.stderr,
        concat!(
            "Warning: Dom\\XPath::query(): Undefined namespace prefix: p\n",
            "Warning: Dom\\XPath::query(): Undefined namespace prefix: p\n",
            "Warning: Dom\\XPath::query(): Undefined namespace prefix: p\n",
        )
    );
}

/// Verifies the exact php-src XPath string quoting algorithm.
#[test]
fn xpath_quote_matches_php_for_all_quote_shapes() {
    let out = compile_and_run(
        r#"<?php
$inputs = ["", "foo", "\"foo", "'foo", "'foo\"bar", "\"'foo", "'foo\"\"bar"];
foreach ($inputs as $input) {
    echo DOMXPath::quote($input), "\n";
    echo Dom\XPath::quote($input), "\n";
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "''\n''\n",
            "'foo'\n'foo'\n",
            "'\"foo'\n'\"foo'\n",
            "\"'foo\"\n\"'foo\"\n",
            "concat(\"'foo\",'\"bar')\nconcat(\"'foo\",'\"bar')\n",
            "concat('\"',\"'foo\")\nconcat('\"',\"'foo\")\n",
            "concat(\"'foo\",'\"\"bar')\nconcat(\"'foo\",'\"\"bar')\n",
        )
    );
}

/// Verifies legacy false, modern Error, and wrong-document context rejection.
///
/// XPath cloneability is pinned separately in `dom_xpath_matrix.rs` because both XPath
/// families are uncloneable in the frozen PHP 8.5.8 oracle.
#[test]
fn xpath_preserves_family_errors_and_context_rejection() {
    let out = compile_and_run_capture(
        r#"<?php
$legacyDocument = new DOMDocument();
$legacyDocument->loadXML("<root/>");
$legacy = new DOMXPath($legacyDocument);
var_dump($legacy->evaluate("["));

$modernDocument = Dom\XMLDocument::createFromString("<root/>");
$modern = new Dom\XPath($modernDocument);
try {
    $modern->query("[");
} catch (Error $error) {
    echo get_class($error), "|", $error->getMessage(), "\n";
}

$other = Dom\XMLDocument::createFromString("<other/>");
try {
    $modern->query(".", $other->documentElement);
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "bool(false)\n",
            "Error|Could not evaluate XPath expression\n",
            "Node from wrong document\n",
        )
    );
    assert_eq!(
        out.stderr,
        concat!(
            "Warning: DOMXPath::evaluate(): Invalid expression\n",
            "Warning: Dom\\XPath::query(): Invalid expression\n",
        )
    );
}

/// Verifies dynamic indexed and associative spreads retain XPath optional-argument presence.
#[test]
fn xpath_dynamic_spreads_preserve_register_node_ns_omission() {
    let out = compile_and_run_capture(
        r#"<?php
$document = Dom\XMLDocument::createFromString(
    '<root xmlns:p="urn:one"><scope><p:item/></scope></root>'
);
$scope = $document->documentElement->firstChild;
$xpath = new Dom\XPath($document);

$one = ["//p:item"];
$two = ["//p:item", $scope];
$three = ["//p:item", $scope, 0];
$named_one = ["expression" => "//p:item"];
$named_two = ["expression" => "//p:item", "contextNode" => $scope];
$named_three = [
    "expression" => "//p:item",
    "contextNode" => $scope,
    "registerNodeNS" => false,
];

echo $xpath->query(...$one)->length, ":";
echo $xpath->query(...$two)->length, ":";
try {
    $xpath->query(...$three);
} catch (Error $error) {
    echo "E:";
}
echo $xpath->query(...$named_one)->length, ":";
echo $xpath->query(...$named_two)->length, ":";
try {
    $xpath->query(...$named_three);
} catch (Error $error) {
    echo "E:";
}
$xpath->registerNodeNamespaces = false;
$prefix = ["//p:item"];
try {
    $xpath->query(...$prefix, contextNode: $scope);
} catch (Error $error) {
    echo "E:";
}
echo $xpath->query(
    ...$prefix,
    contextNode: $scope,
    registerNodeNS: true,
)->length, "\n";
"#,
    );
    assert!(
        out.success,
        "program failed after stdout {:?}: {}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stdout, "1:1:E:1:1:E:E:1\n");
    assert_eq!(
        out.stderr,
        concat!(
            "Warning: Dom\\XPath::query(): Undefined namespace prefix: p\n",
            "Warning: Dom\\XPath::query(): Undefined namespace prefix: p\n",
            "Warning: Dom\\XPath::query(): Undefined namespace prefix: p\n",
        )
    );
}

/// Verifies a dynamic associative prefix keeps third-parameter omission before named arguments.
#[test]
fn xpath_dynamic_assoc_spread_before_named_args_preserves_omission() {
    let out = compile_and_run_capture(
        r#"<?php
$document = Dom\XMLDocument::createFromString(
    '<root xmlns:p="urn:one"><scope><p:item/></scope></root>'
);
$scope = $document->documentElement->firstChild;
$xpath = new Dom\XPath($document);
$xpath->registerNodeNamespaces = false;
$prefix = ["expression" => "//p:item"];
try {
    $xpath->query(...$prefix, contextNode: $scope);
} catch (Error $error) {
    echo "E:";
}
echo $xpath->query(
    ...$prefix,
    contextNode: $scope,
    registerNodeNS: true,
)->length, "\n";
"#,
    );
    assert!(
        out.success,
        "program failed after stdout {:?}: {}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stdout, "E:1\n");
    assert_eq!(
        out.stderr,
        "Warning: Dom\\XPath::query(): Undefined namespace prefix: p\n"
    );
}

/// Verifies both XPath families and both evaluation methods share dynamic-spread rules.
#[test]
fn xpath_dynamic_spreads_cover_legacy_modern_query_evaluate_and_source_order() {
    let out = compile_and_run_capture(
        r#"<?php
function xpath_args_mark(string $label, array $value): array {
    echo $label;
    return $value;
}

function xpath_context_mark(string $label, Dom\Element $value): Dom\Element {
    echo $label;
    return $value;
}

function xpath_bool_mark(string $label, bool $value): bool {
    echo $label;
    return $value;
}

$xml = '<root xmlns:p="urn:one"><scope><p:item/></scope></root>';

$legacy_document = new DOMDocument();
$legacy_document->loadXML($xml);
$legacy_xpath = new DOMXPath($legacy_document);
$legacy_scope = $legacy_document->documentElement->firstChild;
if (!$legacy_scope instanceof DOMElement) {
    throw new Exception("legacy scope missing");
}
$legacy_evaluate = ["count(//p:item)"];
$legacy_query = ["//p:item", $legacy_scope, false];
echo "LE:", $legacy_xpath->evaluate(...$legacy_evaluate), ":";
echo $legacy_xpath->query(...$legacy_query) === false ? "LF:" : "LX:";

$modern_document = Dom\XMLDocument::createFromString($xml);
$modern_xpath = new Dom\XPath($modern_document);
$modern_scope = $modern_document->documentElement->firstChild;
if (!$modern_scope instanceof Dom\Element) {
    throw new Exception("modern scope missing");
}
$modern_evaluate = ["count(//p:item)"];
$modern_query = ["//p:item", $modern_scope, false];
echo "ME:", $modern_xpath->evaluate(...$modern_evaluate), ":";
try {
    $modern_xpath->query(...$modern_query);
    echo "MX:";
} catch (Error $error) {
    echo "MF:";
}
$ordered = $modern_xpath->query(
    ...xpath_args_mark("A", ["//p:item"]),
    contextNode: xpath_context_mark("B", $modern_scope),
    registerNodeNS: xpath_bool_mark("C", true),
);
echo ":", $ordered->length, "\n";
"#,
    );
    assert!(
        out.success,
        "program failed after stdout {:?}: {}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stdout, "LE:1:LF:ME:1:MF:ABC:1\n");
    assert_eq!(
        out.stderr,
        concat!(
            "Warning: DOMXPath::query(): Undefined namespace prefix: p\n",
            "Warning: Dom\\XPath::query(): Undefined namespace prefix: p\n",
        )
    );
}

/// Verifies multiple dynamic, associative, and mixed static XPath spreads share one prefix.
#[test]
fn xpath_multiple_dynamic_spreads_preserve_values_order_and_omission() {
    let out = compile_and_run_capture(
        r#"<?php
function xpath_expression_mark(array $value): array {
    echo "A";
    return $value;
}

function xpath_context_array_mark(array $value): array {
    echo "B";
    return $value;
}

function xpath_register_mark(array $value): array {
    echo "C";
    return $value;
}

$document = Dom\XMLDocument::createFromString(
    '<root xmlns:p="urn:one"><scope><p:item/></scope></root>'
);
$scope = $document->documentElement->firstChild;
if (!$scope instanceof Dom\Element) {
    throw new Exception("scope missing");
}
$xpath = new Dom\XPath($document);
$xpath->registerNodeNamespaces = false;

$expression = ["//p:item"];
$context = [$scope];
$register = [true];
echo $xpath->query(
    ...xpath_expression_mark($expression),
    ...xpath_context_array_mark($context),
    ...xpath_register_mark($register),
)->length, ":";

$named_expression = ["expression" => "//p:item"];
$named_context = ["contextNode" => $scope];
$named_register = ["registerNodeNS" => true];
echo $xpath->query(
    ...$named_expression,
    ...$named_context,
    ...$named_register,
)->length, ":";

$legacy_document = new DOMDocument();
$legacy_document->loadXML(
    '<root xmlns:p="urn:one"><scope><p:item/></scope></root>'
);
$legacy_scope = $legacy_document->documentElement->firstChild;
if (!$legacy_scope instanceof DOMElement) {
    throw new Exception("legacy scope missing");
}
$legacy_xpath = new DOMXPath($legacy_document);
$legacy_xpath->registerNodeNamespaces = false;
$omitted_expression = ["//p:item"];
$omitted_context = [$legacy_scope];
echo $legacy_xpath->query(...$omitted_expression, ...$omitted_context) === false
    ? "F:"
    : "X:";

$mixed_context = [$scope];
echo $xpath->query(...["//p:item"], ...$mixed_context, ...[true])->length, "\n";
"#,
    );
    assert!(
        out.success,
        "program failed after stdout {:?}: {}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stdout, "ABC1:1:F:1\n");
    assert_eq!(
        out.stderr,
        "Warning: DOMXPath::query(): Undefined namespace prefix: p\n"
    );
}

/// Verifies an XPath argument supplied twice through a spread throws PHP's catchable `Error`.
#[test]
fn xpath_dynamic_spread_duplicate_is_catchable_error() {
    let out = compile_and_run(
        r#"<?php
function xpath_duplicate_args(array $arguments): array {
    echo "A";
    return $arguments;
}

function xpath_duplicate_expression(): string {
    echo "B";
    return ".";
}

$document = Dom\XMLDocument::createFromString("<root/>");
$xpath = new Dom\XPath($document);
try {
    $xpath->query(
        ...xpath_duplicate_args(["."]),
        expression: xpath_duplicate_expression(),
    );
} catch (Error $error) {
    echo "|", get_class($error), "|", $error->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "AB|Error|Named parameter $expression ",
            "overwrites previous argument\n",
        )
    );
}

/// Verifies the `DOMNameSpaceNode` concrete class, wrapper identity, recreation
/// after release, wrapper validity after the originating list/xpath/document are
/// released, the ten properties, independent cloning, and `__sleep`/`__wakeup`
/// serialization rejections, matching the PHP 8.5 oracle lifetimes.
#[test]
fn namespace_node_wrapper_lifetime_and_identity() {
    let out = compile_and_run_capture(
        r#"<?php
$d = new DOMDocument();
$d->loadXML('<root xmlns:p="urn:p"/>');
$root = $d->documentElement;
$l = (new DOMXPath($d))->query('//namespace::*');
$a = $l->item(1);
$b = $l->item(1);
var_dump($a === $b);
var_dump($a instanceof DOMNameSpaceNode);
echo get_class($a), "\n";
echo $a->nodeName, "|", $a->nodeValue, "|", $a->nodeType, "|", $a->prefix, "|", $a->localName, "|", $a->namespaceURI, "\n";
var_dump($a->isConnected);
var_dump($a->ownerDocument === $d);
var_dump($a->parentNode === $root);
var_dump($a->parentElement === $root);
unset($a, $b);
$c = $l->item(1);
var_dump($c instanceof DOMNameSpaceNode);
var_dump($c === null);
echo $c->nodeName, "\n";
if ($c instanceof DOMNameSpaceNode) {
    $clone = clone $c;
    var_dump($clone instanceof DOMNameSpaceNode);
    var_dump($clone === $c);
    echo $clone->nodeName, "|", $clone->prefix, "\n";
    unset($clone);
    try {
        $c->__sleep();
    } catch (Exception $e) {
        echo "sleep:", get_class($e), ":", $e->getMessage(), "\n";
    }
    try {
        $c->__wakeup();
    } catch (Exception $e) {
        echo "wakeup:", get_class($e), ":", $e->getMessage(), "\n";
    }
}
$keep = $l->item(1);
unset($l, $d, $root);
var_dump($keep instanceof DOMNameSpaceNode);
echo $keep->nodeName, "|", $keep->prefix, "|", $keep->nodeValue, "\n";
echo get_class($keep->parentNode), "\n";
"#,
    );
    assert!(out.success, "program failed: stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "bool(true)\n",
            "bool(true)\n",
            "DOMNameSpaceNode\n",
            "xmlns:p|urn:p|18|p|p|urn:p\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(false)\n",
            "xmlns:p\n",
            "bool(true)\n",
            "bool(false)\n",
            "xmlns:p|p\n",
            "sleep:Exception:Serialization of 'DOMNameSpaceNode' is not allowed, unless serialization methods are implemented in a subclass\n",
            "wakeup:Exception:Unserialization of 'DOMNameSpaceNode' is not allowed, unless unserialization methods are implemented in a subclass\n",
            "bool(true)\n",
            "xmlns:p|p|urn:p\n",
            "DOMElement\n",
        )
    );
}

/// Verifies dynamic XPath contexts use exact PHP 8.5 TypeErrors and nullable nodes.
#[test]
fn xpath_dynamic_context_node_type_errors_are_exact_and_nullable() {
    let out = compile_and_run_capture(
        r#"<?php
function opaque_xpath_context(mixed $value): mixed {
    return $value;
}

class OrderedXPathString {
    public function __toString(): string {
        echo "S|";
        return ".";
    }
}

$legacy_document = new DOMDocument();
$legacy_document->loadXML("<root><item/></root>");
$legacy_xpath = new DOMXPath($legacy_document);
$legacy_node = $legacy_document->documentElement;

try {
    $legacy_xpath->query(".", opaque_xpath_context(new stdClass()));
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
try {
    $legacy_xpath->evaluate(".", opaque_xpath_context(42));
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
try {
    $legacy_xpath->query(".", opaque_xpath_context([]));
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
try {
    $legacy_xpath->query(new OrderedXPathString(), opaque_xpath_context(7));
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
echo "legacy-null:", $legacy_xpath->query(".", opaque_xpath_context(null))->length, "\n";
echo "legacy-node:", $legacy_xpath->query(".", opaque_xpath_context($legacy_node))->length, "\n";

$modern_document = Dom\XMLDocument::createFromString("<root><item/></root>");
$modern_xpath = new Dom\XPath($modern_document);
$modern_node = $modern_document->documentElement;

try {
    $modern_xpath->query(".", opaque_xpath_context(new stdClass()));
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
try {
    $modern_xpath->evaluate(".", opaque_xpath_context("root"));
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
try {
    $modern_xpath->query(".", opaque_xpath_context(true));
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
echo "modern-null:", $modern_xpath->query(".", opaque_xpath_context(null))->length, "\n";
echo "modern-node:", $modern_xpath->query(".", opaque_xpath_context($modern_node))->length, "\n";
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
            "DOMXPath::query(): Argument #2 ($contextNode) must be of type ?DOMNode, stdClass given\n",
            "DOMXPath::evaluate(): Argument #2 ($contextNode) must be of type ?DOMNode, int given\n",
            "DOMXPath::query(): Argument #2 ($contextNode) must be of type ?DOMNode, array given\n",
            "S|DOMXPath::query(): Argument #2 ($contextNode) must be of type ?DOMNode, int given\n",
            "legacy-null:1\n",
            "legacy-node:1\n",
            "Dom\\XPath::query(): Argument #2 ($contextNode) must be of type ?Dom\\Node, stdClass given\n",
            "Dom\\XPath::evaluate(): Argument #2 ($contextNode) must be of type ?Dom\\Node, string given\n",
            "Dom\\XPath::query(): Argument #2 ($contextNode) must be of type ?Dom\\Node, true given\n",
            "modern-null:1\n",
            "modern-node:1\n",
        )
    );
    assert_eq!(out.stderr, "");
}

/// Verifies caught XPath TypeErrors unwind owned arguments and coerced strings cleanly.
#[test]
fn xpath_dynamic_context_type_errors_are_heap_debug_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function opaque_xpath_heap_value(mixed $value): mixed {
    return $value;
}

class HeapXPathString {
    public function __toString(): string {
        return str_repeat("x", 32);
    }
}

$document = new DOMDocument();
$document->loadXML("<root/>");
$xpath = new DOMXPath($document);
for ($index = 0; $index < 12; $index++) {
    try {
        $xpath->query(
            new HeapXPathString(),
            opaque_xpath_heap_value(42),
        );
    } catch (TypeError $error) {
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
        "expected caught XPath TypeErrors to remain heap-clean, got: {}",
        out.stderr
    );
}
