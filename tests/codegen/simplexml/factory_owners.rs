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
function factoryProbeDynamicMarker(mixed $xml, string $method): mixed { return $xml->$method(factoryProbeMarkerArgument()); }
function factoryProbeNullsafeDynamicMarker(?NativeFactoryAllocationMarker $xml, string $method): mixed { return $xml?->$method(factoryProbeMarkerArgument()); }
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
try { factoryProbeDynamicMarker($evalMarker, $method); }
catch (Error $error) { echo $error->getMessage() === 'Cannot call abstract method EvalAbstractFactoryXml::marker()' ? 'vm-abstract|' : $error->getMessage(); }
try { factoryProbeNullsafeDynamicMarker($evalMarker, $method); }
catch (Error $error) { echo $error->getMessage() === 'Cannot call abstract method EvalAbstractFactoryXml::marker()' ? 'vm-abstract|' : $error->getMessage(); }
unset($evalMarker, $method, $call, $error);
class EvalMagicFactoryXml extends SimpleXMLElement {
    public function __call(string $name, array $args): mixed { echo 'vm-magic|'; return null; }
}
abstract class EvalProtectedMagicFactoryXml extends EvalMagicFactoryXml {
    abstract protected function marker(int $value): string;
}
$protectedMagic = simplexml_load_string('<r/>', 'EvalProtectedMagicFactoryXml');
$protectedMagic->marker(factoryProbeMarkerArgument());
$protectedMethod = 'MaRkEr';
$protectedMagic->$protectedMethod(factoryProbeMarkerArgument());
$protectedMagic?->$protectedMethod(factoryProbeMarkerArgument());
unset($protectedMagic, $protectedMethod);
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
    catch (Error $error) { echo 'native-reflection:', $error->getMessage() === 'Cannot instantiate abstract class NativeAbstractFactoryXml' ? get_class($error) : $error->getMessage(), '|'; }
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
        "vm-abstract|vm-abstract|",
        "argument|vm-magic|argument|vm-magic|argument|vm-magic|",
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

/// A public declaring-class reflector keeps its abstract native target refusal in both modes.
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
$member = new ReflectionMethod(ReflectionAbstractFactoryXml::class, 'marker');
$reflected = $member->getDeclaringClass();
echo $reflected->getName(), ':', (int)$reflected->isAbstract(), '|';
try { $reflected->newInstanceWithoutConstructor(); }
catch (Error $error) { echo $error->getMessage() === 'Cannot instantiate abstract class ReflectionAbstractFactoryXml' ? get_class($error) : $error->getMessage(), '|'; }
unset($root, $member, $reflected, $error);
"#;
    let run = || {
        let output = compile_and_run_with_heap_debug(source);
        assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
        output.stdout
    };
    let expected = "ReflectionAbstractFactoryXml:ok|ReflectionAbstractFactoryXml:1|Error|";
    assert_eq!(run(), expected);
    assert_eq!(without_ir_opt(run), expected);
}

/// AOT dynamic names and parent magic dispatch reject native abstract methods before arguments.
#[test]
fn test_simplexml_aot_abstract_factory_method_dispatch_bundle() {
    if skip_without_xml_native("test_simplexml_aot_abstract_factory_method_dispatch_bundle") { return; }
    let source = r#"<?php
abstract class GuardAbstractFactoryXml extends SimpleXMLElement {
    abstract public function marker(int $value): string;
}
class GuardConcreteFactoryXml extends GuardAbstractFactoryXml {
    public function marker(int $value): string { return 'concrete:' . $value; }
}
class GuardMagicFactoryXml extends SimpleXMLElement {
    public function __call(string $name, array $args): mixed { return 'magic:' . $name . ':' . $args[0]; }
}
abstract class GuardMagicChildFactoryXml extends GuardMagicFactoryXml {
    abstract public function marker(int $value): string;
}
abstract class GuardProtectedMagicFactoryXml extends GuardMagicFactoryXml {
    abstract protected function marker(int $value): string;
}
function guardFactoryArgument(): int { echo 'argument|'; return 7; }
function guardFactoryName(): string { echo 'name|'; return 'MaRkEr'; }
function guardFactoryDynamic(GuardAbstractFactoryXml $xml, string $name): mixed { return $xml->$name(guardFactoryArgument()); }
function guardFactoryMixed(mixed $xml, mixed $name): mixed { return $xml->$name(guardFactoryArgument()); }
function guardFactoryNullsafe(?GuardAbstractFactoryXml $xml, string $name): mixed { return $xml?->$name(guardFactoryArgument()); }
function guardFactoryMagic(GuardMagicFactoryXml $xml): mixed { return $xml->marker(guardFactoryArgument()); }
function guardFactoryMagicNullsafe(?GuardMagicFactoryXml $xml): mixed { return $xml?->marker(guardFactoryArgument()); }
function guardFactoryMagicDynamic(GuardMagicFactoryXml $xml, string $name): mixed { return $xml->$name(guardFactoryArgument()); }
function guardFactoryCreate(DOMDocument $document, int $factory, string $class): mixed {
    if ($factory === 0) { return simplexml_load_string('<r/>', $class); }
    if ($factory === 1) { return simplexml_load_file('factory-dispatch.xml', $class); }
    return simplexml_import_dom($document, $class);
}
function guardFactoryError(Error $error, string $class): void {
    echo $error->getMessage() === 'Cannot call abstract method ' . $class . '::marker()' ? 'Error|' : $error->getMessage();
}
$document = new DOMDocument();
$document->loadXML('<r/>');
file_put_contents('factory-dispatch.xml', '<r/>');
try {
    foreach ([0, 1, 2] as $factory) {
        foreach ([GuardAbstractFactoryXml::class, GuardMagicChildFactoryXml::class] as $class) {
            $xml = guardFactoryCreate($document, $factory, $class);
            if ($xml instanceof GuardAbstractFactoryXml) {
                try { guardFactoryDynamic($xml, guardFactoryName()); } catch (Error $error) { guardFactoryError($error, $class); }
                try { guardFactoryMixed($xml, 'marker'); } catch (Error $error) { guardFactoryError($error, $class); }
                try { guardFactoryNullsafe($xml, 'marker'); } catch (Error $error) { guardFactoryError($error, $class); }
            }
            if ($xml instanceof GuardMagicFactoryXml) {
                try { guardFactoryMagic($xml); } catch (Error $error) { guardFactoryError($error, $class); }
                try { guardFactoryMagicNullsafe($xml); } catch (Error $error) { guardFactoryError($error, $class); }
                try { guardFactoryMagicDynamic($xml, 'marker'); } catch (Error $error) { guardFactoryError($error, $class); }
            }
            unset($xml, $error);
        }
    }
    $concrete = simplexml_load_string('<r/>', GuardConcreteFactoryXml::class);
    if ($concrete instanceof GuardAbstractFactoryXml) { echo guardFactoryDynamic($concrete, 'marker'), '|'; }
    $magic = new GuardMagicFactoryXml('<r/>');
    echo guardFactoryMagic($magic), '|';
    $protectedMagic = simplexml_load_string('<r/>', GuardProtectedMagicFactoryXml::class);
    if ($protectedMagic instanceof GuardMagicFactoryXml) { echo guardFactoryMagic($protectedMagic), '|'; }
    echo 'null:', (int)(guardFactoryNullsafe(null, 'marker') === null), ':', (int)(guardFactoryMagicNullsafe(null) === null), '|';
    unset($concrete, $magic, $protectedMagic, $class, $factory, $document);
} finally { unlink('factory-dispatch.xml'); }
"#;
    let expected = concat!(
        "name|Error|Error|Error|Error|Error|Error|",
        "name|Error|Error|Error|Error|Error|Error|",
        "name|Error|Error|Error|Error|Error|Error|",
        "argument|concrete:7|argument|magic:marker:7|argument|magic:marker:7|null:1:1|",
    );
    let run = || {
        let output = compile_and_run_with_heap_debug(source);
        assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
        output.stdout
    };
    assert_eq!(run(), expected);
    assert_eq!(without_ir_opt(run), expected);
}
