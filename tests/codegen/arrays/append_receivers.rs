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
function CapturedAppendRhs(CapturedAppendRegistry $registry): string {
    echo json_encode($registry->handlers), '|';
    return 'd';
}
$app = new CapturedAppendApp();
$app->on('save', 'a');
$app->on('save', 'b');
$app->registry()->handlers[CapturedAppendKey()][] = 'c';
$captured = $app->registry();
$snapshot = $captured->handlers;
$captured->handlers['late'][] = CapturedAppendRhs($captured);
echo json_encode($app->registry()->handlers);
echo '|snapshot:', json_encode($snapshot);
unset($app, $captured, $snapshot);
"###;
    let renamed = source.replace("CapturedAppend", "EvalCapturedAppend");
    let body = renamed.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let eval = format!("<?php $source = $argc > 0 ? '{body}' : ''; eval($source);");
    let expected = "registry|registry|registry|key|registry|{\"save\":[\"a\",\"b\",\"c\"]}|registry|{\"save\":[\"a\",\"b\",\"c\"],\"late\":[\"d\"]}|snapshot:{\"save\":[\"a\",\"b\",\"c\"]}";
    // Distinct property names let a strict heap report identify the receiver path
    // that retained a carrier, while all cases still compile in one program.
    let (definitions, _) = source.split_once("$app = new CapturedAppendApp();").unwrap();
    let paths = [
        ("nested_receiver_eval_method", r#"
$app = new CapturedAppendApp();
$app->on('save', 'a'); $app->on('save', 'b');
unset($app);
"#, "registry|registry|"),
        ("nested_receiver_eval_result", r#"
$app = new CapturedAppendApp();
$captured = $app->registry();
unset($app, $captured);
"#, "registry|"),
        ("nested_receiver_eval_snapshot", r#"
$app = new CapturedAppendApp();
$captured = $app->registry();
$snapshot = $captured->handlers;
unset($app, $captured, $snapshot);
"#, "registry|"),
        ("nested_receiver_eval_cow_rhs", r#"
$app = new CapturedAppendApp(); $app->on('save', 'a');
$captured = $app->registry();
$snapshot = $captured->handlers;
$captured->handlers['late'][] = CapturedAppendRhs($captured);
unset($app, $captured, $snapshot);
"#, "registry|registry|{\"save\":[\"a\"]}|"),
        ("nested_receiver_eval_computed_key", r#"
$app = new CapturedAppendApp();
$app->registry()->handlers[CapturedAppendKey()][] = 'c';
unset($app);
"#, "registry|key|"),
    ];
    let isolated: Vec<_> = paths.iter().enumerate().map(|(id, (_, statements, _))| {
        let source = format!("{definitions}{statements}")
            .replace("CapturedAppend", &format!("Isolated{id}CapturedAppend"))
            .replace("handlers", &format!("probe{id}"));
        let body = source.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
        format!("<?php $source = $argc > 0 ? '{body}' : ''; eval($source);")
    }).collect();
    // Keep the full interaction too: reduced paths alone do not exercise an imported
    // AOT binding together with snapshot COW and the final computed property read.
    let full_paths = [
        ("nested_receiver_eval_full_distinct_variables", true),
        ("nested_receiver_eval_full_shared_variables", false),
    ];
    let full_variants: Vec<_> = full_paths.iter().enumerate().map(|(id, (_, distinct))| {
        let mut source = source.replace("CapturedAppend", &format!("Whole{id}CapturedAppend"))
            .replace("handlers", &format!("whole{id}"));
        if *distinct {
            source = source.replace("$app", "$evalOnlyApp")
                .replace("$captured", "$evalOnlyCaptured").replace("$snapshot", "$evalOnlySnapshot");
        }
        let body = source.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
        format!("<?php $source = $argc > 0 ? '{body}' : ''; eval($source);")
    }).collect();
    let mut cases = vec![
        BundleCase { name: "nested_receiver_aot", source, expected },
        BundleCase { name: "nested_receiver_eval", source: &eval, expected },
    ];
    cases.extend(paths.iter().zip(&isolated).map(|((name, _, expected), source)|
        BundleCase { name, source, expected }));
    cases.extend(full_paths.iter().zip(&full_variants).map(|((name, _), source)|
        BundleCase { name, source, expected }));
    assert_bundle_with(&cases, |source| {
        let out = compile_and_run_with_heap_debug(source);
        assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        assert!(!out.stderr.contains("Warning:"), "{}", out.stderr);
        out.stdout
    });
}
