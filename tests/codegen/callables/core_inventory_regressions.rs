//! Purpose:
//! Pins inventory visibility and snapshot ownership across native and opaque eval code.
//!
//! Called from:
//! - `cargo test --test codegen_tests test_core_inventory_regression_`.
//!
//! Key details:
//! - Runtime-dependent eval source prevents compile-time declaration discovery.

use crate::support::*;

/// Eval resource type registration and close events are visible to both inventory consumers.
#[test]
fn test_core_inventory_regression_eval_resource_close_and_filter_type() {
    let out = compile_and_run(r#"<?php
$source = '$stream = fopen("php://memory", "w+");
$filter = stream_filter_append($stream, "string.toupper", STREAM_FILTER_WRITE);
echo count(get_resources("stream filter")), ":";
stream_filter_remove($filter);
fclose($stream);
echo count(get_resources("stream")), ":";' . ' // ' . $argc;
eval($source);
echo count(get_resources('stream')), ':', count(get_resources('stream filter'));
"#);
    assert_eq!(out, "1:3:3:0");
}

/// Flat and categorized constant snapshots preserve nested arrays and independent mutation.
#[test]
fn test_core_inventory_regression_array_constants() {
    let out = compile_and_run(r#"<?php
const INVENTORY_ITEMS = [1, 2];
const INVENTORY_NESTED = ['items' => [3, 'four'], 'nullable' => null, '42' => true];
$flat = get_defined_constants();
$categorized = get_defined_constants(true);
echo $flat['INVENTORY_ITEMS'][1], ':', $categorized['user']['INVENTORY_NESTED']['items'][1];
echo ':', $flat['INVENTORY_NESTED'][42] ? 'true' : 'bad';
$flat['INVENTORY_ITEMS'][1] = 9;
echo ':', $categorized['user']['INVENTORY_ITEMS'][1], ':', INVENTORY_ITEMS[1];
unset($flat);
echo ':', $categorized['user']['INVENTORY_NESTED']['items'][0];
"#);
    assert_eq!(out, "2:four:true:2:2:3");
}

/// Native flat/categorized inventories observe constants and functions declared by eval.
#[test]
fn test_core_inventory_regression_eval_declarations_visible_to_aot() {
    let out = compile_and_run(r#"<?php
$before = get_defined_constants();
$source = 'define("INVENTORY_LIVE", 42); function inventory_live_function(): int { return 7; }' . ' // ' . $argc;
eval($source);
$flat = get_defined_constants();
$categorized = get_defined_constants(true);
$functions = get_defined_functions();
echo isset($before['INVENTORY_LIVE']) ? 'bad' : 'absent';
echo ':', $flat['INVENTORY_LIVE'], ':', $categorized['user']['INVENTORY_LIVE'];
echo ':', in_array('inventory_live_function', $functions['user']) ? 'yes' : 'bad';
unset($flat);
echo ':', $categorized['user']['INVENTORY_LIVE'];
"#);
    assert_eq!(out, "absent:42:42:yes:42");
}

/// Eval sees native streams and a returned inventory alias shares native close state.
#[test]
fn test_core_inventory_regression_resources_shared_with_eval() {
    let out = compile_and_run(r#"<?php
$file = fopen('/dev/null', 'r');
$id = get_resource_id($file);
echo count(get_resources('stream')), ':';
$source = '$resources = get_resources("stream"); echo count($resources), ":";' . ' // ' . $argc;
eval($source);
unset($file);
echo count(get_resources('stream')), ':';
$alias = $resources[$id];
fclose($alias);
echo count(get_resources('stream'));
"#);
    assert_eq!(out, "4:4:4:3");
}
