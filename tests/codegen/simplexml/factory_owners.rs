//! Purpose:
//! Covers constructorless SimpleXML factory classes and native handle cleanup.
//!
//! Called from:
//! - Managed-libxml2 codegen CI on Linux and macOS.
//!
//! Key details:
//! - PHP native factories permit abstract subclasses and bypass private constructors.
//! - Existing handle-presence queries detect native leaks outside the PHP heap.

use crate::support::*;

/// Factory errors release native views, while abstract wrappers retain their class in both modes.
#[test]
fn test_simplexml_constructorless_factory_owners_bundle() {
    if skip_without_xml_native("test_simplexml_constructorless_factory_owners_bundle") { return; }
    let declarations = r#"
interface NativeFactoryAllocationMarker { public function marker(int $value): string; }
abstract class NativeAbstractFactoryXml extends SimpleXMLElement implements NativeFactoryAllocationMarker {
    abstract public function marker(int $value): string;
    public function __construct() { throw new Exception('constructor called'); }
}
class NativeConcreteFactoryXml extends NativeAbstractFactoryXml {
    public function marker(int $value): string { return 'concrete:' . $value; }
}
abstract class NativeImplicitFactoryXml extends SimpleXMLElement implements NativeFactoryAllocationMarker {}
class PrivateConstructorFactoryXml extends SimpleXMLIterator {
    private function __construct() { throw new Exception('constructor called'); }
}
function factoryProbeHandle(): int { return elephc_xml_sx_load('<r/>', 4, 0, '', 0, ''); }
function factoryProbePresent(int $handle): int { return elephc_xml_sx_int($handle, 1); }
function factoryProbeRelease(int $handle): void { elephc_xml_sx_free($handle); }
function factoryProbeAllocate(int $handle, string $class): void { __elephc_simplexml_wrap($handle, $class); }
function factoryProbeStrictAllocate(string $class): mixed { return __elephc_new_without_constructor($class); }
function factoryProbeMarkerArgument(): int { echo 'argument|'; return 7; }
function factoryProbeInterfaceMarker(NativeFactoryAllocationMarker $xml): string { return $xml->marker(factoryProbeMarkerArgument()); }
function factoryProbeMixedMarker(mixed $xml): mixed { return $xml->marker(factoryProbeMarkerArgument()); }
function factoryProbeNullableMarker(?NativeFactoryAllocationMarker $xml): ?string { return $xml?->marker(factoryProbeMarkerArgument()); }
function factoryProbeTypedMarker(NativeAbstractFactoryXml $xml): string { return $xml->marker(factoryProbeMarkerArgument()); }
"#;
    let evaluated = r#"
interface FactoryAllocationFailureInterface {}
abstract class EvalAbstractFactoryXml extends SimpleXMLElement implements NativeFactoryAllocationMarker {
    abstract public function marker(int $value): string;
    public function __construct() { throw new Exception('constructor called'); }
}
$held = new SimpleXMLElement('<held/>');
for ($i = 0; $i < 3; $i++) {
    $handle = factoryProbeHandle();
    echo factoryProbePresent($handle);
    try { factoryProbeAllocate($handle, 'FactoryAllocationFailureInterface'); }
    catch (Error $error) { echo get_class($error); }
    echo factoryProbePresent($handle), '|';
    factoryProbeRelease($handle);
    unset($error);
}
echo $held->getName(), '|';
unset($held, $handle, $i);
$evalMarker = simplexml_load_string('<r/>', 'EvalAbstractFactoryXml');
$method = 'marker';
foreach ([0, 1, 2, 3] as $call) {
    try {
        if ($call === 0) { $evalMarker->marker(factoryProbeMarkerArgument()); }
        elseif ($call === 1) { $evalMarker->$method(factoryProbeMarkerArgument()); }
        elseif ($call === 2) { $evalMarker?->marker(factoryProbeMarkerArgument()); }
        else { $evalMarker?->$method(factoryProbeMarkerArgument()); }
    } catch (Error $error) {
        echo $error->getMessage() === 'Cannot call abstract method EvalAbstractFactoryXml::marker()' ? 'vm-abstract|' : $error->getMessage();
    }
}
unset($evalMarker, $method, $call, $error);
$evalReflected = new ReflectionClass('EvalAbstractFactoryXml');
try { $evalReflected->newInstanceWithoutConstructor(); }
catch (Error $error) { echo 'eval-reflection:', get_class($error), '|'; }
try { factoryProbeStrictAllocate('EvalAbstractFactoryXml'); }
catch (Error $error) { echo 'aot-eval-reflection:', get_class($error), '|'; }
unset($evalReflected, $error);
"#;
    let body = r#"
$document = new DOMDocument();
$document->loadXML('<r><item>ok</item></r>');
file_put_contents('factory-owners.xml', '<r><item>ok</item></r>');
try {
    foreach ([NativeAbstractFactoryXml::class, 'EvalAbstractFactoryXml', PrivateConstructorFactoryXml::class, NativeConcreteFactoryXml::class, NativeImplicitFactoryXml::class] as $class) {
        $root = simplexml_load_string('<r><item>ok</item></r>', $class);
        $file = simplexml_load_file('factory-owners.xml', $class);
        $imported = simplexml_import_dom($document, $class);
        $children = $root->children();
        $paths = $root->xpath('//item');
        $cloned = clone $root;
        echo get_class($root), ':', get_class($root->item), ':', get_class($file), ':';
        echo get_class($imported), ':', get_class($children), ':', get_class($paths[0]), ':';
        echo get_class($cloned), ':';
        echo (string)$imported->item, '|';
        if ($root instanceof NativeFactoryAllocationMarker) {
            $abstractOwner = $root instanceof NativeImplicitFactoryXml ? 'NativeFactoryAllocationMarker' : get_class($root);
            try { echo factoryProbeInterfaceMarker($root), '|'; }
            catch (Error $error) { echo $error->getMessage() === 'Cannot call abstract method ' . $abstractOwner . '::marker()' ? 'abstract|' : $error->getMessage(); }
            try { echo factoryProbeMixedMarker($root), '|'; }
            catch (Error $error) { echo $error->getMessage() === 'Cannot call abstract method ' . $abstractOwner . '::marker()' ? 'abstract|' : $error->getMessage(); }
            try { echo factoryProbeNullableMarker($root), '|'; }
            catch (Error $error) { echo $error->getMessage() === 'Cannot call abstract method ' . $abstractOwner . '::marker()' ? 'abstract|' : $error->getMessage(); }
            if ($root instanceof NativeAbstractFactoryXml) {
                try { echo factoryProbeTypedMarker($root), '|'; }
                catch (Error $error) { echo $error->getMessage() === 'Cannot call abstract method ' . $abstractOwner . '::marker()' ? 'abstract|' : $error->getMessage(); }
            }
        }
        unset($root, $file, $imported, $children, $paths, $cloned, $abstractOwner);
    }
    try { factoryProbeStrictAllocate(NativeAbstractFactoryXml::class); }
    catch (Error $error) { echo 'native-reflection:', get_class($error), '|'; }
    set_error_handler(function(int $level, string $message): bool { return true; });
    echo (int)(simplexml_load_string('<r>', NativeAbstractFactoryXml::class) === false), '|';
    restore_error_handler();
    echo 'null:', (int)(factoryProbeNullableMarker(null) === null), '|';
    unset($error, $class, $document);
} finally { unlink('factory-owners.xml'); }
"#;
    let escaped = evaluated.replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php {declarations} $source = $argc > 0 ? '{escaped}' : ''; eval($source); unset($source); {body}");
    let expected = concat!(
        "1Error0|1Error0|1Error0|held|",
        "vm-abstract|vm-abstract|vm-abstract|vm-abstract|",
        "eval-reflection:Error|aot-eval-reflection:Error|",
        "NativeAbstractFactoryXml:NativeAbstractFactoryXml:NativeAbstractFactoryXml:NativeAbstractFactoryXml:NativeAbstractFactoryXml:NativeAbstractFactoryXml:NativeAbstractFactoryXml:ok|",
        "abstract|abstract|abstract|abstract|",
        "EvalAbstractFactoryXml:EvalAbstractFactoryXml:EvalAbstractFactoryXml:EvalAbstractFactoryXml:EvalAbstractFactoryXml:EvalAbstractFactoryXml:EvalAbstractFactoryXml:ok|",
        "abstract|abstract|abstract|",
        "PrivateConstructorFactoryXml:PrivateConstructorFactoryXml:PrivateConstructorFactoryXml:PrivateConstructorFactoryXml:PrivateConstructorFactoryXml:PrivateConstructorFactoryXml:PrivateConstructorFactoryXml:ok|",
        "NativeConcreteFactoryXml:NativeConcreteFactoryXml:NativeConcreteFactoryXml:NativeConcreteFactoryXml:NativeConcreteFactoryXml:NativeConcreteFactoryXml:NativeConcreteFactoryXml:ok|",
        "argument|concrete:7|argument|concrete:7|argument|concrete:7|argument|concrete:7|",
        "NativeImplicitFactoryXml:NativeImplicitFactoryXml:NativeImplicitFactoryXml:NativeImplicitFactoryXml:NativeImplicitFactoryXml:NativeImplicitFactoryXml:NativeImplicitFactoryXml:ok|",
        "abstract|abstract|abstract|",
        "native-reflection:Error|1|null:1|",
    );
    let run = || {
        let output = compile_and_run_with_heap_debug(&source);
        assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
        output.stdout
    };
    assert_eq!(run(), expected);
    assert_eq!(without_ir_opt(run), expected);
}

/// Public AOT Reflection keeps its abstract native target refusal in an independent heap.
#[test]
fn test_simplexml_aot_reflection_keeps_abstract_factories_strict_bundle() {
    if skip_without_xml_native("test_simplexml_aot_reflection_keeps_abstract_factories_strict_bundle") { return; }
    let source = r#"<?php
interface ReflectionAbstractFactoryMarker { public function marker(int $value): string; }
abstract class ReflectionAbstractFactoryXml extends SimpleXMLElement implements ReflectionAbstractFactoryMarker {
    abstract public function marker(int $value): string;
    public function __construct() { throw new Exception('constructor called'); }
}
$root = simplexml_load_string('<r><item>ok</item></r>', ReflectionAbstractFactoryXml::class);
echo get_class($root), ':', (string)$root->item, '|';
$reflected = new ReflectionClass(ReflectionAbstractFactoryXml::class);
try { $reflected->newInstanceWithoutConstructor(); }
catch (Error $error) { echo get_class($error), '|'; }
unset($root, $reflected, $error);
"#;
    let run = || {
        let output = compile_and_run_with_heap_debug(source);
        assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
        output.stdout
    };
    let expected = "ReflectionAbstractFactoryXml:ok|Error|";
    assert_eq!(run(), expected);
    assert_eq!(without_ir_opt(run), expected);
}
