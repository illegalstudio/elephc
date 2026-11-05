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
interface NativeFactoryAllocationMarker { public function marker(): string; }
abstract class NativeAbstractFactoryXml extends SimpleXMLElement implements NativeFactoryAllocationMarker {
    abstract public function marker(): string;
    public function __construct() { throw new Exception('constructor called'); }
}
class PrivateConstructorFactoryXml extends SimpleXMLIterator {
    private function __construct() { throw new Exception('constructor called'); }
}
function factoryProbeHandle(): int { return elephc_xml_sx_load('<r/>', 4, 0, '', 0, ''); }
function factoryProbePresent(int $handle): int { return elephc_xml_sx_int($handle, 1); }
function factoryProbeRelease(int $handle): void { elephc_xml_sx_free($handle); }
function factoryProbeAllocate(int $handle, string $class): void { __elephc_simplexml_wrap($handle, $class); }
function factoryProbeStrictAllocate(string $class): mixed { return __elephc_new_without_constructor($class); }
"#;
    let evaluated = r#"
interface FactoryAllocationFailureInterface {}
abstract class EvalAbstractFactoryXml extends SimpleXMLElement {
    abstract public function marker(): string;
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
    foreach ([NativeAbstractFactoryXml::class, 'EvalAbstractFactoryXml', PrivateConstructorFactoryXml::class] as $class) {
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
        unset($root, $file, $imported, $children, $paths, $cloned);
    }
    $reflected = new ReflectionClass(NativeAbstractFactoryXml::class);
    try { $reflected->newInstanceWithoutConstructor(); }
    catch (Error $error) { echo 'reflection:', get_class($error), '|'; }
    set_error_handler(function(int $level, string $message): bool { return true; });
    echo (int)(simplexml_load_string('<r>', NativeAbstractFactoryXml::class) === false), '|';
    restore_error_handler();
    unset($reflected, $error, $class, $document);
} finally { unlink('factory-owners.xml'); }
"#;
    let escaped = evaluated.replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php {declarations} $source = $argc > 0 ? '{escaped}' : ''; eval($source); unset($source); {body}");
    let expected = concat!(
        "1Error0|1Error0|1Error0|held|",
        "eval-reflection:Error|aot-eval-reflection:Error|",
        "NativeAbstractFactoryXml:NativeAbstractFactoryXml:NativeAbstractFactoryXml:NativeAbstractFactoryXml:NativeAbstractFactoryXml:NativeAbstractFactoryXml:NativeAbstractFactoryXml:ok|",
        "EvalAbstractFactoryXml:EvalAbstractFactoryXml:EvalAbstractFactoryXml:EvalAbstractFactoryXml:EvalAbstractFactoryXml:EvalAbstractFactoryXml:EvalAbstractFactoryXml:ok|",
        "PrivateConstructorFactoryXml:PrivateConstructorFactoryXml:PrivateConstructorFactoryXml:PrivateConstructorFactoryXml:PrivateConstructorFactoryXml:PrivateConstructorFactoryXml:PrivateConstructorFactoryXml:ok|",
        "reflection:Error|1|",
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
