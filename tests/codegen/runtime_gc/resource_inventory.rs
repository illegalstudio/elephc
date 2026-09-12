//! Purpose:
//! Verifies resource inventory aliases share ownership and close state.
//!
//! Called from:
//! - `cargo test --test codegen_tests test_core_resource_inventory_`.
//!
//! Key details:
//! - Native streams exercise descriptor reuse and final-owner destruction.

use crate::support::*;

/// Dropping an extracted resource or its inventory must not close another owner.
#[test]
fn test_core_resource_inventory_alias_drop_preserves_owner() {
    let out = compile_and_run(r#"<?php
$file = fopen('/dev/null', 'w');
$id = get_resource_id($file);
$resources = get_resources('stream');
$alias = $resources[$id];
unset($alias);
echo fwrite($file, 'test'), ':';
unset($resources);
echo fwrite($file, 'ok');
fclose($file);
"#);
    assert_eq!(out, "4:2");
}

/// Closing any alias updates every owner and cannot later close a reused descriptor.
#[test]
fn test_core_resource_inventory_close_shares_state_and_preserves_reused_fd() {
    let out = compile_and_run(r#"<?php
$first = fopen('/dev/null', 'w');
$id = get_resource_id($first);
$resources = get_resources('stream');
$alias = $resources[$id];
fclose($alias);
echo get_resource_type($first), ':', get_resource_type($alias), ':';
$second = fopen('/dev/null', 'w');
unset($first, $resources, $alias);
echo fwrite($second, 'ok');
fclose($second);
"#);
    assert_eq!(out, "Unknown:Unknown:2");
}

/// An inventory retains a live resource until its last extracted owner is released.
#[test]
fn test_core_resource_inventory_retains_last_owner_and_retires_dead_cells() {
    let out = compile_and_run(r#"<?php
$file = fopen('/dev/null', 'w');
$id = get_resource_id($file);
$resources = get_resources('stream');
unset($file);
$alias = $resources[$id];
unset($resources);
echo fwrite($alias, 'ok'), ':';
unset($alias);
$remaining = get_resources();
echo isset($remaining[$id]) ? 'present' : 'absent';
"#);
    assert_eq!(out, "2:absent");
}

/// Destroying an unaliased stream removes its weak inventory entry immediately.
#[test]
fn test_core_resource_inventory_final_owner_is_not_enumerated() {
    let out = compile_and_run(r#"<?php
$file = fopen('/dev/null', 'w');
$id = get_resource_id($file);
unset($file);
$resources = get_resources('stream');
echo isset($resources[$id]) ? 'present' : 'absent';
"#);
    assert_eq!(out, "absent");
}
