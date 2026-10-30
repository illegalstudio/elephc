//! Purpose:
//! Checks ordinary computed receiver incdec without the XML native gate.
//!
//! Called from:
//! - The object codegen integration suite.
//!
//! Key details:
//! - AOT and opaque eval verify typed, private and magic property updates.

use crate::support::*;

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
