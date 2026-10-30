//! Purpose:
//! Checks ordinary computed receiver incdec without the XML native gate.
//!
//! Called from:
//! - The object codegen integration suite.
//!
//! Key details:
//! - AOT and opaque eval verify typed, private and magic property updates.

use crate::support::*;

/// Runtime property names are converted once and stay live for both indexed update halves.
#[test]
fn test_computed_indexed_name_owners_bundle() {
    let source = r#"<?php
class ScopedIndexedBox {
    public array $items = [0];
    public function receiver(): self { echo 'r|'; return $this; }
}
class ScopedIndexedName {
    public function __toString(): string { echo 's|'; return 'items'; }
    public function __destruct() { echo 'd|'; }
}
function makeScopedIndexedName(int $kind): mixed { echo 'n|'; return $kind === 0 ? 'items' : new ScopedIndexedName(); }
function scopedIndexedKey(): int { echo 'k|'; return 0; }
function scopedIndexedValue(): int { echo 'v|'; return 2; }
$box = new ScopedIndexedBox();
for ($kind = 0; $kind < 2; $kind++) {
    $box->receiver()->{makeScopedIndexedName($kind)}[scopedIndexedKey()] += scopedIndexedValue();
    echo json_encode($box->items), '|';
    $box->receiver()->{makeScopedIndexedName($kind)}[scopedIndexedKey()]++;
    echo json_encode($box->items), '|';
    --$box->receiver()->{makeScopedIndexedName($kind)}[scopedIndexedKey()];
    echo json_encode($box->items), '|';
}
unset($box);
"#;
    let renamed = source.replace("ScopedIndexed", "EvalScopedIndexed").replace("scopedIndexed", "evalScopedIndexed");
    let body = renamed.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let eval = format!("<?php $source = $argc > 0 ? '{body}' : ''; eval($source);");
    let expected = concat!(
        "r|n|k|v|[2]|r|n|k|[3]|r|n|k|[2]|",
        "r|n|k|v|s|d|[4]|r|n|k|s|d|[5]|r|n|k|s|d|[4]|",
    );
    assert_bundle_with(&[
        BundleCase { name: "computed_names_aot", source, expected },
        BundleCase { name: "computed_names_eval", source: &eval, expected },
    ], |source| {
        let output = compile_and_run_with_heap_debug(source);
        assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
        output.stdout
    });
}

/// Erased object containers use offsetSet through readable readonly and private(set) properties.
#[test]
fn test_erased_declared_arrayaccess_property_writeback() {
    let out = compile_and_run_with_heap_debug(r#"<?php
interface ErasedObjectContainerPeer {}
class ErasedObjectContainers implements ErasedObjectContainerPeer {
    public ArrayObject $items;
    public readonly ArrayObject $readonlyItems;
    public private(set) ArrayObject $privateItems;
    public function __construct() {
        $this->items = new ArrayObject([1]);
        $this->readonlyItems = new ArrayObject([2]);
        $this->privateItems = new ArrayObject([3]);
    }
}
function writeMixedObjectContainer(mixed $box): void { $box->items[0] = 7; }
function writeRawObjectContainer(object $box): void { $box->items[0] += 1; }
function writeInterfaceObjectContainer(ErasedObjectContainerPeer $box): void { $box->items[0] += 1; }
function writeReadonlyObjectContainer(mixed $box): void { $box->readonlyItems[0] = 8; }
function writePrivateSetObjectContainer(mixed $box): void { $box->privateItems[0] = 9; }
$box = new ErasedObjectContainers();
writeMixedObjectContainer($box); writeRawObjectContainer($box); writeInterfaceObjectContainer($box);
writeReadonlyObjectContainer($box); writePrivateSetObjectContainer($box);
echo $box->items[0], ':', $box->readonlyItems[0], ':', $box->privateItems[0];
unset($box);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "9:8:9");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Erased declared-array receivers keep COW and update their actual property for every write form.
#[test]
fn test_erased_declared_property_dimension_writeback() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ErasedPropertyContainer {
    public array $items = ['9', 4];
    public function receiver(): mixed { echo 'receiver|'; return $this; }
}
$object = new ErasedPropertyContainer();
$copy = $object->items;
$object->receiver()->items[0]++;
--$object->receiver()->items[1];
$object->receiver()->items[1] += 4;
$object->receiver()->items['new'] = 8;
echo json_encode($object->items), ':', json_encode($copy);
unset($object, $copy);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "receiver|receiver|receiver|receiver|{\"0\":10,\"1\":7,\"new\":8}:[\"9\",4]");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Computed property updates preserve typed storage, lexical visibility and magic hooks.
#[test]
fn test_computed_receiver_ordinary_property_incdec_bundle() {
    let source = r###"<?php
class ComputedIncdecStats { public int $hits = 10; }
class ComputedIncdecApp {
    private ComputedIncdecStats $value;
    private int $hidden = 20;
    public function __construct() { $this->value = new ComputedIncdecStats(); }
    public function stats(): ComputedIncdecStats { echo 'stats|'; return $this->value; }
    public function self(): self { echo 'self|'; return $this; }
    public function run(): void {
        $this->stats()->hits++; echo $this->value->hits, '|';
        ++$this->stats()->hits; echo $this->value->hits, '|';
        $this->stats()->hits--; echo $this->value->hits, '|';
        --$this->stats()->hits; echo $this->value->hits, '|';
        $this->self()->hidden++; ++$this->self()->hidden;
        $this->self()->hidden--; --$this->self()->hidden;
        echo $this->hidden, '|';
        echo "done-private\n";
    }
}
class ComputedIncdecMagic {
    private int $value = 30;
    public function self(): self { echo 'magic|'; return $this; }
    public function __get(string $name): int { echo 'get|'; return $this->value; }
    public function __set(string $name, int $value): void { echo 'set:', $value, '|'; $this->value = $value; }
}
$app = new ComputedIncdecApp(); $app->run();
$magic = new ComputedIncdecMagic();
$magic->self()->hits++; ++$magic->self()->hits;
$magic->self()->hits--; --$magic->self()->hits;
echo "done\n";
unset($app, $magic);
"###;
    let renamed = source.replace("ComputedIncdec", "EvalComputedIncdec");
    let body = renamed.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let eval = format!("<?php $source = $argc > 0 ? '{body}' : ''; eval($source);");
    let expected = "stats|11|stats|12|stats|11|stats|10|self|self|self|self|20|done-private\nmagic|get|set:31|magic|get|set:32|magic|get|set:31|magic|get|set:30|done\n";
    assert_bundle_with(&[
        BundleCase { name: "ordinary_incdec_aot", source, expected },
        BundleCase { name: "ordinary_incdec_eval", source: &eval, expected },
    ], |source| {
        let out = compile_and_run_with_heap_debug(source);
        assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        assert!(!out.stderr.contains("Warning:"), "{}", out.stderr);
        out.stdout
    });
}

/// Indexed compound logic retains its assignment result and short-circuit order.
#[test]
fn test_computed_receiver_indexed_compound_logic() {
    let source = r###"<?php
class ComputedLogicRegistry {
    public array $lines = [0, 'a', 0];
    public function receiver(): self { echo 'call|'; return $this; }
    public function run(): void {
        $this->receiver()->lines[ComputedLogicKey()] += 1 or ComputedLogicLog();
        $this->receiver()->lines[1] .= 'b' and ComputedLogicLog();
        $this->receiver()->lines[2] += 1 xor ComputedLogicLog();
    }
}
function ComputedLogicKey(): int { echo 'key|'; return 0; }
function ComputedLogicLog(): bool { echo 'logic|'; return true; }
function ComputedLogicRoot(ComputedLogicRegistry $registry): ComputedLogicRegistry { echo 'root|'; return $registry; }
$registry = new ComputedLogicRegistry(); $registry->run();
ComputedLogicRoot($registry)->lines[0] += 1 or ComputedLogicLog();
echo json_encode($registry->lines), "\n";
unset($registry);
"###;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert!(!out.stderr.contains("Warning:"), "{}", out.stderr);
    assert_eq!(out.stdout, "call|key|call|logic|call|logic|root|[2,\"ab\",1]\n");
}
