//! Purpose:
//! Covers write-fetch semantics for ordinary overloaded property appends.
//!
//! Called from:
//! - Native and opaque-eval runtime GC tests in CI.
//!
//! Key details:
//! - Appends bypass __set, while missing value getters never receive array writeback.
//! - Existing null entries and error-handler reentry retain COW and cleanup.
//! - Missing ordinary getters fail explicitly before invoking unsupported write fetches.

use crate::support::*;

/// Ordinary erased appends bypass setters and diagnose missing getter storage without XML.
#[test]
fn test_magic_property_append_fetch_bundle() { verify(false); }

/// XML capability preserves the ordinary append subset and its explicit getter refusal.
#[test]
fn test_magic_property_xml_append_fetch_bundle() {
    if skip_without_xml_native("test_magic_property_xml_append_fetch_bundle") { return; }
    verify(true);
}

/// Shares the same getter/setter bodies between compiled PHP and opaque eval.
fn verify(xml: bool) {
    let source = r#"<?php
set_error_handler(function(int $level, string $message): bool { echo $level, ':', $message, '|'; return true; });
interface AppendMagicMarker {}
#[AllowDynamicProperties]
class AppendMagicS implements AppendMagicMarker {
    public int $sets = 0;
    public function __set(string $name, mixed $value): void { $this->sets++; $this->$name = $value; }
}
#[AllowDynamicProperties]
class AppendMagicU implements AppendMagicMarker {
    public int $sets = 0;
    public function __set(string $name, mixed $value): void { $this->sets++; }
}
class AppendMagicT implements AppendMagicMarker {
    public int $sets = 0;
    public function __set(string $name, mixed $value): void { $this->sets++; $this->$name = $value; }
}
#[AllowDynamicProperties]
class AppendMagicValue implements AppendMagicMarker {
    public mixed $backing;
    public function __construct(mixed $backing) { $this->backing = $backing; }
    public function __get(string $name): mixed { echo 'get:', $name, '|'; return $this->backing; }
    public function __isset(string $name): bool { echo 'unexpected-isset|'; return false; }
}
class AppendMagicNoHash {
    public function __get(string $name): mixed { echo 'get-nohash|'; return [5]; }
}
class AppendMagicCreate {}
class AppendMagicProtected {
    protected array $items = [1];
    public function size(): int { return count($this->items); }
    public function appendAllowed(): void { $this->items[] = 3; }
}
class AppendMagicPrivate {
    private array $items = [1];
    public function size(): int { return count($this->items); }
    public function appendAllowed(): void { $this->items[] = 3; }
}
class AppendMagicProtectedGetter {
    protected array $items = [1];
    public function size(): int { return count($this->items); }
    public function __get(string $name): mixed { echo 'unexpected-visibility-get|'; return [9]; }
}
class AppendMagicPrivateGetter {
    private array $items = [1];
    public function size(): int { return count($this->items); }
    public function __get(string $name): mixed { echo 'unexpected-visibility-get|'; return [9]; }
}

function AppendMagicMixed(mixed $box): void { $box->items[] = 2; }
function AppendMagicObject(object $box): void { $box->items[] = 2; }
function AppendMagicInterface(AppendMagicMarker $box): void { $box->items[] = 2; }
function AppendMagicSeed(mixed $box): void { @($box->items = ['handler']); }
function AppendMagicValueCase(mixed $backing): void {
    $box = new AppendMagicValue($backing);
    try { AppendMagicMixed($box); }
    catch (Throwable $error) { echo get_class($error), ':', $error->getMessage(), '|'; }
    echo property_exists($box, 'items') ? 'changed:' : 'absent:', json_encode($box->backing), '|';
}
$s = new AppendMagicS(); $s->items = [1];
AppendMagicMixed($s); AppendMagicObject($s); AppendMagicInterface($s);
echo $s->sets, ':', json_encode(get_object_vars($s)['items']), '|';
$u = new AppendMagicU();
AppendMagicMixed($u); AppendMagicObject($u); AppendMagicInterface($u);
echo $u->sets, ':', json_encode(get_object_vars($u)['items']), '|';
$t = new AppendMagicT(); $t->items = [3]; AppendMagicInterface($t);
echo $t->sets, ':', json_encode(get_object_vars($t)['items']), '|';
AppendMagicValueCase([5]); AppendMagicValueCase(null); AppendMagicValueCase(false);
AppendMagicValueCase(7); AppendMagicValueCase('s');
$resource = fopen('php://memory', 'w+');
$resourceBox = new AppendMagicValue($resource);
try { AppendMagicMixed($resourceBox); }
catch (Throwable $error) { echo get_class($error), ':', $error->getMessage(), '|'; }
echo fwrite($resource, 'ok'), '|'; fclose($resource);
$existing = new AppendMagicValue([5]); $existing->items = [7]; AppendMagicObject($existing);
echo json_encode(get_object_vars($existing)['items']), '|';
$existingNull = new AppendMagicValue([5]); $existingNull->items = null; AppendMagicInterface($existingNull);
echo json_encode(get_object_vars($existingNull)['items']), '|';
$object = new AppendMagicValue(new ArrayObject([8]));
try { AppendMagicMixed($object); }
catch (Throwable $error) { echo get_class($error), ':', $error->getMessage(), '|'; }
echo count($object->backing), '|';
$noHash = new AppendMagicNoHash();
try { AppendMagicObject($noHash); }
catch (Throwable $error) { echo get_class($error), ':', $error->getMessage(), '|'; }
echo property_exists($noHash, 'items') ? 'changed|' : 'absent|';
class AppendMagicReadonly {
    public readonly mixed $items;
    public function __construct(mixed $items) { $this->items = $items; }
}
$readonlyResource = fopen('php://memory', 'w+');
$readonlyBox = new AppendMagicReadonly($readonlyResource);
try { AppendMagicObject($readonlyBox); }
catch (Error $error) { echo 'readonly-blocked|'; }
echo fwrite($readonlyResource, 'ok'), '|'; fclose($readonlyResource);
$created = new AppendMagicCreate(); $reserved = clone($created, []);
set_error_handler(function(int $level, string $message) use ($created): bool {
    echo 'create-warning|'; AppendMagicSeed($created); return true;
});
AppendMagicMixed($created);
restore_error_handler();
echo json_encode(get_object_vars($created)['items']), '|';
$visibilityProtected = new AppendMagicProtected();
try { AppendMagicMixed($visibilityProtected); }
catch (Throwable $error) { echo get_class($error), ':', $error->getMessage(), '|'; }
echo $visibilityProtected->size(), '|';
$visibilityPrivate = new AppendMagicPrivate();
try { AppendMagicMixed($visibilityPrivate); }
catch (Throwable $error) { echo get_class($error), ':', $error->getMessage(), '|'; }
echo $visibilityPrivate->size(), '|';
$visibilityProtectedGetter = new AppendMagicProtectedGetter();
try { AppendMagicMixed($visibilityProtectedGetter); }
catch (Throwable $error) { echo get_class($error), ':', $error->getMessage(), '|'; }
echo $visibilityProtectedGetter->size(), '|';
$visibilityPrivateGetter = new AppendMagicPrivateGetter();
try { AppendMagicMixed($visibilityPrivateGetter); }
catch (Throwable $error) { echo get_class($error), ':', $error->getMessage(), '|'; }
echo $visibilityPrivateGetter->size(), '|';
$visibilityProtected->appendAllowed(); $visibilityPrivate->appendAllowed();
echo $visibilityProtected->size(), ':', $visibilityPrivate->size(), '|';
restore_error_handler();
"#;
    let expected = "1:[1,2,2,2]|0:[2,2,2]|8192:Creation of dynamic property AppendMagicT::$items is deprecated|1:[3,2]|Error:Unsupported property append through an overloaded __get()|absent:[5]|Error:Unsupported property append through an overloaded __get()|absent:null|Error:Unsupported property append through an overloaded __get()|absent:false|Error:Unsupported property append through an overloaded __get()|absent:7|Error:Unsupported property append through an overloaded __get()|absent:\"s\"|Error:Unsupported property append through an overloaded __get()|2|[7,2]|[2]|Error:Unsupported property append through an overloaded __get()|1|Error:Unsupported property append through an overloaded __get()|absent|readonly-blocked|2|create-warning|[\"handler\",2]|Error:Cannot access protected property AppendMagicProtected::$items|1|Error:Cannot access private property AppendMagicPrivate::$items|1|Error:Unsupported property append through an overloaded __get()|1|Error:Unsupported property append through an overloaded __get()|1|2:2|";
    let native_capability = if xml { "function AppendMagicXmlCapability(): void { simplexml_load_string('<r/>'); }" } else { "" };
    let eval_capability = if xml { "function AppendMagicEvalXmlCapability(): void { simplexml_load_string('<r/>'); }" } else { "" };
    let native = format!("{source}\n{native_capability}");
    let renamed = source.replace("AppendMagic", "EvalAppendMagic");
    // The AOT parser rejects runtime-name appends; eval has its own supported syntax path.
    let runtime_names = r#"function EvalAppendMagicNamed(mixed $box, string $name): void { $box->{$name}[] = 2; }
try { EvalAppendMagicNamed($visibilityProtected, 'items'); }
catch (Throwable $error) { echo 'named:', get_class($error), ':', $error->getMessage(), '|'; }
echo $visibilityProtected->size(), '|';
try { EvalAppendMagicNamed($visibilityPrivate, 'items'); }
catch (Throwable $error) { echo 'named:', get_class($error), ':', $error->getMessage(), '|'; }
echo $visibilityPrivate->size(), '|';
try { EvalAppendMagicNamed($visibilityProtectedGetter, 'items'); }
catch (Throwable $error) { echo 'named:', get_class($error), ':', $error->getMessage(), '|'; }
echo $visibilityProtectedGetter->size(), '|';
try { EvalAppendMagicNamed($visibilityPrivateGetter, 'items'); }
catch (Throwable $error) { echo 'named:', get_class($error), ':', $error->getMessage(), '|'; }
echo $visibilityPrivateGetter->size(), '|';
"#;
    let renamed = format!("{renamed}\n{runtime_names}");
    let body = renamed.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let eval = format!("<?php {eval_capability} $source = $argc > 0 ? '{body}' : ''; eval($source);");
    let named_expected = "named:Error:Cannot access protected property EvalAppendMagicProtected::$items|2|named:Error:Cannot access private property EvalAppendMagicPrivate::$items|2|named:Error:Unsupported property append through an overloaded __get()|1|named:Error:Unsupported property append through an overloaded __get()|1|";
    let eval_expected = format!("{}{named_expected}", expected.replace("AppendMagic", "EvalAppendMagic"));
    assert_bundle_with(&[
        BundleCase { name: "magic_append_native", source: &native, expected },
        BundleCase { name: "magic_append_eval", source: &eval, expected: &eval_expected },
    ], |source| {
        let out = compile_and_run_with_heap_debug(source);
        assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        out.stdout
    });
}

/// Reference getters are refused before an unsupported receiver-to-cell lifetime transfer.
#[test]
fn test_reference_magic_property_append_bundle() { verify_references(false); }

/// XML capability preserves the same explicit reference-getter subset boundary.
#[test]
fn test_reference_magic_property_xml_append_bundle() {
    if skip_without_xml_native("test_reference_magic_property_xml_append_bundle") { return; }
    verify_references(true);
}

/// A rejected reference getter leaves its backing array and earlier snapshots unchanged.
fn verify_references(xml: bool) {
    let source = r#"<?php
#[AllowDynamicProperties]
class AppendReferenceMagic {
    public array $backing = [1];
    public function &__get(string $name): mixed { echo 'ref:', $name, '|'; return $this->backing; }
}
function AppendReferenceWrite(mixed $box): void { $box->items[] = 2; }
$box = new AppendReferenceMagic(); $snapshot = $box->backing;
try { AppendReferenceWrite($box); }
catch (Throwable $error) { echo $error->getMessage(), "|"; }
echo json_encode($box->backing), ':', json_encode($snapshot), '|';
$box->items = [3]; AppendReferenceWrite($box);
echo json_encode(get_object_vars($box)['items']), '|';
"#;
    let capability = if xml { "function AppendReferenceXmlCapability(): void { simplexml_load_string('<r/>'); }" } else { "" };
    let out = compile_and_run_with_heap_debug(&format!("{source}\n{capability}"));
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "Unsupported property append through an overloaded __get()|[1]:[1]|[3,2]|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Opaque eval refuses to mistake a reference getter's value snapshot for writable storage.
#[test]
fn test_eval_reference_magic_property_append_refusal() {
    let source = r#"<?php
$source = $argc > 0 ? '#[AllowDynamicProperties]
class RefuseEvalMagicAppend {
    public array $backing = [1];
    public function &__get(string $name): mixed { return $this->backing; }
}
$box = new RefuseEvalMagicAppend();
try { $box->items[] = 2; }
catch (Throwable $error) { echo $error->getMessage(), "|"; }
echo json_encode($box->backing), "|";
$box->items = [3]; $box->items[] = 2;
echo json_encode(get_object_vars($box)["items"]);' : '';
eval($source);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "Unsupported property append through an overloaded __get()|[1]|[3,2]");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
