//! Purpose:
//! Exercises repeated eval static writes with native payload COW and balanced owners.
//!
//! Called from:
//! - The runtime GC integration suite.
//!
//! Key details:
//! - Covers eval-owned live slots, static and variable value copies, and AOT getter snapshots.

use crate::support::*;

/// Repeated appends use runtime COW while static copies, element references and native slots survive.
#[test]
fn test_eval_static_array_mutation_owners_bundle() {
    let body = r#"
class EvalArrayWriteFirst { public static mixed $items = null; public static mixed $links = null; }
class EvalArrayWriteSecond { public static mixed $items = null; }
class EvalArrayWriteReference { public static mixed $items = null; }
EvalArrayWriteFirst::$items = [];
for ($i = 0; $i < 2048; $i++) { EvalArrayWriteFirst::$items[] = $i; }
EvalArrayWriteSecond::$items = EvalArrayWriteFirst::$items;
$copy = EvalArrayWriteFirst::$items;
EvalArrayWriteFirst::$items[0] = 77;
$class = 'EvalArrayWriteFirst'; $property = 'items';
$class::${$property}[1] += 1;
EvalArrayWriteFirst::$items['mark'] = 'kept';
echo count(EvalArrayWriteFirst::$items), ':', EvalArrayWriteFirst::$items[0], ':',
    EvalArrayWriteFirst::$items[1], ':', EvalArrayWriteSecond::$items[0], ':',
    $copy[0], ':', count(EvalArrayWriteSecond::$items), ':', EvalArrayWriteFirst::$items['mark'], "\n";
$element = 2;
EvalArrayWriteFirst::$links = [0, 1, &$element];
EvalArrayWriteFirst::$links['extra'] = 9;
EvalArrayWriteFirst::$links[2] = 42;
echo $element, ':', $copy[2], "\n";
$shared = EvalArrayWriteFirst::$items;
EvalArrayWriteFirst::$items = &$shared;
EvalArrayWriteReference::$items = &$shared;
EvalArrayWriteFirst::$items[] = 99;
echo count(EvalArrayWriteReference::$items), ':', count(EvalArrayWriteSecond::$items), "\n";
EvalArrayWriteFirst::$items = null;
EvalArrayWriteFirst::$links = null;
EvalArrayWriteSecond::$items = null;
EvalArrayWriteReference::$items = null;
unset($copy, $element, $shared, $class, $property, $i);
"#;
    let escaped = body.replace('\\', "\\\\").replace('\'', "\\'");
    let eval = format!("<?php $source = $argc > 0 ? '{escaped}' : ''; eval($source);");
    let native = r#"<?php
class NativeStaticArrayWrite { public static mixed $items = null; }
NativeStaticArrayWrite::$items = [1, 3];
$copy = NativeStaticArrayWrite::$items;
$source = $argc > 0 ? 'NativeStaticArrayWrite::$items[0] = 9; NativeStaticArrayWrite::$items[] = 4;' : '';
eval($source);
echo json_encode(NativeStaticArrayWrite::$items), ':', json_encode($copy), "\n";
NativeStaticArrayWrite::$items = null;
unset($copy);
"#;
    let history = r#"<?php
class StaticAppendHistoryBox { public static mixed $items = null; }
$history = [1, 2, 3]; unset($history[2]);
StaticAppendHistoryBox::$items = $history;
StaticAppendHistoryBox::$items[] = 9;
echo json_encode(StaticAppendHistoryBox::$items), "\n";
StaticAppendHistoryBox::$items = [PHP_INT_MAX => 1];
try { StaticAppendHistoryBox::$items[] = 2; }
catch (Error $error) { echo $error->getMessage(), "\n"; }
echo count(StaticAppendHistoryBox::$items), "\n";
StaticAppendHistoryBox::$items = null;
unset($history, $error);
"#;
    let history_body = history.strip_prefix("<?php").unwrap()
        .replace("StaticAppendHistoryBox", "EvalStaticAppendHistoryBox")
        .replace('\\', "\\\\").replace('\'', "\\'");
    let history_eval = format!("<?php $source = $argc > 0 ? '{history_body}' : ''; eval($source);");
    let history_expected = "{\"0\":1,\"1\":2,\"3\":9}\nCannot add element to the array as the next element is already occupied\n1\n";
    assert_bundle_with(&[
        BundleCase { name: "eval_static_live_storage", source: &eval, expected: "2049:77:2:0:0:2048:kept\n42:2\n2050:2048\n" },
        BundleCase { name: "native_static_getter_snapshot", source: native, expected: "[9,3,4]:[1,3]\n" },
        BundleCase { name: "static_append_history_aot", source: history, expected: history_expected },
        BundleCase { name: "static_append_history_eval", source: &history_eval, expected: history_expected },
    ], |source| {
        let out = compile_and_run_with_heap_debug(source);
        assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
        assert!(!out.stderr.contains("Warning:"), "{}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        out.stdout
    });
}
