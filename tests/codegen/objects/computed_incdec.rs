//! Purpose:
//! Checks ordinary computed receiver incdec without the XML native gate.
//!
//! Called from:
//! - The object codegen integration suite.
//!
//! Key details:
//! - AOT and opaque eval verify typed, private and magic property updates.

use crate::support::*;

/// Computed property updates preserve typed storage, lexical visibility and magic hooks.
#[test]
fn test_computed_receiver_ordinary_property_incdec_bundle() {
    let source = r###"<?php
class ComputedIncdecStats { public int $hits = 10; public array $items = ['9', 4]; }
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
        $this->stats()->items[0]++;
        --$this->stats()->items[1];
        $this->stats()->items[1] += 2;
        echo json_encode($this->value->items), "\n";
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
    let expected = "stats|11|stats|12|stats|11|stats|10|self|self|self|self|20|stats|stats|stats|[10,5]\nmagic|get|set:31|magic|get|set:32|magic|get|set:31|magic|get|set:30|done\n";
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
