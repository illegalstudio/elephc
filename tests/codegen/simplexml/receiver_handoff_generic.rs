//! Purpose:
//! Covers actual XML children in ordinary generic and dynamic property storage.
//!
//! Called from:
//! - Managed-libxml2 executable codegen CI with strict heap checks.
//!
//! Key details:
//! - Temporary parent destructors observe the pre-update value for every object-bearing field type.
//! - Existing magic-class entries bypass getters and native object updates need no parent writeback.
//! - Typed XML getter hooks run once, including their null-result failure path.

use crate::support::*;

/// Generic field types and dynamic entries hand off actual XML children in AOT and opaque eval.
#[test]
fn test_simplexml_generic_property_handoff_bundle() {
    if skip_without_xml_native("test_simplexml_generic_property_handoff_bundle") { return; }
    let source = r#"<?php
interface GenericXmlHolderMarker {}
class GenericXmlMixed implements GenericXmlHolderMarker {
    private mixed $view;
    public mixed $items;
    public function __construct(mixed $node) { $this->items = $node; $this->view = $node; }
    public function __destruct() { echo 'mixed:', count($this->view), ':', $this->view[0], '|'; }
}
class GenericXmlObject implements GenericXmlHolderMarker {
    private mixed $view;
    public object $items;
    public function __construct(object $node) { $this->items = $node; $this->view = $node; }
    public function __destruct() { echo 'object:', count($this->view), ':', $this->view[0], '|'; }
}
class GenericXmlCountable implements GenericXmlHolderMarker {
    private mixed $view;
    public Countable $items;
    public function __construct(Countable $node) { $this->items = $node; $this->view = $node; }
    public function __destruct() { echo 'countable:', count($this->view), ':', $this->view[0], '|'; }
}
class GenericXmlIterable implements GenericXmlHolderMarker {
    private mixed $view;
    public iterable $items;
    public function __construct(iterable $node) { $this->items = $node; $this->view = $node; }
    public function __destruct() { echo 'iterable:', count($this->view), ':', $this->view[0], '|'; }
}
class GenericXmlUnion implements GenericXmlHolderMarker {
    private mixed $view;
    public SimpleXMLElement|false $items;
    public function __construct(SimpleXMLElement $node) { $this->items = $node; $this->view = $node; }
    public function __destruct() { echo 'union:', count($this->view), ':', $this->view[0], '|'; }
}
class GenericXmlUntyped implements GenericXmlHolderMarker {
    private mixed $view;
    public $items;
    public function __construct(mixed $node) { $this->items = $node; $this->view = $node; }
    public function __destruct() { echo 'untyped:', count($this->view), ':', $this->view[0], '|'; }
}
class GenericXmlReadonly implements GenericXmlHolderMarker {
    private mixed $view;
    public readonly mixed $items;
    public function __construct(mixed $node) { $this->items = $node; $this->view = $node; }
    public function __destruct() { echo 'readonly:', count($this->view), ':', $this->view[0], '|'; }
}
class GenericXmlPrivateSet implements GenericXmlHolderMarker {
    private mixed $view;
    public private(set) object $items;
    public function __construct(object $node) { $this->items = $node; $this->view = $node; }
    public function __destruct() { echo 'private-set:', count($this->view), ':', $this->view[0], '|'; }
}
#[AllowDynamicProperties]
class GenericXmlDynamic implements GenericXmlHolderMarker {
    public function __destruct() { $items = get_object_vars($this)['items']; echo 'dynamic:', count($items), ':', $items[0], '|'; }
}
#[AllowDynamicProperties]
class GenericXmlGetter implements GenericXmlHolderMarker {
    public function __get(string $name): mixed { echo 'unexpected-get|'; return null; }
    public function __destruct() { $items = get_object_vars($this)['items']; echo 'getter:', count($items), ':', $items[0], '|'; }
}
function GenericXmlSeed(mixed $box, mixed $node): void { $box->items = $node; }
function GenericXmlNode(): SimpleXMLElement { return (new SimpleXMLElement('<r><item>1</item></r>'))->item; }
function GenericXmlMakeMixed(): mixed { return new GenericXmlMixed(GenericXmlNode()); }
function GenericXmlMakeObject(): mixed { return new GenericXmlObject(GenericXmlNode()); }
function GenericXmlMakeCountable(): mixed { return new GenericXmlCountable(GenericXmlNode()); }
function GenericXmlMakeIterable(): mixed { return new GenericXmlIterable(GenericXmlNode()); }
function GenericXmlMakeUnion(): mixed { return new GenericXmlUnion(GenericXmlNode()); }
function GenericXmlMakeUntyped(): mixed { return new GenericXmlUntyped(GenericXmlNode()); }
function GenericXmlMakeReadonly(): mixed { return new GenericXmlReadonly(GenericXmlNode()); }
function GenericXmlMakePrivateSet(): mixed { return new GenericXmlPrivateSet(GenericXmlNode()); }
function GenericXmlMakeDynamic(): mixed { $box = new GenericXmlDynamic(); GenericXmlSeed($box, GenericXmlNode()); return $box; }
function GenericXmlMakeGetter(): mixed { $box = new GenericXmlGetter(); GenericXmlSeed($box, GenericXmlNode()); return $box; }
function GenericXmlKnown(): GenericXmlMixed { return new GenericXmlMixed(GenericXmlNode()); }
function GenericXmlErasedObject(): object { return new GenericXmlObject(GenericXmlNode()); }
function GenericXmlErasedInterface(): GenericXmlHolderMarker { return new GenericXmlCountable(GenericXmlNode()); }
GenericXmlMakeMixed()->items[] = 2;
GenericXmlMakeMixed()->items[0] = 3;
GenericXmlMakeMixed()->items[0] .= 'x';
GenericXmlMakeObject()->items[] = 2;
GenericXmlMakeObject()->items[0] = 3;
GenericXmlMakeObject()->items[0] .= 'x';
GenericXmlMakeCountable()->items[] = 2;
GenericXmlMakeCountable()->items[0] = 3;
GenericXmlMakeCountable()->items[0] .= 'x';
GenericXmlMakeIterable()->items[] = 2;
GenericXmlMakeIterable()->items[0] = 3;
GenericXmlMakeIterable()->items[0] .= 'x';
GenericXmlMakeUnion()->items[] = 2;
GenericXmlMakeUnion()->items[0] = 3;
GenericXmlMakeUnion()->items[0] .= 'x';
GenericXmlMakeUntyped()->items[] = 2;
GenericXmlMakeUntyped()->items[0] = 3;
GenericXmlMakeUntyped()->items[0] .= 'x';
GenericXmlMakeReadonly()->items[] = 2;
GenericXmlMakeReadonly()->items[0] = 3;
GenericXmlMakeReadonly()->items[0] .= 'x';
GenericXmlMakePrivateSet()->items[] = 2;
GenericXmlMakePrivateSet()->items[0] = 3;
GenericXmlMakePrivateSet()->items[0] .= 'x';
GenericXmlMakeDynamic()->items[] = 2;
GenericXmlMakeDynamic()->items[0] = 3;
GenericXmlMakeDynamic()->items[0] .= 'x';
GenericXmlMakeGetter()->items[] = 2;
GenericXmlMakeGetter()->items[0] = 3;
GenericXmlMakeGetter()->items[0] .= 'x';
GenericXmlKnown()->items[] = 2;
GenericXmlErasedObject()->items[] = 2;
GenericXmlErasedInterface()->items[] = 2;
class GenericXmlMutating {
    public function __construct(public mixed $items) {}
    public function __destruct() { $this->items[0] = 'D'; }
}
function GenericXmlMutate(mixed $node): mixed { return new GenericXmlMutating($node); }
$root = new SimpleXMLElement('<r><item>1</item></r>');
GenericXmlMutate($root->item)->items[0] = 'S';
echo 'final:', $root->item[0], '|';
class GenericXmlFallback {
    public function __construct(public mixed $items) {}
    public function __destruct() { echo 'array:', count($this->items), '|'; }
}
function GenericXmlArray(): mixed { return new GenericXmlFallback([1]); }
GenericXmlArray()->items[] = 2;
function GenericXmlBorrowed(object $box): void { $box->items[] = 2; }
$borrowed = new GenericXmlReadonly(GenericXmlNode());
GenericXmlBorrowed($borrowed);
echo 'borrowed:', count($borrowed->items), '|'; unset($borrowed);
class GenericXmlHook {
    private ?SimpleXMLElement $alias;
    public ?SimpleXMLElement $items { get { echo 'hook-get|'; return $this->items; } }
    public function __construct(?SimpleXMLElement $node) { $this->items = $node; $this->alias = $node; }
    public function __destruct() { echo 'hook-drop|'; if ($this->alias !== null) { $this->alias[0] = 'D'; } }
}
function GenericXmlHookKnown(?SimpleXMLElement $node): GenericXmlHook { return new GenericXmlHook($node); }
function GenericXmlHookErased(?SimpleXMLElement $node): mixed { return new GenericXmlHook($node); }
GenericXmlHookKnown($root->item)->items[0] = 'S'; echo 'hook-known:', $root->item[0], '|';
GenericXmlHookErased($root->item)->items[0] = 'S'; echo 'hook-erased:', $root->item[0], '|';
try { GenericXmlHookKnown(null)->items[] = 2; }
catch (Throwable $error) { echo $error->getMessage(), '|'; }
try { GenericXmlHookErased(null)->items[] = 2; }
catch (Throwable $error) { echo $error->getMessage(), '|'; }

$genericKey = 0;
$keyRoot = new SimpleXMLElement('<r><item>A</item><item>B</item></r>');
class GenericXmlKeyHolder {
    public function __construct(public mixed $items) {}
    public function __destruct() { global $genericKey; $genericKey = 1; echo 'key-drop|'; }
}
function GenericXmlKeyMake(mixed $node): mixed { return new GenericXmlKeyHolder($node); }
GenericXmlKeyMake($keyRoot->item)->items[$genericKey] = 'S';
echo 'key:', $keyRoot->item[0], ':', $keyRoot->item[1], ':', $genericKey, '|';
$genericKey = 0;
$keyRoot = new SimpleXMLElement('<r><item>A</item><item>B</item></r>');
GenericXmlKeyMake($keyRoot->item)->items[$genericKey] .= 'S';
echo 'compound-key:', $keyRoot->item[0], ':', $keyRoot->item[1], ':', $genericKey, '|';
GenericXmlMutate($root->item)->items[0] .= 'S'; echo 'compound-final:', $root->item[0], '|';
"#;
    let expected = "mixed:1:1|mixed:1:1|mixed:1:1|object:1:1|object:1:1|object:1:1|countable:1:1|countable:1:1|countable:1:1|iterable:1:1|iterable:1:1|iterable:1:1|union:1:1|union:1:1|union:1:1|untyped:1:1|untyped:1:1|untyped:1:1|readonly:1:1|readonly:1:1|readonly:1:1|private-set:1:1|private-set:1:1|private-set:1:1|dynamic:1:1|dynamic:1:1|dynamic:1:1|getter:1:1|getter:1:1|getter:1:1|mixed:1:1|object:1:1|countable:1:1|final:S|array:1|borrowed:2|readonly:2:1|hook-get|hook-drop|hook-known:S|hook-get|hook-drop|hook-erased:S|hook-get|hook-drop|Indirect modification of GenericXmlHook::$items is not allowed|hook-get|hook-drop|Indirect modification of GenericXmlHook::$items is not allowed|key-drop|key:A:S:1|key-drop|compound-key:A:BS:1|compound-final:DS|";
    let renamed = source.replace("GenericXml", "EvalGenericXml");
    let body = renamed.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let eval = format!("<?php $source = $argc > 0 ? '{body}' : ''; eval($source);");
    let eval_expected = expected.replace("GenericXml", "EvalGenericXml");
    assert_bundle_with(&[
        BundleCase { name: "generic_xml_handoff_native", source, expected },
        BundleCase { name: "generic_xml_handoff_eval", source: &eval, expected: &eval_expected },
    ], |source| {
        let out = compile_and_run_with_heap_debug(source);
        assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        out.stdout
    });
}
