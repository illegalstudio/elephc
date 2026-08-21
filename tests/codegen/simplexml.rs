//! Purpose:
//! End-to-end regressions for SimpleXML loaders and constructor bridge materialization.
//!
//! Called from:
//! - `cargo test --test codegen_tests codegen::simplexml` through Rust's test harness.
//!
//! Key details:
//! - Fixtures cover native parsing, subclass discriminators, runtime exceptions, and wrapper cleanup.

use crate::support::{
    compile_and_run, compile_and_run_capture, compile_and_run_in_dir,
    compile_and_run_with_heap_debug,
};

/// Verifies loaders and direct construction materialize base and user-subclass wrappers.
#[test]
fn simplexml_loaders_and_constructor_materialize_php_classes() {
    let out = compile_and_run(
        r#"<?php
class CustomXml extends SimpleXMLElement {}

$base = simplexml_load_string("<root/>");
if ($base === false) { exit(2); }
$custom = simplexml_load_string("<root/>", CustomXml::class);
if ($custom === false) { exit(3); }
$constructed = new CustomXml("<root/>");
echo get_class($base) . "|" . get_class($custom) . "|" . get_class($constructed);
"#,
    );
    assert_eq!(out, "SimpleXMLElement|CustomXml|CustomXml");
}

/// Verifies loader failure returns false while constructor failure throws `Exception`.
#[test]
fn simplexml_parse_failure_contracts_match_php() {
    let out = compile_and_run_capture(
        r#"<?php
var_dump(@simplexml_load_string("<root>"));
try {
    new SimpleXMLElement("<root>");
} catch (Throwable $error) {
    echo get_class($error) . "|" . $error->getMessage() . "\n";
}
try {
    simplexml_load_string("<root/>", null, 9223372036854775807);
} catch (Throwable $error) {
    echo get_class($error) . "|" . $error->getMessage();
}
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "bool(false)\n",
            "Exception|String could not be parsed as XML\n",
            "ValueError|simplexml_load_string(): Argument #3 ($options) is too large",
        )
    );
}

/// Verifies DOM/SimpleXML interop preserves legacy identity and rejects family conflicts exactly.
#[test]
fn simplexml_dom_interop_preserves_identity_and_claims_one_api_family() {
    let out = compile_and_run(
        r#"<?php
$legacyDocument = new DOMDocument();
$legacyDocument->loadXML('<root/>');
$legacyElement = $legacyDocument->documentElement;
if ($legacyElement === null) { exit(2); }
$legacyView = simplexml_import_dom($legacyElement);
if ($legacyView === null) { exit(3); }
$modernImport = Dom\import_simplexml($legacyView);
echo get_class($modernImport) . "|" . ($modernImport === $legacyElement ? "same" : "different") . "\n";
try {
    dom_import_simplexml($legacyView);
} catch (Throwable $error) {
    echo get_class($error) . "|" . $error->getMessage() . "\n";
}

$freshModernView = simplexml_load_string('<modern/>');
if ($freshModernView === false) { exit(4); }
$modernElement = Dom\import_simplexml($freshModernView);
echo get_class($modernElement) . "\n";
try {
    dom_import_simplexml($freshModernView);
} catch (Throwable $error) {
    echo get_class($error) . "|" . $error->getMessage() . "\n";
}

$freshLegacyView = simplexml_load_string('<legacy/>');
if ($freshLegacyView === false) { exit(5); }
$legacyImport = dom_import_simplexml($freshLegacyView);
echo get_class($legacyImport) . "\n";
try {
    Dom\import_simplexml($freshLegacyView);
} catch (Throwable $error) {
    echo get_class($error) . "|" . $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "DOMElement|same\n",
            "TypeError|dom_import_simplexml(): Argument #1 ($node) must not be already imported as a Dom\\Node\n",
            "Dom\\Element\n",
            "TypeError|dom_import_simplexml(): Argument #1 ($node) must not be already imported as a Dom\\Node\n",
            "DOMElement\n",
            "TypeError|Dom\\import_simplexml(): Argument #1 ($node) must not be already imported as a DOMNode",
        )
    );
}

/// Verifies namespace maps and XPath arrays materialize exact strings and fresh wrappers.
#[test]
fn simplexml_namespace_maps_and_xpath_wrapper_arrays_materialize() {
    let out = compile_and_run_capture(
        r#"<?php
class CustomXPathXml extends SimpleXMLElement {}

$xml = '<root xmlns="urn:d" xmlns:p="urn:p"><p:item>one</p:item><p:item>two</p:item></root>';
$base = simplexml_load_string($xml);
if ($base === false) { exit(2); }
$namespaces = $base->getDocNamespaces();
if ($namespaces === false) { exit(3); }
echo "map|";
echo $namespaces[""] . "|";
echo $namespaces["p"] . "|";
echo ($base->registerXPathNamespace("p", "urn:p") ? "registered" : "failed") . "|";
$nodes = $base->xpath("//p:item");
if ($nodes === false) { exit(4); }
if ($nodes === null) { exit(4); }
$again = $base->xpath("//p:item");
if ($again === false) { exit(5); }
if ($again === null) { exit(5); }
echo count($nodes) . "|" . get_class($nodes[0]) . "|" . $nodes[0]->getName() . "|";
echo ($nodes[0] === $again[0] ? "same" : "fresh") . "|";

$custom = simplexml_load_string($xml, CustomXPathXml::class);
if ($custom === false) { exit(6); }
$custom->registerXPathNamespace("p", "urn:p");
$customNodes = $custom->xpath("//p:item");
if ($customNodes === false) { exit(7); }
if ($customNodes === null) { exit(7); }
echo get_class($customNodes[0]);
"#,
    );
    assert!(
        out.success,
        "program failed after stdout {:?}: {}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "map|urn:d|urn:p|registered|2|SimpleXMLElement|item|fresh|CustomXPathXml"
    );
}

/// Verifies public count, empty selectors, registration failure, and XPath node filtering.
#[test]
fn simplexml_public_method_edge_contracts_match_php() {
    let out = compile_and_run(
        r#"<?php
$xml = simplexml_load_string('<root xmlns="urn:d" xmlns:p="urn:p"><a/><p:b/><c xmlns=""/></root>');
if ($xml === false) { exit(2); }
$omitted = $xml->children();
$empty = $xml->children("");
if ($omitted === null) { exit(3); }
if ($empty === null) { exit(3); }
echo $xml->count() . "|" . $omitted->count() . "|" . $empty->count() . "|";
echo ($xml->registerXPathNamespace("", "urn:p") ? "bad" : "false") . "|";
$root = $xml->xpath("/");
$namespaces = $xml->xpath("//namespace::*");
if ($root === false) { exit(4); }
if ($root === null) { exit(4); }
if ($namespaces === false) { exit(5); }
if ($namespaces === null) { exit(5); }
echo count($root) . "|" . count($namespaces);
"#,
    );
    assert_eq!(out, "2|2|2|false|0|0");
}

/// Verifies namespaced `addChild()` returns the exact php-src view and sibling selection.
#[test]
fn simplexml_add_child_qname_view_matches_php() {
    let out = compile_and_run(
        r#"<?php
$parent = simplexml_load_string('<root xmlns:p="urn:p"/>');
if ($parent === false) { exit(2); }
$first = $parent->addChild('p:c', 'P', 'urn:p');
$second = $parent->addChild('p:c', 'Q', 'urn:p');
if ($first === null || $second === null) { exit(3); }
$children = $parent->children('urn:p');
if ($children === null) { exit(4); }
echo $first->getName() . '|' . (string) $first . '|';
echo count($children->c) . '|' . (string) $children->c[0];
"#,
    );
    assert_eq!(out, "c|P|2|P");
}

/// Verifies eager iterator data keeps one subclass wrapper alive and preserves identity.
#[test]
fn simplexml_iterator_current_has_strong_identity_and_inherited_destructor_order() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class BaseTrackedXml extends SimpleXMLElement {
    public static bool $armed = false;
    public static int $drops = 0;
    public static SimpleXMLElement $root;

    public function __destruct() {
        if (!self::$armed) { return; }
        self::$drops++;
        echo "D|";
        echo (self::$root->current() === $this ? "same" : "different") . "|";
        echo self::$root->key() . "\n";
    }
}
class ChildTrackedXml extends BaseTrackedXml {}

$root = simplexml_load_string(
    '<root><A/><B/></root>',
    ChildTrackedXml::class
);
if ($root === false) { exit(2); }
BaseTrackedXml::$root = $root;
$root->rewind();
$current = $root->current();
echo ($current === $root->getChildren() ? "same" : "different") . "|";
echo get_class($current) . "\n";
unset($current);
echo BaseTrackedXml::$drops . "\n";
BaseTrackedXml::$armed = true;
$root->next();
BaseTrackedXml::$armed = false;
echo $root->current()->getName() . "|" . BaseTrackedXml::$drops . "\n";
"#,
    );
    assert!(
        out.success,
        "program failed after stdout {:?}: {}",
        out.stdout,
        out.stderr
    );
    assert_eq!(
        out.stdout,
        concat!(
            "same|ChildTrackedXml\n",
            "0\n",
            "D|same|A\n",
            "B|1\n",
        )
    );
}

/// Verifies a destructor's re-entrant move supersedes the stale outer move exactly once.
#[test]
fn simplexml_iterator_destructor_reentry_preserves_inner_move() {
    let out = compile_and_run(
        r#"<?php
class ReentrantXml extends SimpleXMLElement {
    public static bool $armed = false;
    public static SimpleXMLElement $root;

    public function __destruct() {
        if (!self::$armed) { return; }
        self::$armed = false;
        echo "D|" . self::$root->key() . "|" . (self::$root->valid() ? "1" : "0") . "\n";
        echo "inner-before|" . self::$root->key() . "\n";
        self::$root->next();
        echo "inner-after|" . self::$root->key() . "\n";
    }
}

$root = simplexml_load_string(
    '<root><A/><B/><C/></root>',
    ReentrantXml::class
);
if ($root === false) { exit(2); }
ReentrantXml::$root = $root;
$root->rewind();
echo "outer-before|" . $root->key() . "\n";
ReentrantXml::$armed = true;
$root->next();
echo "outer-after|" . $root->key() . "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "outer-before|A\n",
            "D|A|1\n",
            "inner-before|A\n",
            "inner-after|B\n",
            "outer-after|B\n",
        )
    );
}

/// Verifies a released iterator wrapper remains a valid native receiver during its destructor.
#[test]
fn simplexml_iterator_destructor_can_call_native_method() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class ReentrantNameXml extends SimpleXMLElement {
    public static bool $armed = false;

    public function __destruct() {
        if (!self::$armed) { return; }
        self::$armed = false;
        echo "D|" . $this->getName() . "\n";
    }
}

$root = simplexml_load_string('<root><A/><B/></root>', ReentrantNameXml::class);
if ($root === false) { exit(2); }
$root->rewind();
echo "before|" . $root->current()->getName() . "\n";
ReentrantNameXml::$armed = true;
$root->next();
echo "after-next|" . $root->current()->getName() . "\n";
ReentrantNameXml::$armed = true;
$root->rewind();
echo "after-rewind|" . $root->current()->getName() . "\n";
ReentrantNameXml::$armed = false;
unset($root);
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
            "before|A\n",
            "D|A\n",
            "after-next|B\n",
            "D|B\n",
            "after-rewind|A\n",
        )
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Verifies SimpleXML cloning deep-copies the view and resets private iterator data.
#[test]
fn simplexml_clone_resets_iterator_without_calling_user_clone_hook() {
    let out = compile_and_run(
        r#"<?php
class CloneXml extends SimpleXMLElement {
    public function __clone() { echo "hook|"; }
}

$source = simplexml_load_string('<root><A/></root>', CloneXml::class);
if ($source === false) { exit(2); }
$source->rewind();
$current = $source->current();
$clone = clone $source;
echo get_class($clone) . "|";
echo ($source === $clone ? "same" : "different") . "|";
echo ($clone->valid() ? "valid" : "invalid") . "|";
try {
    $clone->current();
} catch (Throwable $error) {
    echo get_class($error) . "|" . $error->getMessage() . "|";
}
echo ($clone->asXML() === $source->asXML() ? "xml-same" : "xml-diff") . "|";
$clone->addChild('B');
echo ($clone->asXML() === $source->asXML() ? "linked" : "independent");
"#,
    );
    assert_eq!(
        out,
        concat!(
            "CloneXml|different|invalid|",
            "Error|Iterator not initialized or already consumed|",
            "xml-same|independent",
        )
    );
}

/// Verifies a loader's `SimpleXMLElement|false` result can be cloned without prior narrowing.
#[test]
fn simplexml_clone_accepts_fallible_loader_result() {
    let out = compile_and_run(
        r#"<?php
$clone = clone simplexml_load_string('<root><child/></root>');
echo get_class($clone) . "|" . $clone->getName() . "|" . $clone->child->getName();
"#,
    );
    assert_eq!(out, "SimpleXMLElement|root|child");
}

/// Verifies cloning a realized loader failure throws PHP's exact catchable `TypeError`.
#[test]
fn simplexml_clone_loader_false_throws_exact_type_error_and_cleans_temporary() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
try {
    $clone = clone @simplexml_load_string('<root>');
} catch (TypeError $error) {
    echo get_class($error) . "|" . $error->getMessage();
}
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
        "TypeError|clone(): Argument #1 ($object) must be of type object, false given"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Verifies an inline nullable `children()` view clone retains and releases its receiver once.
#[test]
fn simplexml_clone_inline_children_view_is_heap_debug_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$root = simplexml_load_string('<root><a/><b/></root>');
if ($root === false) { exit(2); }
$clone = clone $root->children();
echo $clone->getName() . "|" . $clone->a->getName() . "|" . $clone->b->getName();
unset($clone);
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stdout, "a|a|b");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Verifies the GC descriptor traces and later releases the hidden iterator wrapper.
#[test]
fn simplexml_iterator_hidden_owner_is_heap_debug_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$root = simplexml_load_string('<root><A/><B/></root>');
if ($root === false) { exit(2); }
$root->rewind();
echo ($root->valid() ? $root->current()->getName() : "invalid") . "|";
$root->next();
echo ($root->valid() ? $root->current()->getName() : "invalid") . "|";
$root->next();
echo ($root->valid() ? "valid" : "end");
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stdout, "A|B|end");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Verifies foreach exposes the exact native current wrapper and preserves dimension ownership.
#[test]
fn simplexml_foreach_current_identity_and_attribute_read_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$xml = simplexml_load_string('<pres><content><file glob="slide_*.xml"/></content></pres>');
if ($xml === false) { exit(2); }
$files = $xml->content->file;
foreach ($files as $file) {
    echo ($file === $files->current() ? 'same' : 'different') . '|';
    echo (string) $file['glob'];
}
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stdout, "same|slide_*.xml");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean iterator-wrapper ownership, got: {}",
        out.stderr
    );
}

/// Verifies foreach honors a SimpleXML subclass override returning a non-wrapper Mixed value.
#[test]
fn simplexml_foreach_honors_userland_current_mixed_override() {
    let out = compile_and_run(
        r#"<?php
class ScalarCurrentXml extends SimpleXMLElement {
    #[\ReturnTypeWillChange]
    public function current(): mixed {
        return 42;
    }
}

$xml = new ScalarCurrentXml('<r><file/></r>');
foreach ($xml as $value) {
    var_dump($value);
}
"#,
    );
    assert_eq!(out, "int(42)\n");
}

/// Verifies property and dimension handlers preserve PHP read/probe/mutation semantics.
#[test]
fn simplexml_object_handlers_read_write_probe_and_unset_match_php() {
    let out = compile_and_run(
        r#"<?php
$xml = simplexml_load_string('<r id="7"><a>one</a><a>two</a><zero>0</zero></r>');
if ($xml === false) { exit(2); }
echo count($xml->a) . '|' . (string) $xml->a[1] . '|';
echo (isset($xml->a) ? '1' : '0') . '|' . (empty($xml->zero) ? '1' : '0') . '|';
echo (isset($xml['id']) ? '1' : '0') . '|';
$xml->new = 'N';
$xml['id'] = '8';
$xml->a[1] = 'T';
echo (string) $xml->new . '|' . (string) $xml['id'] . '|' . (string) $xml->a[1] . '|';
unset($xml->zero);
unset($xml['id']);
echo (isset($xml->zero) ? '1' : '0') . '|' . (isset($xml['id']) ? '1' : '0');
"#,
    );
    assert_eq!(out, "2|two|1|1|1|N|8|T|0|0");
}

/// Verifies scalar casts and same-class node comparison use SimpleXML object handlers.
#[test]
fn simplexml_object_handlers_cast_and_compare_match_php() {
    let out = compile_and_run(
        r#"<?php
$xml = simplexml_load_string('<r><a>12.5</a><a>z</a></r>');
if ($xml === false) { exit(2); }
$first = $xml->a[0];
$again = $xml->a[0];
$second = $xml->a[1];
var_dump((string) $first, (bool) $first, (int) $first, (float) $first);
var_dump($first == $again, $first != $again, $first <=> $again);
var_dump($first == $second, $first != $second, $first <=> $second);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "string(4) \"12.5\"\n",
            "bool(true)\n",
            "int(12)\n",
            "float(12.5)\n",
            "bool(true)\n",
            "bool(false)\n",
            "int(0)\n",
            "bool(false)\n",
            "bool(true)\n",
            "int(1)\n",
        )
    );
}

/// Verifies php-src PHPT 033's scalar, array, and object casts over SimpleXML instances.
#[test]
fn simplexml_phpt_033_casting_instances_matches_php() {
    let mut out = compile_and_run(
        r#"<?php
$xml = <<<EOF
<people>
test
  <person name="Joe"/>
  <person name="John">
    <children>
      <person name="Joe"/>
    </children>
  </person>
  <person name="Jane"/>
</people>
EOF;

$foo = simplexml_load_string("<foo />");
$people = simplexml_load_string($xml);
var_dump((bool) $foo);
var_dump((bool) $people);
var_dump((int) $foo);
var_dump((int) $people);
var_dump((float) $foo);
var_dump((float) $people);
var_dump((string) $foo);
var_dump((string) $people);
var_dump((array) $foo);
var_dump((array) $people);
var_dump((object) $foo);
var_dump((object) $people);
"#,
    );
    for object_id in (1..=32).rev() {
        out = out.replace(&format!("#{object_id} "), "#%d ");
    }
    assert_eq!(
        out,
        concat!(
            r#"bool(false)
bool(true)
int(0)
int(0)
float(0)
float(0)
string(0) ""
string(15) "
test
"#,
            "\x20\x20\n\x20\x20\n\x20\x20\n",
            r#""
array(0) {
}
array(1) {
  ["person"]=>
  array(3) {
    [0]=>
    object(SimpleXMLElement)#%d (1) {
      ["@attributes"]=>
      array(1) {
        ["name"]=>
        string(3) "Joe"
      }
    }
    [1]=>
    object(SimpleXMLElement)#%d (2) {
      ["@attributes"]=>
      array(1) {
        ["name"]=>
        string(4) "John"
      }
      ["children"]=>
      object(SimpleXMLElement)#%d (1) {
        ["person"]=>
        object(SimpleXMLElement)#%d (1) {
          ["@attributes"]=>
          array(1) {
            ["name"]=>
            string(3) "Joe"
          }
        }
      }
    }
    [2]=>
    object(SimpleXMLElement)#%d (1) {
      ["@attributes"]=>
      array(1) {
        ["name"]=>
        string(4) "Jane"
      }
    }
  }
}
object(SimpleXMLElement)#%d (0) {
}
object(SimpleXMLElement)#%d (1) {
  ["person"]=>
  array(3) {
    [0]=>
    object(SimpleXMLElement)#%d (1) {
      ["@attributes"]=>
      array(1) {
        ["name"]=>
        string(3) "Joe"
      }
    }
    [1]=>
    object(SimpleXMLElement)#%d (2) {
      ["@attributes"]=>
      array(1) {
        ["name"]=>
        string(4) "John"
      }
      ["children"]=>
      object(SimpleXMLElement)#%d (1) {
        ["person"]=>
        object(SimpleXMLElement)#%d (1) {
          ["@attributes"]=>
          array(1) {
            ["name"]=>
            string(3) "Joe"
          }
        }
      }
    }
    [2]=>
    object(SimpleXMLElement)#%d (1) {
      ["@attributes"]=>
      array(1) {
        ["name"]=>
        string(4) "Jane"
      }
    }
  }
}
"#
        )
    );
}

/// Verifies php-src PHPT 034 can replace a selected wrapper with a three-entry array.
#[test]
fn simplexml_phpt_034_cast_to_array_matches_php() {
    let out = compile_and_run(
        r#"<?php
$string = '<?xml version="1.0"?>
<foo><bar>
   <p>Blah 1</p>
   <p>Blah 2</p>
   <p>Blah 3</p>
   <tt>Blah 4</tt>
</bar></foo>
';
$foo = simplexml_load_string($string);
$p = $foo->bar->p;
echo count($p);
$p = (array) $foo->bar->p;
echo count($p);
"#,
    );
    assert_eq!(out, "33");
}

/// Verifies recovered rootless SimpleXML documents preserve php-src's method-specific results.
#[test]
fn simplexml_recovered_rootless_methods_match_php() {
    let out = compile_and_run(
        r#"<?php
$namespaces = @new SimpleXMLElement("X", 1);
var_dump($namespaces->getDocNamespaces());

const XML_PARSE_RECOVER = 1;
$xml = @simplexml_load_string("XXXXXXX^", 'SimpleXMLElement', XML_PARSE_RECOVER);
try {
    var_dump($xml->xpath("BBBB"));
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "bool(false)\nSimpleXMLElement is not properly initialized\n"
    );
}

/// Verifies SimpleXML cast aliases and projected wrapper arrays release every owned value.
#[test]
fn simplexml_object_and_array_casts_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$xml = simplexml_load_string('<people><person name="Joe"/><person name="John"/><person name="Jane"/></people>');
$object = (object) $xml;
$array = (array) $xml;
echo ($object === $xml ? 'same' : 'different') . '|';
echo count($array['person']) . '|' . get_class($array['person'][0]);
unset($xml, $object, $array);
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stdout, "same|3|SimpleXMLElement");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean SimpleXML cast ownership, got: {}",
        out.stderr
    );
}

/// Verifies loader-failure unions use PHP boolean comparison instead of native wrapper marshalling.
#[test]
fn simplexml_fallible_object_comparisons_match_php_and_remain_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
libxml_use_internal_errors(true);
$failed = simplexml_load_string('<');
$empty = simplexml_load_string('<x/>');
$text = simplexml_load_string('<x>1</x>');
$directEmpty = new SimpleXMLElement('<x/>');
$directText = new SimpleXMLElement('<x>1</x>');
var_dump($empty == $failed, $empty != $failed, $empty < $failed, $empty <= $failed);
var_dump($empty > $failed, $empty >= $failed, $empty <=> $failed);
var_dump($failed == $text, $failed != $text, $failed < $text, $failed <= $text);
var_dump($failed > $text, $failed >= $text, $failed <=> $text);
var_dump($directEmpty == false, false == $directEmpty, $directEmpty <=> false);
var_dump($directText == true, true == $directText, true <=> $directText);
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
            "bool(true)\n",
            "bool(false)\n",
            "bool(false)\n",
            "bool(true)\n",
            "bool(false)\n",
            "bool(true)\n",
            "int(0)\n",
            "bool(false)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(false)\n",
            "bool(false)\n",
            "int(-1)\n",
            "bool(true)\n",
            "bool(true)\n",
            "int(0)\n",
            "bool(true)\n",
            "bool(true)\n",
            "int(0)\n",
        )
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean fallible comparison ownership, got: {}",
        out.stderr
    );
}

/// Verifies distinct SimpleXML subclasses use their shared native comparison handler.
#[test]
fn simplexml_subclass_comparisons_match_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class CompareA extends SimpleXMLElement {}
class CompareB extends SimpleXMLElement {}

$document = new DOMDocument();
$document->loadXML('<x/>');
$element = $document->documentElement;
if ($element === null) { exit(2); }
$a = simplexml_import_dom($element, CompareA::class);
$b = simplexml_import_dom($element, CompareB::class);
if ($a === null || $b === null) { exit(3); }
var_dump($a == $b, $a != $b, $a <= $b, $a >= $b, $a <=> $b);
unset($a, $b, $element, $document);
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
            "bool(true)\n",
            "bool(false)\n",
            "bool(true)\n",
            "bool(true)\n",
            "int(0)\n",
        )
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean DOM/SimpleXML interop ownership, got: {}",
        out.stderr
    );
}

/// Verifies SimpleXML arithmetic uses php-src's dynamic `_IS_NUMBER` cast and writes back.
#[test]
fn simplexml_numeric_arithmetic_and_compound_dimension_write_match_php() {
    let out = compile_and_run(
        r#"<?php
$xml = simplexml_load_string('<r><a>30</a><b>12.5</b><c>1e3</c><d/></r>');
if ($xml === false) { exit(2); }
var_dump($xml->a + 5, $xml->b + 0.5, $xml->c + 5, $xml->d + 5, $xml->a['missing'] + 5);
libxml_use_internal_errors(true);
var_dump(simplexml_load_string('<') + 5);
$xml->a['value'] = 30;
$xml->a['value'] += 5;
echo (string) $xml->a['value'];
"#,
    );
    assert_eq!(
        out,
        concat!(
            "int(35)\n",
            "float(13)\n",
            "float(1005)\n",
            "int(5)\n",
            "int(5)\n",
            "int(5)\n",
            "35",
        )
    );
}

/// Verifies dimension writes apply PHP scalar-to-string conversion before the bridge call.
#[test]
fn simplexml_dimension_writes_stringify_scalar_values_like_php() {
    let out = compile_and_run(
        r#"<?php
$xml = simplexml_load_string('<r><v/></r>');
$xml->v['i'] = 42;
$xml->v['f'] = 1.5;
$xml->v['t'] = true;
$xml->v['false'] = false;
$xml->v['null'] = null;
echo $xml->asXML();
"#,
    );
    assert_eq!(
        out,
        "<?xml version=\"1.0\"?>\n<r><v i=\"42\" f=\"1.5\" t=\"1\" false=\"\" null=\"\"/></r>\n"
    );
}

/// Verifies loose wrapper/string comparisons cast only the SimpleXML operand like PHP.
#[test]
fn simplexml_dimension_wrapper_string_comparisons_match_php() {
    let out = compile_and_run(
        r#"<?php
$xml = simplexml_load_string('<root><name attr="foo" number="1">bar</name></root>');
$attr = $xml->name['attr'];
$number = $xml->name['number'];
$missing = $xml->name['missing'];
var_dump($attr == 'foo', 'foo' == $attr, $attr != 'foo', $attr === 'foo');
var_dump($number == '01', $missing == '');
"#,
    );
    assert_eq!(
        out,
        concat!(
            "bool(true)\n",
            "bool(true)\n",
            "bool(false)\n",
            "bool(false)\n",
            "bool(true)\n",
            "bool(true)\n",
        )
    );
}

/// Verifies fallible loader unions route chained selectors and string coercion through handlers.
#[test]
fn simplexml_fallible_loader_property_chains_match_php_without_manual_narrowing() {
    let out = compile_and_run(
        r#"<?php
$xml = simplexml_load_string('<root><child><leaf>A</leaf></child><child><leaf>B</leaf></child></root>');
echo count($xml->child) . '|';
echo (string) $xml->child[0]->leaf . '|';
echo (string) $xml->child[1]->leaf . '|';
echo trim($xml->child[0]->leaf);
"#,
    );
    assert_eq!(out, "2|A|B|A");
}

/// Verifies `isset()` probes the native handler on an unguarded loader union.
#[test]
fn simplexml_fallible_loader_property_isset_matches_php_without_manual_narrowing() {
    let out = compile_and_run(
        r#"<?php
$xml = simplexml_load_string('<root><exists>foo</exists></root>');
echo (isset($xml->exists) ? '1' : '0') . '|';
echo (isset($xml->missing) ? '1' : '0') . '|';
libxml_use_internal_errors(true);
$invalid = simplexml_load_string('<');
echo (isset($invalid->missing) ? '1' : '0') . '|';
echo (empty($invalid->missing) ? '1' : '0');
"#,
    );
    assert_eq!(out, "1|0|0|1");
}

/// Verifies unset dispatch survives chained SimpleXML selectors and nullable method results.
#[test]
fn simplexml_chained_dimension_unset_matches_php() {
    let out = compile_and_run(
        r#"<?php
$xml = simplexml_load_string('<root id="a"><child>x</child><child>y</child></root>');
unset($xml->child[1]);
echo (isset($xml->child[1]) ? '1' : '0') . '|';
$attributes = $xml->attributes();
unset($attributes[0]);
echo (isset($attributes[0]) ? '1' : '0');
"#,
    );
    assert_eq!(out, "0|0");
}

/// Verifies a property-dimension write reaches SimpleXML child autovivification.
#[test]
fn simplexml_property_dimension_write_autovivifies_missing_child() {
    let out = compile_and_run(
        r#"<?php
$xml = simplexml_load_string('<people/>');
$xml->person['name'] = 'John';
echo $xml->asXML();
"#,
    );
    assert_eq!(
        out,
        "<?xml version=\"1.0\"?>\n<people><person name=\"John\"/></people>\n"
    );
}

/// Verifies chained SimpleXML property writes materialize every missing parent view.
#[test]
fn simplexml_chained_property_write_autovivifies_missing_parents() {
    let out = compile_and_run_capture(
        r#"<?php
$xml = simplexml_load_string('<root/>');
$xml->bla->posts->name = 'FooBar';
echo $xml->asXML();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "<?xml version=\"1.0\"?>\n<root><bla><posts><name>FooBar</name></posts></bla></root>\n"
    );
}

/// Verifies indexed chained SimpleXML writes materialize missing property parents.
#[test]
fn simplexml_chained_indexed_property_write_autovivifies_missing_parents() {
    let out = compile_and_run_capture(
        r#"<?php
$xml = simplexml_load_string('<root/>');
var_dump(isset($xml->bla->posts));
$xml->bla->posts[0]->name = 'FooBar';
echo $xml->asXML();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(false)\n<?xml version=\"1.0\"?>\n<root><bla><posts><name>FooBar</name></posts></bla></root>\n"
    );
}

/// Verifies ordinary absent-property reads preserve SimpleXML's empty-view semantics.
#[test]
fn simplexml_ordinary_missing_property_read_does_not_autovivify() {
    let out = compile_and_run(
        r#"<?php
$xml = simplexml_load_string('<root/>');
echo (string) $xml->missing . '|';
echo $xml->asXML();
"#,
    );
    assert_eq!(out, "|<?xml version=\"1.0\"?>\n<root/>\n");
}

/// Verifies an untyped closure parameter can use guarded Mixed SimpleXML offsets.
#[test]
fn simplexml_untyped_closure_parameter_reads_dimension_through_mixed_dispatch() {
    let out = compile_and_run(
        r#"<?php
$xml = simplexml_load_string('<root attr="value"/>');
$callback = function ($node) {
    return (bool) $node['attr'];
};
var_dump($callback($xml));
"#,
    );
    assert_eq!(out, "bool(true)\n");
}

/// Replays php-src bug55098 through untyped closures without losing the selected
/// SimpleXML view during nested method calls, object-handler mutations, or iteration.
#[test]
fn simplexml_bug55098_untyped_callbacks_preserve_iteration_and_mutation_contract() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$xmlString = "<root><a><b>1</b><b>2</b><b>3</b></a></root>";
$xml = simplexml_load_string($xmlString);

$nodes = $xml->a->b;

function test($nodes, $name, $callable) {
    echo "--- $name ---\n";
    foreach ($nodes as $nodeData) {
        echo "nodeData: " . $nodeData . "\n";
        $callable($nodes);
    }
}

test($nodes, "asXml", fn ($n) => $n->asXml());
test($nodes, "attributes", fn ($n) => $n->attributes());
test($nodes, "children", fn ($n) => $n->children());
test($nodes, "getNamespaces", fn ($n) => $n->getNamespaces());
test($nodes, "xpath", fn ($n) => $n->xpath("/root/a/b"));
test($nodes, "var_dump", fn ($n) => var_dump($n));
test($nodes, "manipulation combined with querying", function ($n) {
    $n->addAttribute("attr", "value");
    (bool) $n["attr"];
    $n->addChild("child", "value");
    $n->outer[]->inner = "foo";
    (bool) $n->outer;
    (bool) $n;
    isset($n->outer);
    isset($n["attr"]);
    unset($n->outer);
    unset($n["attr"]);
    unset($n->child);
});
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "--- asXml ---\n",
            "nodeData: 1\nnodeData: 2\nnodeData: 3\n",
            "--- attributes ---\n",
            "nodeData: 1\nnodeData: 2\nnodeData: 3\n",
            "--- children ---\n",
            "nodeData: 1\nnodeData: 2\nnodeData: 3\n",
            "--- getNamespaces ---\n",
            "nodeData: 1\nnodeData: 2\nnodeData: 3\n",
            "--- xpath ---\n",
            "nodeData: 1\nnodeData: 2\nnodeData: 3\n",
            "--- var_dump ---\n",
            "nodeData: 1\n",
            "object(SimpleXMLElement)#3 (3) {\n",
            "  [0]=>\n  string(1) \"1\"\n",
            "  [1]=>\n  string(1) \"2\"\n",
            "  [2]=>\n  string(1) \"3\"\n}\n",
            "nodeData: 2\n",
            "object(SimpleXMLElement)#3 (3) {\n",
            "  [0]=>\n  string(1) \"1\"\n",
            "  [1]=>\n  string(1) \"2\"\n",
            "  [2]=>\n  string(1) \"3\"\n}\n",
            "nodeData: 3\n",
            "object(SimpleXMLElement)#3 (3) {\n",
            "  [0]=>\n  string(1) \"1\"\n",
            "  [1]=>\n  string(1) \"2\"\n",
            "  [2]=>\n  string(1) \"3\"\n}\n",
            "--- manipulation combined with querying ---\n",
            "nodeData: 1\nnodeData: 2\nnodeData: 3\n",
        )
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected bug55098 callback ownership to be balanced, got: {}",
        out.stderr
    );
}

/// Verifies every SimpleXML object-handler family remains available to an untyped
/// closure parameter while ordinary Mixed dispatch keeps the receiver dynamic.
#[test]
fn simplexml_untyped_closure_parameter_supports_methods_mutation_casts_and_probes() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$xml = simplexml_load_string('<root id="before"/>');
$operate = function ($node): void {
    echo $node->getName() . '|';
    echo (string) $node['id'] . '|';
    $node->status = 'active';
    $node->child->name = 'Ada';
    $node->outer[]->inner = 'foo';
    echo count((array) $node) . '|';
    echo get_class((object) $node) . '|';
    echo (isset($node->outer) ? '1' : '0');
    echo (isset($node['id']) ? '1' : '0') . '|';
    unset($node->outer);
    unset($node['id']);
    echo $node->asXML();
};
$operate($xml);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "root|before|4|SimpleXMLElement|11|",
            "<?xml version=\"1.0\"?>\n",
            "<root><status>active</status><child><name>Ada</name></child></root>\n",
        )
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected untyped closure handler temporaries to be balanced, got: {}",
        out.stderr
    );
}

/// Ensures the generic Mixed fallback remains usable for non-SimpleXML values passed to
/// the same untyped closure shape; no static SimpleXML assumption may leak into it.
#[test]
fn simplexml_untyped_closure_dimension_dispatch_keeps_generic_mixed_array_fallback() {
    let out = compile_and_run(
        r#"<?php
$read = function ($value) {
    return $value['name'];
};
echo $read(['name' => 'array']) . '|';
$xml = simplexml_load_string('<root name="xml"/>');
echo $read($xml);
"#,
    );
    assert_eq!(out, "array|xml");
}

/// Verifies nested SimpleXML append dimensions materialize a child before writing its property.
#[test]
fn simplexml_chained_append_dimension_write_matches_php() {
    let out = compile_and_run_capture(
        r#"<?php
$xml = simplexml_load_string('<root/>');
$xml->bla->posts[]->name = 'FooBar';
echo $xml->asXML();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "<?xml version=\"1.0\"?>\n<root><bla><posts><name>FooBar</name></posts></bla></root>\n"
    );
}

/// Verifies a chained append adds one sibling instead of rewriting an existing named child.
#[test]
fn simplexml_chained_append_dimension_write_adds_to_existing_children() {
    let out = compile_and_run_capture(
        r#"<?php
$xml = simplexml_load_string('<root><bla><posts><name>old</name></posts></bla></root>');
$xml->bla->posts[]->name = 'new';
echo $xml->asXML();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "<?xml version=\"1.0\"?>\n<root><bla><posts><name>old</name></posts><posts><name>new</name></posts></bla></root>\n"
    );
}

/// Verifies a literal null dimension does not autovivify the empty append path.
#[test]
fn simplexml_null_dimension_read_is_not_append() {
    let out = compile_and_run(
        r#"<?php
$xml = simplexml_load_string('<root><p>old</p></root>');
var_dump($xml->p[null]);
echo $xml->asXML();
"#,
    );
    assert_eq!(
        out,
        "NULL\n<?xml version=\"1.0\"?>\n<root><p>old</p></root>\n"
    );
}

/// Verifies numeric write gaps carry exact callable, source, line, and suppression context.
#[test]
fn simplexml_numeric_write_gap_warning_uses_php_callsite_context() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$xml = new SimpleXMLElement('<r><a>one</a><a>two</a></r>');
$xml->a[3] = 'three';
function add_gap(SimpleXMLElement $xml, int $index, string $value): void
{
    $xml->a[$index] = $value;
}
add_gap($xml, 4, 'four');
@add_gap($xml, 7, 'seven');
echo count($xml->a) . '|' . (string) $xml->a[2] . '|' . (string) $xml->a[3] . '|' . (string) $xml->a[4];
"#,
    );
    let source = dir.join("test.php");
    assert_eq!(
        out,
        format!(
            "\nWarning: main(): Cannot add element a number 3 when only 2 such elements exist in {} on line 3\n\nWarning: add_gap(): Cannot add element a number 4 when only 3 such elements exist in {} on line 6\n5|three|four|seven",
            source.display(),
            source.display(),
        )
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies XPath warnings preserve the method prefix and add exact call-site locations.
#[test]
fn simplexml_xpath_warnings_use_php_callsite_locations() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$xml = simplexml_load_string('<r/>');
if ($xml === false) { exit(2); }
var_dump($xml->xpath('***'));
function invalid_xpath(SimpleXMLElement $xml): void
{
    var_dump($xml->xpath('**'));
}
invalid_xpath($xml);
@$xml->xpath('***');
echo 'done';
"#,
    );
    let source = dir.join("test.php");
    assert_eq!(
        out,
        format!(
            "\nWarning: SimpleXMLElement::xpath(): XPath expression must return a node set, number returned in {} on line 4\nbool(false)\n\nWarning: SimpleXMLElement::xpath(): Invalid expression in {} on line 7\nbool(false)\ndone",
            source.display(),
            source.display(),
        )
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies mutator warnings preserve method names and exact call-site locations.
#[test]
fn simplexml_add_mutator_warnings_use_php_callsite_locations() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$xml = simplexml_load_string('<root id="1"/>');
$xml->addAttribute('id', '2');
$attributes = $xml->attributes();
$attributes->addChild('child');
@$xml->addAttribute('id', '3');
echo $xml->asXML();
"#,
    );
    let source = dir.join("test.php");
    assert_eq!(
        out,
        format!(
            "\nWarning: SimpleXMLElement::addAttribute(): Attribute already exists in {} on line 3\n\nWarning: SimpleXMLElement::addChild(): Cannot add element to attributes in {} on line 5\n<?xml version=\"1.0\"?>\n<root id=\"1\"/>\n",
            source.display(),
            source.display(),
        )
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies untyped parameters and foreach values retain SimpleXML handlers and balanced ownership.
#[test]
fn simplexml_dynamic_mixed_parameter_and_foreach_dispatch_match_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function inspect($xml): void {
    foreach ($xml->children() as $person) {
        echo (string) $person['name'] . ':' . count($person) . ':';
        foreach ($person->children() as $child) {
            echo (string) $child['name'] . ',';
        }
        echo ';';
    }
    echo '|';
    for ($i = 0; $i < count($xml->person); $i++) {
        echo (string) $xml->person[$i]['name'] . ';';
    }
}

inspect(simplexml_load_string(
    '<people><person name="Joe"><child name="Ann"/><child name="Marray"/></person><person name="Boe"><child name="Joe"/><child name="Ann"/></person></people>'
));
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stdout, "Joe:2:Ann,Marray,;Boe:2:Joe,Ann,;|Joe;Boe;");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean dynamic SimpleXML ownership, got: {}",
        out.stderr
    );
}

/// Verifies dynamic iterator values are released on exhaustion, break, and return.
#[test]
fn simplexml_dynamic_mixed_foreach_source_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function inspect($xml): void {
    foreach ($xml as $person) {
        echo (string) $person['name'];
    }
}

function stop($xml): void {
    foreach ($xml as $person) {
        echo (string) $person['name'];
        break;
    }
}

function stopWithReturn($xml): void {
    foreach ($xml as $person) {
        echo (string) $person['name'];
        return;
    }
}

inspect(simplexml_load_string('<people><person name="Joe"/><person name="Boe"/></people>'));
echo '|';
stop(simplexml_load_string('<people><person name="Ann"/><person name="Marray"/></people>'));
echo '|';
stopWithReturn(simplexml_load_string('<people><person name="One"/><person name="Two"/></people>'));
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stdout, "JoeBoe|Ann|One");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean borrowed dynamic iterator ownership, got: {}",
        out.stderr
    );
}

/// Verifies direct and loader-union SimpleXML values use the native count handler.
#[test]
fn simplexml_count_accepts_fallible_loaders_and_selected_children() {
    let out = compile_and_run(
        r#"<?php
$people = simplexml_load_string('<people><person/><person/></people>');
echo count($people) . '|' . count($people->person);
"#,
    );
    assert_eq!(out, "2|2");
}

/// Verifies the failed loader arm of SimpleXML count raises PHP's catchable TypeError.
#[test]
fn simplexml_count_rejects_a_failed_loader_at_runtime() {
    let out = compile_and_run(
        r#"<?php
libxml_use_internal_errors(true);
$xml = simplexml_load_string('<');
try {
    count($xml);
} catch (TypeError $error) {
    echo $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "count(): Argument #1 ($value) must be of type Countable|array, false given"
    );
}

/// Verifies XPath array, false, and null arms match PHP and release every owned result.
#[test]
fn simplexml_xpath_fallible_array_count_matches_php_and_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
libxml_use_internal_errors(true);
$xml = simplexml_load_string('<root id="1"><child/></root>');
echo count($xml->xpath('/root/child')) . '|';
echo count($xml->xpath('/root/missing')) . '|';
try {
    count($xml->xpath('//*['));
} catch (TypeError $error) {
    echo $error->getMessage() . '|';
}
try {
    count($xml->attributes()->xpath('.'));
} catch (TypeError $error) {
    echo $error->getMessage();
}
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
        "1|0|count(): Argument #1 ($value) must be of type Countable|array, false given|count(): Argument #1 ($value) must be of type Countable|array, null given"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean fallible XPath count ownership, got: {}",
        out.stderr
    );
}

/// Verifies a user override can call the bodyless native base `count()` method.
#[test]
fn simplexml_count_override_can_call_parent_native_method() {
    let out = compile_and_run(
        r#"<?php
class CountingXml extends SimpleXMLElement {
    public function count(): int {
        echo "Called Count!\n";
        return parent::count();
    }
}

$xml = new CountingXml('<root><child/><child/></root>');
var_dump(count($xml));
"#,
    );
    assert_eq!(out, "Called Count!\nint(2)\n");
}

/// Verifies dynamic handler names, by-reference receivers, absent views, and
/// iterator boundary errors all preserve php-src's independent state machines.
#[test]
fn simplexml_handler_state_matrix_matches_php_for_dynamic_and_by_ref_access() {
    let out = compile_and_run(
        r#"<?php
function rewrite_entry(SimpleXMLElement &$view, string $name): void {
    $view->$name[1] = 'B';
}

$xml = simplexml_load_string('<r><entry>A</entry><entry>C</entry></r>');
if ($xml === false) { exit(2); }
$name = 'entry';
rewrite_entry($xml, $name);
$missing = $xml->{'missing'};
echo (string) $xml->$name[1] . '|';
echo (isset($xml->{'missing'}) ? 'set' : 'unset') . '|';
echo (empty($xml->{'missing'}) ? 'empty' : 'full') . '|';
echo ((bool) $missing ? 'true' : 'false') . '|' . count($missing) . '|';

$iterator = simplexml_load_string('<r><a>A</a><b>B</b></r>');
if ($iterator === false) { exit(3); }
$iterator->rewind();
echo ($iterator->valid() ? 'V' : 'I') . '|' . $iterator->key() . '|';
echo $iterator->current()->getName() . '|';
$iterator->next();
echo ($iterator->valid() ? 'V' : 'I') . '|' . $iterator->key() . '|';
echo $iterator->current()->getName() . '|';
$iterator->next();
echo ($iterator->valid() ? 'V' : 'I') . '|';
try {
    $iterator->key();
} catch (Throwable $error) {
    echo get_class($error) . '|' . $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "B|unset|empty|false|0|V|a|a|V|b|b|I|Error|Iterator not initialized or already consumed"
    );
}

/// Verifies direct, nested, and user-subclass SimpleXML serialization are all
/// prohibited, even when the subclass defines a PHP `__serialize()` method.
#[test]
fn simplexml_serialization_restrictions_ignore_subclass_serialize_hooks_and_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class SerializeOverrideXml extends SimpleXMLElement {
    public function __serialize(): array {
        return ['name' => $this->getName()];
    }
}

$base = simplexml_load_string('<root><child/></root>');
if ($base === false) { exit(2); }
$subclass = new SerializeOverrideXml('<subclass/>');
foreach ([$base, $subclass, ['nested' => $base]] as $value) {
    try {
        serialize($value);
    } catch (Throwable $error) {
        echo get_class($error) . '|' . $error->getMessage() . "\n";
    }
}
unset($base, $subclass);
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
            "Exception|Serialization of 'SimpleXMLElement' is not allowed\n",
            "Exception|Serialization of 'SerializeOverrideXml' is not allowed\n",
            "Exception|Serialization of 'SimpleXMLElement' is not allowed\n",
        )
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean serialization rejection ownership, got: {}",
        out.stderr
    );
}

/// Verifies imported subclass views stay live through DOM mutation and detachment,
/// while a SimpleXML clone owns an independent document graph and releases cleanly.
#[test]
fn simplexml_dom_imported_subclass_mutation_detach_and_clone_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class ImportedXml extends SimpleXMLElement {}

$document = new DOMDocument();
$document->loadXML('<root><child>old</child></root>');
$root = $document->documentElement;
if ($root === null) { exit(2); }
$xml = simplexml_import_dom($root, ImportedXml::class);
if ($xml === null) { exit(3); }
$xml->child = 'new';
$clone = clone $xml;
$clone->addChild('clone', 'yes');
$document->removeChild($root);
$xml->child = 'detached';
echo get_class($xml) . '|';
echo $document->saveXML($root) . '|';
echo $clone->asXML() . '|';
echo ($root->parentNode === null ? 'detached' : 'attached');
unset($clone, $xml, $root, $document);
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
            "ImportedXml|<root><child>detached</child></root>|",
            "<?xml version=\"1.0\"?>\n<root><child>new</child><clone>yes</clone></root>\n",
            "|detached",
        )
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean interop-detach clone ownership, got: {}",
        out.stderr
    );
}

/// Verifies a rejected cross-family import leaves the already-claimed modern
/// wrapper usable and releases every temporary participating in the failure.
#[test]
fn simplexml_cross_family_import_failure_preserves_claim_and_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$xml = simplexml_load_string('<root><child/></root>');
if ($xml === false) { exit(2); }
$modern = Dom\import_simplexml($xml);
try {
    dom_import_simplexml($xml);
} catch (Throwable $error) {
    echo get_class($modern) . '|';
    echo get_class($error) . '|' . $error->getMessage() . '|';
}
echo $modern->nodeName;
unset($modern, $xml);
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
        "Dom\\Element|TypeError|dom_import_simplexml(): Argument #1 ($node) must not be already imported as a Dom\\Node|root"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean rejected-family import ownership, got: {}",
        out.stderr
    );
}
