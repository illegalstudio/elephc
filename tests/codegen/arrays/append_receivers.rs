//! Purpose:
//! Preserves computed receiver and key captures through nested property appends.
//!
//! Called from:
//! - The ordinary codegen array suite, without a managed XML dependency.
//!
//! Key details:
//! - AOT and opaque eval share the PHP oracle and strict heap/no-warning checks.

use crate::support::*;

/// Nested append planners evaluate each registry receiver and key once before writeback.
#[test]
fn test_nested_append_computed_property_receivers_bundle() {
    let source = r###"<?php
class CapturedAppendRegistry { public array $handlers = []; }
class CapturedAppendApp {
    private CapturedAppendRegistry $registry;
    public function __construct() { $this->registry = new CapturedAppendRegistry(); }
    public function registry(): CapturedAppendRegistry { echo 'registry|'; return $this->registry; }
    public function on(string $event, string $handler): void { $this->registry()->handlers[$event][] = $handler; }
}
function CapturedAppendKey(): string { echo 'key|'; return 'save'; }
$app = new CapturedAppendApp();
$app->on('save', 'a');
$app->on('save', 'b');
$app->registry()->handlers[CapturedAppendKey()][] = 'c';
echo json_encode($app->registry()->handlers);
unset($app);
"###;
    let renamed = source.replace("CapturedAppend", "EvalCapturedAppend");
    let body = renamed.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let eval = format!("<?php $source = $argc > 0 ? '{body}' : ''; eval($source);");
    let expected = "registry|registry|registry|key|registry|{\"save\":[\"a\",\"b\",\"c\"]}";
    assert_bundle_with(&[
        BundleCase { name: "nested_receiver_aot", source, expected },
        BundleCase { name: "nested_receiver_eval", source: &eval, expected },
    ], |source| {
        let out = compile_and_run_with_heap_debug(source);
        assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        assert!(!out.stderr.contains("Warning:"), "{}", out.stderr);
        out.stdout
    });
}
